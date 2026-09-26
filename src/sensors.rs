//! Live hardware readings for the HUD's sensor tabs (`omniscient --sensors`).
//!
//! Read-only by design: everything comes from `/sys` and `/proc`, nothing is
//! written, and fan or RGB *control* is left to the vendor tools this module
//! only detects. Every path is resolved under a root directory (normally `/`,
//! a fake tree in tests, or `OMNISCIENT_SYSFS_ROOT`), so layouts for hardware
//! that is not present (Ryzen, amdgpu, `NVMe`, ASUS) are tested with fixtures
//! modeled on real sysfs trees.
//!
//! Output is bounded: at most [`MAX_CHIPS`] chips, [`MAX_ENTRIES`] readings
//! of each kind per chip, [`MAX_CORES`] cores and [`MAX_DRIVES`] drives.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const MAX_CHIPS: usize = 64;
pub const MAX_ENTRIES: usize = 32;
pub const MAX_CORES: usize = 256;
pub const MAX_DRIVES: usize = 32;
const MAX_GPUS: usize = 8;

/// One temperature.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Temp {
    pub label: String,
    pub celsius: f64,
    pub max_celsius: Option<f64>,
    pub crit_celsius: Option<f64>,
}

/// One fan.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fan {
    pub label: String,
    pub rpm: u64,
}

/// One PWM output as a duty percentage (read-only).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pwm {
    pub label: String,
    pub percent: f64,
}

/// A fan curve exposed through `pwmN_auto_pointM_{temp,pwm}` (ASUS custom
/// fan curves, many Super I/O chips).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Curve {
    pub fan: String,
    /// `(celsius, duty percent)` points in order.
    pub points: Vec<(f64, f64)>,
}

