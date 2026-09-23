//! Privilege elevation backends.
//!
//! The repository stays distro-neutral: terminal `sudo` is the default.
//! A user may opt into a graphical Polkit flow by setting
//! `OMNISCIENT_AUTH=pkexec` in their own environment.

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

pub fn args<'a>(command: &'a str, args: &[&'a str]) -> Vec<&'a str> {
    let mut elevated = vec![command];
    elevated.extend_from_slice(args);
    elevated
}

pub fn uses_graphical() -> bool {
    backend() == Backend::Pkexec
}

pub fn label() -> &'static str {
    match backend() {
        Backend::Sudo => "SUDO",
        Backend::Pkexec => "POLKIT",
    }
}
