use anyhow::{Context, Result};
use serde::Serialize;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

/// Versioned, read-only state for desktop surfaces such as the Omarchy HUD.
///
/// The snapshot is deliberately independent of the Ratatui UI so a future
/// daemon, CLI mode, or Quickshell plugin can consume the same contract.
#[derive(Debug, Serialize)]
pub struct AuditSnapshot {
    pub schema_version: u32,
    pub application: &'static str,
    pub state: String,
    pub updated_at: String,
    pub host: String,
    pub selected_count: usize,
    pub completed_count: usize,
    pub health: Option<HealthSnapshot>,
    pub modules: Vec<ModuleSnapshot>,
    pub reports: Vec<String>,
    pub summary_path: Option<String>,
    pub error: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct HealthSnapshot {
    pub score: i32,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ModuleSnapshot {
    pub name: String,
    pub slug: String,
    pub state: String,
    pub selected: bool,
    pub requires_sudo: bool,
}

/// Resolve the state file consumed by local HUD clients.
///
/// `OMNISCIENT_SNAPSHOT_PATH` is useful for tests and controlled integrations.
/// Runtime state otherwise lives under `$XDG_RUNTIME_DIR`; the report-state
/// directory is the portable fallback when a runtime directory is absent.
pub fn path() -> PathBuf {
    if let Some(path) = non_empty_env("OMNISCIENT_SNAPSHOT_PATH") {
        return PathBuf::from(path);
    }
    if let Some(runtime_dir) = non_empty_env("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir)
            .join("omniscient")
            .join("snapshot.json");
    }
    crate::paths::report_root().join("snapshot.json")
}

/// Atomically publish a snapshot so readers never observe partial JSON.
pub fn write(snapshot: &AuditSnapshot) -> Result<PathBuf> {
    let destination = path();
    let parent = destination
        .parent()
        .context("snapshot path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("creating snapshot directory {}", parent.display()))?;

    let temporary = destination.with_extension("json.tmp");
    let mut file = File::create(&temporary)
        .with_context(|| format!("creating temporary snapshot {}", temporary.display()))?;
    serde_json::to_writer_pretty(&mut file, snapshot).context("encoding audit snapshot")?;
    file.write_all(b"\n")
        .context("terminating audit snapshot")?;
    file.sync_all().context("flushing audit snapshot")?;
    fs::rename(&temporary, &destination).with_context(|| {
        format!(
            "publishing snapshot {} from {}",
            destination.display(),
            temporary.display()
        )
    })?;
    Ok(destination)
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn snapshot_serializes_the_hud_contract() {
        let snapshot = AuditSnapshot {
            schema_version: 1,
            application: "omniscient",
            state: "ready".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            host: "test-host".to_string(),
            selected_count: 1,
            completed_count: 0,
            health: Some(HealthSnapshot {
                score: 100,
                notes: vec![],
            }),
            modules: vec![ModuleSnapshot {
                name: "Network Nexus".to_string(),
                slug: "network".to_string(),
                state: "queued".to_string(),
                selected: true,
                requires_sudo: false,
            }],
            reports: vec![],
            summary_path: None,
            error: None,
            message: "READY".to_string(),
        };

        let json = serde_json::to_value(snapshot).expect("snapshot should serialize");
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["application"], "omniscient");
        assert_eq!(json["state"], "ready");
        assert_eq!(json["modules"][0]["slug"], "network");
    }

    #[test]
    fn snapshot_path_honors_explicit_override() {
        let previous = std::env::var_os("OMNISCIENT_SNAPSHOT_PATH");
        std::env::set_var(
            "OMNISCIENT_SNAPSHOT_PATH",
            "/tmp/omniscient-test-snapshot.json",
        );
        assert_eq!(path(), Path::new("/tmp/omniscient-test-snapshot.json"));
        match previous {
            Some(value) => std::env::set_var("OMNISCIENT_SNAPSHOT_PATH", value),
            None => std::env::remove_var("OMNISCIENT_SNAPSHOT_PATH"),
        }
    }
}
