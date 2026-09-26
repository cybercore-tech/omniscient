//! Privilege elevation backends.
//!
//! The repository stays distro-neutral: terminal `sudo` is the default.
//! A user may opt into a graphical Polkit flow by setting
//! `OMNISCIENT_AUTH=pkexec` in their own environment.

use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

const ENV: &str = "/usr/bin/env";
const ID: &str = "/usr/bin/id";
const CHOWN: &str = "/usr/bin/chown";
const PKEXEC: &str = "/usr/bin/pkexec";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Sudo,
    Pkexec,
}

#[must_use]
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

#[must_use]
pub fn program() -> &'static str {
    match backend() {
        Backend::Sudo => "/usr/bin/sudo",
        Backend::Pkexec => PKEXEC,
    }
}

/// True while the whole audit is running inside the single authenticated
/// privileged child used by graphical/HUD runs.
#[must_use]
pub fn is_privileged() -> bool {
    // Asked hundreds of times per audit; the answer cannot change within a
    // process, so run `id -u` once.
    static PRIVILEGED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PRIVILEGED.get_or_init(|| {
        std::env::var("OMNISCIENT_PRIVILEGED").is_ok_and(|value| value == "1")
            && command_output(ID, &["-u"]).is_ok_and(|uid| uid == "0")
    })
}

/// Arguments for running one command through the elevation program without
/// ever prompting: `sudo -n` succeeds only with a credential the dashboard
/// already cached and otherwise fails at once. `pkexec` always prompts, so
/// per-command pkexec is refused (`None`); graphical runs elevate the whole
/// audit once instead.
#[must_use]
pub fn non_interactive_args<'a>(command: &'a str, args: &[&'a str]) -> Option<Vec<&'a str>> {
    match backend() {
        Backend::Sudo => {
            let mut elevated = vec!["-n", command];
            elevated.extend_from_slice(args);
            Some(elevated)
        }
        Backend::Pkexec => None,
    }
}

#[must_use]
pub fn uses_graphical() -> bool {
    backend() == Backend::Pkexec
}

/// Authenticate once and re-execute the HUD audit as root. Returning `true`
/// means the caller was the unprivileged parent and the child has completed.
///
/// # Errors
///
/// Returns an error when authentication is refused or the elevated child
/// cannot be started or fails.
pub fn reexec_graphical() -> Result<bool> {
    if !uses_graphical() || is_privileged() {
        return Ok(false);
    }

    let executable = std::env::current_exe().context("locating omniscient executable")?;
    let report_dir = crate::paths::report_root();
    let snapshot_path = crate::snapshot::path();
    let uid = command_output(ID, &["-u"])?;
    let gid = command_output(ID, &["-g"])?;
    validate_owner_id(&uid, "uid")?;
    validate_owner_id(&gid, "gid")?;
    validate_executable(&executable, &uid)?;
    validate_user_path(&report_dir, &uid, "report directory")?;
    if let Some(parent) = snapshot_path.parent() {
        validate_user_path(parent, &uid, "snapshot directory")?;
    } else {
        bail!("snapshot path has no parent directory");
    }

    let mut command = Command::new(PKEXEC);
    command
        .arg(ENV)
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
///
/// # Errors
///
/// Returns an error when the owner is missing or invalid, a path is not
/// safely user-owned, or ownership cannot be restored.
pub fn restore_user_files() -> Result<()> {
    if !is_privileged() {
        return Ok(());
    }

    let uid = std::env::var("OMNISCIENT_OWNER_UID").context("missing audit owner uid")?;
    let gid = std::env::var("OMNISCIENT_OWNER_GID").context("missing audit owner gid")?;
    validate_owner_id(&uid, "uid")?;
    validate_owner_id(&gid, "gid")?;
    let owner = format!("{uid}:{gid}");
    let report_dir = crate::paths::report_root();
    validate_user_path(&report_dir, &uid, "report directory")?;
    chown_tree(&report_dir, &owner)?;
    let snapshot_path = crate::snapshot::path();
    if let Some(parent) = snapshot_path.parent() {
        validate_user_path(parent, &uid, "snapshot directory")?;
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

fn validate_owner_id(value: &str, name: &str) -> Result<()> {
    let parsed = value
        .parse::<u32>()
        .with_context(|| format!("invalid audit owner {name}"))?;
    if name == "uid" && parsed == 0 {
        bail!("refusing to restore files to root ownership");
    }
    Ok(())
}

/// Verify that a privileged child may only write below an existing path owned
/// by the invoking user.  Root-owned system ancestors are allowed, but once
/// the user-owned portion begins, every existing component must remain user
/// owned and no symlink or parent traversal is accepted.
fn validate_user_path(path: &Path, uid: &str, label: &str) -> Result<()> {
    if !path.is_absolute() {
        bail!("{label} must be absolute");
    }

    let expected_uid = uid
        .parse::<u32>()
        .with_context(|| format!("invalid owner uid for {label}"))?;
    let mut current = PathBuf::from("/");
    let mut user_owned_boundary = false;

    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(part) => {
                current.push(part);
                match fs::symlink_metadata(&current) {
                    Ok(metadata) => {
                        if metadata.file_type().is_symlink() {
                            bail!("{label} contains a symlink: {}", current.display());
                        }
                        if user_owned_boundary && metadata.uid() != expected_uid {
                            bail!(
                                "{label} leaves the invoking user's ownership at {}",
                                current.display()
                            );
                        }
                        if metadata.uid() == expected_uid {
                            user_owned_boundary = true;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("checking ownership of {}", current.display())
                        })
                    }
                }
            }
            Component::CurDir | Component::ParentDir => {
                bail!("{label} contains an unsafe path component");
            }
            Component::Prefix(_) => bail!("{label} has an unsupported path prefix"),
        }
    }

    if !user_owned_boundary {
        bail!("{label} has no existing component owned by the invoking user");
    }
    Ok(())
}

fn validate_executable(path: &Path, uid: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("checking audit executable {}", path.display()))?;
    let expected_uid = uid
        .parse::<u32>()
        .with_context(|| "invalid audit executable owner uid")?;
    if metadata.file_type().is_symlink() {
        bail!("audit executable may not be a symlink: {}", path.display());
    }
    if !metadata.is_file() || metadata.uid() != expected_uid {
        bail!("audit executable must be a regular file owned by the invoking user");
    }
    if metadata.mode() & 0o022 != 0 {
        bail!("audit executable is writable by group or other users");
    }
    Ok(())
}

fn chown_tree(path: &Path, owner: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    Command::new(CHOWN)
        .args(["--no-dereference", "-R", owner, &path.to_string_lossy()])
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
    Command::new(CHOWN)
        .arg("--no-dereference")
        .arg(owner)
        .arg(path)
        .status()
        .with_context(|| format!("restoring ownership of {}", path.display()))?
        .success()
        .then_some(())
        .with_context(|| format!("chown failed for {}", path.display()))
}

#[must_use]
pub fn label() -> &'static str {
    match backend() {
        Backend::Sudo => "SUDO",
        Backend::Pkexec => "POLKIT",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn elevation_from_a_module_never_prompts() {
        if std::env::var_os("OMNISCIENT_AUTH").is_some() {
            return;
        }
        let args = super::non_interactive_args("/usr/bin/btrfs", &["device", "stats", "/"])
            .expect("sudo backend");
        assert_eq!(args, vec!["-n", "/usr/bin/btrfs", "device", "stats", "/"]);
    }
}
