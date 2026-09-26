//! "What changed since the last audit": compares this audit's deep signals
//! with the most recent earlier audit that has them, and writes `CHANGES.md`.

use crate::signals::{Finding, Signals, SIGNALS_JSON};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Most added or removed entries listed per fact before summarizing.
const MAX_LISTED: usize = 40;

/// Finds the signals of the newest audit under `report_root` that is older
/// than `current_root` (audit directories sort by their timestamp).
#[must_use]
pub fn previous(report_root: &Path, current_root: &Path) -> Option<(String, Signals)> {
    let current = current_root.file_name()?.to_string_lossy().into_owned();
    let mut audits = fs::read_dir(report_root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("full_system_audit-") && *name < current)
        .collect::<Vec<_>>();
    audits.sort();
    audits.into_iter().rev().find_map(|name| {
        let dir = report_root.join(&name);
        let signals_dir = fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("signals-"))
            })?;
        crate::signals::load(&signals_dir.join(SIGNALS_JSON)).map(|signals| (name, signals))
    })
}

fn keyed(findings: &[Finding]) -> BTreeMap<&str, &Finding> {
    findings
        .iter()
        .map(|finding| (finding.key.as_str(), finding))
        .collect()
}

fn list(out: &mut String, label: &str, items: &[&String]) {
    if items.is_empty() {
        return;
    }
    let _ = writeln!(out, "{label} ({}):", items.len());
    for item in items.iter().take(MAX_LISTED) {
        let _ = writeln!(out, "  {item}");
    }
    if items.len() > MAX_LISTED {
        let _ = writeln!(out, "  … and {} more", items.len() - MAX_LISTED);
    }
}

/// Renders the change report body.
#[must_use]
pub fn render(previous: Option<(&str, &Signals)>, current: &Signals) -> String {
    let mut out = String::from("# 🔀 CHANGES SINCE THE LAST AUDIT\n\n");
    let Some((label, previous)) = previous else {
        out.push_str(
            "This is the first audit with deep signals; the next one will be compared with it.\n",
        );
        return out;
    };
    let _ = writeln!(out, "Compared with `{label}`.\n");

    let (before, after) = (keyed(&previous.findings), keyed(&current.findings));
    out.push_str("## findings\n\n```\n");
    let mut any = false;
    for (key, finding) in &after {
        if !before.contains_key(key) {
            any = true;
            let _ = writeln!(
                out,
                "NEW       [{}] {}",
                finding.severity.label().to_uppercase(),
                finding.title
            );
        } else if before[key].severity != finding.severity {
            any = true;
            let _ = writeln!(
                out,
                "CHANGED   [{} → {}] {}",
                before[key].severity.label().to_uppercase(),
                finding.severity.label().to_uppercase(),
                finding.title
            );
        }
    }
    for (key, finding) in &before {
        if !after.contains_key(key) {
            any = true;
            let _ = writeln!(
                out,
                "RESOLVED  [{}] {}",
                finding.severity.label().to_uppercase(),
                finding.title
            );
        }
    }
    if !any {
        out.push_str("no finding appeared, changed or was resolved\n");
    }
    out.push_str("```\n\n");

    let names = previous
        .facts
        .keys()
        .chain(current.facts.keys())
        .collect::<BTreeSet<_>>();
    out.push_str("## inventory\n\n```\n");
    let mut changed = false;
    let empty = Vec::new();
    for name in names {
        let old = previous
            .facts
            .get(name)
            .unwrap_or(&empty)
            .iter()
            .collect::<BTreeSet<_>>();
        let new = current
            .facts
            .get(name)
            .unwrap_or(&empty)
            .iter()
            .collect::<BTreeSet<_>>();
        let added = new.difference(&old).copied().collect::<Vec<_>>();
        let removed = old.difference(&new).copied().collect::<Vec<_>>();
        if added.is_empty() && removed.is_empty() {
            continue;
        }
        changed = true;
        let _ = writeln!(out, "{}", name.to_uppercase());
        list(&mut out, "  added", &added);
        list(&mut out, "  removed", &removed);
    }
    if !changed {
        out.push_str("no inventory changes\n");
    }
    out.push_str("```\n");
    out
}

