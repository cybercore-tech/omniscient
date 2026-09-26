use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// One audit module. Each knows its own display name, the slug used
/// for directory naming, the tools it depends on (for the capability
/// matrix and health score), and how to actually run itself.
///
/// This replaces the fish version's triple-duplicated switch/case
/// dispatch (menu labels, full-audit loop, single-module loop all
/// repeated the same 9-way match by hand) with one list, defined once.
pub trait AuditModule {
    fn name(&self) -> &'static str;
    fn slug(&self) -> &'static str;
    /// The Markdown filename emitted inside this module's report directory.
    /// Most modules use their slug, but a legacy report such as Storage Matrix
    /// has a user-facing filename that differs from its directory slug.
    fn report_filename(&self) -> String {
        format!("{}.md", self.slug())
    }
    fn menu_label(&self) -> &'static str;
    /// Tools this module relies on — used for the capability matrix
    /// and the health score, not just duplicated as a magic list.
    fn tools(&self) -> &'static [&'static str];
    /// Tools that improve coverage when installed but should not lower the
    /// health score when intentionally absent on this system.
    fn optional_tools(&self) -> &'static [&'static str] {
        &[]
    }
    fn requires_sudo(&self) -> bool {
        false
    }
    /// Runs the module and writes its report into `dir`.
    ///
    /// # Errors
    ///
    /// Returns an error when the report cannot be written.
    fn run(&self, dir: &Path) -> Result<()>;
}

/// Runs a command, returning its bounded, cleaned stdout (+ a note if it
/// failed, timed out, or wasn't found) rather than silently swallowing
/// errors the way `2>/dev/null` did in the fish version.
fn capture(cmd: &str, args: &[&str]) -> String {
    capture_with(cmd, args, crate::capture::Limits::default())
}

fn capture_with(cmd: &str, args: &[&str], limits: crate::capture::Limits) -> String {
    let Some(executable) = crate::pathcheck::resolve(cmd) else {
        return format!("_{cmd}: not installed, skipped_\n");
    };
    match crate::capture::run(&executable, args, limits) {
        Ok(out) => {
            let mut s = crate::capture::bound_text(
                &out.stdout.text(),
                out.stdout.dropped(),
                crate::capture::MAX_SECTION_BYTES,
                crate::capture::MAX_SECTION_LINES,
            );
            if out.timed_out {
                let _ = write!(
                    s,
                    "\n_[{cmd} timed out after {}s and was stopped]_\n",
                    limits.timeout.as_secs()
                );
            } else if !out.success() {
                let status = out.status.map_or_else(
                    || "an unknown status".to_owned(),
                    |status| status.to_string(),
                );
                let _ = write!(s, "\n_[{cmd} exited with {status}]_\n");
                let err = crate::capture::bound_text(&out.stderr.text(), 0, 16 * 1024, 200);
                if !err.trim().is_empty() {
                    let _ = writeln!(s, "{}", err.trim());
                }
            }
            if s.trim().is_empty() {
                s = format!("_{cmd} returned no data._\n");
            }
            s
        }
        Err(e) => format!("_{cmd}: failed to run ({e})_\n"),
    }
}

fn capture_privileged(command: &str, args: &[&str]) -> String {
    let Some(executable) = crate::pathcheck::resolve(command) else {
        return format!("_{command}: not installed, skipped_\n");
    };
    let executable = executable.to_string_lossy().into_owned();
    if crate::elevation::is_privileged() {
        return capture(&executable, args);
    }
    let elevated_args = crate::elevation::args(&executable, args);
    capture(crate::elevation::program(), &elevated_args)
}

/// Most bytes of one module report. Sections are already bounded by
/// [`crate::capture::MAX_SECTION_BYTES`]; this caps a report with many of them.
pub const MAX_REPORT_BYTES: usize = 2 * 1024 * 1024;

fn write_report<S: AsRef<str>>(
    dir: &Path,
    filename: &str,
    title: &str,
    sections: &[(S, String)],
) -> Result<()> {
    let path = dir.join(filename);
    let mut f = File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    f.write_all(render_report(title, sections).as_bytes())?;
    Ok(())
}

/// Renders a module report, bounding every section and the whole report so
/// no report can grow past [`MAX_REPORT_BYTES`] (plus its headings).
fn render_report<S: AsRef<str>>(title: &str, sections: &[(S, String)]) -> String {
    let mut out = format!("# {} {title}\n\n", report_emoji(title));
    let mut budget = MAX_REPORT_BYTES;
    for (heading, body) in sections {
        let heading = crate::capture::sanitize(heading.as_ref()).replace('\n', " ");
        let _ = write!(out, "## {heading}\n\n");
        if budget == 0 {
            out.push_str("_omitted: the report reached its size limit_\n\n");
            continue;
        }
        let body = crate::capture::bound_text(
            body.trim_end(),
            0,
            budget.min(crate::capture::MAX_SECTION_BYTES),
            crate::capture::MAX_SECTION_LINES,
        );
        budget = budget.saturating_sub(body.len());
        let _ = write!(out, "```\n{}\n```\n\n", body.trim_end());
    }
    out
}

