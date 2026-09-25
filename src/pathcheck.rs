use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Whether `cmd` is found and executable somewhere on $PATH.
/// A minimal stand-in for the `which` crate: this project only ever
/// needs a yes/no answer, not the resolved path, so a small local
/// search avoids pulling in an extra dependency for one boolean check.
pub fn exists(cmd: &str) -> bool {
    resolve(cmd).is_some()
}

/// Resolve only executable files from a fixed, system-owned search path.
/// This keeps privileged probes from inheriting a user-writable PATH entry.
pub fn resolve(cmd: &str) -> Option<PathBuf> {
    if cmd.contains('/') {
        let path = PathBuf::from(cmd);
        return is_executable(&path).then_some(path);
    }

    let path_var = if crate::elevation::is_privileged() {
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string()
    } else {
        std::env::var("PATH").ok()?
    };
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(cmd);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && (meta.permissions().mode() & 0o111 != 0),
        Err(_) => false,
    }
}
