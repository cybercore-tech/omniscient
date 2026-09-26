//! Deep signals: the checks most audit tools skip.
//!
//! Each check is split into a *collector* that reads the system (files under
//! `/proc` and `/sys`, or bounded commands through [`crate::capture`]) and a
//! pure *assessor* that turns the collected text into report lines and
//! [`Finding`]s. The assessors are unit-tested against real output shapes;
//! the collectors degrade to an explanatory line when a source is missing or
//! needs privileges the current run does not have.
//!
//! Findings feed the health score and the fix center; facts feed the
//! "what changed since the last audit" report ([`crate::changes`]).

use crate::capture::{self, Limits};
use crate::history::{self, Sample};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How serious a finding is. Ordered from least to most serious.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Watch,
    Warning,
    Urgent,
}

impl Severity {
    /// Lowercase label used in reports, notes and the HUD.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Watch => "watch",
            Self::Warning => "warning",
            Self::Urgent => "urgent",
        }
    }

    /// Health-score points this severity costs.
    #[must_use]
    pub fn penalty(self) -> i32 {
        match self {
            Self::Info => 0,
            Self::Watch => 2,
            Self::Warning => 7,
            Self::Urgent => 15,
        }
    }
}

/// One thing worth a human's attention.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable identity across audits (used by the change report).
    pub key: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    /// A read-only command to inspect it further.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub docs_url: String,
}

impl Finding {
    fn new(
        key: impl Into<String>,
        severity: Severity,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            severity,
            title: title.into(),
            detail: detail.into(),
            command: String::new(),
            docs_url: String::new(),
        }
    }

    fn inspect(mut self, command: &str, docs_url: &str) -> Self {
        command.clone_into(&mut self.command);
        docs_url.clone_into(&mut self.docs_url);
        self
    }
}

/// Machine-readable output of the deep-signals module (`signals.json`).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Signals {
    pub version: u32,
    pub generated_at: String,
    pub findings: Vec<Finding>,
    /// Named sets compared between audits by the change report.
    pub facts: BTreeMap<String, Vec<String>>,
}

/// File name of the machine-readable signals inside the module directory.
pub const SIGNALS_JSON: &str = "signals.json";
/// Most crash, unit or file entries listed in any one section.
const MAX_LISTED: usize = 40;

/// One section of the signals report.
#[derive(Debug, Default)]
pub struct Section {
    pub heading: &'static str,
    pub body: String,
    pub findings: Vec<Finding>,
    pub facts: Vec<(&'static str, Vec<String>)>,
}

impl Section {
    fn new(heading: &'static str) -> Self {
        Self {
            heading,
            ..Self::default()
        }
    }

    fn line(&mut self, text: impl AsRef<str>) {
        self.body.push_str(text.as_ref());
        self.body.push('\n');
    }
}

// ------------------------------------------------------------ collection --

fn limits(seconds: u64, retain: usize) -> Limits {
    Limits {
        timeout: Duration::from_secs(seconds),
        retain_bytes: retain,
    }
}

/// Stdout of a successful command, or `None`.
fn stdout_of(command: &str, args: &[&str]) -> Option<String> {
    let executable = crate::pathcheck::resolve(command)?;
    let output = capture::run(&executable, args, limits(60, 4 * 1024 * 1024)).ok()?;
    output.success().then(|| output.stdout.text())
}

/// Stdout of a command run with root privileges, or `None`.
fn elevated_stdout_of(command: &str, args: &[&str]) -> Option<String> {
    let executable = crate::pathcheck::resolve(command)?;
    let output = capture::run_elevated(&executable, args, limits(120, 4 * 1024 * 1024)).ok()?;
    output.success().then(|| output.stdout.text())
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

fn read_u64(path: &Path) -> Option<u64> {
    read_trimmed(path)?.parse().ok()
}

fn now_epoch() -> i64 {
    chrono::Local::now().timestamp()
}

/// The UID whose desktop session this audit describes: the invoking user
/// when running as the elevated child, otherwise this process's UID.
fn owner_uid() -> Option<u32> {
    if let Some(uid) = std::env::var("OMNISCIENT_OWNER_UID")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        return Some(uid);
    }
    let status = fs::read_to_string("/proc/self/status").ok()?;
    parse_status_uid(&status)
}

fn parse_status_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|uid| uid.parse().ok())
}

/// Runs every deep check and returns the sections in report order.
#[must_use]
pub fn collect() -> Vec<Section> {
    vec![
        update_hygiene(),
        pressure(),
        crash_trends(),
        service_health(),
        boot_history(),
        btrfs_health(),
        drive_wear(),
        thermal(),
        kernel_taint(),
        battery(),
        ecosystem(),
        shell_watch(),
        inventory(),
    ]
}

/// Flattens sections into the machine-readable signals.
#[must_use]
pub fn to_signals(sections: &[Section], generated_at: &str) -> Signals {
    let mut signals = Signals {
        version: 1,
        generated_at: generated_at.to_owned(),
        ..Signals::default()
    };
    for section in sections {
        signals.findings.extend(section.findings.iter().cloned());
        for (name, values) in &section.facts {
            let entry = signals.facts.entry((*name).to_owned()).or_default();
            entry.extend(values.iter().cloned());
            entry.sort();
            entry.dedup();
        }
    }
    signals
        .findings
        .sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.key.cmp(&b.key)));
    signals
}

/// Reads a previous audit's signals; missing or malformed means `None`.
#[must_use]
pub fn load(path: &Path) -> Option<Signals> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Renders the findings table that opens the signals report.
#[must_use]
pub fn render_findings(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "No deep signals need attention.\n".to_owned();
    }
    let mut out = String::new();
    for finding in findings {
        let _ = writeln!(
            out,
            "[{}] {} / {}",
            finding.severity.label().to_uppercase(),
            finding.title,
            finding.detail
        );
        if !finding.command.is_empty() {
            let _ = writeln!(out, "    inspect: {}", finding.command);
        }
    }
    out
}

// ------------------------------------------------ 1. update hygiene ------

/// Whether the running kernel's modules are still installed. On Arch an
/// upgrade removes the old tree, so new modules cannot load until a reboot.
#[must_use]
pub fn assess_kernel(
    running: &str,
    modules_present: bool,
    installed: &[String],
) -> Option<Finding> {
    if modules_present {
        return None;
    }
    Some(
        Finding::new(
            "reboot-needed",
            Severity::Warning,
            "Reboot needed: the running kernel was upgraded",
            format!(
                "Running {running}, but its modules are no longer installed (installed: {}). New drivers and USB devices may fail to load until you reboot.",
                if installed.is_empty() { "unknown".to_owned() } else { installed.join(", ") }
            ),
        )
        .inspect("uname -r; ls /usr/lib/modules", "https://wiki.archlinux.org/title/Kernel#Troubleshooting"),
    )
}

/// Groups processes still mapping deleted shared libraries by executable,
/// from `(exe, maps text)` pairs.
#[must_use]
pub fn deleted_library_users(processes: &[(String, String)]) -> BTreeMap<String, usize> {
    let mut users = BTreeMap::new();
    for (exe, maps) in processes {
        let stale = maps.lines().any(|line| {
            let Some(path) = line.strip_suffix(" (deleted)") else {
                return false;
            };
            // Only shared objects hold code an upgrade replaces; locale
            // archives, caches and memfds do not need a restart.
            let name = path.rsplit('/').next().unwrap_or_default();
            name.split('.').any(|part| part == "so")
        });
        if stale {
            *users.entry(exe.clone()).or_insert(0) += 1;
        }
    }
    users
}

fn update_hygiene() -> Section {
    let mut section = Section::new("update hygiene / reboot and config merges");
    let running = read_trimmed(Path::new("/proc/sys/kernel/osrelease")).unwrap_or_default();
    let modules_root = Path::new("/usr/lib/modules");
    let present = modules_root.join(&running).join("modules.dep").exists();
    let mut installed = fs::read_dir(modules_root)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().join("vmlinuz").exists())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    installed.sort();
    section.line(format!("running kernel: {running}"));
    section.line(format!("installed kernels: {}", installed.join(", ")));
    section.line(format!(
        "running kernel modules: {}",
        if present {
            "installed"
        } else {
            "MISSING (reboot needed)"
        }
    ));
    section
        .findings
        .extend(assess_kernel(&running, present, &installed));
    section.facts.push(("installed kernels", installed));

    // Processes still using libraries an upgrade replaced (needrestart-style).
    let mut processes = Vec::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.filter_map(Result::ok) {
            let pid = entry.file_name();
            if !pid.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let Ok(exe) = fs::read_link(entry.path().join("exe")) else {
                continue;
            };
            let Ok(maps) = fs::read_to_string(entry.path().join("maps")) else {
                continue;
            };
            processes.push((exe.to_string_lossy().into_owned(), maps));
        }
    }
    let stale = deleted_library_users(&processes);
    if stale.is_empty() {
        section.line("processes using replaced libraries: none found");
    } else {
        section.line(format!(
            "processes using replaced libraries ({}):",
            stale.len()
        ));
        for (exe, count) in stale.iter().take(MAX_LISTED) {
            section.line(format!("  {exe} ×{count}"));
        }
        section.findings.push(
            Finding::new(
                "stale-libraries",
                Severity::Watch,
                format!("{} program(s) still run replaced libraries", stale.len()),
                "They keep the old code (including any security fixes' absence) until restarted.",
            )
            .inspect(
                "lsof +c0 -nP 2>/dev/null | grep 'DEL.*\\.so'",
                "https://wiki.archlinux.org/title/Pacman#Restarting_services_after_an_update",
            ),
        );
    }
    section.facts.push((
        "programs using replaced libraries",
        stale.keys().cloned().collect(),
    ));

    let unmerged = find_pacnew(Path::new("/etc"), 6);
    if unmerged.is_empty() {
        section.line("unmerged .pacnew/.pacsave files: none");
    } else {
        section.line(format!(
            "unmerged .pacnew/.pacsave files ({}):",
            unmerged.len()
        ));
        for path in unmerged.iter().take(MAX_LISTED) {
            section.line(format!("  {path}"));
        }
        section.findings.push(
            Finding::new(
                "pacnew",
                Severity::Watch,
                format!("{} configuration update(s) waiting to be merged", unmerged.len()),
                "pacman installed new default configs next to files you changed; the running config may be missing new options or fixes.",
            )
            .inspect("pacdiff -o", "https://wiki.archlinux.org/title/Pacman/Pacnew_and_Pacsave"),
        );
    }
    section.facts.push(("unmerged config files", unmerged));
    section
}

