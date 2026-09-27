//! Watch mode (`omniscient --watch`): a light, unprivileged check meant for
//! an hourly user timer. It runs the Deep Signals checks, compares them with
//! the previous watch, sends a desktop notification for each *new* finding
//! at warning or above (clicking it opens the HUD where the problem is), and
//! publishes `watch.json` for the bar widget.
//!
//! It never elevates: module elevation is `sudo -n` only, and the shipped
//! systemd unit sets `NoNewPrivileges=yes` on top, so no password prompt can
//! ever come from a timer.

use crate::signals::{self, Finding, Severity};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Most notifications one watch sends; beyond that one summary is sent.
pub const MAX_TOASTS: usize = 3;
/// Most findings published for the bar widget.
pub const MAX_PUBLISHED: usize = 20;
const PLUGIN_ID: &str = "io.github.cybercore-tech.omniscient";

/// Remembered between watches (in the report root, so it survives reboots).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub keys: Vec<String>,
    pub updated_at: String,
}

/// One finding as published for the bar widget and HUD.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Published {
    pub key: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub new: bool,
    /// Unit to open in the JOURNAL tab, when the finding names one.
    pub unit: String,
}

/// `watch.json`, next to the HUD snapshot.
#[derive(Debug, Serialize)]
pub struct Report {
    pub version: u32,
    pub updated_at: String,
    pub worst: Option<Severity>,
    pub counts: Counts,
    pub new_count: usize,
    pub resolved_count: usize,
    pub findings: Vec<Published>,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub urgent: usize,
    pub warning: usize,
    pub watch: usize,
}

/// The unit a finding is about, for opening the JOURNAL tab on it.
#[must_use]
pub fn unit_of(key: &str) -> String {
    let parts = key.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        ["restart-loop" | "ecosystem", _, unit] => (*unit).to_owned(),
        ["timer-failed", _, timer] => timer
            .strip_suffix(".timer")
            .map_or_else(String::new, |base| format!("{base}.service")),
        _ => String::new(),
    }
}

/// Compares this watch's findings with the previous state. Returns the
/// published list (most serious first, new ones marked) and how many
/// previously seen findings are gone.
#[must_use]
pub fn compare(findings: &[Finding], previous: &State) -> (Vec<Published>, usize) {
    let mut published = findings
        .iter()
        .filter(|f| f.severity > Severity::Info)
        .map(|f| Published {
            key: f.key.clone(),
            severity: f.severity,
            title: f.title.clone(),
            detail: f.detail.clone(),
            new: !previous.keys.contains(&f.key),
            unit: unit_of(&f.key),
        })
        .collect::<Vec<_>>();
    published.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| b.new.cmp(&a.new))
            .then_with(|| a.key.cmp(&b.key))
    });
    let resolved = previous
        .keys
        .iter()
        .filter(|key| {
            !findings
                .iter()
                .any(|f| &f.key == *key && f.severity > Severity::Info)
        })
        .count();
    (published, resolved)
}

/// Findings worth a notification: new, at warning or above.
#[must_use]
pub fn alerts(published: &[Published]) -> Vec<&Published> {
    published
        .iter()
        .filter(|p| p.new && p.severity >= Severity::Warning)
        .collect()
}

/// HUD payload opening the panel where a finding is best inspected.
#[must_use]
pub fn payload_for(finding: Option<&Published>) -> String {
    match finding.filter(|f| !f.unit.is_empty()) {
        Some(f) => {
            serde_json::json!({ "tab": "journal", "unit": f.unit, "priority": 4 }).to_string()
        }
        None => serde_json::json!({ "tab": "audit", "watch": true }).to_string(),
    }
}

