//! Parsers for text Omniscient reads from the system: none may panic, and
//! derived values stay in range.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let (some, full) = omniscient::signals::parse_psi(&text);
    let _ = omniscient::signals::assess_pressure("memory", some, full);
    let grouped = omniscient::signals::group_coredumps(&text);
    let _ = omniscient::signals::assess_crashes(&grouped);
    let units = omniscient::signals::parse_show(&text);
    let _ = omniscient::signals::assess_restarts(&units, "system", 3);
    let _ = omniscient::signals::assess_timers(&units, &units, "user");
    let _ = omniscient::signals::parse_boots(&text);
    let _ = omniscient::signals::ended_cleanly(&text);
    let _ = omniscient::signals::parse_device_stats(&text);
    let _ = omniscient::signals::parse_scrub_started(&text);
    if let Some(ratio) = omniscient::signals::parse_metadata_ratio(&text) {
        assert!(ratio >= 0.0 && ratio.is_finite());
    }
    let _ = omniscient::signals::parse_listeners(&text);
    let _ = omniscient::signals::parse_rss_kib(&text);
    let _ = omniscient::signals::decode_taint(u64::from_le_bytes(data.get(..8).and_then(|b| b.try_into().ok()).unwrap_or([0; 8])));
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
        let _ = omniscient::signals::assess_smart("/dev/sda", &json);
    }
    let _ = omniscient::history::parse(&text);
    let _ = omniscient::helper::btrfs_mountpoints(&text);
    let _ = omniscient::sensors::parse_power_profiles(&text);
    let middle = text.char_indices().nth(text.chars().count() / 2).map_or(text.len(), |(i, _)| i);
    let (before, after) = text.split_at(middle);
    if let Some(load) = omniscient::sensors::utilization(before, after) {
        assert!((0.0..=100.0).contains(&load), "utilization {load}");
    }
});
