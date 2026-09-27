//! Journal views for the HUD's JOURNAL tab (`omniscient --journal`).
//!
//! Read-only and unprivileged: it shows whatever the user's journal access
//! allows (system journal via the `systemd-journal`/`wheel` groups, plus the
//! user's own journal). Every filter arrives from a text field in the HUD, so
//! each is validated against a strict grammar before it becomes a
//! `journalctl` argument (argv only, never a shell).
//!
//! Output is bounded: at most [`MAX_LIMIT`] entries, messages capped at
//! [`MAX_MESSAGE_CHARS`], repeated lines collapsed into one entry with a
//! count, and the offender scan stops after [`OFFENDER_SCAN_LINES`] lines.

use crate::capture::{self, Limits};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const DEFAULT_LIMIT: usize = 300;
pub const MAX_LIMIT: usize = 2000;
pub const MAX_MESSAGE_CHARS: usize = 2000;
pub const MAX_GREP_CHARS: usize = 120;
/// The offender scan reads the most recent warnings of the boot.
pub const OFFENDER_SCAN_LINES: usize = 50_000;
/// A user unit's lines also carry `_SYSTEMD_UNIT=user@UID.service`; the
/// more specific field wins.
const SOURCE_FIELDS: [&str; 4] = [
    "_SYSTEMD_USER_UNIT",
    "_SYSTEMD_UNIT",
    "SYSLOG_IDENTIFIER",
    "_COMM",
];
const OFFENDER_SCAN_TIME: Duration = Duration::from_secs(10);

/// A validated journal query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// Most verbose priority shown (0 emerg … 7 debug).
    pub priority: u8,
    /// `0`, `-N`, or a 32-hex boot id.
    pub boot: String,
    pub unit: Option<String>,
    /// `15m`, `1h`, `today`, or `None` for the whole boot.
    pub since: Option<String>,
    pub grep: Option<String>,
    pub limit: usize,
    pub after_cursor: Option<String>,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            priority: 4,
            boot: "0".to_owned(),
            unit: None,
            since: None,
            grep: None,
            limit: DEFAULT_LIMIT,
            after_cursor: None,
        }
    }
}

fn valid_boot(value: &str) -> bool {
    value == "0"
        || value
            .strip_prefix('-')
            .is_some_and(|n| !n.is_empty() && n.len() <= 4 && n.bytes().all(|b| b.is_ascii_digit()))
        || (value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn valid_unit(value: &str) -> bool {
    (1..=256).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@._:-\\".contains(&b))
}

fn valid_cursor(value: &str) -> bool {
    (1..=512).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"=;_-".contains(&b))
}

/// Parses `--journal` arguments into a validated query.
///
/// # Errors
///
/// Returns an error naming the first invalid or unknown argument.
pub fn parse_args(args: &[String]) -> Result<Query> {
    let mut query = Query::default();
    let mut iter = args.iter().skip_while(|arg| *arg != "--journal").skip(1);
    while let Some(flag) = iter.next() {
        let mut value = || iter.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--priority" => {
                let v = value()?;
                query.priority = v
                    .parse()
                    .ok()
                    .filter(|p| *p <= 7)
                    .with_context(|| format!("invalid priority {v:?}"))?;
            }
            "--boot" => {
                let v = value()?;
                anyhow::ensure!(valid_boot(v), "invalid boot {v:?}");
                v.clone_into(&mut query.boot);
            }
            "--unit" => {
                let v = value()?;
                anyhow::ensure!(valid_unit(v), "invalid unit {v:?}");
                query.unit = Some(v.clone());
            }
            "--since" => {
                let v = value()?;
                anyhow::ensure!(
                    matches!(v.as_str(), "15m" | "1h" | "today" | "boot"),
                    "invalid since {v:?}"
                );
                query.since = (v != "boot").then(|| v.clone());
            }
            "--grep" => {
                let v = value()?;
                anyhow::ensure!(
                    v.chars().count() <= MAX_GREP_CHARS,
                    "grep pattern longer than {MAX_GREP_CHARS}"
                );
                anyhow::ensure!(
                    !v.chars().any(char::is_control),
                    "grep pattern contains control characters"
                );
                query.grep = (!v.is_empty()).then(|| v.clone());
            }
            "--limit" => {
                let v = value()?;
                query.limit = v
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=MAX_LIMIT).contains(n))
                    .with_context(|| format!("limit must be 1..={MAX_LIMIT}"))?;
            }
            "--after-cursor" => {
                let v = value()?;
                anyhow::ensure!(valid_cursor(v), "invalid cursor");
                query.after_cursor = Some(v.clone());
            }
            "--offenders" => {}
            other => bail!("unknown journal argument {other:?}"),
        }
    }
    Ok(query)
}