/// One hwmon chip.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Chip {
    pub name: String,
    pub temps: Vec<Temp>,
    pub fans: Vec<Fan>,
    pub pwms: Vec<Pwm>,
    pub curves: Vec<Curve>,
    pub power_watts: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Core {
    pub cpu: usize,
    pub mhz: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Cpu {
    pub model: String,
    pub vendor: String,
    pub driver: String,
    pub governor: String,
    pub boost: Option<bool>,
    pub amd_pstate: Option<String>,
    pub package_celsius: Option<f64>,
    pub package_source: String,
    pub utilization_percent: Option<f64>,
    pub package_watts: Option<f64>,
    pub cores: Vec<Core>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Gpu {
    pub card: String,
    pub vendor: String,
    pub driver: String,
    pub busy_percent: Option<f64>,
    pub vram_used_mib: Option<f64>,
    pub vram_total_mib: Option<f64>,
    pub clock_mhz: Option<f64>,
    pub power_watts: Option<f64>,
    pub fan_rpm: Option<u64>,
    pub temps: Vec<Temp>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Drive {
    pub name: String,
    pub model: String,
    pub kind: String,
    pub celsius: Option<f64>,
    pub temps: Vec<Temp>,
    pub hint: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Asus {
    pub wmi: bool,
    pub keyboard_brightness: Option<u64>,
    pub keyboard_max_brightness: Option<u64>,
    pub throttle_policy: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Platform {
    pub vendor: String,
    pub product: String,
    pub profile: Option<String>,
    pub profile_choices: Vec<String>,
    pub asus: Option<Asus>,
    /// Control tools found on `PATH` (Omniscient never drives them).
    pub tools: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Memory {
    pub total_mib: f64,
    pub available_mib: f64,
    pub swap_total_mib: f64,
    pub swap_free_mib: f64,
}

/// One complete reading.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Reading {
    pub version: u32,
    pub taken_at: String,
    pub cpu: Cpu,
    pub gpus: Vec<Gpu>,
    pub chips: Vec<Chip>,
    pub drives: Vec<Drive>,
    pub platform: Platform,
    pub memory: Memory,
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------- helpers --

fn read(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn read_f64(path: &Path) -> Option<f64> {
    read(path)?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn read_u64(path: &Path) -> Option<u64> {
    read(path)?.parse().ok()
}

/// Sorted directory entries whose names start with `prefix`.
fn entries(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut found = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
                .map(|entry| entry.path())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    found.sort_by_key(|path| natural_key(path));
    found
}

/// Orders `hwmon10` after `hwmon9`.
fn natural_key(path: &Path) -> (String, u64) {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let digits = name
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    let number = digits
        .chars()
        .rev()
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    (
        name.trim_end_matches(|c: char| c.is_ascii_digit())
            .to_owned(),
        number,
    )
}

fn indexes(dir: &Path, prefix: &str, suffix: &str) -> Vec<u32> {
    let mut found = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.strip_prefix(prefix)?
                        .strip_suffix(suffix)?
                        .parse::<u32>()
                        .ok()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    found.sort_unstable();
    found.dedup();
    found
}

fn milli(value: f64) -> f64 {
    (value / 100.0).round() / 10.0
}

// ------------------------------------------------------------------ hwmon --

/// Reads one hwmon chip directory.
#[must_use]
pub fn read_chip(dir: &Path) -> Option<Chip> {
    let name = read(&dir.join("name"))?;
    let temps = indexes(dir, "temp", "_input")
        .into_iter()
        .take(MAX_ENTRIES)
        .filter_map(|n| {
            let celsius = milli(read_f64(&dir.join(format!("temp{n}_input")))?);
            Some(Temp {
                label: read(&dir.join(format!("temp{n}_label")))
                    .unwrap_or_else(|| format!("temp{n}")),
                celsius,
                max_celsius: read_f64(&dir.join(format!("temp{n}_max")))
                    .map(milli)
                    .filter(|v| *v > 0.0),
                crit_celsius: read_f64(&dir.join(format!("temp{n}_crit")))
                    .map(milli)
                    .filter(|v| *v > 0.0),
            })
        })
        .collect();
    let fans = indexes(dir, "fan", "_input")
        .into_iter()
        .take(MAX_ENTRIES)
        .filter_map(|n| {
            Some(Fan {
                label: read(&dir.join(format!("fan{n}_label")))
                    .unwrap_or_else(|| format!("fan{n}")),
                rpm: read_u64(&dir.join(format!("fan{n}_input")))?,
            })
        })
        .collect();
    let pwms = indexes(dir, "pwm", "")
        .into_iter()
        .take(MAX_ENTRIES)
        .filter_map(|n| {
            let raw = read_f64(&dir.join(format!("pwm{n}")))?;
            Some(Pwm {
                label: format!("pwm{n}"),
                percent: (raw / 255.0 * 1000.0).round() / 10.0,
            })
        })
        .collect();
    let curves = read_curves(dir);
    let power_watts = read_f64(&dir.join("power1_average"))
        .or_else(|| read_f64(&dir.join("power1_input")))
        .map(|micro| (micro / 10_000.0).round() / 100.0);
    Some(Chip {
        name,
        temps,
        fans,
        pwms,
        curves,
        power_watts,
    })
}

fn read_curves(dir: &Path) -> Vec<Curve> {
    let mut curves = Vec::new();
    for fan in 1..=8 {
        let mut points = Vec::new();
        for point in 1..=16 {
            let (Some(temp), Some(pwm)) = (
                read_f64(&dir.join(format!("pwm{fan}_auto_point{point}_temp"))),
                read_f64(&dir.join(format!("pwm{fan}_auto_point{point}_pwm"))),
            ) else {
                continue;
            };
            // ASUS curves use °C; Super I/O chips use millidegrees.
            let celsius = if temp > 1000.0 { milli(temp) } else { temp };
            points.push((celsius, (pwm / 255.0 * 1000.0).round() / 10.0));
        }
        if !points.is_empty() {
            curves.push(Curve {
                fan: format!("pwm{fan}"),
                points,
            });
        }
    }
    curves
}

/// Every hwmon chip under `root`.
#[must_use]
pub fn read_chips(root: &Path) -> Vec<Chip> {
    entries(&root.join("sys/class/hwmon"), "hwmon")
        .iter()
        .take(MAX_CHIPS)
        .filter_map(|dir| read_chip(dir))
        .collect()
}

// -------------------------------------------------------------------- cpu --

/// Picks the CPU package temperature from the chips: Intel `coretemp`
/// "Package id 0", AMD `k10temp` Tctl/Tdie, or `zenpower` Tdie.
#[must_use]
pub fn package_temperature(chips: &[Chip]) -> Option<(f64, String)> {
    let pick = |chip: &str, labels: &[&str]| {
        chips.iter().filter(|c| c.name == chip).find_map(|c| {
            labels
                .iter()
                .find_map(|label| c.temps.iter().find(|t| t.label.starts_with(label)))
                .map(|t| (t.celsius, format!("{chip} {}", t.label)))
        })
    };
    pick("coretemp", &["Package id"])
        .or_else(|| pick("k10temp", &["Tctl", "Tdie"]))
        .or_else(|| pick("zenpower", &["Tdie", "Tctl"]))
        .or_else(|| pick("cpu_thermal", &["temp1"]))
}

/// Busy share between two `/proc/stat` "cpu" lines.
#[must_use]
pub fn utilization(before: &str, after: &str) -> Option<f64> {
    let parse = |text: &str| -> Option<(f64, f64)> {
        let line = text.lines().find(|line| line.starts_with("cpu "))?;
        let values = line
            .split_whitespace()
            .skip(1)
            .filter_map(|v| v.parse::<f64>().ok())
            .collect::<Vec<_>>();
        let idle = values.get(3)? + values.get(4).unwrap_or(&0.0);
        Some((values.iter().sum(), idle))
    };
    let (total_a, idle_a) = parse(before)?;
    let (total_b, idle_b) = parse(after)?;
    let total = total_b - total_a;
    (total > 0.0).then(|| ((1.0 - (idle_b - idle_a) / total) * 1000.0).round() / 10.0)
}

fn cpu_identity(cpuinfo: &str) -> (String, String) {
    let field = |name: &str| {
        cpuinfo
            .lines()
            .find(|line| line.starts_with(name))
            .and_then(|line| line.split_once(':'))
            .map(|(_, value)| value.trim().to_owned())
            .unwrap_or_default()
    };
    (field("model name"), field("vendor_id"))
}

fn read_cpu(root: &Path, chips: &[Chip], sample: Duration, notes: &mut Vec<String>) -> Cpu {
    let (model, vendor) = cpu_identity(&read(&root.join("proc/cpuinfo")).unwrap_or_default());
    let base = root.join("sys/devices/system/cpu");
    let mut cores = entries(&base, "cpu")
        .into_iter()
        .filter_map(|dir| {
            let cpu = dir
                .file_name()?
                .to_string_lossy()
                .strip_prefix("cpu")?
                .parse()
                .ok()?;
            let khz = read_f64(&dir.join("cpufreq/scaling_cur_freq"))?;
            Some(Core {
                cpu,
                mhz: (khz / 1000.0).round(),
            })
        })
        .collect::<Vec<_>>();
    cores.sort_by_key(|core| core.cpu);
    cores.truncate(MAX_CORES);
    let boost = read(&base.join("cpufreq/boost"))
        .map(|v| v == "1")
        .or_else(|| read(&base.join("intel_pstate/no_turbo")).map(|v| v == "0"));
    let (package_celsius, package_source) =
        package_temperature(chips).map_or((None, String::new()), |(c, s)| (Some(c), s));

    let stat = root.join("proc/stat");
    let energy = root.join("sys/class/powercap/intel-rapl:0/energy_uj");
    let (stat_a, energy_a) = (read(&stat), read_f64(&energy));
    if !sample.is_zero() {
        std::thread::sleep(sample);
    }
    let (stat_b, energy_b) = (read(&stat), read_f64(&energy));
    let utilization_percent = stat_a.zip(stat_b).and_then(|(a, b)| utilization(&a, &b));
    let package_watts = match (energy_a, energy_b) {
        (Some(a), Some(b)) if b >= a && !sample.is_zero() => {
            Some(((b - a) / sample.as_secs_f64() / 10_000.0).round() / 100.0)
        }
        (None, _) if root.join("sys/class/powercap/intel-rapl:0").exists() => {
            notes.push(
                "CPU package power needs the elevated audit (RAPL energy is root-only)".to_owned(),
            );
            None
        }
        _ => None,
    };
    Cpu {
        model,
        vendor,
        driver: read(&base.join("cpu0/cpufreq/scaling_driver")).unwrap_or_default(),
        governor: read(&base.join("cpu0/cpufreq/scaling_governor")).unwrap_or_default(),
        boost,
        amd_pstate: read(&base.join("amd_pstate/status")),
        package_celsius,
        package_source,
        utilization_percent,
        package_watts,
        cores,
    }
}

// -------------------------------------------------------------------- gpu --

fn driver_of(device: &Path) -> String {
    fs::read_link(device.join("driver"))
        .ok()
        .and_then(|link| link.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

fn read_gpus(root: &Path) -> Vec<Gpu> {
    entries(&root.join("sys/class/drm"), "card")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|n| !n.to_string_lossy().contains('-'))
        })
        .take(MAX_GPUS)
        .map(|card| {
            let device = card.join("device");
            let vendor = match read(&device.join("vendor")).as_deref() {
                Some("0x1002") => "AMD",
                Some("0x8086") => "Intel",
                Some("0x10de") => "NVIDIA",
                _ => "unknown",
            };
            let hwmon = entries(&device.join("hwmon"), "hwmon")
                .into_iter()
                .find_map(|dir| read_chip(&dir));
            let sclk = read(&device.join("pp_dpm_sclk")).and_then(|text| {
                text.lines()
                    .find(|line| line.trim_end().ends_with('*'))
                    .and_then(|line| {
                        let value = line.split_whitespace().nth(1)?;
                        value
                            .trim_end_matches('*')
                            .trim_end_matches("Mhz")
                            .trim_end_matches("MHz")
                            .parse()
                            .ok()
                    })
            });
            Gpu {
                card: card
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                vendor: vendor.to_owned(),
                driver: driver_of(&device),
                busy_percent: read_f64(&device.join("gpu_busy_percent")),
                vram_used_mib: read_f64(&device.join("mem_info_vram_used"))
                    .map(|b| (b / 1_048_576.0).round()),
                vram_total_mib: read_f64(&device.join("mem_info_vram_total"))
                    .map(|b| (b / 1_048_576.0).round()),
                clock_mhz: sclk.or_else(|| read_f64(&card.join("gt_cur_freq_mhz"))),
                power_watts: hwmon.as_ref().and_then(|chip| chip.power_watts),
                fan_rpm: hwmon
                    .as_ref()
                    .and_then(|chip| chip.fans.first().map(|fan| fan.rpm)),
                temps: hwmon.map(|chip| chip.temps).unwrap_or_default(),
            }
        })
        .collect()
}

// ----------------------------------------------------------------- drives --

/// Every physical disk with its live temperature where the kernel exposes
/// one (`NVMe` always; SATA with the `drivetemp` module).
#[must_use]
pub fn read_drives(root: &Path) -> Vec<Drive> {
    let skip = ["loop", "ram", "zram", "dm-", "sr", "md", "nbd", "fd"];
    entries(&root.join("sys/block"), "")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|n| !skip.iter().any(|prefix| n.to_string_lossy().starts_with(prefix)))
        })
        .take(MAX_DRIVES)
        .map(|block| {
            let name = block.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let device = block.join("device");
            let resolved = fs::canonicalize(&device).unwrap_or_default().to_string_lossy().into_owned();
            let kind = if name.starts_with("nvme") {
                "nvme"
            } else if resolved.contains("/usb") || read(&block.join("removable")).as_deref() == Some("1") {
                "usb"
            } else if name.starts_with("mmcblk") {
                "mmc"
            } else {
                "sata"
            };
            let mut dirs = entries(&device.join("hwmon"), "hwmon");
            dirs.extend(entries(&device, "hwmon"));
            let temps = dirs.iter().find_map(|dir| read_chip(dir)).map(|chip| chip.temps).unwrap_or_default();
            let celsius = temps
                .iter()
                .find(|t| t.label == "Composite")
                .or_else(|| temps.first())
                .map(|t| t.celsius);
            let hint = match (celsius, kind) {
                (None, "sata") => "load the drivetemp kernel module for live SATA temperatures (modprobe drivetemp)".to_owned(),
                (None, "usb") => "USB bridges rarely report temperature".to_owned(),
                _ => String::new(),
            };
            Drive {
                name,
                model: read(&device.join("model")).unwrap_or_default(),
                kind: kind.to_owned(),
                celsius,
                temps,
                hint,
            }
        })
        .collect()
}

// --------------------------------------------------------------- platform --

fn read_platform(root: &Path, detect_tools: bool) -> Platform {
    let dmi = root.join("sys/class/dmi/id");
    let acpi = root.join("sys/firmware/acpi");
    let asus_wmi = root.join("sys/devices/platform/asus-nb-wmi");
    let keyboard = root.join("sys/class/leds/asus::kbd_backlight");
    let asus = (asus_wmi.exists() || keyboard.exists()).then(|| Asus {
        wmi: asus_wmi.exists(),
        keyboard_brightness: read_u64(&keyboard.join("brightness")),
        keyboard_max_brightness: read_u64(&keyboard.join("max_brightness")),
        throttle_policy: read(&asus_wmi.join("throttle_thermal_policy")).map(|value| {
            match value.as_str() {
                "0" => "balanced",
                "1" => "performance",
                "2" => "quiet",
                other => other,
            }
            .to_owned()
        }),
    });
    let tools = if detect_tools {
        [
            "asusctl",
            "rog-control-center",
            "supergfxctl",
            "openrgb",
            "coolercontrol",
            "fancontrol",
            "nvidia-smi",
        ]
        .into_iter()
        .filter(|tool| crate::pathcheck::exists(tool))
        .map(str::to_owned)
        .collect()
    } else {
        Vec::new()
    };
    Platform {
        vendor: read(&dmi.join("sys_vendor")).unwrap_or_default(),
        product: read(&dmi.join("product_name")).unwrap_or_default(),
        profile: read(&acpi.join("platform_profile")),
        profile_choices: read(&acpi.join("platform_profile_choices"))
            .map(|v| v.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default(),
        asus,
        tools,
    }
}

fn read_memory(root: &Path) -> Memory {
    let text = read(&root.join("proc/meminfo")).unwrap_or_default();
    let field = |name: &str| {
        text.lines()
            .find(|line| line.starts_with(name))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|kib| kib.parse::<f64>().ok())
            .map_or(0.0, |kib| (kib / 1024.0).round())
    };
    Memory {
        total_mib: field("MemTotal:"),
        available_mib: field("MemAvailable:"),
        swap_total_mib: field("SwapTotal:"),
        swap_free_mib: field("SwapFree:"),
    }
}

// ------------------------------------------------------------------ entry --

/// The root sensors are read from: `OMNISCIENT_SYSFS_ROOT` or `/`.
#[must_use]
pub fn root() -> PathBuf {
    std::env::var_os("OMNISCIENT_SYSFS_ROOT").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// Takes one reading under `root`, sampling utilization and power over
/// `sample`. `detect_tools` looks up control tools on `PATH`.
#[must_use]
pub fn read_all(root: &Path, sample: Duration, detect_tools: bool) -> Reading {
    let mut notes = Vec::new();
    let chips = read_chips(root);
    let cpu = read_cpu(root, &chips, sample, &mut notes);
    let drives = read_drives(root);
    if drives
        .iter()
        .any(|d| d.kind == "sata" && d.celsius.is_none())
    {
        notes.push("SATA temperatures appear once the drivetemp module is loaded".to_owned());
    }
    Reading {
        version: 1,
        taken_at: chrono::Local::now().to_rfc3339(),
        cpu,
        gpus: read_gpus(root),
        chips,
        drives,
        platform: read_platform(root, detect_tools),
        memory: read_memory(root),
        notes,
    }
}

/// `omniscient --sensors`: prints one reading as JSON.
///
/// # Errors
///
/// Returns an error when the reading cannot be encoded or written.
pub fn run() -> anyhow::Result<()> {
    let root = root();
    let reading = read_all(&root, Duration::from_millis(250), root == Path::new("/"));
    let json = serde_json::to_string(&reading)?;
    anyhow::ensure!(json.len() <= 512 * 1024, "sensor reading exceeded 512 KiB");
    println!("{json}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a fake sysfs/procfs tree from `(relative path, contents)`.
    fn tree(files: &[(&str, &str)]) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "omniscient-sensors-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        for (path, contents) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            fs::write(path, contents).expect("write");
        }
        root
    }

    fn link(root: &Path, from: &str, to: &str) {
        let from = root.join(from);
        fs::create_dir_all(from.parent().expect("parent")).expect("dirs");
        std::os::unix::fs::symlink(to, from).expect("symlink");
    }

    #[test]
    fn intel_laptop_layout_matches_this_machine() {
        // Modeled on the dev laptop (i3-8130U, coretemp, i915, SATA SSD).
        let root = tree(&[
            ("sys/class/hwmon/hwmon3/name", "coretemp\n"),
            ("sys/class/hwmon/hwmon3/temp1_input", "53000\n"),
            ("sys/class/hwmon/hwmon3/temp1_label", "Package id 0\n"),
            ("sys/class/hwmon/hwmon3/temp1_max", "100000\n"),
            ("sys/class/hwmon/hwmon3/temp1_crit", "100000\n"),
            ("sys/class/hwmon/hwmon3/temp2_input", "51000\n"),
            ("sys/class/hwmon/hwmon3/temp2_label", "Core 0\n"),
            ("sys/class/hwmon/hwmon2/name", "pch_skylake\n"),
            ("sys/class/hwmon/hwmon2/temp1_input", "50000\n"),
            ("proc/cpuinfo", "vendor_id\t: GenuineIntel\nmodel name\t: Intel(R) Core(TM) i3-8130U CPU @ 2.20GHz\n"),
            ("proc/stat", "cpu  100 0 100 800 0 0 0 0 0 0\n"),
            ("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq", "3400041\n"),
            ("sys/devices/system/cpu/cpu0/cpufreq/scaling_governor", "powersave\n"),
            ("sys/devices/system/cpu/cpu0/cpufreq/scaling_driver", "intel_pstate\n"),
            ("sys/devices/system/cpu/cpu1/cpufreq/scaling_cur_freq", "900000\n"),
            ("sys/devices/system/cpu/intel_pstate/no_turbo", "0\n"),
            ("sys/class/drm/card1/device/vendor", "0x8086\n"),
            ("sys/class/drm/card1/gt_cur_freq_mhz", "350\n"),
            ("sys/class/drm/card1-eDP-1/status", "connected\n"),
            ("sys/block/sda/device/model", "SSD 256GB\n"),
            ("sys/block/sda/removable", "0\n"),
            ("sys/class/powercap/intel-rapl:0/name", "package-0\n"),
            ("proc/meminfo", "MemTotal:       19922944 kB\nMemAvailable:   14680064 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n"),
        ]);
        let reading = read_all(&root, Duration::ZERO, false);
        assert_eq!(reading.cpu.vendor, "GenuineIntel");
        assert_eq!(reading.cpu.package_celsius, Some(53.0));
        assert_eq!(reading.cpu.package_source, "coretemp Package id 0");
        assert_eq!(
            reading.cpu.cores,
            vec![
                Core {
                    cpu: 0,
                    mhz: 3400.0
                },
                Core { cpu: 1, mhz: 900.0 }
            ]
        );
        assert_eq!(reading.cpu.boost, Some(true));
        assert_eq!(reading.gpus.len(), 1, "connector entries are not cards");
        assert_eq!(reading.gpus[0].vendor, "Intel");
        assert_eq!(reading.gpus[0].clock_mhz, Some(350.0));
        assert_eq!(reading.drives[0].kind, "sata");
        assert!(reading.drives[0].hint.contains("drivetemp"));
        assert!(
            reading.notes.iter().any(|n| n.contains("RAPL")),
            "unreadable RAPL is explained"
        );
        assert!((reading.memory.total_mib - 19_456.0).abs() < 1.0);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one fixture tree modeled on a single real machine"
    )]
    fn ryzen_amdgpu_nvme_and_asus_layouts_parse() {
        // Modeled on a Ryzen + Radeon ASUS laptop (k10temp, amdgpu, nvme,
        // asus-nb-wmi, asus_custom_fan_curve).
        let root = tree(&[
            ("sys/class/hwmon/hwmon1/name", "k10temp\n"),
            ("sys/class/hwmon/hwmon1/temp1_input", "68375\n"),
            ("sys/class/hwmon/hwmon1/temp1_label", "Tctl\n"),
            ("sys/class/hwmon/hwmon1/temp3_input", "61500\n"),
            ("sys/class/hwmon/hwmon1/temp3_label", "Tccd1\n"),
            ("sys/class/hwmon/hwmon5/name", "asus\n"),
            ("sys/class/hwmon/hwmon5/fan1_input", "2900\n"),
            ("sys/class/hwmon/hwmon5/fan1_label", "cpu_fan\n"),
            ("sys/class/hwmon/hwmon5/fan2_input", "3100\n"),
            ("sys/class/hwmon/hwmon5/fan2_label", "gpu_fan\n"),
            ("sys/class/hwmon/hwmon6/name", "asus_custom_fan_curve\n"),
            ("sys/class/hwmon/hwmon6/pwm1_auto_point1_temp", "30\n"),
            ("sys/class/hwmon/hwmon6/pwm1_auto_point1_pwm", "0\n"),
            ("sys/class/hwmon/hwmon6/pwm1_auto_point2_temp", "70\n"),
            ("sys/class/hwmon/hwmon6/pwm1_auto_point2_pwm", "153\n"),
            ("sys/class/hwmon/hwmon6/pwm1_auto_point3_temp", "90\n"),
            ("sys/class/hwmon/hwmon6/pwm1_auto_point3_pwm", "255\n"),
            ("sys/class/hwmon/hwmon10/name", "nct6798\n"),
            ("sys/class/hwmon/hwmon10/pwm2", "128\n"),
            ("sys/class/hwmon/hwmon10/pwm2_auto_point1_temp", "40000\n"),
            ("sys/class/hwmon/hwmon10/pwm2_auto_point1_pwm", "64\n"),
            ("proc/cpuinfo", "vendor_id\t: AuthenticAMD\nmodel name\t: AMD Ryzen 9 7940HS w/ Radeon 780M Graphics\n"),
            ("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq", "4012000\n"),
            ("sys/devices/system/cpu/cpu0/cpufreq/scaling_driver", "amd-pstate-epp\n"),
            ("sys/devices/system/cpu/cpufreq/boost", "1\n"),
            ("sys/devices/system/cpu/amd_pstate/status", "active\n"),
            ("sys/class/drm/card0/device/vendor", "0x1002\n"),
            ("sys/class/drm/card0/device/gpu_busy_percent", "37\n"),
            ("sys/class/drm/card0/device/mem_info_vram_used", "1073741824\n"),
            ("sys/class/drm/card0/device/mem_info_vram_total", "4294967296\n"),
            ("sys/class/drm/card0/device/pp_dpm_sclk", "0: 800Mhz\n1: 1800Mhz *\n2: 2700Mhz\n"),
            ("sys/class/drm/card0/device/hwmon/hwmon4/name", "amdgpu\n"),
            ("sys/class/drm/card0/device/hwmon/hwmon4/temp1_input", "55000\n"),
            ("sys/class/drm/card0/device/hwmon/hwmon4/temp1_label", "edge\n"),
            ("sys/class/drm/card0/device/hwmon/hwmon4/power1_average", "18250000\n"),
            ("sys/block/nvme0n1/device/model", "Samsung SSD 990 PRO 2TB\n"),
            ("sys/block/nvme0n1/device/hwmon2/name", "nvme\n"),
            ("sys/block/nvme0n1/device/hwmon2/temp1_input", "44850\n"),
            ("sys/block/nvme0n1/device/hwmon2/temp1_label", "Composite\n"),
            ("sys/block/nvme0n1/device/hwmon2/temp2_input", "52850\n"),
            ("sys/block/nvme0n1/device/hwmon2/temp2_label", "Sensor 1\n"),
            ("sys/block/sda/device/model", "WDC WD40EFRX\n"),
            ("sys/block/sda/device/hwmon/hwmon9/name", "drivetemp\n"),
            ("sys/block/sda/device/hwmon/hwmon9/temp1_input", "36000\n"),
            ("sys/block/loop0/size", "0\n"),
            ("sys/firmware/acpi/platform_profile", "balanced\n"),
            ("sys/firmware/acpi/platform_profile_choices", "quiet balanced performance\n"),
            ("sys/devices/platform/asus-nb-wmi/throttle_thermal_policy", "1\n"),
            ("sys/class/leds/asus::kbd_backlight/brightness", "2\n"),
            ("sys/class/leds/asus::kbd_backlight/max_brightness", "3\n"),
            ("sys/class/dmi/id/sys_vendor", "ASUSTeK COMPUTER INC.\n"),
            ("sys/class/dmi/id/product_name", "ROG Zephyrus G14 GA402XV\n"),
        ]);
        link(
            &root,
            "sys/class/drm/card0/device/driver",
            "../../bus/pci/drivers/amdgpu",
        );
        let reading = read_all(&root, Duration::ZERO, false);
        assert_eq!(reading.cpu.package_celsius, Some(68.4));
        assert_eq!(reading.cpu.package_source, "k10temp Tctl");
        assert_eq!(reading.cpu.amd_pstate.as_deref(), Some("active"));
        assert_eq!(reading.cpu.boost, Some(true));
        let gpu = &reading.gpus[0];
        assert_eq!(
            (gpu.vendor.as_str(), gpu.driver.as_str()),
            ("AMD", "amdgpu")
        );
        assert_eq!(gpu.busy_percent, Some(37.0));
        assert_eq!(
            (gpu.vram_used_mib, gpu.vram_total_mib),
            (Some(1024.0), Some(4096.0))
        );
        assert_eq!(gpu.clock_mhz, Some(1800.0));
        assert_eq!(gpu.power_watts, Some(18.25));
        assert_eq!(gpu.temps[0].label, "edge");
        let names = reading
            .chips
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["k10temp", "asus", "asus_custom_fan_curve", "nct6798"],
            "hwmon10 sorts after hwmon6"
        );
        let fans = &reading.chips[1].fans;
        assert_eq!(
            fans[0],
            Fan {
                label: "cpu_fan".into(),
                rpm: 2900
            }
        );
        let curve = &reading.chips[2].curves[0];
        assert_eq!(curve.points, vec![(30.0, 0.0), (70.0, 60.0), (90.0, 100.0)]);
        assert_eq!(
            reading.chips[3].curves[0].points,
            vec![(40.0, 25.1)],
            "millidegree curves are converted"
        );
        assert!((reading.chips[3].pwms[0].percent - 50.2).abs() < 1e-9);
        let drives = reading
            .drives
            .iter()
            .map(|d| (d.name.as_str(), d.kind.as_str(), d.celsius))
            .collect::<Vec<_>>();
        assert_eq!(
            drives,
            vec![("nvme0n1", "nvme", Some(44.9)), ("sda", "sata", Some(36.0))],
            "loop devices are skipped"
        );
        assert!(reading.notes.is_empty());
        let platform = &reading.platform;
        assert_eq!(platform.profile.as_deref(), Some("balanced"));
        assert_eq!(
            platform.profile_choices,
            vec!["quiet", "balanced", "performance"]
        );
        let asus = platform.asus.as_ref().expect("asus");
        assert_eq!(asus.throttle_policy.as_deref(), Some("performance"));
        assert_eq!(
            (asus.keyboard_brightness, asus.keyboard_max_brightness),
            (Some(2), Some(3))
        );
        assert!(platform.product.contains("Zephyrus"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn utilization_uses_idle_and_iowait() {
        let a = "cpu  100 0 100 700 100 0 0 0 0 0\n";
        let b = "cpu  200 0 200 1300 100 0 0 0 0 0\n";
        assert_eq!(utilization(a, b), Some(25.0));
        assert_eq!(utilization(a, a), None);
        assert_eq!(utilization("garbage", b), None);
    }

    #[test]
    fn damaged_files_are_skipped_not_fatal() {
        let root = tree(&[
            ("sys/class/hwmon/hwmon0/name", "broken\n"),
            ("sys/class/hwmon/hwmon0/temp1_input", "not a number\n"),
            ("sys/class/hwmon/hwmon0/fan1_input", "\n"),
            ("sys/class/hwmon/hwmon1/temp1_input", "40000\n"),
        ]);
        let chips = read_chips(&root);
        assert_eq!(chips.len(), 1, "a chip without a name is skipped");
        assert!(chips[0].temps.is_empty() && chips[0].fans.is_empty());
        let empty = read_all(&root.join("missing"), Duration::ZERO, false);
        assert!(empty.chips.is_empty() && empty.drives.is_empty() && empty.gpus.is_empty());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn counts_are_capped() {
        let files = (0..100)
            .map(|n| {
                (
                    format!("sys/class/hwmon/hwmon0/temp{n}_input"),
                    "30000".to_owned(),
                )
            })
            .chain(std::iter::once((
                "sys/class/hwmon/hwmon0/name".to_owned(),
                "many".to_owned(),
            )))
            .collect::<Vec<_>>();
        let refs = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_str()))
            .collect::<Vec<_>>();
        let root = tree(&refs);
        assert_eq!(read_chips(&root)[0].temps.len(), MAX_ENTRIES);
        fs::remove_dir_all(root).expect("cleanup");
    }
}
