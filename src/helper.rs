//! The privileged helper: the only code in Omniscient that runs as root.
//!
//! The audit itself always runs as the invoking user and writes every file
//! as that user. When elevated checks are wanted, one `pkexec` authorization
//! starts `omniscient --privileged-helper` from a **root-owned** executable;
//! the helper reads one JSON request per line on stdin and runs only an
//! allowlisted set of read-only commands (plus the two allowlisted repairs),
//! returning bounded output. It writes no files in the user's directories
//! and exits when its stdin closes.
//!
//! Why this shape: a root process that writes into user-owned directories
//! can be raced with symlinks by code running as that user, and a root
//! process started from a user-writable binary can be replaced by it. The
//! helper avoids both: it never touches user paths, and it refuses to start
//! unless its own executable is owned and writable only by root.

// A panic here drops the elevated session and an unchecked index or overflow
// is a root-side crash on attacker-shaped input, so these restriction lints
// are hard errors for this module (tests are exempt; see `mod tests`).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use crate::capture::{self, Limits};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

/// Longest request line the helper reads.
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Most arguments in one request.
pub const MAX_ARGS: usize = 8;
/// Longest single argument.
pub const MAX_ARG_BYTES: usize = 256;
/// Output kept per stream in a reply.
const REPLY_RETAIN: usize = 1024 * 1024;
/// Where the root-owned executable is installed (`./install.sh --system`).
pub const SYSTEM_EXECUTABLE: &str = "/usr/local/bin/omniscient";
const PKEXEC: &str = "/usr/bin/pkexec";
const ENV: &str = "/usr/bin/env";

/// One request from the unprivileged audit.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    /// Run one allowlisted command.
    Run { program: String, args: Vec<String> },
    /// Apply one allowlisted repair.
    Fix { id: String },
}

/// The helper's reply to one request.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Reply {
    pub ok: bool,
    pub status: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub dropped: u64,
    pub error: String,
}

// ------------------------------------------------------------- allowlist --

/// Whether `device` is a whole-disk device name Omniscient inspects.
#[must_use]
pub fn valid_disk_name(device: &str) -> bool {
    let Some(name) = device.strip_prefix("/dev/") else {
        return false;
    };
    let lower = |s: &str, max: usize| {
        !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_lowercase())
    };
    let digits = |s: &str| !s.is_empty() && s.len() <= 3 && s.bytes().all(|b| b.is_ascii_digit());
    if let Some(rest) = name.strip_prefix("nvme") {
        return rest
            .split_once('n')
            .is_some_and(|(ctrl, ns)| digits(ctrl) && digits(ns));
    }
    if let Some(rest) = name.strip_prefix("mmcblk") {
        return digits(rest);
    }
    ["sd", "vd", "hd", "xvd"]
        .iter()
        .any(|prefix| name.strip_prefix(prefix).is_some_and(|rest| lower(rest, 3)))
}

/// Btrfs mountpoints from `/proc/self/mounts` text (octal escapes decoded).
#[must_use]
pub fn btrfs_mountpoints(mounts: &str) -> Vec<String> {
    mounts
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            (fields.get(2) == Some(&"btrfs"))
                .then(|| decode_mount_path(fields.get(1).copied().unwrap_or_default()))
        })
        .filter(|path| path.starts_with('/'))
        .collect()
}

/// Decodes the octal escapes (`\\040` for a space) in a mount path.
#[must_use]
pub fn decode_mount_path(raw: &str) -> String {
    String::from_utf8_lossy(&decode_mount_bytes(raw.as_bytes())).into_owned()
}

/// The byte-level core of [`decode_mount_path`], kept free of `String` so
/// Kani can check it exhaustively. Never returns more bytes than it is given.
#[must_use]
pub fn decode_mount_bytes(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut rest = raw;
    while let Some((&first, tail)) = rest.split_first() {
        if let Some((byte, after)) = (first == b'\\').then(|| octal_escape(tail)).flatten() {
            out.push(byte);
            rest = after;
        } else {
            out.push(first);
            rest = tail;
        }
    }
    out
}