fn report_emoji(title: &str) -> &'static str {
    match title {
        "HARDWARE CORE" | "OMARCHY SURFACE" => "🖥️",
        "STORAGE MATRIX" => "💾",
        "BTRFS SNAPSHOTS" => "📸",
        "NETWORK" => "🌐",
        "CONTAINERS" => "📦",
        "SERVICES" => "⚙️",
        "LOGS" => "📜",
        "BLUETOOTH" => "📡",
        "DEVICES" => "🔌",
        "SECURITY POSTURE" => "🛡️",
        "ACCOUNTS & AUTH" => "🔐",
        "PERSISTENCE WATCH" => "🧬",
        "PACKAGE INTEGRITY" => "🧾",
        "RECOVERY READINESS" => "🧰",
        "RELIABILITY SIGNALS" => "📈",
        "PERFORMANCE PULSE" => "⚡",
        _ => "🛰️",
    }
}

pub struct Hardware;
impl AuditModule for Hardware {
    fn name(&self) -> &'static str {
        "Hardware Core"
    }
    fn slug(&self) -> &'static str {
        "hardware"
    }
    fn menu_label(&self) -> &'static str {
        "🖥️  Hardware Core"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["lscpu", "lshw", "lsusb", "lspci"]
    }
    fn requires_sudo(&self) -> bool {
        true
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "hardware.md",
            "HARDWARE CORE",
            &[
                ("lscpu", capture("lscpu", &[])),
                ("lshw -short", capture_privileged("lshw", &["-short"])),
                ("lsusb", capture("lsusb", &[])),
                ("lspci", capture("lspci", &[])),
            ],
        )
    }
}

pub struct Disks;
impl AuditModule for Disks {
    fn name(&self) -> &'static str {
        "Storage Matrix"
    }
    fn slug(&self) -> &'static str {
        "disks"
    }
    fn report_filename(&self) -> String {
        "storage.md".to_string()
    }
    fn menu_label(&self) -> &'static str {
        "💾 Storage Matrix"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["lsblk", "smartctl"]
    }
    fn requires_sudo(&self) -> bool {
        true
    }
    fn run(&self, dir: &Path) -> Result<()> {
        let mut sections: Vec<(String, String)> = vec![(
            "lsblk".to_string(),
            capture("lsblk", &["-o", "NAME,SIZE,MODEL,TYPE,MOUNTPOINT"]),
        )];

        // Per-disk SMART info, same as the fish version's loop, but
        // each disk gets its own labeled section instead of one
        // undifferentiated blob.
        let disk_list = capture("lsblk", &["-dn", "-o", "NAME,TYPE"]);
        for line in disk_list.lines() {
            let mut fields = line.split_whitespace();
            let Some(disk) = fields.next() else {
                continue;
            };
            if fields.next() != Some("disk") {
                continue;
            }
            let dev = format!("/dev/{disk}");
            let smart = capture_privileged("smartctl", &["-i", &dev]);
            sections.push((format!("smartctl: {dev}"), smart));
        }

        write_report(dir, "storage.md", "STORAGE MATRIX", &sections)
    }
}

pub struct Snapshots;
impl AuditModule for Snapshots {
    fn name(&self) -> &'static str {
        "Btrfs Snapshots"
    }
    fn slug(&self) -> &'static str {
        "snapshots"
    }
    fn menu_label(&self) -> &'static str {
        "📸 Btrfs Snapshots"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["findmnt", "btrfs"]
    }
    fn requires_sudo(&self) -> bool {
        true
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "snapshots.md",
            "BTRFS SNAPSHOTS",
            &[
                ("findmnt -t btrfs", capture("findmnt", &["-t", "btrfs"])),
                (
                    "btrfs subvolume list /",
                    capture_privileged("btrfs", &["subvolume", "list", "/"]),
                ),
            ],
        )
    }
}

