use crate::modules::AuditModule;
use std::path::Path;

/// A health score based on real signal, not just whether tools are
/// installed. The fish version only ever checked tool presence, so a
/// fully-populated system with a failing drive or crashed service
/// scored identically to a genuinely healthy one.
#[derive(Clone)]
pub struct HealthReport {
    pub score: i32,
    pub notes: Vec<String>,
}

#[must_use]
pub fn compute(modules: &[Box<dyn AuditModule>]) -> HealthReport {
    compute_iter(modules.iter().map(std::convert::AsRef::as_ref))
}

#[must_use]
pub fn compute_selected(modules: &[Box<dyn AuditModule>], selected: &[usize]) -> HealthReport {
    compute_iter(
        selected
            .iter()
            .filter_map(|index| modules.get(*index).map(std::convert::AsRef::as_ref)),
    )
}

fn compute_iter<'a, I>(modules: I) -> HealthReport
where
    I: IntoIterator<Item = &'a dyn AuditModule>,
{
    let mut score: i32 = 100;
    let mut notes = Vec::new();
    let mut smart_requested = false;

    // Missing tools: same baseline signal as before, but deduplicated
    // across modules that share a dependency instead of double-counting.
    let mut seen = std::collections::HashSet::new();
    for m in modules {
        smart_requested |= m.tools().contains(&"smartctl");
        for tool in m.tools() {
            if m.optional_tools().contains(tool) {
                continue;
            }
            if seen.insert(*tool) && !crate::pathcheck::exists(tool) {
                score -= 5;
                notes.push(format!("missing tool: {tool}"));
            }
        }
    }

    // Real signal #1: failed systemd units.
    if let Some(systemctl) = crate::pathcheck::resolve("systemctl") {
        if let Ok(out) = crate::capture::run(
            &systemctl,
            &["list-units", "--failed", "--no-legend", "--plain"],
            probe_limits(),
        ) {
            let failed = out.stdout.text();
            let units = failed
                .lines()
                .filter_map(|line| line.split_whitespace().next())
                .filter(|unit| !unit.is_empty())
                .collect::<Vec<_>>();
            if !units.is_empty() {
                let penalty = i32::try_from(units.len())
                    .unwrap_or(i32::MAX)
                    .saturating_mul(10);
                score = score.saturating_sub(penalty);
                notes.push(format!("failed systemd units: {}", units.join(", ")));
            }
        }
    }

    // Real signal #2: SMART health status on any disk that reports it.
    if smart_requested && crate::pathcheck::exists("smartctl") {
        if let Some(lsblk) = crate::pathcheck::resolve("lsblk") {
            if let Ok(out) =
                crate::capture::run(&lsblk, &["-dn", "-o", "NAME,TYPE"], probe_limits())
            {
                let disks = out.stdout.text();
                for line in disks.lines() {
                    let mut fields = line.split_whitespace();
                    let Some(disk) = fields.next() else {
                        continue;
                    };
                    if fields.next() != Some("disk") {
                        continue;
                    }
                    let dev = format!("/dev/{disk}");
                    let smartctl = crate::pathcheck::resolve("smartctl")
                        .expect("smartctl was checked before the health scan");
                    let smartctl = smartctl.to_string_lossy().into_owned();
                    // Prompt-free: the elevated child runs it directly,
                    // otherwise only a cached sudo credential is used.
                    let smart = crate::capture::run_elevated(
                        Path::new(&smartctl),
                        &["-H", &dev],
                        probe_limits(),
                    );
                    if let Ok(smart) = smart {
                        let text = smart.stdout.text();
                        if text.contains("FAILED") {
                            score -= 25;
                            notes.push(format!("SMART health check FAILED on {dev}"));
                        }
                    }
                }
            }
        }
    }

    HealthReport {
        score: score.max(0),
        notes,
    }
}

/// Most points deep signals can take off the score, so a single noisy
/// category cannot zero it.
pub const MAX_SIGNAL_PENALTY: i32 = 45;

/// Folds deep-signal findings into a health report: each finding costs its
/// severity's penalty (capped in total at [`MAX_SIGNAL_PENALTY`]) and every
/// finding above `info` is listed as a note.
pub fn apply_signals(health: &mut HealthReport, findings: &[crate::signals::Finding]) {
    let penalty = findings
        .iter()
        .map(|finding| finding.severity.penalty())
        .sum::<i32>()
        .min(MAX_SIGNAL_PENALTY);
    health.score = (health.score - penalty).max(0);
    for finding in findings
        .iter()
        .filter(|f| f.severity > crate::signals::Severity::Info)
    {
        health.notes.push(format!(
            "signal [{}]: {}",
            finding.severity.label(),
            finding.title
        ));
    }
}

/// Health probes answer quickly and print little; bound them anyway.
fn probe_limits() -> crate::capture::Limits {
    crate::capture::Limits {
        timeout: std::time::Duration::from_secs(60),
        retain_bytes: 1024 * 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_signals, HealthReport, MAX_SIGNAL_PENALTY};
    use crate::signals::{Finding, Severity};

    fn finding(severity: Severity) -> Finding {
        Finding {
            key: "k".into(),
            severity,
            title: "t".into(),
            detail: String::new(),
            command: String::new(),
            docs_url: String::new(),
        }
    }

    #[test]
    fn signals_lower_the_score_with_a_cap_and_add_notes() {
        let mut health = HealthReport {
            score: 90,
            notes: vec![],
        };
        apply_signals(
            &mut health,
            &[finding(Severity::Warning), finding(Severity::Info)],
        );
        assert_eq!(health.score, 83);
        assert_eq!(health.notes, vec!["signal [warning]: t"]);
        let mut worst = HealthReport {
            score: 100,
            notes: vec![],
        };
        apply_signals(&mut worst, &vec![finding(Severity::Urgent); 10]);
        assert_eq!(worst.score, 100 - MAX_SIGNAL_PENALTY);
    }
}
