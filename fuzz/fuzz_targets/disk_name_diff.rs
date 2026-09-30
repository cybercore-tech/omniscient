//! Differential: the helper's disk-name check against a regular expression
//! written from the same specification.
#![no_main]
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

fuzz_target!(|data: &[u8]| {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"^/dev/(?:(?:sd|vd|hd|xvd)[a-z]{1,3}|nvme[0-9]{1,3}n[0-9]{1,3}|mmcblk[0-9]{1,3})$").unwrap()
    });
    let name = String::from_utf8_lossy(data);
    assert_eq!(omniscient::helper::valid_disk_name(&name), re.is_match(&name), "input {name:?}");
});