pub struct Network;
impl AuditModule for Network {
    fn name(&self) -> &'static str {
        "Network Nexus"
    }
    fn slug(&self) -> &'static str {
        "network"
    }
    fn menu_label(&self) -> &'static str {
        "🌐 Network Nexus"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["ip", "ss", "resolvectl", "networkctl", "nmcli"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        self.tools()
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "network.md",
            "NETWORK",
            &[
                ("ip a", capture("ip", &["a"])),
                ("ip route", capture("ip", &["route"])),
                ("ss -tulanp", capture("ss", &["-tulanp"])),
                ("resolvectl status", capture("resolvectl", &["status"])),
                ("networkctl list", capture("networkctl", &["list"])),
                (
                    "nmcli general status",
                    capture("nmcli", &["general", "status"]),
                ),
            ],
        )
    }
}

pub struct Containers;
impl AuditModule for Containers {
    fn name(&self) -> &'static str {
        "Container Realm"
    }
    fn slug(&self) -> &'static str {
        "containers"
    }
    fn menu_label(&self) -> &'static str {
        "📦 Container Realm"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["docker", "flatpak", "snap"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        &["flatpak", "snap"]
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "containers.md",
            "CONTAINERS",
            &[
                ("docker ps -a", capture("docker", &["ps", "-a"])),
                (
                    "docker images",
                    capture(
                        "docker",
                        &[
                            "images",
                            "--format",
                            "table {{.Repository}}\\t{{.Tag}}\\t{{.Size}}",
                        ],
                    ),
                ),
                ("docker system df", capture("docker", &["system", "df"])),
                ("flatpak list", capture("flatpak", &["list"])),
                ("flatpak apps", capture("flatpak", &["list", "--app"])),
                ("snap list", capture("snap", &["list"])),
            ],
        )
    }
}

pub struct Services;
impl AuditModule for Services {
    fn name(&self) -> &'static str {
        "Services & Daemons"
    }
    fn slug(&self) -> &'static str {
        "services"
    }
    fn menu_label(&self) -> &'static str {
        "⚙️  Services & Daemons"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["systemctl"]
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "services.md",
            "SERVICES",
            &[
                (
                    "running services",
                    capture(
                        "systemctl",
                        &["list-units", "--type=service", "--state=running"],
                    ),
                ),
                (
                    "failed units",
                    capture("systemctl", &["list-units", "--failed"]),
                ),
            ],
        )
    }
}

pub struct Logs;
impl AuditModule for Logs {
    fn name(&self) -> &'static str {
        "Kernel Logs"
    }
    fn slug(&self) -> &'static str {
        "logs"
    }
    fn menu_label(&self) -> &'static str {
        "📜 Kernel Logs"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["journalctl", "dmesg"]
    }
    fn requires_sudo(&self) -> bool {
        true
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "logs.md",
            "LOGS",
            &[
                ("journalctl -n 50", capture("journalctl", &["-n", "50"])),
                ("dmesg (last 50)", tail_dmesg()),
            ],
        )
    }
}

fn tail_dmesg() -> String {
    let full = capture_privileged("dmesg", &[]);
    full.lines()
        .rev()
        .take(50)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

pub struct Bluetooth;
impl AuditModule for Bluetooth {
    fn name(&self) -> &'static str {
        "Bluetooth Deep"
    }
    fn slug(&self) -> &'static str {
        "bluetooth"
    }
    fn menu_label(&self) -> &'static str {
        "📡 Bluetooth Deep"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["bluetoothctl"]
    }
    fn run(&self, dir: &Path) -> Result<()> {
        let bluetooth = bluetooth_report();
        write_report(
            dir,
            "bluetooth.md",
            "BLUETOOTH",
            &[("bluetoothctl devices", bluetooth)],
        )
    }
}

fn bluetooth_report() -> String {
    if !crate::pathcheck::exists("bluetoothctl") {
        return "_bluetoothctl: not installed, skipped_\n".to_string();
    }

    // bluetoothctl can abort inside libdbus when no system bus is available.
    // A service-state check turns that environment condition into a useful
    // report instead of allowing a child crash/core dump to look like an
    // empty successful scan.
    let service = crate::pathcheck::resolve("systemctl").and_then(|systemctl| {
        Command::new(systemctl)
            .args(["is-active", "--quiet", "bluetooth"])
            .status()
            .ok()
    });
    if !matches!(service, Some(status) if status.success()) {
        return "_Bluetooth service is inactive or unavailable; no device scan was attempted._\n"
            .to_string();
    }

    let controller = capture("bluetoothctl", &["--timeout", "5", "show"]);
    let devices = capture("bluetoothctl", &["--timeout", "5", "devices"]);
    let devices = if devices.contains("bluetoothctl returned no data") {
        "No paired or known devices were returned. The controller is available; use a deliberate scan when you want to discover nearby devices.".to_string()
    } else {
        devices
    };
    format!(
        "CONTROLLER STATUS / bluetoothctl show\n{controller}\n\nKNOWN DEVICES / bluetoothctl devices\n{devices}"
    )
}

