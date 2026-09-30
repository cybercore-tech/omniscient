//! Journal JSON lines are written by any program on the machine. Parsing
//! must not panic, must bound every message, and the collapse key (`shape`)
//! must agree with an independent regex implementation.
#![no_main]
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

fn reference_shape(message: &str) -> String {
    static TOKEN: OnceLock<regex::Regex> = OnceLock::new();
    let re = TOKEN.get_or_init(|| regex::Regex::new(r"[A-Za-z0-9]+").unwrap());
    re.replace_all(message, |caps: &regex::Captures| {
        let t = &caps[0];
        let hex = t.bytes().all(|b| b.is_ascii_hexdigit());
        let numeric = hex && t.bytes().any(|b| b.is_ascii_digit());
        if numeric || (hex && (t.len() >= 8 || t.len() <= 2)) { "#".to_owned() } else { t.to_owned() }
    })
    .into_owned()
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let (entries, _) = omniscient::journal::parse_entries(&text);
    for entry in &entries {
        assert!(entry.count >= 1);
        assert!(entry.message.chars().count() <= omniscient::journal::MAX_MESSAGE_CHARS + 2);
        assert!(!entry.message.contains('\u{1b}'));
    }
    assert_eq!(omniscient::journal::shape(&text), reference_shape(&text));
});