/// Exactly three octal digits whose value fits a byte (the kernel never
/// writes more than `\377`); anything else is literal text. The value is
/// built with checked arithmetic so an out-of-range escape is simply not an
/// escape: `\777` in unchecked u8 math overflowed and panicked the root
/// helper (found by the `mount-decode-diff` fuzz target).
fn octal_escape(tail: &[u8]) -> Option<(u8, &[u8])> {
    let (digits, after) = tail.split_first_chunk::<3>()?;
    let byte = digits.iter().try_fold(0_u8, |acc, &digit| {
        let value = (b'0'..=b'7')
            .contains(&digit)
            .then(|| digit.checked_sub(b'0'))??;
        acc.checked_mul(8)?.checked_add(value)
    })?;
    Some((byte, after))
}

/// Facts about the running system the allowlist checks requests against.
pub struct SystemFacts {
    pub btrfs_mounts: Vec<String>,
    /// Whether a path is a block device.
    pub is_block_device: Box<dyn Fn(&str) -> bool>,
}

impl SystemFacts {
    /// The live system.
    #[must_use]
    pub fn live() -> Self {
        Self {
            btrfs_mounts: std::fs::read_to_string("/proc/self/mounts")
                .map(|text| btrfs_mountpoints(&text))
                .unwrap_or_default(),
            is_block_device: Box::new(|path| {
                std::fs::metadata(path).is_ok_and(|meta| meta.file_type().is_block_device())
            }),
        }
    }
}

/// Checks a run request against the allowlist. Returns the program name to
/// resolve on the fixed system path.
///
/// # Errors
///
/// Returns why the request is refused.
pub fn check_run(program: &str, args: &[String], context: &SystemFacts) -> Result<&'static str> {
    if args.len() > MAX_ARGS
        || args
            .iter()
            .any(|arg| arg.len() > MAX_ARG_BYTES || arg.contains('\0'))
    {
        bail!("request arguments exceed the helper's limits");
    }
    let a = args.iter().map(String::as_str).collect::<Vec<_>>();
    let mount = |path: &str| context.btrfs_mounts.iter().any(|m| m == path);
    let disk = |path: &str| valid_disk_name(path) && (context.is_block_device)(path);
    let allowed = match (program, a.as_slice()) {
        ("lshw", ["-short"]) => "lshw",
        ("dmesg", []) => "dmesg",
        ("btrfs", ["subvolume", "list", target]) if mount(target) => "btrfs",
        (
            "btrfs",
            ["subvolume", "list", "-s", target]
            | ["device", "stats", target]
            | ["scrub", "status", target],
        ) if mount(target) => "btrfs",
        ("btrfs", ["filesystem", "usage", "-b", target]) if mount(target) => "btrfs",
        ("smartctl", ["-i" | "-H", device]) if disk(device) => "smartctl",
        ("smartctl", ["-j", "-H", "-A", device]) if disk(device) => "smartctl",
        _ => bail!("not on the helper allowlist: {program} {}", a.join(" ")),
    };
    Ok(allowed)
}

// ---------------------------------------------------------------- server --

fn reply_for(output: &capture::Output) -> Reply {
    Reply {
        ok: output.success(),
        status: output.status.and_then(|s| s.code()),
        timed_out: output.timed_out,
        dropped: output.stdout.dropped(),
        stdout: output.stdout.text(),
        stderr: capture::bound_text(&output.stderr.text(), 0, 16 * 1024, 200),
        error: String::new(),
    }
}

fn refuse(error: impl std::fmt::Display) -> Reply {
    Reply {
        error: error.to_string(),
        ..Reply::default()
    }
}

/// What the helper will do for one request line, decided without running
/// anything. Everything a caller can influence passes through here.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Run this allowlisted program (resolved on the fixed system path).
    Run {
        program: &'static str,
        args: Vec<String>,
    },
    /// Apply this allowlisted repair.
    Fix { id: String },
}

/// Parses and checks one request line.
///
/// # Errors
///
/// Returns why the request is refused.
pub fn decide(line: &str, context: &SystemFacts) -> std::result::Result<Decision, String> {
    let request = serde_json::from_str::<Request>(line)
        .map_err(|error| format!("malformed request: {error}"))?;
    match request {
        Request::Run { program, args } => {
            let name = check_run(&program, &args, context).map_err(|error| error.to_string())?;
            Ok(Decision::Run {
                program: name,
                args,
            })
        }
        Request::Fix { id } => {
            crate::fix::validate_fix(&id).map_err(|error| format!("{error:#}"))?;
            Ok(Decision::Fix { id })
        }
    }
}