/// `.pacnew`/`.pacsave` files under `root`, at most `depth` levels deep.
/// Unreadable directories and symlinks are skipped.
#[must_use]
pub fn find_pacnew(root: &Path, depth: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, level)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() && level < depth {
                stack.push((path, level + 1));
            } else if kind.is_file() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.ends_with(".pacnew") || name.ends_with(".pacsave") {
                    found.push(path.display().to_string());
                }
            }
        }
        if found.len() > 1_000 {
            break;
        }
    }
    found.sort();
    found
}

// ----------------------------------------------------- 2. pressure -------

/// Averages from one `/proc/pressure/*` line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pressure {
    pub avg10: f64,
    pub avg60: f64,
    pub avg300: f64,
}

/// Parses a PSI file into its `some` and `full` lines.
#[must_use]
pub fn parse_psi(text: &str) -> (Option<Pressure>, Option<Pressure>) {
    let mut some = None;
    let mut full = None;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let kind = fields.next();
        let mut pressure = Pressure::default();
        let mut seen = 0;
        for field in fields {
            let Some((name, value)) = field.split_once('=') else {
                continue;
            };
            let Ok(value) = value.parse::<f64>() else {
                continue;
            };
            match name {
                "avg10" => pressure.avg10 = value,
                "avg60" => pressure.avg60 = value,
                "avg300" => pressure.avg300 = value,
                _ => continue,
            }
            seen += 1;
        }
        if seen == 3 {
            match kind {
                Some("some") => some = Some(pressure),
                Some("full") => full = Some(pressure),
                _ => {}
            }
        }
    }
    (some, full)
}

/// Judges sustained (5-minute) pressure for one resource.
#[must_use]
pub fn assess_pressure(
    resource: &str,
    some: Option<Pressure>,
    full: Option<Pressure>,
) -> Option<Finding> {
    let some = some?.avg300;
    let full = full.map_or(0.0, |pressure| pressure.avg300);
    let (watch, warning, urgent) = match resource {
        "memory" => (5.0, 10.0, 30.0),
        "io" => (10.0, 20.0, 50.0),
        _ => (50.0, 80.0, 95.0),
    };
    let severity = if some >= urgent || (resource == "memory" && full >= 10.0) {
        Severity::Urgent
    } else if some >= warning || (resource == "memory" && full >= 5.0) {
        Severity::Warning
    } else if some >= watch {
        Severity::Watch
    } else {
        return None;
    };
    Some(
        Finding::new(
            format!("pressure-{resource}"),
            severity,
            format!("Sustained {resource} pressure"),
            format!("Tasks waited on {resource} {some:.1}% of the last 5 minutes (full stall {full:.1}%)."),
        )
        .inspect(&format!("cat /proc/pressure/{resource}"), "https://docs.kernel.org/accounting/psi.html"),
    )
}

fn pressure() -> Section {
    let mut section = Section::new("pressure stall information (PSI)");
    section.line("share of time tasks waited on each resource; load average only estimates this");
    for resource in ["cpu", "memory", "io"] {
        let Some(text) = read_trimmed(&Path::new("/proc/pressure").join(resource)) else {
            section.line(format!(
                "{resource}: unavailable (kernel without CONFIG_PSI?)"
            ));
            continue;
        };
        let (some, full) = parse_psi(&text);
        let show = |p: Option<Pressure>| {
            p.map_or_else(
                || "n/a".to_owned(),
                |p| format!("{:.2} / {:.2} / {:.2}", p.avg10, p.avg60, p.avg300),
            )
        };
        section.line(format!(
            "{resource:<6} some {}   full {}   (avg 10s / 60s / 300s)",
            show(some),
            show(full)
        ));
        section
            .findings
            .extend(assess_pressure(resource, some, full));
    }
    section
}

// ------------------------------------------------- 3. crash trends -------

#[derive(Deserialize)]
struct Coredump {
    #[serde(default)]
    exe: Option<String>,
    #[serde(default)]
    time: Option<i64>,
    #[serde(default)]
    sig: Option<i64>,
}

/// Crash counts grouped by executable: `(exe, count, newest microseconds,
/// signals)`, most crashes first.
#[must_use]
pub fn group_coredumps(json: &str) -> Vec<(String, usize, i64, Vec<i64>)> {
    let Ok(dumps) = serde_json::from_str::<Vec<Coredump>>(json) else {
        return Vec::new();
    };
    let mut groups: BTreeMap<String, (usize, i64, Vec<i64>)> = BTreeMap::new();
    for dump in dumps {
        let exe = dump.exe.unwrap_or_else(|| "unknown".to_owned());
        let group = groups.entry(exe).or_default();
        group.0 += 1;
        group.1 = group.1.max(dump.time.unwrap_or(0));
        if let Some(sig) = dump.sig {
            if !group.2.contains(&sig) {
                group.2.push(sig);
            }
        }
    }
    let mut grouped = groups
        .into_iter()
        .map(|(exe, (count, newest, signals))| (exe, count, newest, signals))
        .collect::<Vec<_>>();
    grouped.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    grouped
}

/// A crashing program is a finding at two or more crashes in the window.
#[must_use]
pub fn assess_crashes(grouped: &[(String, usize, i64, Vec<i64>)]) -> Vec<Finding> {
    grouped
        .iter()
        .filter(|(_, count, _, _)| *count >= 2)
        .map(|(exe, count, _, _)| {
            let name = Path::new(exe)
                .file_name()
                .map_or_else(|| exe.clone(), |n| n.to_string_lossy().into_owned());
            Finding::new(
                format!("crash:{name}"),
                if *count >= 5 {
                    Severity::Warning
                } else {
                    Severity::Watch
                },
                format!("{name} crashed {count} times in 7 days"),
                format!("{exe} keeps crashing; the core dumps hold the backtrace."),
            )
            .inspect(
                &format!("coredumpctl info {exe}"),
                "https://wiki.archlinux.org/title/Core_dump",
            )
        })
        .collect()
}

fn crash_trends() -> Section {
    let mut section = Section::new("crash trends / last 7 days");
    let Some(json) = stdout_of(
        "coredumpctl",
        &["list", "--no-pager", "--json=short", "--since=-7d"],
    ) else {
        section.line("no crashes recorded in the last 7 days (or coredumpctl unavailable)");
        return section;
    };
    let grouped = group_coredumps(&json);
    if grouped.is_empty() {
        section.line("no crashes recorded in the last 7 days");
    }
    for (exe, count, newest, signals) in grouped.iter().take(MAX_LISTED) {
        let when = chrono::DateTime::from_timestamp_micros(*newest).map_or_else(
            || "unknown".to_owned(),
            |t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            },
        );
        let sigs = signals
            .iter()
            .map(|s| signal_name(*s))
            .collect::<Vec<_>>()
            .join(",");
        section.line(format!(
            "{count:>4} ×  {exe}   last {when}   signals {sigs}"
        ));
    }
    section.findings.extend(assess_crashes(&grouped));
    section.facts.push((
        "crashing programs (7 days)",
        grouped.iter().map(|(exe, ..)| exe.clone()).collect(),
    ));
    section
}

fn signal_name(signal: i64) -> String {
    match signal {
        4 => "SIGILL".to_owned(),
        6 => "SIGABRT".to_owned(),
        7 => "SIGBUS".to_owned(),
        8 => "SIGFPE".to_owned(),
        11 => "SIGSEGV".to_owned(),
        5 => "SIGTRAP".to_owned(),
        other => other.to_string(),
    }
}

// ------------------------------------------------ 4. service health ------

