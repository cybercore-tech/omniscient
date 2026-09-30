//! Kani model-checking proofs (`cargo kani`, run by `scripts/verify.sh`).
//!
//! Unlike tests and fuzzing, which try many inputs, each harness here is
//! checked for every input within its bounds: a property that holds is
//! proved for all of them, and a counterexample is a concrete input.
//! The harnesses cover the root helper's allowlist and the parsers it
//! trusts, plus the one numeric invariant the HUD's meters depend on.

use crate::helper::{check_run, decode_mount_bytes, valid_disk_name, SystemFacts};

/// An arbitrary ASCII string of up to `N` bytes.
fn any_ascii<const N: usize>() -> String {
    let bytes: [u8; N] = kani::any();
    let len: usize = kani::any_where(|l| *l <= N);
    kani::assume(bytes.iter().all(u8::is_ascii));
    String::from_utf8(bytes[..len].to_vec()).unwrap_or_default()
}

/// The mount-path decoder never panics (it did, on `\777`, before the
/// `mount-decode-diff` fuzz target found it) and never grows its input, for
/// every byte string up to 8 bytes: all escapes, partial escapes and
/// out-of-range values included.
#[kani::proof]
#[kani::unwind(10)]
fn decode_mount_path_never_panics() {
    let bytes: [u8; 8] = kani::any();
    let len: usize = kani::any_where(|l| *l <= 8);
    let raw = &bytes[..len];
    let decoded = decode_mount_bytes(raw);
    assert!(decoded.len() <= raw.len());
}

/// Any disk name the helper accepts is `/dev/` followed by letters and
/// digits only: no `/`, no `.`, so no traversal, no other device nodes.
#[kani::proof]
#[kani::unwind(16)]
fn accepted_disk_names_cannot_escape_dev() {
    let device = any_ascii::<14>();
    if valid_disk_name(&device) {
        let name = device.strip_prefix("/dev/").expect("prefix");
        assert!(name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()));
        assert!(
            name.starts_with("sd")
                || name.starts_with("vd")
                || name.starts_with("hd")
                || name.starts_with("xvd")
                || name.starts_with("nvme")
                || name.starts_with("mmcblk")
        );
        assert!(name.len() <= 13);
    }
}

/// The helper never admits a program outside its four, and never with
/// arguments it was not asked for: whatever `program` and single argument
/// arrive, success means one of the fixed commands.
#[kani::proof]
#[kani::unwind(12)]
fn allowlist_admits_only_fixed_commands() {
    let program = any_ascii::<8>();
    let args = vec![any_ascii::<6>()];
    let facts = SystemFacts {
        btrfs_mounts: vec!["/".to_owned()],
        is_block_device: Box::new(|_| true),
    };
    if let Ok(allowed) = check_run(&program, &args, &facts) {
        assert_eq!(allowed, program.as_str());
        assert!(
            (program == "lshw" && args[0] == "-short")
                || (program == "smartctl" && valid_disk_name(&args[0])),
            "a one-argument request is only lshw -short or a smartctl identity/health query"
        );
    }
}

/// The HUD's CPU meter reads [`crate::sensors::busy_percent`]: for every
/// pair of doubles (NaN, infinities, negatives included) the result is a
/// percentage or nothing.
#[kani::proof]
fn busy_percent_is_a_percentage() {
    let total: f64 = kani::any();
    let idle: f64 = kani::any();
    if let Some(percent) = crate::sensors::busy_percent(total, idle) {
        assert!((0.0..=100.0).contains(&percent));
    }
}