fn handle(line: &str, context: &SystemFacts) -> Reply {
    match decide(line, context) {
        Err(error) => refuse(error),
        Ok(Decision::Run { program, args }) => {
            let Some(executable) = crate::pathcheck::resolve(program) else {
                return refuse(format!("{program} is not installed"));
            };
            // Never execute, as root, a binary an unprivileged user could
            // have replaced (a writable /usr/local/bin on some systems).
            if let Err(error) = check_root_owned(&executable) {
                return refuse(format!("{error:#}"));
            }
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            let limits = Limits {
                timeout: Duration::from_secs(120),
                retain_bytes: REPLY_RETAIN,
            };
            // `executable` is the root-owned system path of `program`, a
            // &'static str that `decide` returned from the allowlist.
            // nosemgrep: sast.helper-spawns-only-allowlisted-programs
            match capture::run(&executable, &refs, limits) {
                Ok(output) => reply_for(&output),
                Err(error) => refuse(format!("cannot run {program}: {error}")),
            }
        }
        Ok(Decision::Fix { id }) => match crate::fix::apply_as_root(&id) {
            Ok(outcome) => Reply {
                ok: outcome.success,
                stdout: outcome.transcript,
                ..Reply::default()
            },
            Err(error) => refuse(format!("{error:#}")),
        },
    }
}

/// Whether the running executable may serve as the root helper: a regular
/// file owned by root and writable by no one else.
///
/// # Errors
///
/// Returns why the executable is not acceptable.
pub fn check_root_owned(path: &Path) -> Result<()> {
    let meta =
        std::fs::symlink_metadata(path).with_context(|| format!("checking {}", path.display()))?;
    anyhow::ensure!(
        !meta.file_type().is_symlink(),
        "{} is a symlink",
        path.display()
    );
    anyhow::ensure!(meta.is_file(), "{} is not a regular file", path.display());
    anyhow::ensure!(meta.uid() == 0, "{} is not owned by root", path.display());
    anyhow::ensure!(
        meta.mode() & 0o022 == 0,
        "{} is writable by group or others",
        path.display()
    );
    Ok(())
}

/// `omniscient --privileged-helper`: serve requests until stdin closes.
///
/// # Errors
///
/// Returns an error when not started as root through pkexec from a
/// root-owned executable, or when replies cannot be written.
pub fn serve() -> Result<()> {
    anyhow::ensure!(
        std::env::var("OMNISCIENT_HELPER").as_deref() == Ok("1"),
        "the helper is started by Omniscient itself"
    );
    anyhow::ensure!(crate::elevation::is_root(), "the helper must run as root");
    check_root_owned(&std::env::current_exe().context("locating the helper")?)?;
    let context = SystemFacts::live();
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = std::io::stdout().lock();
    loop {
        let mut line = Vec::new();
        let read = (&mut reader)
            .take((MAX_REQUEST_BYTES as u64).saturating_add(1))
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(());
        }
        let reply = if line.len() > MAX_REQUEST_BYTES {
            // Drop the rest of an oversized line, then refuse it.
            let mut rest = Vec::new();
            reader.read_until(b'\n', &mut rest)?;
            refuse("request too large")
        } else {
            handle(String::from_utf8_lossy(&line).trim(), &context)
        };
        let mut text = serde_json::to_string(&reply)?;
        text.push('\n');
        if stdout
            .write_all(text.as_bytes())
            .and_then(|()| stdout.flush())
            .is_err()
        {
            return Ok(());
        }
    }
}

// ---------------------------------------------------------------- client --