/// Writes `CHANGES.md` into the audit root and returns its path.
///
/// # Errors
///
/// Returns an error when the report cannot be written.
pub fn write(report_root: &Path, audit_root: &Path, current: &Signals) -> Result<PathBuf> {
    let previous = previous(report_root, audit_root);
    let body = render(
        previous
            .as_ref()
            .map(|(label, signals)| (label.as_str(), signals)),
        current,
    );
    let path = audit_root.join("CHANGES.md");
    fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{previous, render};
    use crate::signals::{Finding, Severity, Signals, SIGNALS_JSON};

    fn finding(key: &str, severity: Severity) -> Finding {
        Finding {
            key: key.into(),
            severity,
            title: format!("title {key}"),
            detail: String::new(),
            command: String::new(),
            docs_url: String::new(),
        }
    }

    fn signals(findings: Vec<Finding>, facts: &[(&str, &[&str])]) -> Signals {
        Signals {
            version: 1,
            generated_at: String::new(),
            findings,
            facts: facts
                .iter()
                .map(|(name, values)| {
                    ((*name).into(), values.iter().map(|v| (*v).into()).collect())
                })
                .collect(),
        }
    }

    #[test]
    fn findings_and_facts_are_diffed() {
        let old = signals(
            vec![
                finding("gone", Severity::Watch),
                finding("kept", Severity::Watch),
            ],
            &[("installed packages", &["a 1", "b 1"])],
        );
        let new = signals(
            vec![
                finding("kept", Severity::Warning),
                finding("fresh", Severity::Urgent),
            ],
            &[
                ("installed packages", &["a 1", "b 2"]),
                ("listening sockets", &["tcp 0.0.0.0:22"]),
            ],
        );
        let text = render(Some(("full_system_audit-old", &old)), &new);
        assert!(text.contains("NEW       [URGENT] title fresh"));
        assert!(text.contains("CHANGED   [WATCH → WARNING] title kept"));
        assert!(text.contains("RESOLVED  [WATCH] title gone"));
        assert!(text.contains("INSTALLED PACKAGES\n  added (1):\n  b 2\n  removed (1):\n  b 1"));
        assert!(text.contains("LISTENING SOCKETS\n  added (1):\n  tcp 0.0.0.0:22"));
        assert_eq!(text.matches("```").count() % 2, 0);
    }

    #[test]
    fn identical_audits_say_so_and_first_audit_explains() {
        let same = signals(vec![finding("k", Severity::Watch)], &[("x", &["1"])]);
        let text = render(Some(("prev", &same)), &same);
        assert!(text.contains("no finding appeared, changed or was resolved"));
        assert!(text.contains("no inventory changes"));
        assert!(render(None, &same).contains("first audit with deep signals"));
    }

    #[test]
    fn long_lists_are_summarized() {
        let many = (0..100).map(|n| format!("p{n:03} 1")).collect::<Vec<_>>();
        let refs = many.iter().map(String::as_str).collect::<Vec<_>>();
        let new = signals(vec![], &[("installed packages", &refs)]);
        let text = render(Some(("prev", &signals(vec![], &[]))), &new);
        assert!(text.contains("… and 60 more"));
    }

    #[test]
    fn previous_picks_the_newest_older_audit_with_signals() {
        let root = std::env::temp_dir().join(format!("omniscient-changes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |audit: &str, with_signals: bool| {
            let dir = root.join(audit).join("signals-x");
            std::fs::create_dir_all(&dir).expect("dir");
            if with_signals {
                let s = signals(vec![finding(audit, Severity::Watch)], &[]);
                std::fs::write(
                    dir.join(SIGNALS_JSON),
                    serde_json::to_string(&s).expect("json"),
                )
                .expect("write");
            }
        };
        write("full_system_audit-2026-01-01_00-00-00", true);
        write("full_system_audit-2026-01-02_00-00-00", true);
        write("full_system_audit-2026-01-03_00-00-00", false);
        write("full_system_audit-2026-01-04_00-00-00", true);
        let current = root.join("full_system_audit-2026-01-04_00-00-00");
        let (label, found) = previous(&root, &current).expect("previous");
        assert_eq!(
            label, "full_system_audit-2026-01-02_00-00-00",
            "skips audits without signals and newer ones"
        );
        assert_eq!(
            found.findings[0].key,
            "full_system_audit-2026-01-02_00-00-00"
        );
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