pub struct ConnectedDevices;
impl AuditModule for ConnectedDevices {
    fn name(&self) -> &'static str {
        "Connected Devices"
    }
    fn slug(&self) -> &'static str {
        "devices"
    }
    fn menu_label(&self) -> &'static str {
        "🔌 Connected Devices"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["hyprctl", "wlr-randr", "xrandr", "aplay"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        &["hyprctl", "wlr-randr", "xrandr"]
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "devices.md",
            "DEVICES",
            &[
                ("display outputs", display_report()),
                ("aplay -l", capture("aplay", &["-l"])),
            ],
        )
    }
}

fn display_report() -> String {
    if crate::pathcheck::exists("hyprctl") {
        capture("hyprctl", &["monitors", "all"])
    } else if crate::pathcheck::exists("wlr-randr") {
        capture("wlr-randr", &[])
    } else {
        capture("xrandr", &[])
    }
}

pub struct SecurityPosture;
impl AuditModule for SecurityPosture {
    fn name(&self) -> &'static str {
        "Security Posture"
    }
    fn slug(&self) -> &'static str {
        "security"
    }
    fn menu_label(&self) -> &'static str {
        "🛡️  Security Posture"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["ss", "ufw", "nft", "mokutil", "sysctl", "systemctl"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        self.tools()
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "security.md",
            "SECURITY POSTURE",
            &[
                ("ss -tulpen", capture("ss", &["-tulpen"])),
                ("ufw status verbose", capture("ufw", &["status", "verbose"])),
                ("nft list ruleset", capture("nft", &["list", "ruleset"])),
                ("mokutil --sb-state", capture("mokutil", &["--sb-state"])),
                (
                    "kernel.kptr_restrict",
                    capture("sysctl", &["kernel.kptr_restrict"]),
                ),
                (
                    "kernel.dmesg_restrict",
                    capture("sysctl", &["kernel.dmesg_restrict"]),
                ),
                (
                    "kernel.yama.ptrace_scope",
                    capture("sysctl", &["kernel.yama.ptrace_scope"]),
                ),
                (
                    "net forwarding",
                    capture(
                        "sysctl",
                        &["net.ipv4.ip_forward", "net.ipv6.conf.all.forwarding"],
                    ),
                ),
                (
                    "systemctl --failed",
                    capture("systemctl", &["--failed", "--no-pager"]),
                ),
            ],
        )
    }
}

pub struct Accounts;
impl AuditModule for Accounts {
    fn name(&self) -> &'static str {
        "Accounts & Authentication"
    }
    fn slug(&self) -> &'static str {
        "accounts"
    }
    fn menu_label(&self) -> &'static str {
        "🔐 Accounts & Auth"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["getent", "lastb", "loginctl", "sshd"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        self.tools()
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "accounts.md",
            "ACCOUNTS & AUTH",
            &[
                ("getent passwd", capture("getent", &["passwd"])),
                ("getent group wheel", capture("getent", &["group", "wheel"])),
                ("lastb -n 25", capture("lastb", &["-n", "25"])),
                (
                    "loginctl list-sessions",
                    capture("loginctl", &["list-sessions"]),
                ),
                ("sshd -T", capture("sshd", &["-T"])),
            ],
        )
    }
}

fn home_path(path: &str) -> String {
    std::env::var("HOME").map_or_else(|_| path.to_string(), |home| format!("{home}/{path}"))
}

pub struct Persistence;
impl AuditModule for Persistence {
    fn name(&self) -> &'static str {
        "Persistence Watch"
    }
    fn slug(&self) -> &'static str {
        "persistence"
    }
    fn menu_label(&self) -> &'static str {
        "🧬 Persistence Watch"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["systemctl", "crontab", "find"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        self.tools()
    }
    fn run(&self, dir: &Path) -> Result<()> {
        let user_autostart = home_path(".config/autostart");
        let user_cron = capture("crontab", &["-l"]);
        let user_autostart_report = capture(
            "find",
            &[
                user_autostart.as_str(),
                "-maxdepth",
                "1",
                "-type",
                "f",
                "-print",
            ],
        );
        write_report(
            dir,
            "persistence.md",
            "PERSISTENCE WATCH",
            &[
                (
                    "system enabled units",
                    capture(
                        "systemctl",
                        &["list-unit-files", "--state=enabled", "--no-pager"],
                    ),
                ),
                (
                    "system timers",
                    capture("systemctl", &["list-timers", "--all", "--no-pager"]),
                ),
                (
                    "user enabled units",
                    capture(
                        "systemctl",
                        &["--user", "list-unit-files", "--state=enabled", "--no-pager"],
                    ),
                ),
                ("user crontab", user_cron),
                ("user autostart", user_autostart_report),
                (
                    "system autostart",
                    capture(
                        "find",
                        &[
                            "/etc/xdg/autostart",
                            "-maxdepth",
                            "1",
                            "-type",
                            "f",
                            "-print",
                        ],
                    ),
                ),
            ],
        )
    }
}