/// `omarchy-notification-send` arguments for one alert (or a summary).
#[must_use]
pub fn toast_args(alerts: &[&Published]) -> Vec<Vec<String>> {
    let open = |payload: String| {
        vec![
            "--exec".to_owned(),
            "omarchy-shell".to_owned(),
            "shell".to_owned(),
            "summon".to_owned(),
            PLUGIN_ID.to_owned(),
            payload,
        ]
    };
    let one = |f: &Published| {
        let mut args = vec![
            "--app-name".to_owned(),
            "Omniscient".to_owned(),
            "-g".to_owned(),
            "󰓦".to_owned(),
            "-u".to_owned(),
            if f.severity == Severity::Urgent {
                "critical"
            } else {
                "normal"
            }
            .to_owned(),
            format!("Omniscient: {}", crate::capture::sanitize(&f.title)),
            crate::capture::bound_text(&f.detail, 0, 300, 3)
                .trim()
                .to_owned(),
        ];
        args.extend(open(payload_for(Some(f))));
        args
    };
    if alerts.len() <= MAX_TOASTS {
        return alerts.iter().map(|f| one(f)).collect();
    }
    let mut summary = vec![
        "--app-name".to_owned(),
        "Omniscient".to_owned(),
        "-g".to_owned(),
        "󰓦".to_owned(),
        "-u".to_owned(),
        if alerts.iter().any(|f| f.severity == Severity::Urgent) {
            "critical"
        } else {
            "normal"
        }
        .to_owned(),
        format!("Omniscient: {} new findings", alerts.len()),
        alerts
            .iter()
            .take(4)
            .map(|f| crate::capture::sanitize(&f.title))
            .collect::<Vec<_>>()
            .join("\n"),
    ];
    summary.extend(open(payload_for(None)));
    vec![summary]
}

/// The single toast of a first watch.
#[must_use]
pub fn baseline_toast(tracked: usize) -> Vec<String> {
    vec![
        "--app-name".to_owned(),
        "Omniscient".to_owned(),
        "-g".to_owned(),
        "󰓦".to_owned(),
        "-u".to_owned(),
        "low".to_owned(),
        "Omniscient watch is on".to_owned(),
        format!("Tracking {tracked} current findings; you will be told when a new one appears."),
        "--exec".to_owned(),
        "omarchy-shell".to_owned(),
        "shell".to_owned(),
        "summon".to_owned(),
        PLUGIN_ID.to_owned(),
        payload_for(None),
    ]
}

fn state_path() -> PathBuf {
    crate::paths::report_root().join("watch-state.json")
}

