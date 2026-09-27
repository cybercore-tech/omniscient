//! Process-level tests of the commands the HUD reads.

use std::process::{Command, Stdio};

/// The HUD closes a reader when the panel closes or the tab changes. The
/// binary must exit cleanly then; `println!` used to panic on the broken
/// pipe, which `panic = "abort"` turned into a crash with a core dump.
#[test]
fn a_closed_reader_is_not_a_crash() {
    for args in [&["--sensors"][..], &["--journal", "--limit", "5"]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_omniscient"))
            .args(args)
            .env("OMNISCIENT_SYSFS_ROOT", std::env::temp_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn omniscient");
        // Close the read end before the command writes its JSON.
        drop(child.stdout.take());
        let output = child.wait_with_output().expect("wait");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{args:?}: {:?} / {stderr}",
            output.status
        );
        assert!(!stderr.contains("Broken pipe"), "{args:?}: {stderr}");
    }
}

#[test]
fn invalid_journal_filters_are_refused_before_journalctl_runs() {
    let output = Command::new(env!("CARGO_BIN_EXE_omniscient"))
        .args(["--journal", "--unit", "$(touch /tmp/pwned)"])
        .output()
        .expect("run omniscient");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid unit"));
}