/// Parses `systemctl show -p ...` output (blank-line separated blocks) into
/// one property map per unit.
#[must_use]
pub fn parse_show(text: &str) -> Vec<BTreeMap<String, String>> {
    let mut blocks = Vec::new();
    let mut current = BTreeMap::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            current.insert(key.to_owned(), value.to_owned());
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

/// Units that restarted `threshold` or more times since they were started.
#[must_use]
pub fn assess_restarts(
    units: &[BTreeMap<String, String>],
    scope: &str,
    threshold: u64,
) -> Vec<Finding> {
    units
        .iter()
        .filter_map(|unit| {
            let id = unit.get("Id")?;
            let restarts = unit.get("NRestarts")?.parse::<u64>().ok()?;
            (restarts >= threshold).then(|| {
                Finding::new(
                    format!("restart-loop:{scope}:{id}"),
                    if restarts >= 20 { Severity::Urgent } else { Severity::Warning },
                    format!("{id} restarted {restarts} times"),
                    format!("The {scope} unit keeps failing and being restarted by systemd."),
                )
                .inspect(
                    &format!("systemctl {}status {id} --no-pager -l", if scope == "user" { "--user " } else { "" }),
                    "https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html#Restart=",
                )
            })
        })
        .collect()
}

/// Timers whose most recent activation ended in failure.
#[must_use]
pub fn assess_timers(
    timers: &[BTreeMap<String, String>],
    services: &[BTreeMap<String, String>],
    scope: &str,
) -> Vec<Finding> {
    timers
        .iter()
        .filter_map(|timer| {
            let id = timer.get("Id")?;
            let unit = timer.get("Unit")?;
            let service = services.iter().find(|s| s.get("Id") == Some(unit))?;
            let result = service.get("Result").map_or("", String::as_str);
            let triggered = timer
                .get("LastTriggerUSec")
                .is_some_and(|v| !v.is_empty() && v != "n/a" && v != "0");
            (triggered && !result.is_empty() && result != "success").then(|| {
                Finding::new(
                    format!("timer-failed:{scope}:{id}"),
                    Severity::Warning,
                    format!("{id}: last run of {unit} failed"),
                    format!("result {result}; the scheduled job is not doing its work."),
                )
                .inspect(
                    &format!(
                        "journalctl {} {unit} -n 50 --no-pager",
                        if scope == "user" { "--user-unit" } else { "-u" }
                    ),
                    "https://wiki.archlinux.org/title/Systemd/Timers",
                )
            })
        })
        .collect()
}

/// `systemctl` arguments for a scope; the user scope, when running as root
/// for another user, targets that user's manager.
fn systemctl_scope(scope: &str) -> Option<Vec<String>> {
    if scope == "system" {
        return Some(Vec::new());
    }
    if crate::elevation::is_privileged() {
        let uid = owner_uid()?;
        let name = stdout_of("id", &["-nu", &uid.to_string()])?;
        Some(vec![
            "--user".to_owned(),
            format!("--machine={}@.host", name.trim()),
        ])
    } else {
        Some(vec!["--user".to_owned()])
    }
}

fn systemctl_lines(scope_args: &[String], args: &[&str]) -> Option<String> {
    let mut all = scope_args.iter().map(String::as_str).collect::<Vec<_>>();
    all.extend_from_slice(args);
    stdout_of("systemctl", &all)
}

fn show_units(
    scope_args: &[String],
    names: &[String],
    properties: &[&str],
) -> Vec<BTreeMap<String, String>> {
    let mut blocks = Vec::new();
    for chunk in names.chunks(200) {
        let mut args = vec!["show"];
        for property in properties {
            args.push("-p");
            args.push(property);
        }
        args.extend(chunk.iter().map(String::as_str));
        if let Some(text) = systemctl_lines(scope_args, &args) {
            blocks.extend(parse_show(&text));
        }
    }
    blocks
}

fn unit_names(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| name.contains('.'))
        .map(str::to_owned)
        .collect()
}

fn service_health() -> Section {
    let mut section = Section::new("service health / restart loops, failing timers, masked units");
    for scope in ["system", "user"] {
        scope_health(scope, &mut section);
    }
    section
}

/// Unit names from a `systemctl list-*` invocation in a scope.
fn listed(scope_args: &[String], args: &[&str]) -> Vec<String> {
    systemctl_lines(scope_args, args)
        .map(|text| unit_names(&text))
        .unwrap_or_default()
}

fn scope_health(scope: &'static str, section: &mut Section) {
    let Some(scope_args) = systemctl_scope(scope) else {
        section.line(format!("{scope}: manager not reachable"));
        return;
    };
    let services = listed(
        &scope_args,
        &[
            "list-units",
            "--type=service",
            "--all",
            "--no-legend",
            "--plain",
            "--no-pager",
        ],
    );
    let service_props = show_units(
        &scope_args,
        &services,
        &["Id", "NRestarts", "Result", "ActiveState"],
    );
    let restarts = assess_restarts(&service_props, scope, 3);
    let timers = listed(
        &scope_args,
        &[
            "list-units",
            "--type=timer",
            "--all",
            "--no-legend",
            "--plain",
            "--no-pager",
        ],
    );
    let timer_props = show_units(&scope_args, &timers, &["Id", "Unit", "LastTriggerUSec"]);
    let timer_failures = assess_timers(&timer_props, &service_props, scope);
    let failed = listed(
        &scope_args,
        &[
            "list-units",
            "--failed",
            "--no-legend",
            "--plain",
            "--no-pager",
        ],
    );
    let masked = listed(
        &scope_args,
        &[
            "list-unit-files",
            "--state=masked",
            "--no-legend",
            "--plain",
            "--no-pager",
        ],
    );
    section.line(format!(
        "{scope}: {} services, {} timers, {} failed, {} masked, {} restart loops, {} failing timers",
        services.len(),
        timers.len(),
        failed.len(),
        masked.len(),
        restarts.len(),
        timer_failures.len()
    ));
    for name in failed.iter().take(MAX_LISTED) {
        section.line(format!("  failed: {name}"));
    }
    for finding in restarts.iter().chain(&timer_failures) {
        section.line(format!("  {}: {}", finding.severity.label(), finding.title));
    }
    if !masked.is_empty() {
        section.line(format!(
            "  masked: {}",
            masked
                .iter()
                .take(MAX_LISTED)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    section.findings.extend(restarts);
    section.findings.extend(timer_failures);
    section.facts.push((
        if scope == "system" {
            "failed system units"
        } else {
            "failed user units"
        },
        failed,
    ));
    section.facts.push((
        if scope == "system" {
            "masked system units"
        } else {
            "masked user units"
        },
        masked,
    ));
}

// ------------------------------------------------ 5. boot history --------

#[derive(Deserialize)]
struct Boot {
    index: i64,
    #[serde(rename = "boot_id")]
    id: String,
    #[serde(default)]
    first_entry: Option<i64>,
}

/// Parses `journalctl --list-boots -o json`, newest first.
#[must_use]
pub fn parse_boots(json: &str) -> Vec<(i64, String, i64)> {
    let Ok(mut boots) = serde_json::from_str::<Vec<Boot>>(json) else {
        return Vec::new();
    };
    boots.sort_by_key(|boot| std::cmp::Reverse(boot.index));
    boots
        .into_iter()
        .map(|boot| (boot.index, boot.id, boot.first_entry.unwrap_or(0)))
        .collect()
}

/// Whether the tail of a boot's journal shows an orderly shutdown. The
/// persistent journal often closes before journald's own "Journal stopped",
/// so the unit and mount teardown that precedes it counts too; a crash or
/// power loss ends in ordinary activity instead.
#[must_use]
pub fn ended_cleanly(tail: &str) -> bool {
    const MARKERS: [&str; 12] = [
        "Journal stopped",
        "systemd-shutdown",
        "Reached target System Shutdown",
        "Reached target System Power Off",
        "Reached target System Reboot",
        "Reached target System Halt",
        "Reached target Shutdown",
        "Reached target Unmount All Filesystems",
        "Stopped target Multi-User System",
        "Stopped target Graphical Interface",
        "Stopped target Local File Systems",
        "Unmounted /",
    ];
    tail.lines()
        .any(|line| MARKERS.iter().any(|marker| line.contains(marker)))
}

/// Unclean ends among recent boots.
#[must_use]
pub fn assess_boots(unclean: usize, checked: usize) -> Option<Finding> {
    if unclean == 0 {
        return None;
    }
    Some(
        Finding::new(
            "unclean-shutdowns",
            if unclean >= 2 { Severity::Warning } else { Severity::Watch },
            format!("{unclean} of the last {checked} boots ended without a clean shutdown"),
            "Power loss, a hard freeze or a forced power-off; filesystems and databases may have been interrupted.",
        )
        .inspect("journalctl --list-boots; journalctl -b -1 -n 50 --no-pager", "https://wiki.archlinux.org/title/Systemd/Journal"),
    )
}

fn boot_history() -> Section {
    let mut section = Section::new("boot history / unclean shutdowns and error counts");
    let Some(json) = stdout_of("journalctl", &["--list-boots", "-o", "json", "--no-pager"]) else {
        section.line("journal boot list unavailable");
        return section;
    };
    let boots = parse_boots(&json);
    let mut unclean = 0;
    let mut checked = 0;
    for (index, boot_id, first) in boots.iter().take(6) {
        let errors = crate::pathcheck::resolve("journalctl")
            .and_then(|exe| {
                capture::run(
                    &exe,
                    &["-b", boot_id, "-p", "err", "-q", "-o", "cat", "--no-pager"],
                    limits(60, 1024),
                )
                .ok()
            })
            .map_or_else(|| "?".to_owned(), |out| out.stdout.lines.to_string());
        let started = chrono::DateTime::from_timestamp_micros(*first).map_or_else(
            || "?".to_owned(),
            |t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            },
        );
        let ending = if *index == 0 {
            "current".to_owned()
        } else {
            checked += 1;
            let tail = stdout_of(
                "journalctl",
                &["-b", boot_id, "-n", "60", "-o", "cat", "--no-pager"],
            )
            .unwrap_or_default();
            if ended_cleanly(&tail) {
                "clean shutdown".to_owned()
            } else {
                unclean += 1;
                "UNCLEAN END".to_owned()
            }
        };
        section.line(format!(
            "boot {index:>3}  started {started}  errors {errors:>6}  {ending}"
        ));
    }
    section.findings.extend(assess_boots(unclean, checked));
    section
}

// ------------------------------------------------ 6. btrfs health --------

/// Non-zero `btrfs device stats` counters as `(device, counter, value)`.
#[must_use]
pub fn parse_device_stats(text: &str) -> Vec<(String, String, u64)> {
    text.lines()
        .filter_map(|line| {
            let (name, value) = line.rsplit_once(char::is_whitespace)?;
            let value = value.trim().parse::<u64>().ok()?;
            let name = name.trim();
            let (device, counter) = name.strip_prefix('[')?.split_once("].")?;
            (value > 0).then(|| (device.to_owned(), counter.to_owned(), value))
        })
        .collect()
}

/// When the last scrub started, from `btrfs scrub status`; `None` if never.
#[must_use]
pub fn parse_scrub_started(text: &str) -> Option<chrono::NaiveDateTime> {
    let value = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("Scrub started:"))?;
    // "Sun Sep  7 10:00:01 2026": drop the weekday (never trusted) and the
    // padding, then parse the rest.
    let fields = value
        .split_whitespace()
        .skip(1)
        .collect::<Vec<_>>()
        .join(" ");
    chrono::NaiveDateTime::parse_from_str(&fields, "%b %d %H:%M:%S %Y").ok()
}

/// Metadata used/size ratio from `btrfs filesystem usage -b`.
#[must_use]
pub fn parse_metadata_ratio(text: &str) -> Option<f64> {
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("Metadata,"))?;
    let size = number_after(line, "Size:")?;
    let used = number_after(line, "Used:")?;
    #[expect(clippy::cast_precision_loss, reason = "byte counts well below 2^52")]
    (size > 0).then(|| used as f64 / size as f64)
}

