//! Differential: the helper's `/proc/self/mounts` path decoder against an
//! independent reference decoder.
#![no_main]
use libfuzzer_sys::fuzz_target;

/// Reference: octal escapes are exactly three digits 0-7 with a value
/// that fits in a byte; anything else is literal.
fn reference(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() + 1 && i + 4 <= bytes.len() {
            let digits = &bytes[i + 1..i + 4];
            if digits.iter().all(|b| (b'0'..=b'7').contains(b)) {
                let value = u32::from(digits[0] - b'0') * 64 + u32::from(digits[1] - b'0') * 8 + u32::from(digits[2] - b'0');
                if let Ok(byte) = u8::try_from(value) {
                    out.push(byte);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fuzz_target!(|data: &[u8]| {
    let raw = String::from_utf8_lossy(data);
    assert_eq!(omniscient::helper::decode_mount_path(&raw), reference(&raw), "input {raw:?}");
});