/// `journalctl` arguments for a query.
#[must_use]
pub fn journalctl_args(query: &Query) -> Vec<String> {
    let mut args = vec![
        "--no-pager".to_owned(),
        "-o".to_owned(),
        "json".to_owned(),
        "-p".to_owned(),
        query.priority.to_string(),
        "-b".to_owned(),
        query.boot.clone(),
        "-n".to_owned(),
        query.limit.to_string(),
    ];
    if let Some(since) = &query.since {
        args.push("--since".to_owned());
        args.push(
            match since.as_str() {
                "15m" => "-15min",
                "1h" => "-1h",
                _ => "today",
            }
            .to_owned(),
        );
    }
    if let Some(grep) = &query.grep {
        args.push("-g".to_owned());
        args.push(grep.clone());
    }
    if let Some(cursor) = &query.after_cursor {
        args.push("--after-cursor".to_owned());
        args.push(cursor.clone());
    }
    if let Some(unit) = &query.unit {
        // System and user units, and plain syslog identifiers.
        args.push(format!("_SYSTEMD_UNIT={unit}"));
        args.push("+".to_owned());
        args.push(format!("_SYSTEMD_USER_UNIT={unit}"));
        args.push("+".to_owned());
        args.push(format!("SYSLOG_IDENTIFIER={unit}"));
    }
    args
}

/// One (possibly collapsed) journal line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Microseconds since the epoch.
    pub time: i64,
    pub priority: u8,
    pub source: String,
    pub pid: String,
    pub message: String,
    /// How many consecutive identical lines this entry stands for.
    pub count: usize,
    pub boot: String,
}