fn number_after(line: &str, label: &str) -> Option<u64> {
    let rest = &line[line.find(label)? + label.len()..];
    let digits = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits.parse().ok()
}

/// Judges one btrfs filesystem.
#[must_use]
pub fn assess_btrfs(
    target: &str,
    errors: &[(String, String, u64)],
    scrub_age_days: Option<i64>,
    metadata: Option<f64>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    if !errors.is_empty() {
        let detail = errors
            .iter()
            .map(|(d, c, v)| format!("{d} {c}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        findings.push(
            Finding::new(
                format!("btrfs-errors:{target}"),
                Severity::Urgent,
                format!("btrfs recorded device errors on {target}"),
                detail,
            )
            .inspect(
                &format!("sudo btrfs device stats {target}"),
                "https://wiki.archlinux.org/title/Btrfs#Scrub",
            ),
        );
    }
    match scrub_age_days {
        None => findings.push(
            Finding::new(
                format!("btrfs-scrub:{target}"),
                Severity::Watch,
                format!("{target} has never been scrubbed"),
                "A scrub verifies every checksum and finds silent corruption early.",
            )
            .inspect(
                &format!("sudo btrfs scrub start {target}"),
                "https://wiki.archlinux.org/title/Btrfs#Scrub",
            ),
        ),
        Some(age) if age > 45 => findings.push(
            Finding::new(
                format!("btrfs-scrub:{target}"),
                Severity::Watch,
                format!("{target} last scrubbed {age} days ago"),
                "Monthly scrubs are the usual recommendation.",
            )
            .inspect(
                &format!("sudo btrfs scrub start {target}"),
                "https://wiki.archlinux.org/title/Btrfs#Scrub",
            ),
        ),
        Some(_) => {}
    }
    if let Some(ratio) = metadata.filter(|ratio| *ratio >= 0.9) {
        findings.push(
            Finding::new(
                format!("btrfs-metadata:{target}"),
                Severity::Warning,
                format!("{target} metadata is {:.0}% full", ratio * 100.0),
                "Btrfs can report \"no space left\" while data space remains when metadata fills.",
            )
            .inspect(
                &format!("sudo btrfs filesystem usage {target}"),
                "https://wiki.archlinux.org/title/Btrfs#Balance",
            ),
        );
    }
    findings
}

fn btrfs_health() -> Section {
    let mut section = Section::new("btrfs health / device errors, scrub age, metadata, snapshots");
    let Some(mounts) = stdout_of("findmnt", &["-t", "btrfs", "-rno", "SOURCE,TARGET"]) else {
        section.line("no btrfs filesystems mounted");
        return section;
    };
    let mut seen = Vec::new();
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let (Some(source), Some(target)) = (fields.next(), fields.next()) else {
            continue;
        };
        let device = source.split('[').next().unwrap_or(source).to_owned();
        if seen.contains(&device) {
            continue;
        }
        seen.push(device.clone());
        section.line(format!("{device} (checked at {target})"));
        let Some(stats) = elevated_stdout_of("btrfs", &["device", "stats", target]) else {
            section.line("  device stats, scrub and usage need the elevated audit");
            continue;
        };
        let errors = parse_device_stats(&stats);
        section.line(format!(
            "  device errors: {}",
            if errors.is_empty() {
                "none".to_owned()
            } else {
                format!("{errors:?}")
            }
        ));
        let scrub = elevated_stdout_of("btrfs", &["scrub", "status", target]).unwrap_or_default();
        let age = parse_scrub_started(&scrub)
            .map(|started| (chrono::Local::now().naive_local() - started).num_days());
        section.line(format!(
            "  last scrub: {}",
            age.map_or_else(|| "never".to_owned(), |d| format!("{d} days ago"))
        ));
        let usage =
            elevated_stdout_of("btrfs", &["filesystem", "usage", "-b", target]).unwrap_or_default();
        let metadata = parse_metadata_ratio(&usage);
        section.line(format!(
            "  metadata used: {}",
            metadata.map_or_else(|| "?".to_owned(), |r| format!("{:.1}%", r * 100.0))
        ));
        let snapshots = elevated_stdout_of("btrfs", &["subvolume", "list", "-s", target])
            .map_or(0, |t| t.lines().count());
        section.line(format!("  snapshots: {snapshots}"));
        section
            .findings
            .extend(assess_btrfs(target, &errors, age, metadata));
    }
    section
}

// ------------------------------------------------ 7. drive wear ----------

fn smart_finding(
    device: &str,
    key: &str,
    severity: Severity,
    title: String,
    detail: &str,
) -> Finding {
    Finding::new(format!("{key}:{device}"), severity, title, detail).inspect(
        &format!("sudo smartctl -a {device}"),
        "https://wiki.archlinux.org/title/S.M.A.R.T.",
    )
}