pub struct PackageIntegrity;
impl AuditModule for PackageIntegrity {
    fn name(&self) -> &'static str {
        "Package Integrity"
    }
    fn slug(&self) -> &'static str {
        "packages"
    }
    fn menu_label(&self) -> &'static str {
        "🧾 Package Integrity"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["pacman", "flatpak", "snap"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        self.tools()
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "packages.md",
            "PACKAGE INTEGRITY",
            &[
                (
                    "installed packages / repository categories / versions",
                    package_repository_report(),
                ),
                ("update status", pacman_updates_report()),
                ("explicitly installed packages", capture("pacman", &["-Qe"])),
                ("orphan candidates", capture("pacman", &["-Qdtq"])),
                (
                    "foreign packages / AUR candidates",
                    capture("pacman", &["-Qm"]),
                ),
                (
                    "package file integrity",
                    capture_with(
                        "pacman",
                        &["-Qkk"],
                        crate::capture::Limits {
                            timeout: std::time::Duration::from_mins(15),
                            ..crate::capture::Limits::default()
                        },
                    ),
                ),
                (
                    "flatpak applications / versions",
                    capture(
                        "flatpak",
                        &[
                            "list",
                            "--app",
                            "--columns=application,version,branch,origin",
                        ],
                    ),
                ),
                ("snap packages / versions", capture("snap", &["list"])),
                (
                    "omarchy package surface",
                    capture("pacman", &["-Qs", "omarchy"]),
                ),
            ],
        )
    }
}

/// Full stdout of a command whose output is parsed, not reported. Output
/// past 16 MiB (far beyond any package inventory) is treated as a failure
/// rather than parsed partially.
fn command_stdout(command: &str, args: &[&str]) -> Option<String> {
    let executable = crate::pathcheck::resolve(command)?;
    let limits = crate::capture::Limits {
        retain_bytes: 16 * 1024 * 1024,
        ..crate::capture::Limits::default()
    };
    let output = crate::capture::run(&executable, args, limits).ok()?;
    if !output.success() || output.stdout.truncated() {
        return None;
    }
    Some(output.stdout.text())
}

fn pacman_updates_report() -> String {
    let Some(executable) = crate::pathcheck::resolve("pacman") else {
        return "_pacman: not installed, skipped_\n".to_string();
    };
    match crate::capture::run(&executable, &["-Qu"], crate::capture::Limits::default()) {
        Ok(output) => {
            let stdout = output.stdout.text();
            let stderr = output.stderr.text();
            if !output.timed_out
                && output.status.and_then(|status| status.code()) == Some(1)
                && stdout.trim().is_empty()
                && stderr.trim().is_empty()
            {
                return "No package updates are currently reported by pacman.\n".to_string();
            }

            let mut report = crate::capture::bound_text(
                &stdout,
                output.stdout.dropped(),
                crate::capture::MAX_SECTION_BYTES,
                crate::capture::MAX_SECTION_LINES,
            );
            if output.timed_out {
                report.push_str("\n_[pacman timed out and was stopped]_\n");
            } else if !output.success() {
                let status = output.status.map_or_else(
                    || "an unknown status".to_owned(),
                    |status| status.to_string(),
                );
                let _ = write!(report, "\n_[pacman exited with {status}]_\n");
                let err = crate::capture::bound_text(&stderr, 0, 16 * 1024, 200);
                if !err.trim().is_empty() {
                    let _ = writeln!(report, "{}", err.trim());
                }
            }
            if report.trim().is_empty() {
                "_pacman returned no update data._\n".clone_into(&mut report);
            }
            report
        }
        Err(error) => format!("_pacman: failed to run ({error})_\n"),
    }
}

