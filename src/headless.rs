use crate::elevation;
use crate::health::{self, HealthReport};
use crate::modules;
use crate::paths;
use crate::report;
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
pub fn run() -> Result<()> {
    run_with_selection(None)
}

/// Run only the package-integrity module for a fast, explicit package scan.
/// This is intentionally separate from the full audit because package
/// verification can be expensive on large installations.
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

fn run_audit(selected_slugs: Option<&[&str]>) -> Result<()> {
    let modules = modules::all_modules();
    let selected = match selected_slugs {
        Some(slugs) => modules
            .iter()
            .enumerate()
            .filter(|(_, module)| slugs.contains(&module.slug()))
            .map(|(index, _)| index)
            .collect::<Vec<_>>(),
        None => (0..modules.len()).collect::<Vec<_>>(),
    };
    if selected.is_empty() {
        bail!("no audit modules matched the requested selection");
    }
    let health = health::compute_selected(&modules, &selected);
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let base_dir = paths::report_root();
    let root = base_dir.join(format!("full_system_audit-{timestamp}"));
    std::fs::create_dir_all(&root)
        .with_context(|| format!("creating report directory {}", root.display()))?;

    let mut states = vec!["idle".to_string(); modules.len()];
    let mut reports = Vec::new();
    let suggestions = suggestions::from_health(&health);
    let suggestions_path = suggestions::write_report(&root, &timestamp, &health, &suggestions)?;
    publish(
        &modules,
        &states,
        &health,
        &reports,
        &suggestions,
        Some(suggestions_path.display().to_string()),
        None,
        if selected_slugs.is_some() {
            "PACKAGE SCAN QUEUED"
        } else {
            "FULL SYSTEM AUDIT QUEUED"
        },
        None,
        &selected,
    );

    for (position, index) in selected.iter().copied().enumerate() {
        let module = &modules[index];
        states[index] = "running".to_string();
        publish(
            &modules,
            &states,
            &health,
            &reports,
            &suggestions,
            Some(suggestions_path.display().to_string()),
            None,
            &format!(
                "[{}/{}] SCANNING {}",
                position + 1,
                selected.len(),
                module.name().to_uppercase()
            ),
            None,
            &selected,
        );

        let dir = root.join(format!("{}-{timestamp}", module.slug()));
        let result = std::fs::create_dir_all(&dir)
            .and_then(|_| module.run(&dir).map_err(std::io::Error::other));
        match result {
            Ok(()) => {
                let report_path = dir.join(module.report_filename());
                reports.push(report_path.display().to_string());
                states[index] = "complete".to_string();
                publish(
                    &modules,
                    &states,
                    &health,
                    &reports,
                    &suggestions,
                    Some(suggestions_path.display().to_string()),
                    None,
                    &format!("COMPLETE / {}", report_path.display()),
                    None,
                    &selected,
                );
            }
            Err(error) => {
                states[index] = "failed".to_string();
                publish(
                    &modules,
                    &states,
                    &health,
                    &reports,
                    &suggestions,
                    Some(suggestions_path.display().to_string()),
                    None,
                    &format!("FAILED / {} / {}", module.name(), error),
                    None,
                    &selected,
                );
            }
        }
    }

    let relative_refs = reports
        .iter()
        .filter_map(|path| {
            let absolute = Path::new(path);
            let name = absolute
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("module")
                .to_string();
            let relative = absolute
                .strip_prefix(&root)
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
    report::write_summary(&root, &timestamp, &health, &summary_refs)?;
    let summary_path = root.join("SUMMARY.md").display().to_string();
    publish(
        &modules,
        &states,
        &health,
        &reports,
        &suggestions,
        Some(suggestions_path.display().to_string()),
        Some(summary_path),
        if selected_slugs.is_some() {
            "PACKAGE SCAN COMPLETE"
        } else {
            "AUDIT COMPLETE"
        },
        None,
        &selected,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn publish(
    modules: &[Box<dyn modules::AuditModule>],
    states: &[String],
    health: &HealthReport,
    reports: &[String],
    suggestions: &[Suggestion],
    suggestions_path: Option<String>,
    summary_path: Option<String>,
    message: &str,
    error: Option<String>,
    selected: &[usize],
) {
    let completed_count = states
        .iter()
        .filter(|state| state.as_str() == "complete")
        .count();
    let snapshot = AuditSnapshot {
        schema_version: 1,
        application: "omniscient",
        state: if error.is_some() {
            "error".to_string()
        } else if summary_path.is_some() {
            "complete".to_string()
        } else {
            "running".to_string()
        },
        updated_at: Local::now().to_rfc3339(),
        host: std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown-host".to_string()),
        selected_count: selected.len(),
        completed_count,
        health: Some(HealthSnapshot {
            score: health.score,
            notes: health.notes.clone(),
        }),
        modules: modules
            .iter()
            .enumerate()
            .map(|(index, module)| ModuleSnapshot {
                name: module.name().to_string(),
                slug: module.slug().to_string(),
                state: states
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string()),
                selected: selected.contains(&index),
                requires_sudo: module.requires_sudo(),
            })
            .collect(),
        reports: reports.to_vec(),
        suggestions: suggestions.to_vec(),
        suggestions_path,
        summary_path,
        error,
        message: message.to_string(),
    };
    let _ = snapshot::write(&snapshot);
}