/// A journal field as text; `MESSAGE` may be a byte array.
fn field(value: &Value, name: &str) -> Option<String> {
    match value.get(name)? {
        Value::String(text) => Some(text.clone()),
        Value::Array(bytes) => {
            let bytes = bytes
                .iter()
                .filter_map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
                .collect::<Vec<_>>();
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// A message with numbers and hex runs masked, so lines that differ only by
/// counters, addresses or ports (firewall logs, retries) collapse.
#[must_use]
pub fn shape(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if token.is_empty() {
            return;
        }
        let hex = token.bytes().all(|b| b.is_ascii_hexdigit());
        let numeric = hex && token.bytes().any(|b| b.is_ascii_digit());
        // Long hex words are ids; one- and two-digit hex tokens are octets
        // (MAC addresses such as ff:ff:ff:ff).
        let hex_id = hex && (token.len() >= 8 || token.len() <= 2);
        out.push_str(if numeric || hex_id { "#" } else { token });
        token.clear();
    };
    for c in message.chars() {
        if c.is_ascii_alphanumeric() {
            token.push(c);
        } else {
            flush(&mut token, &mut out);
            out.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}

/// Parses `journalctl -o json` lines, collapsing consecutive repeats (same
/// source, priority and message shape; the newest text is kept).
/// Returns the entries and the cursor of the last line read.
#[must_use]
pub fn parse_entries(text: &str) -> (Vec<Entry>, Option<String>) {
    let mut entries: Vec<Entry> = Vec::new();
    let mut cursor = None;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(c) = field(&value, "__CURSOR") {
            cursor = Some(c);
        }
        let source = SOURCE_FIELDS
            .iter()
            .find_map(|name| field(&value, name))
            .unwrap_or_else(|| "kernel".to_owned());
        let mut message = capture::sanitize(&field(&value, "MESSAGE").unwrap_or_default());
        if message.chars().count() > MAX_MESSAGE_CHARS {
            message = message.chars().take(MAX_MESSAGE_CHARS).collect::<String>() + " …";
        }
        let entry = Entry {
            time: field(&value, "__REALTIME_TIMESTAMP")
                .and_then(|t| t.parse().ok())
                .unwrap_or(0),
            priority: field(&value, "PRIORITY")
                .and_then(|p| p.parse().ok())
                .unwrap_or(6),
            pid: field(&value, "_PID").unwrap_or_default(),
            boot: field(&value, "_BOOT_ID").unwrap_or_default(),
            source,
            message,
            count: 1,
        };
        if let Some(last) = entries.last_mut() {
            if last.source == entry.source
                && last.priority == entry.priority
                && shape(&last.message) == shape(&entry.message)
            {
                last.count += 1;
                last.time = entry.time;
                last.message = entry.message;
                continue;
            }
        }
        entries.push(entry);
    }
    (entries, cursor)
}

/// Counts warning-or-worse lines per source from `journalctl -o json`
/// lines, most first.
#[must_use]
pub fn count_offenders<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for line in lines {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let source = SOURCE_FIELDS
            .iter()
            .find_map(|name| field(&value, name))
            .unwrap_or_else(|| "kernel".to_owned());
        *counts.entry(source).or_insert(0) += 1;
    }
    let mut sorted = counts.into_iter().collect::<Vec<_>>();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted.truncate(25);
    sorted
}

#[derive(Debug, Serialize)]
pub struct Offender {
    pub source: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct Boot {
    pub index: i64,
    pub id: String,
    pub first: i64,
}

/// The JSON printed for the HUD.
#[derive(Debug, Serialize)]
pub struct Output {
    pub version: u32,
    pub entries: Vec<Entry>,
    pub cursor: Option<String>,
    pub boots: Vec<Boot>,
    pub offenders: Vec<Offender>,
    /// Whether the offender scan stopped early.
    pub offenders_partial: bool,
    pub error: String,
}

fn journalctl() -> Option<PathBuf> {
    crate::pathcheck::resolve("journalctl")
}

fn boots() -> Vec<Boot> {
    let Some(exe) = journalctl() else {
        return Vec::new();
    };
    let limits = Limits {
        timeout: Duration::from_secs(20),
        retain_bytes: 1024 * 1024,
    };
    let Ok(out) = capture::run(&exe, &["--list-boots", "-o", "json", "--no-pager"], limits) else {
        return Vec::new();
    };
    crate::signals::parse_boots(&out.stdout.text())
        .into_iter()
        .take(20)
        .map(|(index, id, first)| Boot { index, id, first })
        .collect()
}

/// Scans the boot's warning-or-worse lines, streaming, and counts them per
/// source. Stops after [`OFFENDER_SCAN_LINES`] lines or 20 s.
fn offenders(boot: &str) -> (Vec<Offender>, bool) {
    let Some(exe) = journalctl() else {
        return (Vec::new(), false);
    };
    let Ok(mut child) = Command::new(exe)
        .args([
            "--no-pager",
            "-o",
            "json",
            "-p",
            "4",
            "-b",
            boot,
            "-n",
            &OFFENDER_SCAN_LINES.to_string(),
            "--output-fields=_SYSTEMD_UNIT,_SYSTEMD_USER_UNIT,SYSLOG_IDENTIFIER,_COMM",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return (Vec::new(), false);
    };
    let started = Instant::now();
    let mut lines = Vec::new();
    let mut partial = false;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            lines.push(line);
            if lines.len() >= OFFENDER_SCAN_LINES || started.elapsed() > OFFENDER_SCAN_TIME {
                partial = true;
                break;
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let counted = count_offenders(lines.iter().map(String::as_str))
        .into_iter()
        .map(|(source, count)| Offender { source, count })
        .collect();
    (counted, partial)
}

/// `omniscient --journal [filters]`: prints one bounded JSON view.
///
/// # Errors
///
/// Returns an error for invalid arguments or when output cannot be written.
pub fn run(args: &[String]) -> Result<()> {
    let query = parse_args(args)?;
    let want_offenders = args.iter().any(|arg| arg == "--offenders");
    let mut output = Output {
        version: 1,
        entries: Vec::new(),
        cursor: query.after_cursor.clone(),
        boots: Vec::new(),
        offenders: Vec::new(),
        offenders_partial: false,
        error: String::new(),
    };
    match journalctl() {
        None => "journalctl is not installed".clone_into(&mut output.error),
        Some(exe) => {
            let args = journalctl_args(&query);
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            let limits = Limits {
                timeout: Duration::from_secs(20),
                retain_bytes: 16 * 1024 * 1024,
            };
            match capture::run(&exe, &refs, limits) {
                Ok(out) => {
                    let (entries, cursor) = parse_entries(&out.stdout.text());
                    output.entries = entries;
                    if cursor.is_some() {
                        output.cursor = cursor;
                    }
                    if !out.success() && output.entries.is_empty() {
                        capture::bound_text(&out.stderr.text(), 0, 2048, 10)
                            .trim()
                            .clone_into(&mut output.error);
                    }
                }
                Err(error) => output.error = error.to_string(),
            }
        }
    }
    if query.after_cursor.is_none() {
        output.boots = boots();
    }
    if want_offenders {
        (output.offenders, output.offenders_partial) = offenders(&query.boot);
    }
    let json = serde_json::to_string(&output)?;
    anyhow::ensure!(json.len() <= 8 * 1024 * 1024, "journal view exceeded 8 MiB");
    crate::emit(&json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("omniscient")
            .chain(std::iter::once("--journal"))
            .chain(list.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn arguments_are_validated_strictly() {
        let q = parse_args(&args(&[
            "--priority",
            "3",
            "--boot",
            "-1",
            "--unit",
            "cyberdeck-diag-deck.service",
            "--since",
            "1h",
            "--grep",
            "failed",
            "--limit",
            "500",
        ]))
        .expect("valid");
        assert_eq!((q.priority, q.boot.as_str(), q.limit), (3, "-1", 500));
        assert_eq!(q.unit.as_deref(), Some("cyberdeck-diag-deck.service"));
        assert!(parse_args(&args(&["--boot", "a3f0621128b448e683f87135bb9a47c9"])).is_ok());
        for bad in [
            &["--priority", "8"][..],
            &["--boot", "1; rm -rf /"],
            &["--boot", "-99999"],
            &["--unit", "a b"],
            &["--unit", "$(x)"],
            &["--since", "yesterday"],
            &["--limit", "0"],
            &["--limit", "5000"],
            &["--grep", "line\nbreak"],
            &["--after-cursor", "s=1 2"],
            &["--rm"],
            &["--priority"],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
        let long = "x".repeat(MAX_GREP_CHARS + 1);
        assert!(parse_args(&args(&["--grep", &long])).is_err());
    }

    #[test]
    fn journalctl_arguments_are_built_from_the_query() {
        let q = Query {
            priority: 3,
            boot: "-1".into(),
            unit: Some("sshd.service".into()),
            since: Some("15m".into()),
            grep: Some("fail".into()),
            limit: 50,
            after_cursor: Some("s=abc;i=1".into()),
        };
        let a = journalctl_args(&q);
        let joined = a.join(" ");
        assert!(joined.starts_with("--no-pager -o json -p 3 -b -1 -n 50"));
        assert!(
            joined.contains("--since -15min")
                && joined.contains("-g fail")
                && joined.contains("--after-cursor s=abc;i=1")
        );
        assert!(joined.ends_with("_SYSTEMD_UNIT=sshd.service + _SYSTEMD_USER_UNIT=sshd.service + SYSLOG_IDENTIFIER=sshd.service"));
    }

    #[test]
    fn entries_parse_collapse_and_bound() {
        let line = |msg: &str, cursor: &str| {
            format!(
                r#"{{"__CURSOR":"{cursor}","__REALTIME_TIMESTAMP":"1790400000000000","PRIORITY":"3","_SYSTEMD_UNIT":"x.service","_PID":"42","MESSAGE":"{msg}","_BOOT_ID":"b"}}"#
            )
        };
        let bytes = r#"{"__CURSOR":"c9","PRIORITY":"4","SYSLOG_IDENTIFIER":"kernel","MESSAGE":[104,105,27,91,49,109,33]}"#;
        let text = [
            line("boom", "c1"),
            line("boom", "c2"),
            line("boom", "c3"),
            line("other", "c4"),
            bytes.to_owned(),
            "not json".to_owned(),
        ]
        .join("\n");
        let (entries, cursor) = parse_entries(&text);
        assert_eq!(cursor.as_deref(), Some("c9"));
        assert_eq!(entries.len(), 3);
        assert_eq!((entries[0].message.as_str(), entries[0].count), ("boom", 3));
        assert_eq!(entries[0].pid, "42");
        assert_eq!(
            entries[2].message, "hi!",
            "byte-array messages decode and lose escapes"
        );
        let long = line(&"y".repeat(5000), "c1");
        assert!(parse_entries(&long).0[0].message.chars().count() <= MAX_MESSAGE_CHARS + 2);
        // Real UFW lines differ only by MAC, ports and ids: they collapse.
        let ufw = |mac: &str, port: u32| {
            format!(
                r#"{{"PRIORITY":"4","SYSLOG_IDENTIFIER":"kernel","MESSAGE":"[UFW BLOCK] IN=wlp2s0 MAC={mac} SRC=192.168.1.{port} DPT={port} ID=5{port}"}}"#
            )
        };
        let spam = [
            ufw("01:00:5e:00", 1),
            ufw("33:33:00:01", 22),
            ufw("ff:ff:ff:ff", 137),
        ]
        .join("\n");
        let (collapsed, _) = parse_entries(&spam);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].count, 3);
        assert!(
            collapsed[0].message.contains("DPT=137"),
            "the newest text is kept"
        );
        assert_eq!(shape("port 22 retry 3 id deadbeef"), "port # retry # id #");
        assert_eq!(
            shape("MAC=01:00:5e wlp2s0 face"),
            "MAC=#:#:# wlp2s0 face",
            "words and interface names survive"
        );
    }

    #[test]
    fn offenders_are_counted_and_ranked() {
        let lines = [
            r#"{"_SYSTEMD_UNIT":"a.service"}"#,
            r#"{"_SYSTEMD_UNIT":"b.service"}"#,
            r#"{"_SYSTEMD_UNIT":"a.service"}"#,
            r#"{"_COMM":"kworker"}"#,
            r#"{"_SYSTEMD_UNIT":"user@1000.service","_SYSTEMD_USER_UNIT":"a.service"}"#,
            "garbage",
        ];
        let ranked = count_offenders(lines.into_iter());
        assert_eq!(
            ranked[0],
            ("a.service".to_owned(), 3),
            "the user unit wins over user@UID.service"
        );
        assert_eq!(ranked.len(), 3);
    }
}
