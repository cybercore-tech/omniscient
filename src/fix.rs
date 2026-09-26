use crate::elevation;
use crate::paths;
use anyhow::{bail, Context, Result};
use chrono::Local;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Applies one allowlisted repair (`install-tool:<tool>`) and writes a fix
/// report.
///
/// # Errors
///
/// Returns an error for a request outside the allowlist, when elevation or
/// the package install fails, or when the report cannot be written.
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

/// Kernel modules the fix center may enable: read-only sensor drivers only.
pub const SENSOR_MODULES: [&str; 1] = ["drivetemp"];
/// The one file the sensor fix writes.
pub const MODULES_LOAD_FILE: &str = "/etc/modules-load.d/omniscient-drivetemp.conf";

/// A finished repair, ready for its report.
struct Outcome {
    slug: String,
    title: String,
    commands: Vec<String>,
    transcript: String,
    success: bool,
    references: Vec<(&'static str, String)>,
}

fn run_fix(fix_id: &str) -> Result<()> {
    let outcome = if let Some(tool) = fix_id.strip_prefix("install-tool:") {
        install_tool(tool)?
    } else if let Some(module) = fix_id.strip_prefix("enable-sensor:") {
        enable_sensor(module)?
    } else {
        bail!("unsupported fix request: {fix_id}");
    };
    write_report(&outcome)
}

fn fix_limits() -> crate::capture::Limits {
    crate::capture::Limits {
        timeout: std::time::Duration::from_mins(15),
        ..crate::capture::Limits::default()
    }
}

/// Runs a repair command as root: directly in the elevated child, otherwise
/// through the elevation program. Only reached after the user confirmed
/// the repair and authorized it.
fn run_root(program: &str, args: &[&str]) -> Result<crate::capture::Output> {
    if elevation::is_privileged() {
        crate::capture::run(Path::new(program), args, fix_limits())
            .with_context(|| format!("running {program}"))
    } else {
        let mut elevated = vec![program];
        elevated.extend_from_slice(args);
        crate::capture::run(Path::new(elevation::program()), &elevated, fix_limits())
            .with_context(|| format!("running authenticated {program}"))
    }
}

fn transcript(output: &crate::capture::Output) -> String {
    crate::capture::bound_text(
        &format!("{}{}", output.stdout.text(), output.stderr.text()),
        output.stdout.dropped() + output.stderr.dropped(),
        crate::capture::MAX_SECTION_BYTES,
        crate::capture::MAX_SECTION_LINES,
    )
}

fn install_tool(tool: &str) -> Result<Outcome> {
    let package = crate::suggestions::package_for_tool(tool)
        .with_context(|| format!("no allowlisted package mapping for tool: {tool}"))?;
    let output = run_root(
        "/usr/bin/pacman",
        &["-S", "--needed", "--noconfirm", package],
    )?;
    Ok(Outcome {
        slug: format!("install-{tool}"),
        title: format!("Install `{tool}` via package `{package}`"),
        commands: vec![format!("pacman -S --needed --noconfirm {package}")],
        transcript: transcript(&output),
        success: output.success(),
        references: vec![
            (
                "Manual page",
                format!("https://man.archlinux.org/man/{tool}.1.en"),
            ),
            (
                "Pacman documentation",
                "https://wiki.archlinux.org/title/Pacman".to_owned(),
            ),
        ],
    })
}

/// Loads an allowlisted read-only sensor driver now and at every boot.
fn enable_sensor(module: &str) -> Result<Outcome> {
    anyhow::ensure!(
        SENSOR_MODULES.contains(&module),
        "sensor module not allowlisted: {module}"
    );
    let loaded = run_root("/usr/bin/modprobe", &[module])?;
    let mut log = transcript(&loaded);
    let mut success = loaded.success();
    if success {
        match persist_module(module) {
            Ok(()) => {
                let _ = writeln!(
                    log,
                    "wrote {MODULES_LOAD_FILE}: the module loads at every boot"
                );
            }
            Err(error) => {
                success = false;
                let _ = writeln!(log, "could not write {MODULES_LOAD_FILE}: {error:#}");
            }
        }
    }
    Ok(Outcome {
        slug: format!("enable-{module}"),
        title: format!("Enable the `{module}` sensor driver"),
        commands: vec![
            format!("modprobe {module}"),
            format!("echo {module} > {MODULES_LOAD_FILE}"),
        ],
        transcript: log,
        success,
        references: vec![
            (
                "Kernel documentation",
                "https://docs.kernel.org/hwmon/drivetemp.html".to_owned(),
            ),
            (
                "modules-load.d",
                "https://man.archlinux.org/man/modules-load.d.5.en".to_owned(),
            ),
        ],
    })
}

/// Writes the modules-load.d entry atomically. Refuses to follow a symlink
/// at the destination. Needs root (the elevated fix child).
fn persist_module(module: &str) -> Result<()> {
    anyhow::ensure!(
        elevation::is_privileged(),
        "persisting the module needs the elevated fix"
    );
    let destination = Path::new(MODULES_LOAD_FILE);
    if let Ok(meta) = fs::symlink_metadata(destination) {
        anyhow::ensure!(
            !meta.file_type().is_symlink(),
            "{MODULES_LOAD_FILE} is a symlink"
        );
    }
    let parent = destination
        .parent()
        .context("modules-load.d path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let temporary = parent.join(".omniscient-drivetemp.conf.tmp");
    fs::write(
        &temporary,
        format!("# Written by Omniscient (fix center): live SATA temperatures.\n{module}\n"),
    )
    .with_context(|| format!("writing {}", temporary.display()))?;
    fs::rename(&temporary, destination)
        .with_context(|| format!("replacing {MODULES_LOAD_FILE}"))?;
    Ok(())
}

fn write_report(outcome: &Outcome) -> Result<()> {
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let fixes_dir = paths::report_root().join("fixes");
    fs::create_dir_all(&fixes_dir)
        .with_context(|| format!("creating fix report directory {}", fixes_dir.display()))?;
    let report_path = fixes_dir.join(format!("{}-{timestamp}.md", outcome.slug));
    let status = if outcome.success {
        "COMPLETE"
    } else {
        "FAILED"
    };
    let mut report = File::create(&report_path)
        .with_context(|| format!("creating {}", report_path.display()))?;
    writeln!(report, "# 🛠️ Omniscient Fix Report — {timestamp}\n")?;
    writeln!(report, "**Status:** {status}")?;
    writeln!(report, "**Fix:** {}", outcome.title)?;
    for command in &outcome.commands {
        writeln!(report, "**Command:** `{command}`")?;
    }
    writeln!(
        report,
        "\n## Output\n\n```text\n{}\n```\n",
        outcome.transcript.trim_end()
    )?;
    for (label, url) in &outcome.references {
        writeln!(report, "- [{label}]({url})")?;
    }
    println!("FIX REPORT / {}", report_path.display());
    if !outcome.success {
        bail!("fix failed with {status}; see {}", report_path.display());
    }
    Ok(())
}

fn validate_fix(fix_id: &str) -> Result<()> {
    if let Some(module) = fix_id.strip_prefix("enable-sensor:") {
        anyhow::ensure!(
            SENSOR_MODULES.contains(&module),
            "sensor module not allowlisted: {module}"
        );
        return Ok(());
    }
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
        assert!(validate_fix("enable-sensor:drivetemp").is_ok());
        assert!(
            validate_fix("enable-sensor:nvidia").is_err(),
            "only allowlisted sensor drivers"
        );
        assert!(validate_fix("enable-sensor:../drivetemp").is_err());
        assert!(validate_fix("enable-sensor:").is_err());
    }
}
