use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Whether `cmd` is found and executable somewhere on $PATH.
/// A minimal stand-in for the `which` crate: this project only ever
/// needs a yes/no answer, not the resolved path, so a small local
/// search avoids pulling in an extra dependency for one boolean check.
pub fn exists(cmd: &str) -> bool {
    find(cmd).is_some()
}

fn find(cmd: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
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