fn assess_nvme(
    device: &str,
    log: &serde_json::Value,
    lines: &mut Vec<String>,
    findings: &mut Vec<Finding>,
) {
    let get = |name: &str| {
        log.get(name)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    let (used, media, unsafe_off, critical) = (
        get("percentage_used"),
        get("media_errors"),
        get("unsafe_shutdowns"),
        get("critical_warning"),
    );
    let (spare, threshold) = (get("available_spare"), get("available_spare_threshold"));
    lines.push(format!(
        "NVMe wear {used}% used, spare {spare}% (threshold {threshold}%), media errors {media}, unsafe shutdowns {unsafe_off}"
    ));
    if critical != 0 {
        findings.push(smart_finding(
            device,
            "nvme-critical",
            Severity::Urgent,
            format!("{device} reports an NVMe critical warning ({critical:#x})"),
            "Back up now and inspect the drive.",
        ));
    }
    if used >= 80 {
        let severity = if used >= 90 {
            Severity::Urgent
        } else {
            Severity::Warning
        };
        findings.push(smart_finding(
            device,
            "nvme-wear",
            severity,
            format!("{device} has used {used}% of its rated endurance"),
            "Plan a replacement.",
        ));
    }
    if media > 0 {
        findings.push(smart_finding(
            device,
            "nvme-media",
            Severity::Warning,
            format!("{device} logged {media} media errors"),
            "Unrecovered data integrity errors occurred.",
        ));
    }
    if threshold > 0 && spare < threshold {
        findings.push(smart_finding(
            device,
            "nvme-spare",
            Severity::Urgent,
            format!("{device} spare capacity is below its threshold"),
            &format!("{spare}% < {threshold}%"),
        ));
    }
}

fn assess_ata(
    device: &str,
    table: &[serde_json::Value],
    lines: &mut Vec<String>,
    findings: &mut Vec<Finding>,
) {
    let attribute = |id: i64| {
        table
            .iter()
            .find(|a| a.get("id").and_then(serde_json::Value::as_i64) == Some(id))
    };
    let raw = |id: i64| {
        attribute(id)
            .and_then(|a| a.pointer("/raw/value"))
            .and_then(serde_json::Value::as_i64)
    };
    let value = |id: i64| {
        attribute(id)
            .and_then(|a| a.get("value"))
            .and_then(serde_json::Value::as_i64)
    };
    let (realloc, pending, uncorrectable) = (
        raw(5).unwrap_or(0),
        raw(197).unwrap_or(0),
        raw(198).unwrap_or(0),
    );
    lines.push(format!(
        "ATA reallocated {realloc}, pending {pending}, offline uncorrectable {uncorrectable}"
    ));
    if pending > 0 || uncorrectable > 0 {
        findings.push(smart_finding(
            device,
            "ata-pending",
            Severity::Urgent,
            format!("{device} has unreadable sectors"),
            &format!("pending {pending}, uncorrectable {uncorrectable}"),
        ));
    }
    if realloc > 0 {
        findings.push(smart_finding(
            device,
            "ata-realloc",
            Severity::Warning,
            format!("{device} has reallocated {realloc} sectors"),
            "The drive is remapping failing areas; watch the trend.",
        ));
    }
    if let Some(life) = [177, 231, 233].into_iter().find_map(value) {
        lines.push(format!("SSD life indicator {life}"));
        if life <= 20 {
            let severity = if life <= 10 {
                Severity::Urgent
            } else {
                Severity::Warning
            };
            findings.push(smart_finding(
                device,
                "ssd-wear",
                severity,
                format!("{device} SSD life indicator is at {life}"),
                "Plan a replacement.",
            ));
        }
    }
}

/// Judges one drive from `smartctl -j -H -A` JSON (`NVMe` or ATA).
#[must_use]
pub fn assess_smart(device: &str, json: &serde_json::Value) -> (Vec<String>, Vec<Finding>) {
    let mut lines = Vec::new();
    let mut findings = Vec::new();
    if let Some(temp) = json
        .pointer("/temperature/current")
        .and_then(serde_json::Value::as_i64)
    {
        lines.push(format!("temperature {temp}°C"));
    }
    if let Some(log) = json.get("nvme_smart_health_information_log") {
        assess_nvme(device, log, &mut lines, &mut findings);
    }
    if let Some(table) = json
        .pointer("/ata_smart_attributes/table")
        .and_then(serde_json::Value::as_array)
    {
        assess_ata(device, table, &mut lines, &mut findings);
    }
    if json.pointer("/smart_status/passed") == Some(&serde_json::Value::Bool(false)) {
        findings.push(smart_finding(
            device,
            "smart-failed",
            Severity::Urgent,
            format!("{device} fails its SMART self-assessment"),
            "Back up now.",
        ));
    }
    (lines, findings)
}

fn drive_wear() -> Section {
    let mut section = Section::new("drive wear and temperature / SMART and NVMe health");
    let Some(disks) = stdout_of("lsblk", &["-dn", "-o", "NAME,TYPE,TRAN"]) else {
        section.line("lsblk unavailable");
        return section;
    };
    for line in disks.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.get(1) != Some(&"disk") || fields.get(2) == Some(&"usb") {
            continue;
        }
        let device = format!("/dev/{}", fields[0]);
        let json = elevated_stdout_of("smartctl", &["-j", "-H", "-A", &device])
            .or_else(|| {
                // smartctl exits non-zero for some warnings but still prints JSON.
                let exe = crate::pathcheck::resolve("smartctl")?;
                let out = capture::run_elevated(
                    &exe,
                    &["-j", "-H", "-A", &device],
                    limits(60, 1024 * 1024),
                )
                .ok()?;
                Some(out.stdout.text())
            })
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
        let Some(json) = json else {
            section.line(format!(
                "{device}: SMART data needs the elevated audit and smartmontools"
            ));
            continue;
        };
        let (lines, findings) = assess_smart(&device, &json);
        section.line(format!(
            "{device}: {}",
            if lines.is_empty() {
                "no SMART data".to_owned()
            } else {
                lines.join("; ")
            }
        ));
        section.findings.extend(findings);
    }
    section
}

// ------------------------------------------------ 8. thermal -------------

/// Judges thermal throttling from summed counters since boot.
#[must_use]
pub fn assess_throttling(package_events: u64, throttled_ms: u64) -> Option<Finding> {
    if package_events == 0 {
        return None;
    }
    let minutes = throttled_ms / 60_000;
    let severity = if throttled_ms >= 30 * 60_000 {
        Severity::Warning
    } else if throttled_ms >= 60_000 {
        Severity::Watch
    } else {
        return None;
    };
    Some(
        Finding::new(
            "thermal-throttling",
            severity,
            format!("CPU throttled for {minutes} min since boot"),
            format!("{package_events} package throttle events: heat is capping performance (dust, fan, paste, or a blocked vent)."),
        )
        .inspect("grep . /sys/devices/system/cpu/cpu*/thermal_throttle/*", "https://wiki.archlinux.org/title/CPU_frequency_scaling"),
    )
}

fn thermal() -> Section {
    let mut section = Section::new("thermal / throttling and zone temperatures");
    let mut package_events = 0;
    let mut package_ms = 0;
    let mut core_events = 0;
    if let Ok(cpus) = fs::read_dir("/sys/devices/system/cpu") {
        for cpu in cpus.filter_map(Result::ok) {
            let dir = cpu.path().join("thermal_throttle");
            if !dir.is_dir() {
                continue;
            }
            // Package counters are per package but mirrored on every CPU.
            package_events =
                package_events.max(read_u64(&dir.join("package_throttle_count")).unwrap_or(0));
            package_ms =
                package_ms.max(read_u64(&dir.join("package_throttle_total_time_ms")).unwrap_or(0));
            core_events += read_u64(&dir.join("core_throttle_count")).unwrap_or(0);
        }
    }
    section.line(format!("package throttle events {package_events} ({package_ms} ms), core throttle events {core_events}"));
    section
        .findings
        .extend(assess_throttling(package_events, package_ms));
    if let Ok(zones) = fs::read_dir("/sys/class/thermal") {
        let mut zones = zones
            .filter_map(Result::ok)
            .map(|z| z.path())
            .filter(|p| p.join("temp").exists())
            .collect::<Vec<_>>();
        zones.sort();
        for zone in zones {
            let kind = read_trimmed(&zone.join("type")).unwrap_or_default();
            if let Some(milli) =
                read_trimmed(&zone.join("temp")).and_then(|t| t.parse::<i64>().ok())
            {
                section.line(format!(
                    "{kind:<18} {}{}.{}°C",
                    if milli < 0 { "-" } else { "" },
                    (milli / 1000).abs(),
                    (milli % 1000).abs() / 100
                ));
            }
        }
    }
    section
}

// ------------------------------------------------ 9. kernel taint --------

/// Decodes `/proc/sys/kernel/tainted` into `(flag letter, meaning)` pairs.
#[must_use]
pub fn decode_taint(value: u64) -> Vec<(char, &'static str)> {
    const FLAGS: [(char, &str); 19] = [
        ('P', "proprietary module loaded"),
        ('F', "module force-loaded"),
        ('S', "kernel running on out-of-spec system"),
        ('R', "module force-unloaded"),
        ('M', "machine check exception (hardware error)"),
        ('B', "bad page referenced (memory corruption)"),
        ('U', "taint requested by user"),
        ('D', "kernel died recently (oops or BUG)"),
        ('A', "ACPI table overridden"),
        ('W', "kernel issued a warning"),
        ('C', "staging driver loaded"),
        ('I', "firmware bug workaround applied"),
        ('O', "out-of-tree module loaded"),
        ('E', "unsigned module loaded"),
        ('L', "soft lockup occurred"),
        ('K', "kernel live-patched"),
        ('X', "auxiliary taint (distribution-defined)"),
        ('T', "built with struct randomization plugin"),
        ('N', "in-kernel test module loaded"),
    ];
    FLAGS
        .iter()
        .enumerate()
        .filter(|(bit, _)| value & (1 << bit) != 0)
        .map(|(_, flag)| *flag)
        .collect()
}

