use anyhow::{Context, Result};
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
    fn menu_label(&self) -> &'static str;
    /// Tools this module relies on — used for the capability matrix
    /// and the health score, not just duplicated as a magic list.
    fn tools(&self) -> &'static [&'static str];
    fn requires_sudo(&self) -> bool {
        false
    }
    fn run(&self, dir: &Path) -> Result<()>;
}

/// Runs a command, returning combined stdout (+ a note if it failed
/// or wasn't found) rather than silently swallowing errors the way
/// `2>/dev/null` did in the fish version.
fn capture(cmd: &str, args: &[&str]) -> String {
    if !crate::pathcheck::exists(cmd) {
        return format!("_{cmd}: not installed, skipped_\n");
    }
    match Command::new(cmd).args(args).output() {
        Ok(out) => {
            let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                s.push_str(&format!("\n_[{cmd} exited with {}]_\n", out.status));
                if !err.trim().is_empty() {
                    s.push_str(&format!("```\n{}\n```\n", err.trim()));
                }
            }
            s
        }
        Err(e) => format!("_{cmd}: failed to run ({e})_\n"),
    }
}

fn capture_privileged(command: &str, args: &[&str]) -> String {
    if !crate::pathcheck::exists(command) {
        return format!("_{command}: not installed, skipped_\n");
    }
    let mut sudo_args = vec![command];
    sudo_args.extend_from_slice(args);
    capture("sudo", &sudo_args)
}

fn write_report<S: AsRef<str>>(
    dir: &Path,
    filename: &str,
    title: &str,
    sections: &[(S, String)],
) -> Result<()> {
    let path = dir.join(filename);
    let mut f = File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    writeln!(f, "# {title}\n")?;
    for (heading, body) in sections {
        writeln!(f, "## {}\n", heading.as_ref())?;
        writeln!(f, "```\n{}\n```\n", body.trim_end())?;
    }
    Ok(())
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
        let disk_list = capture("lsblk", &["-dn", "-o", "NAME"]);
        for disk in disk_list.lines().map(str::trim).filter(|l| !l.is_empty()) {
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
        &["ip", "ss"]
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "network.md",
            "NETWORK",
            &[
                ("ip a", capture("ip", &["a"])),
                ("ss -tulanp", capture("ss", &["-tulanp"])),
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
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "containers.md",
            "CONTAINERS",
            &[
                ("docker ps -a", capture("docker", &["ps", "-a"])),
                ("flatpak list", capture("flatpak", &["list"])),
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
    let full = capture("dmesg", &[]);
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
        write_report(
            dir,
            "bluetooth.md",
            "BLUETOOTH",
            &[(
                "bluetoothctl devices",
                capture("bluetoothctl", &["devices"]),
            )],
        )
    }
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
        &["xrandr", "aplay"]
    }
    fn run(&self, dir: &Path) -> Result<()> {
        write_report(
            dir,
            "devices.md",
            "DEVICES",
            &[
                ("xrandr", capture("xrandr", &[])),
                ("aplay -l", capture("aplay", &["-l"])),
            ],
        )
    }
}

/// Every module, defined once. The menu, the capability matrix, the
/// health score, and both the full-audit and single-module execution
/// paths all iterate this same list — nowhere else is the set of
/// modules spelled out by hand.
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
            HashSet::from(["hardware", "disks", "snapshots"])
        );
    }
}
