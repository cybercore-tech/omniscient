use crate::health::HealthReport;
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub struct Suggestion {
    pub id: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub command: String,
    pub man_url: String,
    pub docs_url: String,
    pub auto_fix: bool,
    pub requires_auth: bool,
}

pub fn from_health(health: &HealthReport) -> Vec<Suggestion> {
    health
        .notes
        .iter()
        .filter_map(|note| suggestion_for_note(note))
        .collect()
}

pub fn write_report(
    dir: &Path,
    timestamp: &str,
    health: &HealthReport,
    suggestions: &[Suggestion],
) -> Result<PathBuf> {
    let path = dir.join("SUGGESTIONS.md");
    let mut file = File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    writeln!(file, "# Omniscient Fix Suggestions — {timestamp}\n")?;
    writeln!(file, "**Health score:** {}/100\n", health.score)?;

    if suggestions.is_empty() {
        writeln!(
            file,
            "No repair suggestions were generated for this audit.\n"
        )?;
        return Ok(path);
    }

    writeln!(file, "Review each action before applying it.\n")?;
    for suggestion in suggestions {
        writeln!(file, "## [{}] {}\n", suggestion.severity, suggestion.title)?;
        writeln!(file, "{}\n", suggestion.detail)?;
        writeln!(file, "- Command: `{}`", suggestion.command)?;
        writeln!(file, "- [Manual page]({})", suggestion.man_url)?;
        writeln!(file, "- [Documentation]({})", suggestion.docs_url)?;
        writeln!(
            file,
            "- Automatic fix: {}\n",
            if suggestion.auto_fix {
                "available after confirmation"
            } else {
                "manual review required"
            }
        )?;
    }

    Ok(path)
}

fn suggestion_for_note(note: &str) -> Option<Suggestion> {
    if let Some(tool) = note.strip_prefix("missing tool: ") {
        return missing_tool_suggestion(tool.trim());
    }

    if note.contains("failed systemd unit") {
        return Some(Suggestion {
            id: "inspect-failed-services".to_string(),
            severity: "warning".to_string(),
            title: "Inspect failed systemd services".to_string(),
            detail: note.to_string(),
            command: "systemctl --failed --no-pager".to_string(),
            man_url: "https://man.archlinux.org/man/systemctl.1.en".to_string(),
            docs_url: "https://www.freedesktop.org/software/systemd/man/latest/systemctl.html"
                .to_string(),
            auto_fix: false,
            requires_auth: false,
        });
    }

    if let Some(device) = note.strip_prefix("SMART health check FAILED on ") {
        return Some(Suggestion {
            id: format!("inspect-smart:{}", device),
            severity: "urgent".to_string(),
            title: format!("Investigate SMART failure on {device}"),
            detail: "Back up important data and inspect the drive before attempting repairs."
                .to_string(),
            command: format!("sudo smartctl -a {device}"),
            man_url: "https://man.archlinux.org/man/smartctl.8.en".to_string(),
            docs_url: "https://www.smartmontools.org/wiki/FAQ".to_string(),
            auto_fix: false,
            requires_auth: true,
        });
    }

    None
}

fn missing_tool_suggestion(tool: &str) -> Option<Suggestion> {
    if !tool
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return None;
    }

    let package = package_for_tool(tool);
    let auto_fix = package.is_some();
    let package_name = package.unwrap_or(tool);
    Some(Suggestion {
        id: format!("install-tool:{tool}"),
        severity: "attention".to_string(),
        title: format!("Install missing tool: {tool}"),
        detail: format!("Omniscient could not find `{tool}`. Install its Arch package to improve audit coverage."),
        command: format!("sudo pacman -S --needed {package_name}"),
        man_url: format!("https://man.archlinux.org/man/{tool}.1.en"),
        docs_url: "https://wiki.archlinux.org/title/Pacman".to_string(),
        auto_fix,
        requires_auth: true,
    })
}

pub fn package_for_tool(tool: &str) -> Option<&'static str> {
    match tool {
        "lshw" => Some("lshw"),
        "smartctl" => Some("smartmontools"),
        "btrfs" => Some("btrfs-progs"),
        "lsusb" => Some("usbutils"),
        "lspci" => Some("pciutils"),
        "bluetoothctl" => Some("bluez-utils"),
        "dmesg" => Some("util-linux"),
        "journalctl" | "systemctl" => Some("systemd"),
        "ip" | "ss" => Some("iproute2"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tool_suggests_allowlisted_install() {
        let health = HealthReport {
            score: 95,
            notes: vec!["missing tool: lshw".to_string()],
        };
        let suggestions = from_health(&health);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].id, "install-tool:lshw");
        assert!(suggestions[0].auto_fix);
    }

    #[test]
    fn smart_failures_are_urgent_and_manual() {
        let health = HealthReport {
            score: 55,
            notes: vec!["SMART health check FAILED on /dev/sda".to_string()],
        };
        let suggestions = from_health(&health);
        assert_eq!(suggestions[0].severity, "urgent");
        assert!(!suggestions[0].auto_fix);
    }
}
