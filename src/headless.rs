use crate::elevation;
use crate::health::{self, HealthReport};
use crate::modules;
use crate::paths;
use crate::report;
use crate::runner;
use crate::snapshot::{self, AuditSnapshot, HealthSnapshot, ModuleSnapshot};
use crate::suggestions::{self, Suggestion};
use anyhow::{bail, Context, Result};
use chrono::Local;
use std::path::Path;

/// Run a complete audit for desktop surfaces that cannot host a terminal.
///
/// The interactive Ratatui dashboard remains the default executable mode.
/// This path performs the same module work synchronously while publishing
/// atomic snapshots after every meaningful state transition for the Omarchy
/// HUD to render in place.
///
/// # Errors
///
/// Returns an error when elevation fails or reports and snapshots cannot be
/// written.
pub fn run() -> Result<()> {
    run_with_selection(None)
}

/// Run only the package-integrity module for a fast, explicit package scan.
/// This is intentionally separate from the full audit because package
/// verification can be expensive on large installations.
///
/// # Errors
///
/// Returns an error when elevation fails or reports and snapshots cannot be
/// written.
pub fn run_packages() -> Result<()> {
    run_with_selection(Some(&["packages"]))
}

fn run_with_selection(selected_slugs: Option<&[&str]>) -> Result<()> {
    if elevation::reexec_graphical()? {
        return Ok(());
    }

    let result = run_audit(selected_slugs);
    if elevation::is_privileged() {
        elevation::restore_user_files()?;
    }
    result
}

/// Everything a snapshot is built from while an audit runs.
struct Progress<'a> {
    modules: &'a [Box<dyn modules::AuditModule>],
    selected: &'a [usize],
    states: Vec<String>,
    health: HealthReport,
    reports: Vec<String>,
    suggestions: Vec<Suggestion>,
    suggestions_path: Option<String>,
    summary_path: Option<String>,
}