/// Taint flags that indicate a real problem since boot.
#[must_use]
pub fn assess_taint(flags: &[(char, &str)]) -> Option<Finding> {
    let serious = flags
        .iter()
        .filter(|(c, _)| matches!(c, 'D' | 'M' | 'B' | 'L'))
        .collect::<Vec<_>>();
    let warned = flags.iter().any(|(c, _)| *c == 'W');
    let (severity, listed) = if !serious.is_empty() {
        (Severity::Warning, serious)
    } else if warned {
        (
            Severity::Watch,
            flags.iter().filter(|(c, _)| *c == 'W').collect(),
        )
    } else {
        return None;
    };
    Some(
        Finding::new(
            "kernel-taint",
            severity,
            "Kernel is tainted by a runtime problem",
            listed
                .iter()
                .map(|(c, m)| format!("{c}: {m}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
        .inspect(
            "journalctl -k -b -p warning --no-pager",
            "https://docs.kernel.org/admin-guide/tainted-kernels.html",
        ),
    )
}

fn kernel_taint() -> Section {
    let mut section = Section::new("kernel taint");
    let value = read_u64(Path::new("/proc/sys/kernel/tainted")).unwrap_or(0);
    let flags = decode_taint(value);
    section.line(format!("tainted = {value}"));
    if flags.is_empty() {
        section.line("kernel is not tainted");
    }
    for (flag, meaning) in &flags {
        section.line(format!("  {flag}  {meaning}"));
    }
    section.findings.extend(assess_taint(&flags));
    section.facts.push((
        "kernel taint flags",
        flags.iter().map(|(c, m)| format!("{c} {m}")).collect(),
    ));
    section
}

// ------------------------------------------------ 10. battery ------------

/// Health as a percentage of design capacity.
#[must_use]
pub fn battery_health(full: u64, design: u64) -> Option<f64> {
    #[expect(clippy::cast_precision_loss, reason = "capacities are far below 2^52")]
    (design > 0).then(|| full as f64 * 100.0 / design as f64)
}

/// Judges battery wear and its trend (percentage points per 30 days).
#[must_use]
pub fn assess_battery(name: &str, health: f64, per_month: Option<f64>) -> Option<Finding> {
    let severity = if health < 60.0 {
        Severity::Warning
    } else if health < 80.0 || per_month.is_some_and(|rate| rate <= -2.0) {
        Severity::Watch
    } else {
        return None;
    };
    let trend = per_month.map_or_else(
        || "no trend yet".to_owned(),
        |rate| format!("{rate:+.1} points per 30 days"),
    );
    Some(
        Finding::new(
            format!("battery-wear:{name}"),
            severity,
            format!("{name} holds {health:.0}% of its design capacity"),
            format!("Wear trend: {trend}."),
        )
        .inspect(
            &format!("cat /sys/class/power_supply/{name}/uevent"),
            "https://wiki.archlinux.org/title/Laptop#Battery_state",
        ),
    )
}

fn battery() -> Section {
    let mut section = Section::new("battery wear / capacity and trend");
    let Ok(supplies) = fs::read_dir("/sys/class/power_supply") else {
        section.line("no power supply information");
        return section;
    };
    let mut found = false;
    for supply in supplies.filter_map(Result::ok) {
        let dir = supply.path();
        let name = supply.file_name().to_string_lossy().into_owned();
        if read_trimmed(&dir.join("type")).as_deref() != Some("Battery")
            || read_trimmed(&dir.join("scope")).as_deref() == Some("Device")
        {
            continue;
        }
        let pair = |prefix: &str| {
            Some((
                read_u64(&dir.join(format!("{prefix}_full")))?,
                read_u64(&dir.join(format!("{prefix}_full_design")))?,
            ))
        };
        let Some((full, design)) = pair("energy").or_else(|| pair("charge")) else {
            continue;
        };
        let Some(health) = battery_health(full, design) else {
            continue;
        };
        found = true;
        let cycles = read_u64(&dir.join("cycle_count"))
            .filter(|c| *c > 0)
            .map_or_else(|| "not reported".to_owned(), |c| c.to_string());
        let file = history::path("battery");
        let samples = history::record(
            &file,
            &Sample {
                epoch: now_epoch(),
                key: name.clone(),
                value: health,
            },
            43_200,
        )
        .unwrap_or_default();
        let per_month = history::rate_per_day(&samples, &name, 180 * 86_400, 14 * 86_400)
            .map(|rate| rate * 30.0);
        section.line(format!(
            "{name}: {health:.1}% of design ({full} / {design}), cycles {cycles}, trend {}",
            per_month.map_or_else(
                || "collecting (needs 14 days of audits)".to_owned(),
                |r| format!("{r:+.2} points / 30 days")
            )
        ));
        section
            .findings
            .extend(assess_battery(&name, health, per_month));
    }
    if !found {
        section.line("no system battery");
    }
    section
}

// ------------------------------------------------ 11. ecosystem ----------

/// Cybercore security and network tools whose systemd state Omniscient
/// reads: `(display name, unit prefix)`.
pub const ECOSYSTEM: [(&str, &str); 9] = [
    ("SigilWard", "sigilward"),
    ("Undertow", "undertow"),
    ("Argus", "argus"),
    ("SentryGrid", "sentrygrid"),
    ("Chronicle", "chronicle"),
    ("VortexWall", "vortexwall"),
    ("WraithFlow", "wraithflow"),
    ("GhostPort", "ghostport"),
    ("ApexDaemon", "apexdaemon"),
];

/// Judges one ecosystem service from its `systemctl show` properties.
#[must_use]
pub fn assess_ecosystem_unit(
    tool: &str,
    scope: &str,
    unit: &BTreeMap<String, String>,
    recent: &str,
) -> Option<Finding> {
    let id = unit.get("Id")?;
    let result = unit.get("Result").map_or("", String::as_str);
    let active = unit.get("ActiveState").map_or("", String::as_str);
    if result == "success" && active != "failed" {
        return None;
    }
    let status = unit.get("ExecMainStatus").map_or("?", String::as_str);
    let last = recent
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no journal output")
        .trim();
    Some(
        Finding::new(
            format!("ecosystem:{scope}:{id}"),
            Severity::Warning,
            format!("{tool}: {id} failed"),
            format!(
                "result {result}, exit status {status}; last output: {}",
                crate::capture::sanitize(last)
            ),
        )
        .inspect(
            &format!(
                "journalctl {} {id} -n 50 --no-pager",
                if scope == "user" { "--user-unit" } else { "-u" }
            ),
            "https://github.com/cybercore-tech",
        ),
    )
}

fn ecosystem() -> Section {
    let mut section = Section::new("cybercore ecosystem / your security and network tools");
    for (tool, prefix) in ECOSYSTEM {
        let mut lines = Vec::new();
        for scope in ["system", "user"] {
            let Some(scope_args) = systemctl_scope(scope) else {
                continue;
            };
            let pattern = format!("{prefix}*");
            let units = systemctl_lines(
                &scope_args,
                &[
                    "list-unit-files",
                    &pattern,
                    "--no-legend",
                    "--plain",
                    "--no-pager",
                ],
            )
            .map(|t| unit_names(&t))
            .unwrap_or_default();
            let props = show_units(
                &scope_args,
                &units,
                &[
                    "Id",
                    "ActiveState",
                    "Result",
                    "ExecMainStatus",
                    "ExecMainExitTimestamp",
                ],
            );
            for unit in &props {
                let Some(id) = unit.get("Id") else {
                    continue;
                };
                let when = unit
                    .get("ExecMainExitTimestamp")
                    .filter(|v| !v.is_empty())
                    .map_or("—", String::as_str);
                lines.push(format!(
                    "  {scope:<6} {id:<34} {:<9} result {:<10} last exit {when}",
                    unit.get("ActiveState").map_or("?", String::as_str),
                    unit.get("Result").map_or("?", String::as_str),
                ));
                if id.ends_with(".service") {
                    let recent = journal_tail(scope, id);
                    section
                        .findings
                        .extend(assess_ecosystem_unit(tool, scope, unit, &recent));
                }
            }
        }
        if lines.is_empty() {
            section.line(format!("{tool}: not installed"));
        } else {
            section.line(format!("{tool}:"));
            for line in lines {
                section.line(line);
            }
        }
    }
    section.facts.push((
        "failing ecosystem units",
        section.findings.iter().map(|f| f.key.clone()).collect(),
    ));
    section
}

fn journal_tail(scope: &str, unit: &str) -> String {
    let (flag, uid_match);
    let mut args = vec!["-n", "5", "-o", "cat", "--no-pager"];
    if scope == "user" {
        flag = format!("_SYSTEMD_USER_UNIT={unit}");
        uid_match = format!("_UID={}", owner_uid().unwrap_or(0));
        args.push(&flag);
        args.push(&uid_match);
    } else {
        args.push("-u");
        args.push(unit);
    }
    stdout_of("journalctl", &args).unwrap_or_default()
}

// ------------------------------------------------ 12. shell watch --------

/// Resident memory (KiB) from `/proc/<pid>/status`.
#[must_use]
pub fn parse_rss_kib(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kib| kib.parse().ok())
}

/// Judges the desktop shell's memory and its growth rate (KiB per hour).
#[must_use]
pub fn assess_shell(rss_kib: u64, growth_kib_per_hour: Option<f64>) -> Option<Finding> {
    let gib = 1024 * 1024;
    #[expect(
        clippy::cast_precision_loss,
        reason = "memory sizes are far below 2^52"
    )]
    let mib = rss_kib as f64 / 1024.0;
    let severity = if rss_kib >= 3 * gib {
        Severity::Urgent
    } else if rss_kib >= 3 * gib / 2 {
        Severity::Warning
    } else if growth_kib_per_hour.is_some_and(|rate| rate >= 100.0 * 1024.0) {
        Severity::Watch
    } else {
        return None;
    };
    let growth = growth_kib_per_hour.map_or_else(
        || "no trend yet".to_owned(),
        |rate| format!("{:+.0} MiB/hour", rate / 1024.0),
    );
    Some(
        Finding::new(
            "shell-memory",
            severity,
            format!("omarchy-shell is using {mib:.0} MiB"),
            format!("Growth {growth}. A plugin that leaks or re-renders can freeze the desktop; restart the shell and check recently changed plugins."),
        )
        .inspect("omarchy restart shell; journalctl --user -u omarchy-shell -b -p warning", "https://omarchy.org"),
    )
}

