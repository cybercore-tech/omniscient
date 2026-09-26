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
            &["list-units", "--failed", "--no-legend"],
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
                    let elevated_args = crate::elevation::args(&smartctl, &["-H", &dev]);
                    let smart = if crate::elevation::is_privileged() {
                        crate::capture::run(Path::new(&smartctl), &["-H", &dev], probe_limits())
                    } else {
                        crate::capture::run(
                            Path::new(crate::elevation::program()),
                            &elevated_args,
                            probe_limits(),
                        )
                    };
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

/// Health probes answer quickly and print little; bound them anyway.
fn probe_limits() -> crate::capture::Limits {
    crate::capture::Limits {
        timeout: std::time::Duration::from_secs(60),
        retain_bytes: 1024 * 1024,
    }
}
