use crate::elevation;
use crate::paths;
use anyhow::{bail, Context, Result};
use chrono::Local;
use std::fs::{self, File};
use std::io::Write;
use std::process::Command;

pub fn run(fix_id: &str) -> Result<()> {
    // Validate the complete request before asking for credentials.  An
    // unsupported UI or IPC value must never trigger a needless root prompt.
    validate_fix(fix_id)?;
    if elevation::reexec_graphical()? {
        return Ok(());
    }

    let result = run_fix(fix_id);
    if elevation::is_privileged() {
        elevation::restore_user_files()?;
    }
    result
}

fn run_fix(fix_id: &str) -> Result<()> {
    let Some(tool) = fix_id.strip_prefix("install-tool:") else {
        bail!("unsupported fix request: {fix_id}");
    };
    let package = crate::suggestions::package_for_tool(tool)
        .with_context(|| format!("no allowlisted package mapping for tool: {tool}"))?;

    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let fixes_dir = paths::report_root().join("fixes");
    fs::create_dir_all(&fixes_dir)
        .with_context(|| format!("creating fix report directory {}", fixes_dir.display()))?;
    let report_path = fixes_dir.join(format!("install-{tool}-{timestamp}.md"));
    let command_line = format!("pacman -S --needed --noconfirm {package}");

    let output = if elevation::is_privileged() {
        Command::new("/usr/bin/pacman")
            .args(["-S", "--needed", "--noconfirm", package])
            .output()
            .context("running pacman")?
    } else {
        Command::new(elevation::program())
            .args(["/usr/bin/pacman", "-S", "--needed", "--noconfirm", package])
            .output()
            .context("running authenticated pacman")?
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let status = if output.status.success() {
        "COMPLETE"
    } else {
        "FAILED"
    };
    let mut report = File::create(&report_path)
        .with_context(|| format!("creating {}", report_path.display()))?;
    writeln!(report, "# 🛠️ Omniscient Fix Report — {timestamp}\n")?;
    writeln!(report, "**Status:** {status}")?;
    writeln!(report, "**Fix:** Install `{tool}` via package `{package}`")?;
    writeln!(report, "**Command:** `{command_line}`\n")?;
    writeln!(report, "## Output\n\n```text\n{stdout}{stderr}\n```\n")?;
    writeln!(
        report,
        "- [Manual page](https://man.archlinux.org/man/{tool}.1.en)"
    )?;
    writeln!(
        report,
        "- [Pacman documentation](https://wiki.archlinux.org/title/Pacman)"
    )?;

    println!("FIX REPORT / {}", report_path.display());
    if !output.status.success() {
        bail!("fix failed with {status}; see {}", report_path.display());
    }
    Ok(())
}

fn validate_fix(fix_id: &str) -> Result<()> {
    let Some(tool) = fix_id.strip_prefix("install-tool:") else {
        bail!("unsupported fix request: {fix_id}");
    };
    if tool.is_empty()
        || tool.contains('/')
        || !tool.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
    {
        bail!("invalid tool name in fix request: {tool}");
    }
    crate::suggestions::package_for_tool(tool)
        .with_context(|| format!("no allowlisted package mapping for tool: {tool}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_fix;

    #[test]
    fn only_allowlisted_install_ids_are_valid() {
        assert!(validate_fix("install-tool:lshw").is_ok());
        assert!(validate_fix("install-tool:smartctl").is_ok());
        assert!(validate_fix("install-tool:../../pacman").is_err());
        assert!(validate_fix("run-command:rm -rf /").is_err());
        assert!(validate_fix("install-tool:unknown-tool").is_err());
    }
}
