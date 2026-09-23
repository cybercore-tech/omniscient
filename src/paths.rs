use std::path::{Path, PathBuf};

/// Resolve the directory where audit reports are stored.
///
/// `OMNISCIENT_REPORT_DIR` is an explicit per-user override. Without it,
/// Omniscient follows the XDG state-directory convention and uses
/// `$XDG_STATE_HOME/omniscient` or `~/.local/state/omniscient`.
pub fn report_root() -> PathBuf {
    if let Some(path) = non_empty_env("OMNISCIENT_REPORT_DIR") {
        return PathBuf::from(path);
    }

    state_home().join("omniscient")
}

fn state_home() -> PathBuf {
    non_empty_env("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/state"))
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("/").to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_home_is_xdg_compatible() {
        assert_eq!(
            state_home_for(Path::new("/home/example"), None),
            PathBuf::from("/home/example/.local/state")
        );
        assert_eq!(
            state_home_for(
                Path::new("/home/example"),
                Some(Path::new("/var/lib/example"))
            ),
            PathBuf::from("/var/lib/example")
        );
    }

    #[test]
    fn report_root_appends_omniscient_namespace() {
        assert_eq!(
            report_root_for(Path::new("/home/example"), None, None),
            PathBuf::from("/home/example/.local/state/omniscient")
        );
        assert_eq!(
            report_root_for(
                Path::new("/home/example"),
                Some(Path::new("/var/state")),
                None
            ),
            PathBuf::from("/var/state/omniscient")
        );
        assert_eq!(
            report_root_for(
                Path::new("/home/example"),
                Some(Path::new("/var/state")),
                Some(Path::new("/srv/example-reports"))
            ),
            PathBuf::from("/srv/example-reports")
        );
    }

    fn state_home_for(home: &Path, xdg_state_home: Option<&Path>) -> PathBuf {
        xdg_state_home
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join(".local/state"))
    }

    fn report_root_for(
        home: &Path,
        xdg_state_home: Option<&Path>,
        report_dir: Option<&Path>,
    ) -> PathBuf {
        report_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(|| state_home_for(home, xdg_state_home).join("omniscient"))
    }
}