fn package_repository_report() -> String {
    let Some(installed_output) = command_stdout("pacman", &["-Q"]) else {
        return "Package inventory is unavailable; pacman did not return installed package data.\n"
            .to_string();
    };

    let mut installed = BTreeMap::new();
    for line in installed_output.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else {
            continue;
        };
        let Some(version) = fields.next() else {
            continue;
        };
        installed.insert(name.to_string(), version.to_string());
    }

    let mut repositories = BTreeMap::new();
    if let Some(repo_output) = command_stdout("pacman", &["-Sl"]) {
        for line in repo_output.lines() {
            let mut fields = line.split_whitespace();
            let Some(repository) = fields.next() else {
                continue;
            };
            let Some(name) = fields.next() else {
                continue;
            };
            if !installed.contains_key(name) {
                continue;
            }

            let category = package_repository_category(repository);
            let replace = repositories.get(name).is_none_or(|existing: &String| {
                package_repository_priority(existing) < package_repository_priority(&category)
            });
            if replace {
                repositories.insert(name.to_string(), category);
            }
        }
    }

    let updates = pacman_update_names();
    let mut categories: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, version) in installed {
        let category = repositories
            .get(&name)
            .cloned()
            .unwrap_or_else(|| "AUR / FOREIGN".to_string());
        let status = if updates.contains(&name) {
            "UPDATE AVAILABLE"
        } else {
            "CURRENT"
        };
        categories
            .entry(category)
            .or_default()
            .push(format!("{name} {version} — {status}"));
    }

    let mut report = String::new();
    report.push_str("Repository origin is inferred from pacman's local sync metadata; pacman marks unlisted packages as foreign, which can include AUR or locally built packages.\n\n");
    for category in [
        "ARCH OFFICIAL",
        "OMARCHY",
        "BLACKARCH",
        "CHAOTIC AUR",
        "AUR / FOREIGN",
    ] {
        let packages = categories.remove(category).unwrap_or_default();
        let _ = writeln!(report, "{category} / {} package(s)", packages.len());
        if packages.is_empty() {
            report.push_str("  None detected.\n\n");
        } else {
            for package in packages {
                report.push_str("  ");
                report.push_str(&package);
                report.push('\n');
            }
            report.push('\n');
        }
    }

    for (category, packages) in categories {
        let _ = writeln!(report, "{category} / {} package(s)", packages.len());
        for package in packages {
            report.push_str("  ");
            report.push_str(&package);
            report.push('\n');
        }
        report.push('\n');
    }
    report
}

fn pacman_update_names() -> HashSet<String> {
    command_stdout("pacman", &["-Qu"])
        .map(|output| {
            output
                .lines()
                .filter_map(|line| line.split_whitespace().next())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn package_repository_category(repository: &str) -> String {
    match repository.to_ascii_lowercase().as_str() {
        "omarchy" => "OMARCHY".to_string(),
        "blackarch" => "BLACKARCH".to_string(),
        "chaotic-aur" | "chaotic" => "CHAOTIC AUR".to_string(),
        "core" | "extra" | "multilib" => "ARCH OFFICIAL".to_string(),
        other => format!("REPOSITORY / {}", other.to_ascii_uppercase()),
    }
}

fn package_repository_priority(category: &str) -> u8 {
    match category {
        "CHAOTIC AUR" => 5,
        "BLACKARCH" => 4,
        "OMARCHY" => 3,
        "ARCH OFFICIAL" => 2,
        _ => 1,
    }
}

pub struct Recovery;
impl AuditModule for Recovery {
    fn name(&self) -> &'static str {
        "Recovery Readiness"
    }
    fn slug(&self) -> &'static str {
        "recovery"
    }
    fn menu_label(&self) -> &'static str {
        "🧰 Recovery Readiness"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["df", "findmnt", "btrfs", "systemctl"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        self.tools()
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "recovery.md",
            "RECOVERY READINESS",
            &[
                ("df -hT", capture("df", &["-hT"])),
                ("df -ih", capture("df", &["-ih"])),
                (
                    "findmnt",
                    capture("findmnt", &["-o", "TARGET,SOURCE,FSTYPE,OPTIONS"]),
                ),
                (
                    "btrfs scrub status -d /",
                    capture("btrfs", &["scrub", "status", "-d", "/"]),
                ),
                (
                    "fstrim.timer",
                    capture("systemctl", &["status", "fstrim.timer", "--no-pager", "-l"]),
                ),
            ],
        )
    }
}

pub struct Reliability;
impl AuditModule for Reliability {
    fn name(&self) -> &'static str {
        "Reliability Signals"
    }
    fn slug(&self) -> &'static str {
        "reliability"
    }
    fn menu_label(&self) -> &'static str {
        "📈 Reliability Signals"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["journalctl", "coredumpctl", "sensors", "free"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        self.tools()
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "reliability.md",
            "RELIABILITY SIGNALS",
            &[
                (
                    "kernel warning and alert journal",
                    capture(
                        "journalctl",
                        &[
                            "-k",
                            "-p",
                            "warning..alert",
                            "-b",
                            "-n",
                            "500",
                            "--no-pager",
                        ],
                    ),
                ),
                (
                    "hardware and memory error journal",
                    capture(
                        "journalctl",
                        &[
                            "-b",
                            "-g",
                            "oom|out of memory|machine check|hardware error",
                            "-n",
                            "500",
                            "--no-pager",
                        ],
                    ),
                ),
                (
                    "coredumpctl list",
                    capture("coredumpctl", &["list", "--no-pager"]),
                ),
                ("sensors", capture("sensors", &[])),
                ("free -h", capture("free", &["-h"])),
            ],
        )
    }
}