fn shell_watch() -> Section {
    let mut section = Section::new("desktop shell watch / omarchy-shell memory and plugin errors");
    let mut shells = Vec::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.filter_map(Result::ok) {
            let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
                continue;
            };
            let cmdline = String::from_utf8_lossy(&cmdline).replace('\0', " ");
            if cmdline.contains("quickshell") && cmdline.contains("omarchy/shell") {
                shells.push(entry.path());
            }
        }
    }
    if shells.is_empty() {
        section.line("omarchy-shell is not running");
    }
    for proc_dir in shells {
        let pid = proc_dir
            .file_name()
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned());
        let Some(rss) = fs::read_to_string(proc_dir.join("status"))
            .ok()
            .as_deref()
            .and_then(parse_rss_kib)
        else {
            continue;
        };
        let samples = history::record(
            &history::path("shell-memory"),
            #[expect(
                clippy::cast_precision_loss,
                reason = "memory sizes are far below 2^52"
            )]
            &Sample {
                epoch: now_epoch(),
                key: pid.clone(),
                value: rss as f64,
            },
            600,
        )
        .unwrap_or_default();
        let growth =
            history::rate_per_day(&samples, &pid, 7 * 86_400, 3_600).map(|per_day| per_day / 24.0);
        section.line(format!(
            "pid {pid}: {} MiB resident, growth {}",
            rss / 1024,
            growth.map_or_else(
                || "collecting (needs an hour of samples)".to_owned(),
                |g| format!("{:+.1} MiB/hour", g / 1024.0)
            )
        ));
        section.findings.extend(assess_shell(rss, growth));
    }
    let uid = owner_uid().unwrap_or(0).to_string();
    let unit_match = "_SYSTEMD_USER_UNIT=omarchy-shell.service".to_owned();
    let uid_match = format!("_UID={uid}");
    if let Some(exe) = crate::pathcheck::resolve("journalctl") {
        if let Ok(out) = capture::run(
            &exe,
            &[
                "-b",
                "-p",
                "warning",
                "-o",
                "cat",
                "--no-pager",
                &unit_match,
                &uid_match,
            ],
            limits(60, 256 * 1024),
        ) {
            let text = out.stdout.text();
            section.line(format!("warnings this boot: {}", out.stdout.lines));
            let plugin_lines = text
                .lines()
                .filter(|line| line.contains("plugins/") || line.to_lowercase().contains("plugin"))
                .collect::<Vec<_>>();
            for line in plugin_lines.iter().rev().take(15).rev() {
                section.line(format!("  {}", crate::capture::sanitize(line)));
            }
        }
    }
    section
}

// ------------------------------------------------ inventory facts --------

/// `ss -Htuln` lines reduced to `proto address:port` identities.
#[must_use]
pub fn parse_listeners(text: &str) -> Vec<String> {
    let mut listeners = text
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            Some(format!("{} {}", fields.first()?, fields.get(4)?))
        })
        .collect::<Vec<_>>();
    listeners.sort();
    listeners.dedup();
    listeners
}

fn inventory() -> Section {
    let mut section = Section::new("inventory / compared with the previous audit");
    let listeners = stdout_of("ss", &["-Htuln"])
        .map(|t| parse_listeners(&t))
        .unwrap_or_default();
    let packages = stdout_of("pacman", &["-Q"])
        .map(|t| t.lines().map(str::to_owned).collect::<Vec<_>>())
        .unwrap_or_default();
    section.line(format!(
        "{} listening sockets, {} installed packages",
        listeners.len(),
        packages.len()
    ));
    section.facts.push(("listening sockets", listeners));
    section.facts.push(("installed packages", packages));
    section
}

