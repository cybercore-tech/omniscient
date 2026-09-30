//! Test-only scratch directories.
//!
//! `temp_dir().join("omniscient-<pid>")` is predictable: another local user
//! can pre-create it (or plant a symlink there) before the test runs. Each
//! directory here gets an unguessable name, is created with `create_dir`
//! (which fails rather than reusing anything that already exists), and is
//! restricted to the owner.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates a fresh, exclusively owned (0700) directory under the system
/// temp dir. The caller removes it when done.
pub fn dir(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let base = std::env::temp_dir();
    for _ in 0..16 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let name = format!(
            "omniscient-{tag}-{}-{}-{nanos:x}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        );
        let path = base.join(name);
        match std::fs::create_dir(&path) {
            Ok(()) => {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                    .expect("restrict scratch dir");
                return path;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => panic!("create scratch dir: {error}"),
        }
    }
    panic!("could not create a unique scratch dir");
}

#[cfg(test)]
mod tests {
    #[test]
    fn scratch_dirs_are_unique_private_and_fresh() {
        use std::os::unix::fs::PermissionsExt;
        let a = super::dir("t");
        let b = super::dir("t");
        assert_ne!(a, b);
        let mode = std::fs::metadata(&a).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        assert!(std::fs::read_dir(&a).expect("read").next().is_none());
        std::fs::remove_dir(&a).expect("cleanup");
        std::fs::remove_dir(&b).expect("cleanup");
    }
}