pub struct Performance;
impl AuditModule for Performance {
    fn name(&self) -> &'static str {
        "Performance Pulse"
    }
    fn slug(&self) -> &'static str {
        "performance"
    }
    fn menu_label(&self) -> &'static str {
        "⚡ Performance Pulse"
    }
    fn tools(&self) -> &'static [&'static str] {
        &["uptime", "free", "vmstat", "systemd-analyze"]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        self.tools()
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "performance.md",
            "PERFORMANCE PULSE",
            &[
                ("uptime", capture("uptime", &[])),
                ("free -h", capture("free", &["-h"])),
                ("vmstat 1 2", capture("vmstat", &["1", "2"])),
                (
                    "systemd-analyze blame",
                    capture("systemd-analyze", &["blame"]),
                ),
                (
                    "systemd-analyze critical-chain",
                    capture("systemd-analyze", &["critical-chain"]),
                ),
            ],
        )
    }
}

pub struct Omarchy;
impl AuditModule for Omarchy {
    fn name(&self) -> &'static str {
        "Omarchy Surface"
    }
    fn slug(&self) -> &'static str {
        "omarchy"
    }
    fn menu_label(&self) -> &'static str {
        "🖥️  Omarchy Surface"
    }
    fn tools(&self) -> &'static [&'static str] {
        &[
            "omarchy",
            "omarchy-debug",
            "hyprctl",
            "journalctl",
            "quickshell",
        ]
    }
    fn optional_tools(&self) -> &'static [&'static str] {
        self.tools()
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "omarchy.md",
            "OMARCHY SURFACE",
            &[
                ("omarchy version", capture("omarchy", &["version"])),
                // `omarchy-debug` is the binary behind `omarchy debug`; calling it
                // directly avoids depending on the dispatcher's routing, which
                // has rejected `omarchy debug` on some omarchy-dev builds. Its
                // output includes whole journals (75 MB was observed), so the
                // bounded capture is what keeps this section usable.
                (
                    "omarchy debug --no-sudo --print",
                    capture("omarchy-debug", &["--no-sudo", "--print"]),
                ),
                (
                    "hyprctl configerrors",
                    capture("hyprctl", &["configerrors"]),
                ),
                (
                    "hyprctl monitors all",
                    capture("hyprctl", &["monitors", "all"]),
                ),
                (
                    "omarchy-shell journal",
                    capture(
                        "journalctl",
                        &["--user", "-u", "omarchy-shell", "-n", "100", "--no-pager"],
                    ),
                ),
                (
                    "quickshell --version",
                    capture("quickshell", &["--version"]),
                ),
            ],
        )
    }
}