impl Progress<'_> {
    fn publish(&self, message: &str) {
        let completed_count = self
            .states
            .iter()
            .filter(|state| state.as_str() == "complete")
            .count();
        let snapshot = AuditSnapshot {
            schema_version: 1,
            application: "omniscient",
            state: if self.summary_path.is_some() {
                "complete"
            } else {
                "running"
            }
            .to_string(),
            updated_at: Local::now().to_rfc3339(),
            host: std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown-host".to_string()),
            selected_count: self.selected.len(),
            completed_count,
            health: Some(HealthSnapshot {
                score: self.health.score,
                notes: self.health.notes.clone(),
            }),
            modules: self
                .modules
                .iter()
                .enumerate()
                .map(|(index, module)| ModuleSnapshot {
                    name: module.name().to_string(),
                    slug: module.slug().to_string(),
                    state: self
                        .states
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string()),
                    selected: self.selected.contains(&index),
                    requires_sudo: module.requires_sudo(),
                })
                .collect(),
            reports: self.reports.clone(),
            suggestions: self.suggestions.clone(),
            suggestions_path: self.suggestions_path.clone(),
            summary_path: self.summary_path.clone(),
            error: None,
            message: message.to_string(),
        };
        let _ = snapshot::write(&snapshot);
    }

    fn running_names(&self) -> String {
        self.states
            .iter()
            .enumerate()
            .filter(|(_, state)| state.as_str() == "running")
            .filter_map(|(index, _)| self.modules.get(index))
            .map(|module| module.name().to_uppercase())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

fn run_audit(selected_slugs: Option<&[&str]>) -> Result<()> {
    let modules = modules::all_modules();
    let selected = match selected_slugs {
        Some(slugs) => modules
            .iter()
            .enumerate()
            .filter(|(_, module)| slugs.contains(&module.slug()))
            .map(|(index, _)| index)
            .collect::<Vec<_>>(),
        // Package Integrity has a deliberately separate HUD action because
        // pacman file verification can dominate the duration of a normal
        // system pass. Keep the standard headless audit focused on the
        // operational modules; --packages selects it explicitly.
        None => modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.slug() != "packages")
            .map(|(index, _)| index)
            .collect::<Vec<_>>(),
    };
    if selected.is_empty() {
        bail!("no audit modules matched the requested selection");
    }
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let base_dir = paths::report_root();
    let root = base_dir.join(format!("full_system_audit-{timestamp}"));
    std::fs::create_dir_all(&root)
        .with_context(|| format!("creating report directory {}", root.display()))?;

    let health = health::compute_selected(&modules, &selected);
    let suggestions = suggestions::from_health(&health);
    let suggestions_path = suggestions::write_report(&root, &timestamp, &health, &suggestions)?;
    let mut progress = Progress {
        modules: &modules,
        selected: &selected,
        states: vec!["idle".to_string(); modules.len()],
        health,
        reports: Vec::new(),
        suggestions,
        suggestions_path: Some(suggestions_path.display().to_string()),
        summary_path: None,
    };
    for index in &selected {
        progress.states[*index] = "queued".to_string();
    }
    progress.publish(if selected_slugs.is_some() {
        "PACKAGE SCAN QUEUED"
    } else {
        "FULL SYSTEM AUDIT QUEUED"
    });

    let workers = runner::workers();
    let mut finished = 0;
    let mut report_order = Vec::new();
    runner::run(
        &modules,
        &selected,
        &root,
        &timestamp,
        workers,
        |event| match event {
            runner::Event::Started(index) => {
                progress.states[index] = "running".to_string();
                let running = progress.running_names();
                progress.publish(&format!(
                    "[{finished}/{}] SCANNING {running}",
                    selected.len()
                ));
            }
            runner::Event::Finished(index, result) => {
                finished += 1;
                match result {
                    Ok(report_path) => {
                        progress.states[index] = "complete".to_string();
                        report_order.push((index, report_path.display().to_string()));
                        report_order.sort();
                        progress.reports =
                            report_order.iter().map(|(_, path)| path.clone()).collect();
                        progress.publish(&format!(
                            "[{finished}/{}] COMPLETE / {}",
                            selected.len(),
                            report_path.display()
                        ));
                    }
                    Err(error) => {
                        progress.states[index] = "failed".to_string();
                        progress.publish(&format!("FAILED / {} / {error}", modules[index].name()));
                    }
                }
            }
        },
    );

    refine_with_signals(&mut progress, &base_dir, &root, &timestamp)?;
    write_summary(&progress, &root, &timestamp)?;
    progress.summary_path = Some(root.join("SUMMARY.md").display().to_string());
    progress.publish(if selected_slugs.is_some() {
        "PACKAGE SCAN COMPLETE"
    } else {
        "AUDIT COMPLETE"
    });
    Ok(())
}

/// Folds the deep-signals module's findings into health and suggestions and
/// writes the change report. A run without that module is left unchanged.
fn refine_with_signals(
    progress: &mut Progress<'_>,
    base_dir: &Path,
    root: &Path,
    timestamp: &str,
) -> Result<()> {
    let signals_dir = root.join(format!("signals-{timestamp}"));
    let Some(signals) = crate::signals::load(&signals_dir.join(crate::signals::SIGNALS_JSON))
    else {
        return Ok(());
    };
    health::apply_signals(&mut progress.health, &signals.findings);
    let mut refined = suggestions::from_health(&progress.health);
    refined.extend(suggestions::from_findings(&signals.findings));
    suggestions::write_report(root, timestamp, &progress.health, &refined)?;
    progress.suggestions = refined;
    let changes = crate::changes::write(base_dir, root, &signals)?;
    progress.reports.push(changes.display().to_string());
    Ok(())
}

fn write_summary(progress: &Progress<'_>, root: &Path, timestamp: &str) -> Result<()> {
    let relative_refs = progress
        .reports
        .iter()
        .filter_map(|path| {
            let absolute = Path::new(path);
            let name = absolute
                .parent()
                .filter(|parent| *parent != root)
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("changes since last audit")
                .to_string();
            let relative = absolute
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .to_string();
            Some((name, relative))
        })
        .collect::<Vec<_>>();
    let summary_refs = relative_refs
        .iter()
        .map(|(name, path)| (name.as_str(), path.as_str()))
        .collect::<Vec<_>>();
    report::write_summary(root, timestamp, &progress.health, &summary_refs)
}
