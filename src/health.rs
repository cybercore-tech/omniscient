use crate::modules::AuditModule;
use std::process::Command;

/// A health score based on real signal, not just whether tools are
/// installed. The fish version only ever checked tool presence, so a
/// fully-populated system with a failing drive or crashed service
/// scored identically to a genuinely healthy one.
#[derive(Clone)]
pub struct HealthReport {
    pub score: i32,
    pub notes: Vec<String>,
}

pub fn compute(modules: &[Box<dyn AuditModule>]) -> HealthReport {
    compute_iter(modules.iter().map(|module| module.as_ref()))
}

pub fn compute_selected(modules: &[Box<dyn AuditModule>], selected: &[usize]) -> HealthReport {
    compute_iter(
        selected
            .iter()
            .filter_map(|index| modules.get(*index).map(|module| module.as_ref())),
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
    if let Ok(out) = Command::new("systemctl")
        .args(["list-units", "--failed", "--no-legend"])
        .output()
    {
        let failed = String::from_utf8_lossy(&out.stdout);
        let count = failed.lines().filter(|l| !l.trim().is_empty()).count();
        if count > 0 {
            score -= (count as i32) * 10;
            notes.push(format!("{count} failed systemd unit(s)"));
        }
    }

    // Real signal #2: SMART health status on any disk that reports it.
    if smart_requested && crate::pathcheck::exists("smartctl") {
        if let Ok(out) = Command::new("lsblk")
            .args(["-dn", "-o", "NAME,TYPE"])
            .output()
        {
            let disks = String::from_utf8_lossy(&out.stdout);
            for line in disks.lines() {
                let mut fields = line.split_whitespace();
                let Some(disk) = fields.next() else {
                    continue;
                };
                if fields.next() != Some("disk") {
                    continue;
                }
                let dev = format!("/dev/{disk}");
                if let Ok(smart) = Command::new("pkexec")
                    .args(["smartctl", "-H", &dev])
                    .output()
                {
                    let text = String::from_utf8_lossy(&smart.stdout);
                    if text.contains("FAILED") {
                        score -= 25;
                        notes.push(format!("SMART health check FAILED on {dev}"));
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