/// `watch.json` lives next to the HUD snapshot.
#[must_use]
pub fn report_path() -> PathBuf {
    crate::snapshot::path().with_file_name("watch.json")
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let parent = path.parent().context("path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text).with_context(|| format!("writing {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("publishing {}", path.display()))
}

fn notify(args: &[String]) {
    let limits = crate::capture::Limits {
        timeout: Duration::from_secs(10),
        retain_bytes: 4096,
    };
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    if let Some(exe) = crate::pathcheck::resolve("omarchy-notification-send") {
        let _ = crate::capture::run(&exe, &refs, limits);
    } else if let Some(exe) = crate::pathcheck::resolve("notify-send") {
        // Plain fallback without click-to-open: summary and body only.
        let plain = [
            "-a",
            "Omniscient",
            args.get(6).map_or("Omniscient", String::as_str),
            args.get(7).map_or("", String::as_str),
        ];
        let _ = crate::capture::run(&exe, &plain, limits);
    }
}

/// `omniscient --watch`.
///
/// # Errors
///
/// Returns an error when the watch state or report cannot be written.
pub fn run(args: &[String]) -> Result<()> {
    let quiet = args.iter().any(|arg| arg == "--no-notify");
    let sections = signals::collect();
    let now = chrono::Local::now().to_rfc3339();
    let findings = signals::to_signals(&sections, &now).findings;
    let stored = fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str::<State>(&text).ok());
    // The first watch records a baseline: problems that already exist are
    // not "new", so they get one summary instead of a burst of alerts.
    let baseline = stored.is_none();
    let previous = stored.unwrap_or_default();
    let (mut published, resolved) = compare(&findings, &previous);
    if baseline {
        for finding in &mut published {
            finding.new = false;
        }
    }
    let alerting = alerts(&published);
    let cpu = crate::sensors::read_all(&crate::sensors::root(), Duration::ZERO, false)
        .cpu
        .package_celsius;
    crate::trends::record_watch(
        published
            .iter()
            .filter(|p| p.severity >= Severity::Warning)
            .count(),
        cpu,
    );
    if !quiet {
        if baseline {
            notify(&baseline_toast(published.len()));
        } else {
            for toast in toast_args(&alerting) {
                notify(&toast);
            }
        }
    }
    let mut counts = Counts::default();
    for finding in &published {
        match finding.severity {
            Severity::Urgent => counts.urgent += 1,
            Severity::Warning => counts.warning += 1,
            _ => counts.watch += 1,
        }
    }
    let report = Report {
        version: 1,
        updated_at: now.clone(),
        worst: published.first().map(|f| f.severity),
        counts,
        new_count: published.iter().filter(|p| p.new).count(),
        resolved_count: resolved,
        findings: published.iter().take(MAX_PUBLISHED).cloned().collect(),
    };
    write_atomic(&report_path(), &serde_json::to_string_pretty(&report)?)?;
    let state = State {
        keys: published.iter().map(|p| p.key.clone()).collect(),
        updated_at: now,
    };
    write_atomic(&state_path(), &serde_json::to_string_pretty(&state)?)?;
    crate::emit(&format!(
        "watch: {} findings ({} new, {} alerted, {} resolved) -> {}",
        published.len(),
        report.new_count,
        alerting.len(),
        resolved,
        report_path().display()
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(key: &str, severity: Severity) -> Finding {
        Finding {
            key: key.into(),
            severity,
            title: format!("title {key}"),
            detail: "detail".into(),
            command: String::new(),
            docs_url: String::new(),
        }
    }

    #[test]
    fn units_are_derived_from_finding_keys() {
        assert_eq!(
            unit_of("restart-loop:system:cyberdeck-diag-deck.service"),
            "cyberdeck-diag-deck.service"
        );
        assert_eq!(
            unit_of("ecosystem:system:sigilward-check.service"),
            "sigilward-check.service"
        );
        assert_eq!(unit_of("timer-failed:user:backup.timer"), "backup.service");
        assert_eq!(unit_of("crash:chromium"), "");
        assert_eq!(unit_of("pacnew"), "");
    }

    #[test]
    fn only_new_warnings_alert_and_resolved_are_counted() {
        let previous = State {
            keys: vec!["old-warning".into(), "gone".into()],
            updated_at: String::new(),
        };
        let findings = vec![
            finding("old-warning", Severity::Warning),
            finding("restart-loop:system:x.service", Severity::Urgent),
            finding("new-watch", Severity::Watch),
            finding("info", Severity::Info),
        ];
        let (published, resolved) = compare(&findings, &previous);
        assert_eq!(published.len(), 3, "info findings are not published");
        assert_eq!(
            published[0].key, "restart-loop:system:x.service",
            "most serious first"
        );
        assert_eq!(resolved, 1);
        let alerting = alerts(&published);
        assert_eq!(
            alerting.len(),
            1,
            "an old warning and a new watch-level finding do not alert"
        );
        assert_eq!(alerting[0].unit, "x.service");
    }

    #[test]
    fn toasts_open_the_right_place_and_summarize_bursts() {
        let urgent = Published {
            key: "restart-loop:system:x.service".into(),
            severity: Severity::Urgent,
            title: "x restarted 229 times\u{1b}[31m".into(),
            detail: "d".into(),
            new: true,
            unit: "x.service".into(),
        };
        let toasts = toast_args(&[&urgent]);
        assert_eq!(toasts.len(), 1);
        let t = &toasts[0];
        assert_eq!(t[5], "critical");
        assert_eq!(
            t[6], "Omniscient: x restarted 229 times",
            "escape sequences are stripped"
        );
        assert_eq!(&t[8..12], ["--exec", "omarchy-shell", "shell", "summon"]);
        assert_eq!(t[12], PLUGIN_ID);
        let payload: serde_json::Value = serde_json::from_str(&t[13]).expect("payload");
        assert_eq!(payload["tab"], "journal");
        assert_eq!(payload["unit"], "x.service");
        let many = [&urgent, &urgent, &urgent, &urgent];
        let summary = toast_args(&many);
        assert_eq!(
            summary.len(),
            1,
            "more than three alerts become one summary"
        );
        assert!(summary[0][6].contains("4 new findings"));
        let payload: serde_json::Value =
            serde_json::from_str(summary[0].last().expect("payload")).expect("json");
        assert_eq!(payload["tab"], "audit");
    }
}