/// Every module, defined once. The menu, the capability matrix, the
/// health score, and both the full-audit and single-module execution
/// paths all iterate this same list — nowhere else is the set of
/// modules spelled out by hand.
#[must_use]
pub fn all_modules() -> Vec<Box<dyn AuditModule>> {
    vec![
        Box::new(Hardware),
        Box::new(Disks),
        Box::new(Snapshots),
        Box::new(Network),
        Box::new(Containers),
        Box::new(Services),
        Box::new(Logs),
        Box::new(Bluetooth),
        Box::new(ConnectedDevices),
        Box::new(SecurityPosture),
        Box::new(Accounts),
        Box::new(Persistence),
        Box::new(PackageIntegrity),
        Box::new(Recovery),
        Box::new(Reliability),
        Box::new(Performance),
        Box::new(Omarchy),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;

    #[test]
    fn module_registry_has_unique_slugs_and_labels() {
        let modules = all_modules();
        let slugs = modules
            .iter()
            .map(|module| module.slug())
            .collect::<HashSet<_>>();
        let labels = modules
            .iter()
            .map(|module| module.name())
            .collect::<HashSet<_>>();

        assert_eq!(slugs.len(), modules.len());
        assert_eq!(labels.len(), modules.len());
        assert!(modules.iter().all(|module| !module.tools().is_empty()));
    }

    #[test]
    fn only_privileged_modules_require_sudo() {
        let modules = all_modules();
        let privileged = modules
            .iter()
            .filter(|module| module.requires_sudo())
            .map(|module| module.slug())
            .collect::<HashSet<_>>();

        assert_eq!(
            privileged,
            HashSet::from(["hardware", "disks", "snapshots", "logs"])
        );
    }

    #[test]
    fn desktop_package_and_display_tools_are_optional() {
        let modules = all_modules();
        let optional = modules
            .iter()
            .flat_map(|module| module.optional_tools())
            .copied()
            .collect::<HashSet<_>>();

        for tool in [
            "flatpak",
            "snap",
            "hyprctl",
            "wlr-randr",
            "xrandr",
            "ss",
            "ufw",
            "nft",
            "pacman",
            "omarchy",
        ] {
            assert!(optional.contains(tool), "expected {tool} to be optional");
        }
    }

    #[test]
    fn bluetooth_report_is_explicit_when_service_is_unavailable() {
        let directory =
            std::env::temp_dir().join(format!("omniscient-bluetooth-test-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("create test report directory");
        Bluetooth.run(&directory).expect("write bluetooth report");
        let report =
            fs::read_to_string(directory.join("bluetooth.md")).expect("read bluetooth report");
        assert!(report.contains("# 📡 BLUETOOTH"));
        assert!(report.contains("## bluetoothctl devices"));
        assert!(!report.trim().is_empty());
        fs::remove_dir_all(directory).expect("remove test report directory");
    }

    #[test]
    fn a_huge_section_is_bounded_like_the_75_mb_omarchy_debug_dump() {
        // Regression: an unbounded `omarchy debug` section produced a 75 MB
        // report that froze the desktop shell when the HUD opened it.
        let stack = "                    #0  0x000055d1 in frame () from /usr/lib/libx.so\n";
        let huge = stack.repeat(453_000);
        let report = render_report(
            "OMARCHY SURFACE",
            &[
                ("omarchy version", "4.0.0\n".to_owned()),
                ("omarchy debug", huge),
            ],
        );
        assert!(
            report.len() < crate::capture::MAX_SECTION_BYTES * 2,
            "{} bytes",
            report.len()
        );
        assert!(report.lines().count() <= crate::capture::MAX_SECTION_LINES + 20);
        assert!(report.contains("## omarchy debug"));
        assert!(report.contains("[omniscient: output truncated / showing "));
        assert_eq!(report.matches("```").count() % 2, 0, "fences stay balanced");
    }

    #[test]
    fn the_whole_report_is_capped_and_later_sections_say_why_they_are_missing() {
        let big = "x".repeat(200) + "\n";
        let sections = (0..20)
            .map(|index| (format!("section {index}"), big.repeat(2_000)))
            .collect::<Vec<_>>();
        let report = render_report("LOGS", &sections);
        assert!(
            report.len() <= MAX_REPORT_BYTES + 64 * 1024,
            "{} bytes",
            report.len()
        );
        assert!(
            report.contains("## section 19"),
            "every heading is still listed"
        );
        assert!(report.contains("_omitted: the report reached its size limit_"));
        assert_eq!(report.matches("```").count() % 2, 0, "fences stay balanced");
    }

    #[test]
    fn section_bodies_cannot_inject_fences_headings_or_escapes() {
        let report = render_report(
            "LOGS",
            &[(
                "evil\n## injected",
                "```\n# not a title\n\u{1b}[31mred\u{1b}[0m\n".to_owned(),
            )],
        );
        assert!(report.contains("## evil ## injected\n"));
        assert!(!report.contains('\u{1b}'));
        assert_eq!(
            report.matches("```").count(),
            2,
            "only the section's own fence pair"
        );
    }

    /// Host check, not part of the default suite: runs the real Omarchy
    /// module (whose `omarchy debug` section once produced a 75 MB report)
    /// on this machine. Run with `cargo test -- --ignored real_omarchy`.
    #[test]
    #[ignore = "runs real host commands; needs an Omarchy system"]
    fn real_omarchy_report_is_bounded() {
        let directory =
            std::env::temp_dir().join(format!("omniscient-real-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("create report directory");
        Omarchy.run(&directory).expect("module runs");
        let report = fs::read_to_string(directory.join("omarchy.md")).expect("report written");
        fs::remove_dir_all(&directory).expect("remove report directory");
        if std::env::var_os("OMNISCIENT_SHOW_REPORT").is_some() {
            eprintln!("{report}");
        }
        eprintln!(
            "omarchy.md: {} bytes, {} lines",
            report.len(),
            report.lines().count()
        );
        assert!(
            report.len() <= MAX_REPORT_BYTES + 64 * 1024,
            "{} bytes",
            report.len()
        );
        assert!(
            !report.contains('\u{1b}') && !report.contains('\u{3}'),
            "control characters removed"
        );
    }
}