struct Connection {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

static HELPER: Mutex<Option<Connection>> = Mutex::new(None);

/// The root-owned executable to start the helper from, if installed.
#[must_use]
pub fn system_executable() -> Option<PathBuf> {
    let path = PathBuf::from(SYSTEM_EXECUTABLE);
    check_root_owned(&path).ok().map(|()| path)
}

/// Starts the helper with one pkexec authorization. A no-op when it is
/// already running.
///
/// # Errors
///
/// Returns why elevation is unavailable (no root-owned install, pkexec
/// missing, or authorization refused).
pub fn start() -> Result<()> {
    let mut guard = HELPER.lock().unwrap_or_else(PoisonError::into_inner);
    if guard.is_some() {
        return Ok(());
    }
    let executable = system_executable().context(
        "elevated checks need the root-owned install at /usr/local/bin/omniscient (./install.sh --system)",
    )?;
    // pkexec for the HUD; sudo (which may prompt on the terminal) for an
    // explicit repair started from the dashboard.
    let elevator = if crate::elevation::uses_graphical() {
        PKEXEC
    } else {
        crate::elevation::SUDO
    };
    // `elevator` is the absolute PKEXEC or SUDO constant.
    // nosemgrep: sast.helper-spawns-only-allowlisted-programs
    let mut child = Command::new(elevator)
        .arg(ENV)
        .arg("OMNISCIENT_HELPER=1")
        .arg(&executable)
        .arg("--privileged-helper")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("starting pkexec")?;
    let stdin = child.stdin.take().context("helper stdin")?;
    let stdout = BufReader::new(child.stdout.take().context("helper stdout")?);
    let mut connection = Connection {
        child,
        stdin,
        stdout,
    };
    // A ping proves authorization succeeded and the helper is serving.
    let pong = exchange(
        &mut connection,
        &Request::Run {
            program: "ping".into(),
            args: Vec::new(),
        },
    )?;
    anyhow::ensure!(
        pong.error.contains("allowlist"),
        "the privileged helper did not start (authorization refused?)"
    );
    *guard = Some(connection);
    Ok(())
}

/// Whether the helper is running.
#[must_use]
pub fn active() -> bool {
    HELPER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some()
}

fn exchange(connection: &mut Connection, request: &Request) -> Result<Reply> {
    let mut line = serde_json::to_string(request)?;
    line.push('\n');
    connection
        .stdin
        .write_all(line.as_bytes())
        .context("writing to the helper")?;
    connection.stdin.flush().context("writing to the helper")?;
    let mut reply = String::new();
    let read = connection
        .stdout
        .read_line(&mut reply)
        .context("reading from the helper")?;
    anyhow::ensure!(read > 0, "the helper exited");
    serde_json::from_str(&reply).context("malformed helper reply")
}

/// Sends one request to the running helper.
///
/// # Errors
///
/// Returns an error when no helper is running or it stopped answering.
pub fn request(request: &Request) -> Result<Reply> {
    let mut guard = HELPER.lock().unwrap_or_else(PoisonError::into_inner);
    let connection = guard.as_mut().context("no privileged helper is running")?;
    let result = exchange(connection, request);
    if result.is_err() {
        if let Some(mut dead) = guard.take() {
            let _ = dead.child.kill();
            let _ = dead.child.wait();
        }
    }
    result
}

/// Stops the helper (closing its stdin ends it).
pub fn stop() {
    if let Some(mut connection) = HELPER.lock().unwrap_or_else(PoisonError::into_inner).take() {
        drop(connection.stdin);
        let _ = connection.child.wait();
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;

    fn context() -> SystemFacts {
        SystemFacts {
            btrfs_mounts: vec!["/".into(), "/home".into(), "/mnt/my data".into()],
            is_block_device: Box::new(|path| path == "/dev/sda" || path == "/dev/nvme0n1"),
        }
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn only_allowlisted_commands_on_real_targets_pass() {
        let c = context();
        for (program, a) in [
            ("lshw", &["-short"][..]),
            ("dmesg", &[]),
            ("btrfs", &["device", "stats", "/"]),
            ("btrfs", &["scrub", "status", "/home"]),
            ("btrfs", &["filesystem", "usage", "-b", "/mnt/my data"]),
            ("btrfs", &["subvolume", "list", "-s", "/"]),
            ("smartctl", &["-j", "-H", "-A", "/dev/nvme0n1"]),
            ("smartctl", &["-H", "/dev/sda"]),
        ] {
            assert!(check_run(program, &args(a), &c).is_ok(), "{program} {a:?}");
        }
        for (program, a) in [
            ("sh", &["-c", "id"][..]),
            ("lshw", &[]),
            ("lshw", &["-short", "-dump", "/etc/shadow"]),
            ("dmesg", &["--clear"]),
            ("btrfs", &["device", "stats", "/etc"]),
            ("btrfs", &["scrub", "start", "/"]),
            ("btrfs", &["filesystem", "resize", "max", "/"]),
            ("btrfs", &["subvolume", "delete", "/"]),
            ("smartctl", &["-t", "long", "/dev/sda"]),
            ("smartctl", &["-s", "off", "/dev/sda"]),
            ("smartctl", &["-H", "/dev/sdb"]),
            ("smartctl", &["-H", "/dev/../etc/shadow"]),
            ("smartctl", &["-H", "/tmp/sda"]),
            ("ping", &[]),
        ] {
            assert!(
                check_run(program, &args(a), &c).is_err(),
                "{program} {a:?} must be refused"
            );
        }
        let long = "x".repeat(MAX_ARG_BYTES + 1);
        assert!(check_run("btrfs", &args(&["device", "stats", &long]), &c).is_err());
    }

    #[test]
    fn disk_names_are_strict() {
        for good in [
            "/dev/sda",
            "/dev/sdab",
            "/dev/nvme0n1",
            "/dev/nvme12n3",
            "/dev/mmcblk0",
            "/dev/vda",
            "/dev/xvdb",
        ] {
            assert!(valid_disk_name(good), "{good}");
        }
        for bad in [
            "/dev/sda1",
            "/dev/nvme0n1p2",
            "/dev/sd",
            "/dev/nvme0",
            "/dev/../sda",
            "sda",
            "/dev/sdA",
            "/dev/mmcblk0p1",
            "/dev/loop0",
        ] {
            assert!(!valid_disk_name(bad), "{bad}");
        }
    }

    #[test]
    fn btrfs_mountpoints_come_from_proc_mounts() {
        let mounts = "/dev/mapper/root / btrfs rw,subvol=/@ 0 0\n/dev/mapper/root /home btrfs rw 0 0\n\
                      /dev/sdb1 /mnt/my\\040data btrfs rw 0 0\n/dev/nvme0n1p1 /boot vfat rw 0 0\nproc /proc proc rw 0 0\n";
        assert_eq!(
            btrfs_mountpoints(mounts),
            vec!["/", "/home", "/mnt/my data"]
        );
    }

    #[test]
    fn out_of_range_and_short_octal_escapes_stay_literal() {
        // Regression: `\777` overflowed u8 math and panicked (fuzz-found).
        assert_eq!(decode_mount_path("/a\\777b"), "/a\\777b");
        assert_eq!(decode_mount_path("/a\\377"), "/a\u{fffd}");
        assert_eq!(decode_mount_path("/a\\04"), "/a\\04");
        assert_eq!(decode_mount_path("/a\\"), "/a\\");
        assert_eq!(decode_mount_path("\\134\\040"), "\\ ");
    }

    #[test]
    fn requests_parse_strictly_and_refusals_explain() {
        let c = context();
        let reply = handle(r#"{"op":"run","program":"sh","args":["-c","id"]}"#, &c);
        assert!(!reply.ok && reply.error.contains("allowlist"), "{reply:?}");
        let reply = handle("not json", &c);
        assert!(reply.error.contains("malformed"));
        let reply = handle(r#"{"op":"shell","cmd":"id"}"#, &c);
        assert!(reply.error.contains("malformed"), "unknown ops are refused");
        let reply = handle(r#"{"op":"fix","id":"run-command:rm -rf /"}"#, &c);
        assert!(!reply.ok && !reply.error.is_empty());
    }

    #[test]
    fn only_root_owned_executables_may_be_the_helper() {
        let path = crate::scratch::dir("helper-owner").join("binary");
        std::fs::write(&path, "x").expect("write");
        let error = check_root_owned(&path)
            .expect_err("a user-owned file is refused")
            .to_string();
        assert!(error.contains("not owned by root"), "{error}");
        std::fs::remove_dir_all(path.parent().expect("scratch dir")).expect("cleanup");
        if Path::new("/usr/bin/env").exists() {
            assert!(
                check_root_owned(Path::new("/usr/bin/env")).is_ok(),
                "a system binary is acceptable"
            );
        }
    }
}
