//! The root helper's entire request surface: whatever the line, `decide`
//! must not panic, and anything it accepts must be exactly an allowlisted
//! command on a real target.
#![no_main]
use libfuzzer_sys::fuzz_target;
use omniscient::helper::{decide, Decision, SystemFacts};

fn facts() -> SystemFacts {
    SystemFacts {
        btrfs_mounts: vec!["/".into(), "/home".into()],
        is_block_device: Box::new(|p| p == "/dev/sda" || p == "/dev/nvme0n1"),
    }
}

fuzz_target!(|data: &[u8]| {
    let line = String::from_utf8_lossy(data);
    match decide(&line, &facts()) {
        Err(_) => {}
        Ok(Decision::Fix { id }) => {
            assert!(id == "enable-sensor:drivetemp" || id.starts_with("install-tool:"), "{id}");
            assert!(omniscient::fix::validate_fix(&id).is_ok());
        }
        Ok(Decision::Run { program, args }) => {
            let a = args.iter().map(String::as_str).collect::<Vec<_>>();
            let target = |t: &str| ["/", "/home", "/dev/sda", "/dev/nvme0n1"].contains(&t);
            let ok = match (program, a.as_slice()) {
                ("lshw", ["-short"]) | ("dmesg", []) => true,
                ("btrfs", [.., t]) | ("smartctl", [.., t]) => target(t),
                _ => false,
            };
            assert!(ok, "accepted outside the allowlist: {program} {a:?}");
            let flags = ["-short", "subvolume", "list", "-s", "device", "stats", "scrub", "status",
                         "filesystem", "usage", "-b", "-i", "-H", "-j", "-A"];
            for arg in &a[..a.len().saturating_sub(1)] {
                assert!(flags.contains(arg), "unexpected flag {arg:?} in {a:?}");
            }
        }
    }
});
