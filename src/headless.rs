use crate::health::{self, HealthReport};
use crate::modules;
use crate::paths;
use crate::report;
use crate::snapshot::{self, AuditSnapshot, HealthSnapshot, ModuleSnapshot};
use anyhow::{Context, Result};
use chrono::Local;
use std::path::Path;

/// Run a complete audit for desktop surfaces that cannot host a terminal.
///
/// The interactive Ratatui dashboard remains the default executable mode.
/// This path performs the same module work synchronously while publishing
/// atomic snapshots after every meaningful state transition for the Omarchy
/// HUD to render in place.
pub fn run() -> Result<()> {
    let modules = modules::all_modules();
    let selected = (0..modules.len()).collect::<Vec<_>>();
    let health = health::compute_selected(&modules, &selected);
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let base_dir = paths::report_root();
    let root = base_dir.join(format!("full_system_audit-{timestamp}"));
    std::fs::create_dir_all(&root)
        .with_context(|| format!("creating report directory {}", root.display()))?;

    let mut states = vec!["queued".to_string(); modules.len()];
    let mut reports = Vec::new();
    publish(
        &modules,
        &states,
        &health,
        &reports,
        None,
        "FULL SYSTEM AUDIT QUEUED",
        None,
    );

    for (position, index) in selected.iter().copied().enumerate() {
        let module = &modules[index];
        states[index] = "running".to_string();
        publish(
            &modules,
            &states,
            &health,
            &reports,
            None,
            &format!(
                "[{}/{}] SCANNING {}",
                position + 1,
                selected.len(),
                module.name().to_uppercase()
            ),
            None,
        );

        let dir = root.join(format!("{}-{timestamp}", module.slug()));
        let result = std::fs::create_dir_all(&dir)
            .and_then(|_| module.run(&dir).map_err(std::io::Error::other));
        match result {
            Ok(()) => {
                let report_path = dir.join(format!("{}.md", module.slug()));
                reports.push(report_path.display().to_string());
                states[index] = "complete".to_string();
                publish(
                    &modules,
                    &states,
                    &health,
                    &reports,
                    None,
                    &format!("COMPLETE / {}", report_path.display()),
                    None,
                );
            }
            Err(error) => {
                states[index] = "failed".to_string();
                publish(
                    &modules,
                    &states,
                    &health,
                    &reports,
                    None,
                    &format!("FAILED / {} / {}", module.name(), error),
                    None,
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
        Some(summary_path),
        "AUDIT COMPLETE",
        None,
    );
    Ok(())
}

fn publish(
    modules: &[Box<dyn modules::AuditModule>],
    states: &[String],
    health: &HealthReport,
    reports: &[String],
    summary_path: Option<String>,
    message: &str,
    error: Option<String>,
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
        selected_count: modules.len(),
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
                selected: true,
                requires_sudo: module.requires_sudo(),
            })
            .collect(),
        reports: reports.to_vec(),
        summary_path,
        error,
        message: message.to_string(),
    };
    let _ = snapshot::write(&snapshot);
}
