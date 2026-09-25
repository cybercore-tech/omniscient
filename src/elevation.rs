//! Privilege elevation backends.
//!
//! The repository stays distro-neutral: terminal `sudo` is the default.
//! A user may opt into a graphical Polkit flow by setting
//! `OMNISCIENT_AUTH=pkexec` in their own environment.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Sudo,
    Pkexec,
}

pub fn backend() -> Backend {
    match std::env::var("OMNISCIENT_AUTH")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "pkexec" => Backend::Pkexec,
        _ => Backend::Sudo,
    }
}

pub fn program() -> &'static str {
    match backend() {
        Backend::Sudo => "sudo",
        Backend::Pkexec => "pkexec",
    }
}

/// True while the whole audit is running inside the single authenticated
/// privileged child used by graphical/HUD runs.
pub fn is_privileged() -> bool {
    std::env::var("OMNISCIENT_PRIVILEGED")
        .map(|value| value == "1")
        .unwrap_or(false)
}

pub fn args<'a>(command: &'a str, args: &[&'a str]) -> Vec<&'a str> {
    let mut elevated = vec![command];
    elevated.extend_from_slice(args);
    elevated
}

pub fn uses_graphical() -> bool {
    backend() == Backend::Pkexec
}

/// Authenticate once and re-execute the HUD audit as root. Returning `true`
/// means the caller was the unprivileged parent and the child has completed.
pub fn reexec_graphical() -> Result<bool> {
    if !uses_graphical() || is_privileged() {
        return Ok(false);
    }

    let executable = std::env::current_exe().context("locating omniscient executable")?;
    let report_dir = crate::paths::report_root();
    let snapshot_path = crate::snapshot::path();
    let uid = command_output("id", &["-u"])?;
    let gid = command_output("id", &["-g"])?;

    let mut command = Command::new(program());
    command
        .arg("/usr/bin/env")
        .arg("OMNISCIENT_PRIVILEGED=1")
        .arg("OMNISCIENT_AUTH=pkexec")
        .arg(format!("OMNISCIENT_REPORT_DIR={}", report_dir.display()))
        .arg(format!(
            "OMNISCIENT_SNAPSHOT_PATH={}",
            snapshot_path.display()
        ))
        .arg(format!("OMNISCIENT_OWNER_UID={uid}"))
        .arg(format!("OMNISCIENT_OWNER_GID={gid}"))
        .arg(&executable);
    command.args(std::env::args().skip(1));

    let status = command
        .status()
        .context("starting one-time graphical authorization")?;
    if !status.success() {
        bail!("privileged audit exited with {status}");
    }

    Ok(true)
}

/// Return generated reports and runtime state to the invoking user after the
/// root child finishes. The paths are explicit and narrowly scoped.
pub fn restore_user_files() -> Result<()> {
    if !is_privileged() {
        return Ok(());
    }

    let uid = std::env::var("OMNISCIENT_OWNER_UID").context("missing audit owner uid")?;
    let gid = std::env::var("OMNISCIENT_OWNER_GID").context("missing audit owner gid")?;
    let owner = format!("{uid}:{gid}");
    let report_dir = crate::paths::report_root();
    chown_tree(&report_dir, &owner)?;
    let snapshot_path = crate::snapshot::path();
    if let Some(parent) = snapshot_path.parent() {
        chown_path(parent, &owner)?;
    }
    chown_path(&snapshot_path, &owner)?;
    Ok(())
}

fn command_output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("running {program}"))?;
    if !output.status.success() {
        bail!("{program} exited with {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn chown_tree(path: &Path, owner: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    Command::new("chown")
        .args(["-R", owner, &path.to_string_lossy()])
        .status()
        .with_context(|| format!("restoring ownership of {}", path.display()))?
        .success()
        .then_some(())
        .with_context(|| format!("chown failed for {}", path.display()))
}

fn chown_path(path: &Path, owner: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    Command::new("chown")
        .arg(owner)
        .arg(path)
        .status()
        .with_context(|| format!("restoring ownership of {}", path.display()))?
        .success()
        .then_some(())
        .with_context(|| format!("chown failed for {}", path.display()))
}

pub fn label() -> &'static str {
    match backend() {
        Backend::Sudo => "SUDO",
        Backend::Pkexec => "POLKIT",
    }
}
