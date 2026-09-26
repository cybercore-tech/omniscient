use crate::health::HealthReport;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Writes a top-level index tying every module's report together.
/// The fish version produced N separate markdown files with nothing
/// linking them — this gives you one file to open first.
///
/// # Errors
///
/// Returns an error when `SUMMARY.md` cannot be written.
pub fn write_summary(
    dir: &Path,
    timestamp: &str,
    health: &HealthReport,
    run_modules: &[(&str, &str)], // (display name, relative path to its report)
) -> Result<()> {
    let path = dir.join("SUMMARY.md");
    let mut f = File::create(&path).with_context(|| format!("creating {}", path.display()))?;

    writeln!(f, "# 🛰️ Omniscient Audit — {timestamp}\n")?;
    writeln!(
        f,
        "**Health score:** {} {}/100\n",
        health_emoji(health.score),
        health.score
    )?;

    if !health.notes.is_empty() {
        writeln!(f, "## Findings\n")?;
        for note in &health.notes {
            writeln!(f, "- {note}")?;
        }
        writeln!(f)?;
    }

    writeln!(f, "## Modules run\n")?;
    for (name, rel_path) in run_modules {
        writeln!(f, "- [{name}]({rel_path})")?;
    }

    Ok(())
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
