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
    pub explanation: String,
    pub command: String,
    pub manual_steps: Vec<String>,
    pub man_url: String,
    pub docs_url: String,
    pub auto_fix: bool,
    pub auto_fix_reason: String,
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
    writeln!(file, "# 🧰 Omniscient Fix Suggestions — {timestamp}\n")?;
    writeln!(
        file,
        "**Health score:** {} {}/100\n",
        health_emoji(health.score),
        health.score
    )?;

    if suggestions.is_empty() {
        writeln!(
            file,
            "No repair suggestions were generated for this audit.\n"
        )?;
        return Ok(path);
    }

    writeln!(file, "Review each action before applying it.\n")?;
    for suggestion in suggestions {
        writeln!(
            file,
            "## {} [{}] {}\n",
            severity_emoji(&suggestion.severity),
            suggestion.severity,
            suggestion.title
        )?;
        writeln!(file, "{}\n", suggestion.detail)?;
        writeln!(file, "### Explanation\n{}\n", suggestion.explanation)?;
        writeln!(file, "- Command: `{}`", suggestion.command)?;
        writeln!(file, "### Manual recovery\n")?;
        for step in &suggestion.manual_steps {
            writeln!(file, "- `{step}`")?;
        }
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
        if !suggestion.auto_fix_reason.is_empty() {
            writeln!(file, "- Automatic fix note: {}", suggestion.auto_fix_reason)?;
        }
        writeln!(file)?;
    }

    Ok(path)
}

fn severity_emoji(severity: &str) -> &'static str {
    match severity {
        "urgent" => "🚨",
        "warning" => "⚠️",
        "attention" => "🟠",
        _ => "ℹ️",
    }
}

fn health_emoji(score: i32) -> &'static str {
    if score < 40 {
        "🚨"
    } else if score < 70 {
        "⚠️"
    } else if score < 85 {
        "🟠"
    } else {
        "✅"
    }
}

fn suggestion_for_note(note: &str) -> Option<Suggestion> {
    if let Some(tool) = note.strip_prefix("missing tool: ") {
        return missing_tool_suggestion(tool.trim());
    }

    if let Some(units) = note.strip_prefix("failed systemd units: ") {
        let units = units.trim();
        let unit_args = units.replace(", ", " ");
        return Some(Suggestion {
            id: "inspect-failed-services".to_string(),
            severity: "warning".to_string(),
            title: format!("Inspect failed systemd service(s): {units}"),
            detail: format!("{} failed unit(s) require investigation: {units}.", units.split(',').count()),
            explanation: "A failed systemd unit stopped or could not start. The cause may be a bad configuration, a missing dependency, a permissions problem, a device failure, or a recent update. Omniscient will not restart or reset it automatically because doing so can hide the original failure and may interrupt a service that other applications depend on.".to_string(),
            command: "systemctl --failed --no-pager".to_string(),
            manual_steps: vec![
                "systemctl --failed --no-pager".to_string(),
                format!("systemctl status {unit_args} --no-pager -l"),
                format!("journalctl -u {unit_args} -b --no-pager"),
                format!("sudo systemctl restart {unit_args}"),
            ],
            man_url: "https://man.archlinux.org/man/systemctl.1.en".to_string(),
            docs_url: "https://www.freedesktop.org/software/systemd/man/latest/systemctl.html"
                .to_string(),
            auto_fix: false,
            auto_fix_reason: "No safe automatic repair is available. Review the unit status and boot journal before restarting it.".to_string(),
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
            explanation: "SMART is reporting a drive health failure. This can indicate imminent media failure or an unrecoverable hardware condition. Do not attempt filesystem repair or overwrite the device until important data is backed up and the drive's diagnostic output has been reviewed.".to_string(),
            command: format!("sudo smartctl -a {device}"),
            manual_steps: vec![
                format!("sudo smartctl -a {device}"),
                format!("sudo smartctl -t short {device}"),
                "sudo smartctl -l selftest <device>".to_string(),
                "Back up important data before filesystem repair or replacement.".to_string(),
            ],
            man_url: "https://man.archlinux.org/man/smartctl.8.en".to_string(),
            docs_url: "https://www.smartmontools.org/wiki/FAQ".to_string(),
            auto_fix: false,
            auto_fix_reason: "No automatic repair is offered for failing hardware. Replacement and recovery decisions require human review.".to_string(),
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
        explanation: format!("The `{tool}` executable is not available on PATH, so one part of the audit could not collect its normal evidence. Installing the allowlisted package restores that audit coverage; it does not change the underlying system configuration beyond installing the package."),
        command: format!("sudo /usr/bin/pacman -S --needed {package_name}"),
        manual_steps: vec![
            format!("command -v {tool} || pacman -Ss {package_name}"),
            format!("sudo /usr/bin/pacman -S --needed {package_name}"),
            format!("{tool} --help"),
        ],
        man_url: man_url_for_tool(tool),
        docs_url: docs_url_for_tool(tool),
        auto_fix,
        auto_fix_reason: if auto_fix {
            format!("The allowlisted package `{package_name}` can be installed after explicit authorization.")
        } else {
            "No allowlisted package mapping exists, so installation must be handled manually.".to_string()
        },
        requires_auth: true,
    })
}

fn man_url_for_tool(tool: &str) -> String {
    let section = match tool {
        "smartctl" => "8",
        _ => "1",
    };
    format!("https://man.archlinux.org/man/{tool}.{section}.en")
}

fn docs_url_for_tool(tool: &str) -> String {
    match tool {
        "bluetoothctl" => "https://man.archlinux.org/man/bluetoothctl.1.en".to_string(),
        "btrfs" => "https://btrfs.readthedocs.io/en/latest/".to_string(),
        "lshw" => "https://ezix.org/project/wiki/HardwareLiSter".to_string(),
        "lsusb" => "https://man.archlinux.org/man/lsusb.8.en".to_string(),
        "lspci" => "https://man.archlinux.org/man/lspci.8.en".to_string(),
        "smartctl" => "https://www.smartmontools.org/wiki/FAQ".to_string(),
        "systemctl" | "journalctl" => {
            "https://www.freedesktop.org/software/systemd/man/latest/systemctl.html".to_string()
        }
        "ip" | "ss" => "https://wiki.archlinux.org/title/Network_configuration".to_string(),
        _ => "https://wiki.archlinux.org/title/Pacman".to_string(),
    }
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
