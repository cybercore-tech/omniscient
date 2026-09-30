use crate::paths;
use anyhow::{bail, Context, Result};
use chrono::Local;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Applies one allowlisted repair and writes its report.
///
/// This runs as the invoking user. The repair itself is done by the
/// root-owned privileged helper ([`crate::helper`]), which re-validates the
/// id and runs only [`apply_as_root`]; the report is written here, as the
/// user, never by root.
///
/// # Errors
///
/// Returns an error for a request outside the allowlist, when the helper is
/// unavailable or the repair fails, or when the report cannot be written.
pub fn run(fix_id: &str) -> Result<()> {
    // Validate the complete request before asking for credentials. An
    // unsupported UI or IPC value must never trigger a needless root prompt.
    validate_fix(fix_id)?;
    let described = describe(fix_id)?;
    crate::helper::start()?;
    let reply = crate::helper::request(&crate::helper::Request::Fix {
        id: fix_id.to_owned(),
    });
    crate::helper::stop();
    let reply = reply?;
    let transcript = if reply.error.is_empty() {
        reply.stdout
    } else {
        format!("{}{}", reply.stdout, reply.error)
    };
    write_report(&described, &transcript, reply.ok && reply.error.is_empty())
}

/// Kernel modules the fix center may enable: read-only sensor drivers only.
pub const SENSOR_MODULES: [&str; 1] = ["drivetemp"];
/// The one file the sensor fix writes.
pub const MODULES_LOAD_FILE: &str = "/etc/modules-load.d/omniscient-drivetemp.conf";

/// What a repair is, for its report.
struct Described {
    slug: String,
    title: String,
    commands: Vec<String>,
    references: Vec<(&'static str, String)>,
}

/// The result of a repair done as root.
pub struct Applied {
    pub success: bool,
    pub transcript: String,
}

fn describe(fix_id: &str) -> Result<Described> {
    if let Some(tool) = fix_id.strip_prefix("install-tool:") {
        let package = crate::suggestions::package_for_tool(tool)
            .with_context(|| format!("no allowlisted package mapping for tool: {tool}"))?;
        return Ok(Described {
            slug: format!("install-{tool}"),
            title: format!("Install `{tool}` via package `{package}`"),
            commands: vec![format!("pacman -S --needed --noconfirm {package}")],
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
        });
    }
    if let Some(module) = fix_id.strip_prefix("enable-sensor:") {
        anyhow::ensure!(
            SENSOR_MODULES.contains(&module),
            "sensor module not allowlisted: {module}"
        );
        return Ok(Described {
            slug: format!("enable-{module}"),
            title: format!("Enable the `{module}` sensor driver"),
            commands: vec![
                format!("modprobe {module}"),
                format!("echo {module} > {MODULES_LOAD_FILE}"),
            ],
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
        });
    }
    bail!("unsupported fix request: {fix_id}")
}

fn fix_limits() -> crate::capture::Limits {
    crate::capture::Limits {
        timeout: std::time::Duration::from_mins(15),
        ..crate::capture::Limits::default()
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

/// Performs an allowlisted repair. Only the privileged helper calls this,
/// as root; it re-validates the id itself and writes nothing outside the
/// repair's own fixed system target.
///
/// # Errors
///
/// Returns an error for an id outside the allowlist or when not root.
pub fn apply_as_root(fix_id: &str) -> Result<Applied> {
    validate_fix(fix_id)?;
    anyhow::ensure!(
        crate::elevation::is_root(),
        "repairs run in the privileged helper"
    );
    let run = |program: &str, args: &[&str]| {
        crate::capture::run(Path::new(program), args, fix_limits())
            .with_context(|| format!("running {program}"))
    };
    if let Some(tool) = fix_id.strip_prefix("install-tool:") {
        let package =
            crate::suggestions::package_for_tool(tool).context("no allowlisted package mapping")?;
        let output = run(
            "/usr/bin/pacman",
            &["-S", "--needed", "--noconfirm", package],
        )?;
        return Ok(Applied {
            success: output.success(),
            transcript: transcript(&output),
        });
    }
    let module = fix_id
        .strip_prefix("enable-sensor:")
        .context("unsupported fix request")?;
    let loaded = run("/usr/bin/modprobe", &[module])?;
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
    Ok(Applied {
        success,
        transcript: log,
    })
}

/// Writes the modules-load.d entry atomically as root. The temporary file is
/// created exclusively (no following a pre-existing file or symlink) and the
/// destination must not be a symlink.
fn persist_module(module: &str) -> Result<()> {
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
    let _ = fs::remove_file(&temporary);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("creating {}", temporary.display()))?;
    file.write_all(
        format!("# Written by Omniscient (fix center): live SATA temperatures.\n{module}\n")
            .as_bytes(),
    )
    .with_context(|| format!("writing {}", temporary.display()))?;
    file.sync_all().ok();
    fs::rename(&temporary, destination)
        .with_context(|| format!("replacing {MODULES_LOAD_FILE}"))?;
    Ok(())
}

fn write_report(described: &Described, transcript: &str, success: bool) -> Result<()> {
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let fixes_dir = paths::report_root().join("fixes");
    fs::create_dir_all(&fixes_dir)
        .with_context(|| format!("creating fix report directory {}", fixes_dir.display()))?;
    let report_path = fixes_dir.join(format!("{}-{timestamp}.md", described.slug));
    let status = if success { "COMPLETE" } else { "FAILED" };
    let mut report = File::create(&report_path)
        .with_context(|| format!("creating {}", report_path.display()))?;
    writeln!(report, "# 🛠️ Omniscient Fix Report — {timestamp}\n")?;
    writeln!(report, "**Status:** {status}")?;
    writeln!(report, "**Fix:** {}", described.title)?;
    for command in &described.commands {
        writeln!(report, "**Command:** `{command}`")?;
    }
    writeln!(
        report,
        "\n## Output\n\n```text\n{}\n```\n",
        crate::capture::bound_text(
            transcript,
            0,
            crate::capture::MAX_SECTION_BYTES,
            crate::capture::MAX_SECTION_LINES
        )
        .trim_end()
    )?;
    for (label, url) in &described.references {
        writeln!(report, "- [{label}]({url})")?;
    }
    crate::emit(&format!("FIX REPORT / {}", report_path.display()))?;
    if !success {
        bail!("fix failed with {status}; see {}", report_path.display());
    }
    Ok(())
}

/// Checks a repair id against the allowlist.
///
/// # Errors
///
/// Returns why the id is refused.
pub fn validate_fix(fix_id: &str) -> Result<()> {
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