/// Writes `signals.md` and `signals.json` into `dir`.
///
/// # Errors
///
/// Returns an error when either file cannot be written.
pub fn write(dir: &Path, sections: &[Section]) -> anyhow::Result<PathBuf> {
    use anyhow::Context as _;
    let generated_at = chrono::Local::now().to_rfc3339();
    let signals = to_signals(sections, &generated_at);
    let json_path = dir.join(SIGNALS_JSON);
    let json = serde_json::to_string_pretty(&signals).context("encoding signals")?;
    fs::write(&json_path, json).with_context(|| format!("writing {}", json_path.display()))?;
    Ok(json_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_orders_and_costs() {
        assert!(
            Severity::Urgent > Severity::Warning
                && Severity::Warning > Severity::Watch
                && Severity::Watch > Severity::Info
        );
        assert_eq!(Severity::Info.penalty(), 0);
        assert!(Severity::Urgent.penalty() > Severity::Warning.penalty());
    }

    #[test]
    fn kernel_upgrade_needs_reboot_only_when_modules_are_gone() {
        assert!(assess_kernel("6.18.49-3-lts", true, &[]).is_none());
        let finding =
            assess_kernel("6.18.49-3-lts", false, &["6.18.50-1-lts".into()]).expect("finding");
        assert_eq!(finding.key, "reboot-needed");
        assert!(finding.detail.contains("6.18.50-1-lts"));
    }

    #[test]
    fn deleted_libraries_are_grouped_per_program() {
        let maps_stale = "7f00-7f01 r-xp 0 08:01 1 /usr/lib/libssl.so.3 (deleted)\n";
        let maps_fresh =
            "7f00-7f01 r-xp 0 08:01 1 /usr/lib/libssl.so.3\n7f02 rw-s 0 0 0 /memfd:x (deleted)\n7f03 r--p 0 08:01 2 /usr/lib/locale/locale-archive (deleted)\n7f04 r-xp 0 08:01 3 /home/u/target/release/app (deleted)\n";
        let procs = vec![
            ("/usr/bin/sshd".to_owned(), maps_stale.to_owned()),
            ("/usr/bin/sshd".to_owned(), maps_stale.to_owned()),
            ("/usr/bin/fresh".to_owned(), maps_fresh.to_owned()),
        ];
        let users = deleted_library_users(&procs);
        assert_eq!(users.get("/usr/bin/sshd"), Some(&2));
        assert!(
            !users.contains_key("/usr/bin/fresh"),
            "memfds, locale archives and a rebuilt binary are not replaced libraries"
        );
    }

    #[test]
    fn pacnew_files_are_found_within_depth() {
        let root = std::env::temp_dir().join(format!("omniscient-pacnew-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b")).expect("dirs");
        fs::write(root.join("pacman.conf.pacnew"), "").expect("file");
        fs::write(root.join("a/b/x.pacsave"), "").expect("file");
        fs::write(root.join("a/b/normal.conf"), "").expect("file");
        assert_eq!(find_pacnew(&root, 6).len(), 2);
        assert_eq!(find_pacnew(&root, 0).len(), 1, "depth limits the walk");
        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn psi_lines_parse_and_judge() {
        let text = "some avg10=0.12 avg60=0.04 avg300=12.50 total=796674723\nfull avg10=0.12 avg60=0.04 avg300=0.00 total=545465486";
        let (some, full) = parse_psi(text);
        assert!((some.expect("some").avg300 - 12.5).abs() < f64::EPSILON);
        assert!(full.expect("full").avg300.abs() < f64::EPSILON);
        assert_eq!(
            assess_pressure("memory", some, full)
                .expect("finding")
                .severity,
            Severity::Warning
        );
        assert!(
            assess_pressure("cpu", some, full).is_none(),
            "cpu thresholds are higher"
        );
        let (idle, _) = parse_psi("some avg10=0.00 avg60=0.00 avg300=0.01 total=1");
        assert!(assess_pressure("io", idle, None).is_none());
        assert!(parse_psi("garbage\nsome avg10=x").0.is_none());
        let (_, stall) = parse_psi(
            "some avg10=1 avg60=1 avg300=1 total=1\nfull avg10=1 avg60=1 avg300=11 total=1",
        );
        assert_eq!(
            assess_pressure("memory", Some(Pressure::default()), stall)
                .expect("full stall")
                .severity,
            Severity::Urgent
        );
    }

    #[test]
    fn coredumps_group_by_program() {
        let json = r#"[{"time":1789898649809660,"pid":1,"sig":7,"exe":"/usr/bin/a"},
                      {"time":1789909642826351,"pid":2,"sig":11,"exe":"/usr/bin/a"},
                      {"time":1789909642826352,"pid":3,"sig":11,"exe":"/usr/bin/a"},
                      {"time":1789909642826353,"pid":4,"sig":6,"exe":"/opt/b"}]"#;
        let grouped = group_coredumps(json);
        assert_eq!(grouped[0].0, "/usr/bin/a");
        assert_eq!(grouped[0].1, 3);
        assert_eq!(grouped[0].3, vec![7, 11]);
        let findings = assess_crashes(&grouped);
        assert_eq!(findings.len(), 1, "a single crash is not a trend");
        assert_eq!(findings[0].severity, Severity::Watch);
        assert!(group_coredumps("not json").is_empty());
    }

    #[test]
    fn systemctl_show_blocks_parse_and_restart_loops_are_found() {
        let text = "Id=wraithflow.service\nResult=success\nNRestarts=0\n\nId=bad.service\nResult=exit-code\nNRestarts=7\n\nId=worse.service\nNRestarts=25\n";
        let units = parse_show(text);
        assert_eq!(units.len(), 3);
        let findings = assess_restarts(&units, "system", 3);
        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings
                .iter()
                .find(|f| f.title.starts_with("worse"))
                .expect("worse")
                .severity,
            Severity::Urgent
        );
    }

    #[test]
    fn failing_timers_are_found() {
        let timers = parse_show("Id=sigilward-check.timer\nUnit=sigilward-check.service\nLastTriggerUSec=Fri 2026-09-25 00:00:06 PDT\n\nId=never.timer\nUnit=never.service\nLastTriggerUSec=n/a\n");
        let services = parse_show(
            "Id=sigilward-check.service\nResult=exit-code\n\nId=never.service\nResult=exit-code\n",
        );
        let findings = assess_timers(&timers, &services, "system");
        assert_eq!(
            findings.len(),
            1,
            "a timer that never fired is not a failing timer"
        );
        assert!(findings[0].title.contains("sigilward-check.timer"));
    }

    #[test]
    fn boots_parse_and_shutdowns_are_recognized() {
        let json = r#"[{"index":-1,"boot_id":"b","first_entry":2},{"index":0,"boot_id":"c","first_entry":3},{"index":-2,"boot_id":"a","first_entry":1}]"#;
        let boots = parse_boots(json);
        assert_eq!(
            boots.iter().map(|b| b.0).collect::<Vec<_>>(),
            vec![0, -1, -2]
        );
        assert!(ended_cleanly("x\nsystemd-journald[1]: Journal stopped\n"));
        // Real endings from the dev laptop: orderly, but the journal closed
        // before journald's final line.
        assert!(ended_cleanly("systemd[1]: boot.mount: Deactivated successfully.\nsystemd[1]: Unmounted /boot.\nsystemd[1]: Unmounted /home/raven/.sysops.\n"));
        assert!(ended_cleanly("systemd[767]: Stopped target Session envelope of hyprland.desktop Wayland compositor.\nsystemd[767]: Reached target Shutdown graphical session units.\n"));
        assert!(ended_cleanly("Reached target System Reboot."));
        assert!(!ended_cleanly("kernel: usb 1-1: new device\nkernel: i915 0000:00:02.0: [drm] GPU HANG\nsystemd[1]: Started foo.service.\n"));
        assert!(assess_boots(0, 5).is_none());
        assert_eq!(
            assess_boots(2, 5).expect("finding").severity,
            Severity::Warning
        );
    }

    #[test]
    fn btrfs_outputs_parse() {
        let stats = "[/dev/mapper/cryptroot].write_io_errs    0\n[/dev/mapper/cryptroot].corruption_errs  3\n[/dev/mapper/cryptroot].generation_errs  0\n";
        assert_eq!(
            parse_device_stats(stats),
            vec![("/dev/mapper/cryptroot".into(), "corruption_errs".into(), 3)]
        );
        let scrub = "UUID:             x\nScrub started:    Mon Sep  7 10:00:01 2026\nStatus:           finished\n";
        let started = parse_scrub_started(scrub).expect("scrub date");
        assert_eq!(started.to_string(), "2026-09-07 10:00:01");
        assert!(
            parse_scrub_started("Scrub started:    Sun Sep  7 10:00:01 2026").is_some(),
            "the weekday is ignored"
        );
        assert!(parse_scrub_started("UUID: x\n\tno stats available\n").is_none());
        let usage = "Overall:\n    Device size:  100\nMetadata,DUP: Size:1073741824, Used:1020054732 (95.00%)\n";
        assert!((parse_metadata_ratio(usage).expect("ratio") - 0.95).abs() < 0.001);
        let findings = assess_btrfs("/", &parse_device_stats(stats), None, Some(0.95));
        let severities = findings.iter().map(|f| f.severity).collect::<Vec<_>>();
        assert_eq!(
            severities,
            vec![Severity::Urgent, Severity::Watch, Severity::Warning]
        );
        assert!(assess_btrfs("/", &[], Some(10), Some(0.5)).is_empty());
    }

    #[test]
    fn smart_json_is_judged_for_nvme_and_ata() {
        let nvme: serde_json::Value = serde_json::json!({
            "temperature": {"current": 41},
            "nvme_smart_health_information_log": {"critical_warning": 0, "percentage_used": 85, "media_errors": 2,
                "unsafe_shutdowns": 9, "available_spare": 100, "available_spare_threshold": 10}
        });
        let (lines, findings) = assess_smart("/dev/nvme0n1", &nvme);
        assert!(lines[0].contains("41°C"));
        let keys = findings.iter().map(|f| f.key.as_str()).collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec!["nvme-wear:/dev/nvme0n1", "nvme-media:/dev/nvme0n1"]
        );
        let ata: serde_json::Value = serde_json::json!({
            "smart_status": {"passed": true},
            "ata_smart_attributes": {"table": [
                {"id": 5, "value": 100, "raw": {"value": 0}},
                {"id": 197, "value": 100, "raw": {"value": 1}},
                {"id": 177, "value": 15, "raw": {"value": 0}}
            ]}
        });
        let (_, findings) = assess_smart("/dev/sda", &ata);
        let keys = findings.iter().map(|f| f.key.as_str()).collect::<Vec<_>>();
        assert_eq!(keys, vec!["ata-pending:/dev/sda", "ssd-wear:/dev/sda"]);
        let (_, healthy) = assess_smart(
            "/dev/sda",
            &serde_json::json!({"smart_status": {"passed": true}}),
        );
        assert!(healthy.is_empty());
    }

    #[test]
    fn throttling_is_judged_by_time_not_event_count() {
        assert!(assess_throttling(0, 0).is_none());
        assert!(
            assess_throttling(500, 10_000).is_none(),
            "brief throttling is normal"
        );
        assert_eq!(
            assess_throttling(5, 120_000).expect("watch").severity,
            Severity::Watch
        );
        assert_eq!(
            assess_throttling(5, 40 * 60_000).expect("warning").severity,
            Severity::Warning
        );
    }

    #[test]
    fn taint_decodes_this_machines_value() {
        // 12292 = S (bit 2) + O (bit 12) + E (bit 13), read on the dev laptop.
        let flags = decode_taint(12_292)
            .into_iter()
            .map(|(c, _)| c)
            .collect::<String>();
        assert_eq!(flags, "SOE");
        assert!(
            assess_taint(&decode_taint(12_292)).is_none(),
            "module provenance alone is not a problem"
        );
        assert_eq!(
            assess_taint(&decode_taint(1 << 9)).expect("W").severity,
            Severity::Watch
        );
        assert_eq!(
            assess_taint(&decode_taint((1 << 7) | (1 << 9)))
                .expect("D")
                .severity,
            Severity::Warning
        );
    }

    #[test]
    fn battery_wear_is_judged_with_trend() {
        let health = battery_health(22_190_000, 30_000_000).expect("health");
        assert!((health - 73.97).abs() < 0.01);
        assert_eq!(
            assess_battery("BAT0", health, None)
                .expect("watch")
                .severity,
            Severity::Watch
        );
        assert!(assess_battery("BAT0", 95.0, Some(-0.5)).is_none());
        assert_eq!(
            assess_battery("BAT0", 95.0, Some(-3.0))
                .expect("fast wear")
                .severity,
            Severity::Watch
        );
        assert_eq!(
            assess_battery("BAT0", 55.0, None)
                .expect("warning")
                .severity,
            Severity::Warning
        );
        assert!(battery_health(1, 0).is_none());
    }

    #[test]
    fn ecosystem_failures_become_findings() {
        let failed = parse_show(
            "Id=sigilward-check.service\nActiveState=failed\nResult=exit-code\nExecMainStatus=1\n",
        )
        .remove(0);
        let finding = assess_ecosystem_unit(
            "SigilWard",
            "system",
            &failed,
            "checking\n3 files changed\n",
        )
        .expect("finding");
        assert!(
            finding.detail.contains("3 files changed") && finding.detail.contains("exit status 1")
        );
        let ok = parse_show("Id=argus.service\nActiveState=active\nResult=success\n").remove(0);
        assert!(assess_ecosystem_unit("Argus", "system", &ok, "").is_none());
    }

    #[test]
    fn shell_memory_is_judged() {
        assert_eq!(
            parse_rss_kib("Name:\tquickshell\nVmRSS:\t  428064 kB\n"),
            Some(428_064)
        );
        assert!(assess_shell(428_064, None).is_none());
        assert_eq!(
            assess_shell(11 * 1024 * 1024, None)
                .expect("urgent")
                .severity,
            Severity::Urgent
        );
        assert_eq!(
            assess_shell(400 * 1024, Some(200.0 * 1024.0))
                .expect("growth")
                .severity,
            Severity::Watch
        );
    }

    #[test]
    fn listeners_reduce_to_identities() {
        let text = "tcp LISTEN 0 128 127.0.0.1:631 0.0.0.0:*\nudp UNCONN 0 0 0.0.0.0:5353 0.0.0.0:*\ntcp LISTEN 0 128 127.0.0.1:631 0.0.0.0:*\n";
        assert_eq!(
            parse_listeners(text),
            vec!["tcp 127.0.0.1:631", "udp 0.0.0.0:5353"]
        );
    }

    #[test]
    fn status_uid_parses() {
        assert_eq!(
            parse_status_uid("Name:\tx\nUid:\t1000\t1000\t1000\t1000\n"),
            Some(1000)
        );
    }

    #[test]
    fn signals_flatten_sort_and_dedupe_facts() {
        let mut a = Section::new("a");
        a.findings
            .push(Finding::new("k1", Severity::Watch, "t", "d"));
        a.facts.push(("set", vec!["b".into(), "a".into()]));
        let mut b = Section::new("b");
        b.findings
            .push(Finding::new("k2", Severity::Urgent, "t", "d"));
        b.facts.push(("set", vec!["a".into()]));
        let signals = to_signals(&[a, b], "now");
        assert_eq!(signals.findings[0].key, "k2", "most serious first");
        assert_eq!(signals.facts["set"], vec!["a", "b"]);
    }

    #[test]
    #[ignore = "host timing probe"]
    fn real_section_timings() {
        type Check = fn() -> Section;
        let checks: [(&str, Check); 13] = [
            ("update_hygiene", update_hygiene),
            ("pressure", pressure),
            ("crash_trends", crash_trends),
            ("service_health", service_health),
            ("boot_history", boot_history),
            ("btrfs_health", btrfs_health),
            ("drive_wear", drive_wear),
            ("thermal", thermal),
            ("kernel_taint", kernel_taint),
            ("battery", battery),
            ("ecosystem", ecosystem),
            ("shell_watch", shell_watch),
            ("inventory", inventory),
        ];
        for (name, check) in checks {
            let started = std::time::Instant::now();
            let _ = check();
            eprintln!("TIMING {name}: {:?}", started.elapsed());
        }
    }
}
