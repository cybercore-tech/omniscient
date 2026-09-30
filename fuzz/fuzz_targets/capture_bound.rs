//! Everything a module captures passes through `sanitize` and `bound_text`
//! before it reaches a report and the desktop shell.
#![no_main]
use libfuzzer_sys::fuzz_target;
use omniscient::capture::{bound_text, sanitize};

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let max_bytes = usize::from(u16::from_le_bytes([data[0], data[1]]) % 4096);
    let max_lines = usize::from(data[2]);
    let dropped = u64::from(data[3] % 8) * 1000;
    let text = String::from_utf8_lossy(&data[4..]);

    let clean = sanitize(&text);
    assert!(clean.chars().all(|c| c == '\n' || c == '\t' || !c.is_control()), "control char kept");
    assert_eq!(sanitize(&clean), clean, "sanitize is idempotent");

    let out = bound_text(&text, dropped, max_bytes, max_lines);
    let body = out.split("\n\n[omniscient: output truncated").next().unwrap_or_default();
    assert!(body.len() <= max_bytes.max(clean.len().min(max_bytes)) || body.len() <= max_bytes, "body {} > {max_bytes}", body.len());
    assert!(!out.contains("```"), "fences are defused");
    if dropped == 0 && clean.len() <= max_bytes && clean.lines().count() <= max_lines.max(1) && !clean.contains("```") {
        assert_eq!(out, clean, "nothing to cut means nothing cut");
    }
});
