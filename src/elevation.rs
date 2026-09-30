//! Privilege elevation backends.
//!
//! The repository stays distro-neutral: terminal `sudo` is the default.
//! A user may opt into a graphical Polkit flow by setting
//! `OMNISCIENT_AUTH=pkexec` in their own environment.
//!
//! Omniscient itself never runs as root. Elevated work goes through one of
//! two narrow routes, both behind the allowlist in [`crate::helper`]:
//! `sudo -n <system binary> <args>` in the terminal dashboard (a credential
//! the dashboard cached with one `sudo -v`), or the root-owned privileged
//! helper that one `pkexec` authorization starts for the HUD.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Sudo,
    Pkexec,
}

pub const PKEXEC: &str = "/usr/bin/pkexec";
pub const SUDO: &str = "/usr/bin/sudo";

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
        Backend::Sudo => SUDO,
        Backend::Pkexec => PKEXEC,
    }
}

/// The real user id from `/proc/self/status`.
fn real_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|uid| uid.parse().ok())
}

/// Whether this process runs as root (only the privileged helper should,
/// or a user who started the dashboard with sudo themselves).
#[must_use]
pub fn is_root() -> bool {
    static ROOT: OnceLock<bool> = OnceLock::new();
    *ROOT.get_or_init(|| {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| real_uid(&status))
            == Some(0)
    })
}

/// Arguments for running one command through the elevation program without
/// ever prompting: `sudo -n` succeeds only with a credential the dashboard
/// already cached and otherwise fails at once. `pkexec` always prompts, so
/// per-command pkexec is refused (`None`); graphical runs use the helper.
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

/// Display name of the backend for the dashboard log.
#[must_use]
pub fn label() -> &'static str {
    match backend() {
        Backend::Sudo => "SUDO",
        Backend::Pkexec => "POLKIT",
    }
}

#[must_use]
pub fn uses_graphical() -> bool {
    backend() == Backend::Pkexec
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

    #[test]
    fn the_real_uid_is_read_from_proc_status() {
        assert_eq!(
            super::real_uid("Name:\tx\nUid:\t1000\t0\t0\t0\n"),
            Some(1000),
            "the real uid, not the effective one"
        );
        assert_eq!(super::real_uid("Uid:\t0\t0\t0\t0\n"), Some(0));
        assert_eq!(super::real_uid("nothing"), None);
    }
}
