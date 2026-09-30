//! The JOURNAL tab's filters come from HUD text. Whatever the argv, parsing
//! must not panic, and an accepted query must respect the grammar (checked
//! differentially against regular expressions).
#![no_main]
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

fuzz_target!(|data: &[u8]| {
    static UNIT: OnceLock<regex::Regex> = OnceLock::new();
    static CURSOR: OnceLock<regex::Regex> = OnceLock::new();
    let unit_re = UNIT.get_or_init(|| regex::Regex::new(r"^[A-Za-z0-9@._:\\-]{1,256}$").unwrap());
    let cursor_re = CURSOR.get_or_init(|| regex::Regex::new(r"^[A-Za-z0-9=;_-]{1,512}$").unwrap());
    let text = String::from_utf8_lossy(data);
    let mut args = vec!["omniscient".to_owned(), "--journal".to_owned()];
    args.extend(text.split('\u{1}').map(str::to_owned));
    let Ok(query) = omniscient::journal::parse_args(&args) else {
        return;
    };
    assert!(query.priority <= 7);
    assert!((1..=omniscient::journal::MAX_LIMIT).contains(&query.limit));
    if let Some(unit) = &query.unit {
        assert!(unit_re.is_match(unit), "unit {unit:?}");
    }
    if let Some(cursor) = &query.after_cursor {
        assert!(cursor_re.is_match(cursor), "cursor {cursor:?}");
    }
    if let Some(grep) = &query.grep {
        assert!(!grep.chars().any(char::is_control) && grep.chars().count() <= omniscient::journal::MAX_GREP_CHARS);
    }
    let argv = omniscient::journal::journalctl_args(&query);
    assert!(argv.iter().all(|a| !a.contains('\0')));
    assert_eq!(argv[0], "--no-pager");
});
