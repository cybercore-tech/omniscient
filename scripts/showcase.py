#!/usr/bin/env python3
"""Synthetic, privacy-safe data for the README / GitHub Pages screenshots.

Nothing here comes from a real machine: the host is "cyberdeck", the user is
"operator", addresses use documentation ranges (192.0.2.0/24, 2001:db8::) and
locally administered MACs (02:...). The hardware is a Ryzen 9 + Radeon ROG
Zephyrus with two NVMe drives and a SATA disk, so every tab has something to
show. Usage: showcase.py <out-dir> <real omniscient binary>
"""

import json
import math
import os
import stat
import sys
import time

OUT = os.path.abspath(sys.argv[1])
REAL = os.path.abspath(sys.argv[2])
NOW = int(time.time())
DAY = 86400
STAMP = "2026-09-26_20-14-02"
STATE = os.path.join(OUT, "state", "omniscient")
AUDIT = os.path.join(STATE, f"full_system_audit-{STAMP}")

MODULES = [
    ("Hardware Core", "hardware", True), ("Storage Matrix", "disks", True),
    ("Btrfs Snapshots", "snapshots", True), ("Network Nexus", "network", False),
    ("Containers", "containers", False), ("Services", "services", False),
    ("Kernel & Logs", "logs", True), ("Bluetooth", "bluetooth", False),
    ("Connected Devices", "devices", False), ("Security Posture", "security", False),
    ("Accounts & Auth", "accounts", False), ("Persistence Watch", "persistence", False),
    ("Package Integrity", "packages", False), ("Recovery Readiness", "recovery", False),
    ("Reliability Signals", "reliability", False), ("Performance Pulse", "performance", False),
    ("Omarchy Surface", "omarchy", False), ("Deep Signals", "signals", True),
]


def write(path, text, mode=None):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text)
    if mode:
        os.chmod(path, mode)


def report_path(slug):
    name = "storage.md" if slug == "disks" else f"{slug}.md"
    return os.path.join(AUDIT, f"{slug}-{STAMP}", name)


def fenced(title, sections):
    out = [f"# {title}", ""]
    for heading, body in sections:
        out += [f"## {heading}", "", "```", body.strip("\n"), "```", ""]
    return "\n".join(out)


SIGNALS = fenced("🧠 DEEP SIGNALS", [
    ("findings", """
[URGENT] telemetry-bridge.service restarted 184 times / The system unit keeps failing and being restarted by systemd.
    inspect: systemctl status telemetry-bridge.service --no-pager -l
[WARNING] sigilward-check.timer: last run of sigilward-check.service failed / result exit-code; the scheduled job is not doing its work.
    inspect: journalctl -u sigilward-check.service -n 50 --no-pager
[WARNING] Reboot needed: the running kernel was upgraded / Running 6.18.49-3-lts, but its modules are no longer installed (installed: 6.18.52-1-lts).
    inspect: uname -r; ls /usr/lib/modules
[WARNING] chromium crashed 7 times in 7 days / /usr/lib/chromium/chromium keeps crashing; the core dumps hold the backtrace.
    inspect: coredumpctl info /usr/lib/chromium/chromium
[WATCH] / last scrubbed 52 days ago / Monthly scrubs are the usual recommendation.
    inspect: sudo btrfs scrub start /
[WATCH] 2 configuration update(s) waiting to be merged / pacman installed new default configs next to files you changed.
    inspect: pacdiff -o
[WATCH] BAT0 holds 82% of its design capacity / Wear trend: -0.6 points per 30 days.
"""),
    ("update hygiene / reboot and config merges", """
running kernel: 6.18.49-3-lts
installed kernels: 6.18.52-1-lts, 7.2.3-arch1-3
running kernel modules: MISSING (reboot needed)
processes using replaced libraries (6):
  /usr/bin/sshd ×1
  /usr/bin/NetworkManager ×1
  /usr/lib/systemd/systemd-resolved ×1
unmerged .pacnew/.pacsave files (2):
  /etc/pacman.conf.pacnew
  /etc/mkinitcpio.conf.pacnew
"""),
    ("pressure stall information (PSI)", """
share of time tasks waited on each resource; load average only estimates this
cpu    some 4.12 / 3.87 / 2.95   full 0.00 / 0.00 / 0.00   (avg 10s / 60s / 300s)
memory some 0.00 / 0.02 / 0.01   full 0.00 / 0.00 / 0.00   (avg 10s / 60s / 300s)
io     some 1.20 / 0.84 / 0.61   full 0.44 / 0.31 / 0.22   (avg 10s / 60s / 300s)
"""),
    ("crash trends / last 7 days", """
   7 ×  /usr/lib/chromium/chromium   last 2026-09-26 18:02   signals SIGTRAP
   1 ×  /usr/bin/wireplumber   last 2026-09-24 09:41   signals SIGSEGV
"""),
    ("drive wear and temperature / SMART and NVMe health", """
/dev/nvme0n1: temperature 44°C; NVMe wear 3% used, spare 100% (threshold 10%), media errors 0, unsafe shutdowns 11
/dev/nvme1n1: temperature 51°C; NVMe wear 9% used, spare 100% (threshold 10%), media errors 0, unsafe shutdowns 4
/dev/sda: temperature 36°C; ATA reallocated 0, pending 0, offline uncorrectable 0
"""),
    ("kernel taint", """
tainted = 4096
  O  out-of-tree module loaded
"""),
    ("cybercore ecosystem / your security and network tools", """
SigilWard:
  system sigilward-check.service            failed    result exit-code  last exit Sat 2026-09-26 06:00:03
Argus:
  system argus.service                      active    result success    last exit —
VortexWall:
  system vortexwall.service                 active    result success    last exit —
WraithFlow:
  system wraithflow.service                 active    result success    last exit —
"""),
])

CHANGES = """# 🔀 CHANGES SINCE THE LAST AUDIT

Compared with `full_system_audit-2026-09-25_19-15-11`.

## findings

```
NEW       [URGENT] telemetry-bridge.service restarted 184 times
NEW       [WARNING] Reboot needed: the running kernel was upgraded
CHANGED   [WATCH → WARNING] chromium crashed 7 times in 7 days
RESOLVED  [WARNING] / metadata is 91% full
RESOLVED  [WATCH] 58 program(s) still run replaced libraries
```

## inventory

```
INSTALLED PACKAGES
  added (3):
  linux-lts 6.18.52-1
  mesa 1:26.2.1-1
  telemetry-bridge 0.4.0-1
  removed (2):
  linux-lts 6.18.49-3
  mesa 1:26.1.4-2
LISTENING SOCKETS
  added (1):
  tcp 0.0.0.0:9464
UNMERGED CONFIG FILES
  added (1):
  /etc/mkinitcpio.conf.pacnew
```
"""

PACKAGES = fenced("🧾 PACKAGE INTEGRITY", [
    ("package repository report", """
ARCH OFFICIAL / 1247 packages
linux-lts 6.18.52-1 CURRENT
mesa 1:26.2.1-1 CURRENT
systemd 258.3-1 UPDATE AVAILABLE → 258.4-1
pipewire 1:1.4.8-1 CURRENT
OMARCHY / 38 packages
omarchy 4.1.2-1 CURRENT
omarchy-shell 4.1.2-1 UPDATE AVAILABLE → 4.1.3-1
BLACKARCH / 12 packages
nmap 7.98-1 CURRENT
CHAOTIC AUR / 9 packages
brave-bin 1.84.2-1 CURRENT
AUR / FOREIGN / 21 packages
telemetry-bridge 0.4.0-1 CURRENT
"""),
    ("package file integrity", """
1327 packages verified by 4 parallel pacman -Qkk workers

backup file: sudo: /etc/sudoers (Modification time mismatch)
backup file: pacman: /etc/pacman.conf (Size mismatch)
warning: telemetry-bridge: /usr/lib/telemetry-bridge/config.toml (Permissions mismatch)
1327 packages checked: 0 missing files, 1 altered file
"""),
])

STORAGE = fenced("💾 STORAGE MATRIX", [
    ("lsblk", """
NAME          SIZE MODEL                     TYPE MOUNTPOINT
nvme0n1       1.8T Samsung SSD 990 PRO 2TB   disk
├─nvme0n1p1     2G                           part /boot
└─nvme0n1p2   1.8T                           part
  └─cryptroot 1.8T                           crypt /
nvme1n1     931.5G WD_BLACK SN850X 1TB       disk
└─nvme1n1p1 931.5G                           part /data
sda           3.6T WDC WD40EFRX-68N32N0      disk
"""),
    ("btrfs filesystem usage /", """
Device size:          1.82TiB
Device allocated:   612.04GiB
Used:               588.15GiB
Free (estimated):     1.24TiB      (min: 1.24TiB)
Metadata,DUP: Size:4.00GiB, Used:2.61GiB (65.25%)
"""),
])


def snapshot():
    reports = [report_path(slug) for _, slug, _ in MODULES]
    reports.append(os.path.join(AUDIT, "CHANGES.md"))
    suggestions = [
        {"id": "signal:restart-loop:system:telemetry-bridge.service", "severity": "urgent",
         "title": "telemetry-bridge.service restarted 184 times",
         "detail": "The system unit keeps failing and being restarted by systemd.",
         "explanation": "Found by Omniscient's deep signals. Inspect it with the command below before changing anything; Omniscient does not repair these automatically.",
         "command": "systemctl status telemetry-bridge.service --no-pager -l",
         "manual_steps": ["systemctl status telemetry-bridge.service --no-pager -l",
                          "journalctl -u telemetry-bridge.service -b -n 50 --no-pager",
                          "systemctl cat telemetry-bridge.service"],
         "man_url": "https://man.archlinux.org/man/systemctl", "docs_url": "https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html#Restart=",
         "auto_fix": False, "auto_fix_reason": "Deep-signal findings are guidance only; no automatic repair is offered.", "requires_auth": False},
        {"id": "install-tool:smartctl", "severity": "warning", "title": "Install missing tool: smartctl",
         "detail": "Omniscient could not find `smartctl`. Install its Arch package to improve audit coverage.",
         "explanation": "Without smartmontools, drive health, wear and error counters cannot be read. Installing the allowlisted package restores that coverage.",
         "command": "sudo /usr/bin/pacman -S --needed smartmontools",
         "manual_steps": ["command -v smartctl || pacman -Ss smartmontools", "sudo /usr/bin/pacman -S --needed smartmontools", "smartctl --help"],
         "man_url": "https://man.archlinux.org/man/smartctl.8.en", "docs_url": "https://wiki.archlinux.org/title/S.M.A.R.T.",
         "auto_fix": True, "auto_fix_reason": "The allowlisted package `smartmontools` can be installed after explicit authorization.", "requires_auth": True},
        {"id": "signal:reboot-needed", "severity": "warning", "title": "Reboot needed: the running kernel was upgraded",
         "detail": "Running 6.18.49-3-lts, but its modules are no longer installed.",
         "explanation": "Found by Omniscient's deep signals. New drivers and USB devices may fail to load until you reboot.",
         "command": "uname -r; ls /usr/lib/modules", "manual_steps": ["uname -r", "ls /usr/lib/modules"],
         "man_url": "https://man.archlinux.org/man/uname", "docs_url": "https://wiki.archlinux.org/title/Kernel",
         "auto_fix": False, "auto_fix_reason": "Deep-signal findings are guidance only.", "requires_auth": False},
        {"id": "signal:pacnew", "severity": "attention", "title": "2 configuration update(s) waiting to be merged",
         "detail": "pacman installed new default configs next to files you changed.",
         "explanation": "Found by Omniscient's deep signals.", "command": "pacdiff -o", "manual_steps": ["pacdiff -o"],
         "man_url": "https://man.archlinux.org/man/pacdiff", "docs_url": "https://wiki.archlinux.org/title/Pacman/Pacnew_and_Pacsave",
         "auto_fix": False, "auto_fix_reason": "Guidance only.", "requires_auth": False},
    ]
    return {
        "schema_version": 1, "application": "omniscient", "state": "complete",
        "updated_at": "2026-09-26T20:15:48-07:00", "host": "cyberdeck",
        "selected_count": 17, "completed_count": 17,
        "health": {"score": 78, "notes": ["signal [urgent]: telemetry-bridge.service restarted 184 times"]},
        "modules": [{"name": n, "slug": s, "state": "idle" if s == "packages" else "complete", "selected": s != "packages", "requires_sudo": r} for n, s, r in MODULES],
        "reports": reports, "suggestions": suggestions,
        "suggestions_path": os.path.join(AUDIT, "SUGGESTIONS.md"),
        "summary_path": os.path.join(AUDIT, "SUMMARY.md"), "error": None,
        "message": "AUDIT COMPLETE / 17 modules on 4 workers / Deep Signals refined health",
    }


def watch():
    findings = [
        ("restart-loop:system:telemetry-bridge.service", "urgent", "telemetry-bridge.service restarted 184 times", True, "telemetry-bridge.service"),
        ("timer-failed:system:sigilward-check.timer", "warning", "sigilward-check.timer: last run of sigilward-check.service failed", False, "sigilward-check.service"),
        ("reboot-needed", "warning", "Reboot needed: the running kernel was upgraded", True, ""),
        ("crash:chromium", "warning", "chromium crashed 7 times in 7 days", False, ""),
        ("btrfs-scrub:/", "watch", "/ last scrubbed 52 days ago", False, ""),
    ]
    return {"version": 1, "updated_at": "2026-09-26T20:58:31-07:00", "worst": "urgent",
            "counts": {"urgent": 1, "warning": 3, "watch": 1}, "new_count": 2, "resolved_count": 1,
            "findings": [{"key": k, "severity": s, "title": t, "detail": "", "new": n, "unit": u} for k, s, t, n, u in findings]}


def journal():
    base = 1790480000
    lines = []
    def add(t, prio, unit, msg, comm=None):
        entry = {"__CURSOR": f"s=showcase;i={len(lines)}", "__REALTIME_TIMESTAMP": str((base + t) * 1000000),
                 "PRIORITY": str(prio), "_PID": str(1200 + len(lines) * 7), "_BOOT_ID": "5e1ec7ab1e5e1ec7ab1e5e1ec7ab1e5e",
                 "MESSAGE": msg}
        if unit:
            entry["_SYSTEMD_UNIT"] = unit
        else:
            entry["SYSLOG_IDENTIFIER"] = comm or "kernel"
        lines.append(json.dumps(entry))
    t = 0
    for n in range(6):
        add(t, 3, "telemetry-bridge.service", f"Main process exited, code=exited, status=1/FAILURE (attempt {178 + n})"); t += 2
        add(t, 4, "telemetry-bridge.service", "Scheduled restart job, restart counter is at %d." % (178 + n)); t += 30
    for n in range(24):
        add(t, 4, None, f"[UFW BLOCK] IN=wlan0 OUT= MAC=02:00:5e:10:00:{n:02x} SRC=192.0.2.{10 + n} DST=224.0.0.251 PROTO=UDP SPT=5353 DPT=5353", "kernel"); t += 3
    add(t, 3, "sigilward-check.service", "integrity drift: 3 files changed since baseline (/etc/sudoers.d/10-operator ...)"); t += 5
    add(t, 3, "sigilward-check.service", "Failed with result 'exit-code'."); t += 40
    add(t, 4, None, "Snapshot limit mismatch: 6 Snapper snapshots exceed configured MAX_SNAPSHOT_ENTRIES=5", "limine-snapper-sync"); t += 60
    add(t, 4, "NetworkManager.service", "<warn>  [1790480123.4411] device (wlan0): supplicant interface state: completed -> disconnected"); t += 8
    add(t, 5, "NetworkManager.service", "<info>  [1790480131.9921] device (wlan0): Activation: successful, device activated."); t += 20
    add(t, 3, None, "nvme nvme1: I/O tag 12 (100c) opcode 0x2 (Admin Cmd) QID 0 timeout, reset controller", "kernel"); t += 4
    add(t, 4, "bluetooth.service", "src/device.c:device_set_wake_support() Unable to set wake_support without RPA resolution"); t += 30
    for n in range(8):
        add(t, 4, None, f"[UFW BLOCK] IN=wlan0 OUT= MAC=02:00:5e:20:00:{n:02x} SRC=192.0.2.{80 + n} DST=192.0.2.1 PROTO=TCP SPT={40000 + n} DPT=22", "kernel"); t += 2
    add(t, 2, None, "mce: [Hardware Error]: Machine check events logged", "kernel"); t += 3
    add(t, 4, "systemd-resolved.service", "Using degraded feature set UDP instead of UDP+EDNS0 for DNS server 192.0.2.53."); t += 9
    for n in range(3):
        add(t, 3, "telemetry-bridge.service", f"Main process exited, code=exited, status=1/FAILURE (attempt {184 + n})"); t += 31
    return lines


def journal_tools():
    entries = "\n".join(journal()) + "\n"
    write(os.path.join(OUT, "journal-entries.json"), entries)
    write(os.path.join(OUT, "journal-boots.json"), json.dumps([
        {"index": 0, "boot_id": "5e1ec7ab1e5e1ec7ab1e5e1ec7ab1e5e", "first_entry": (NOW - 5 * 3600) * 1000000},
        {"index": -1, "boot_id": "0ddba11c0ffee0ddba11c0ffee0ddba1", "first_entry": (NOW - 2 * DAY) * 1000000},
        {"index": -2, "boot_id": "fee1dead0ddba11fee1dead0ddba11fe", "first_entry": (NOW - 5 * DAY) * 1000000},
        {"index": -3, "boot_id": "c0ffee00c0ffee00c0ffee00c0ffee00", "first_entry": (NOW - 9 * DAY) * 1000000}]) + "\n")
    offenders = []
    for unit, count in [("telemetry-bridge.service", 368), ("kernel", 212), ("sigilward-check.service", 24),
                        ("NetworkManager.service", 17), ("limine-snapper-sync", 12), ("bluetooth.service", 9),
                        ("systemd-resolved.service", 6), ("pipewire.service", 3)]:
        field = "SYSLOG_IDENTIFIER" if unit in ("kernel", "limine-snapper-sync") else "_SYSTEMD_UNIT"
        offenders += [json.dumps({field: unit})] * count
    write(os.path.join(OUT, "journal-offenders.json"), "\n".join(offenders) + "\n")
    write(os.path.join(OUT, "bin", "journalctl"), "#!/bin/sh\n"
          "case \" $* \" in\n"
          f"  *' --list-boots '*) exec cat '{OUT}/journal-boots.json' ;;\n"
          f"  *'--output-fields='*) exec cat '{OUT}/journal-offenders.json' ;;\n"
          "  *' --after-cursor '*) exit 0 ;;\n"
          f"  *) exec cat '{OUT}/journal-entries.json' ;;\n"
          "esac\n", 0o755)


def history():
    directory = os.path.join(STATE, "history")
    def tsv(name, rows):
        write(os.path.join(directory, f"{name}.tsv"), "".join(f"{e}\t{k}\t{v}\n" for e, k, v in rows))
    health = [92, 91, 93, 90, 88, 89, 86, 90, 91, 87, 84, 85, 82, 86, 80, 78]
    tsv("health", [(NOW - (30 - n * 2) * DAY, "score", s) for n, s in enumerate(health)])
    watch_rows = []
    for h in range(0, 30 * 24, 6):
        e = NOW - (30 * 24 - h) * 3600
        watch_rows.append((e, "alerts", max(0, round(1.5 + h / 180 + math.sin(h / 17) * 1.2))))
        watch_rows.append((e, "cpu", round(52 + 9 * math.sin(h / 11) + 6 * math.sin(h / 37) + (h % 24 > 12) * 4, 1)))
    tsv("watch", watch_rows)
    tsv("battery", [(NOW - (30 - n * 3) * DAY, "BAT0", round(83.4 - n * 0.14, 2)) for n in range(11)])
    tsv("shell-memory", [(NOW - (48 - h) * 3600, "3301", 356000 + h * 1400 + (h % 5) * 2000) for h in range(0, 48, 2)])


def sysroot(root, phase):
    """Hardware readings; `phase` animates them for the live sparklines."""
    wave = math.sin(phase / 3.0)
    files = {
        "proc/cpuinfo": "vendor_id\t: AuthenticAMD\nmodel name\t: AMD Ryzen 9 7940HS w/ Radeon 780M Graphics\n",
        "proc/meminfo": "MemTotal: 32245000 kB\nMemAvailable: 19870000 kB\nSwapTotal: 16777216 kB\nSwapFree: 16240000 kB\n",
        "sys/devices/system/cpu/cpufreq/boost": "1", "sys/devices/system/cpu/amd_pstate/status": "active",
        "sys/devices/system/cpu/cpu0/cpufreq/scaling_driver": "amd-pstate-epp",
        "sys/devices/system/cpu/cpu0/cpufreq/scaling_governor": "powersave",
        "sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference": "balance_performance",
        "sys/devices/system/cpu/cpu0/cpufreq/energy_performance_available_preferences": "default performance balance_performance balance_power power",
        "sys/class/hwmon/hwmon1/name": "k10temp",
        "sys/class/hwmon/hwmon1/temp1_label": "Tctl", "sys/class/hwmon/hwmon1/temp1_input": str(int((61 + 7 * wave + 3 * math.sin(phase)) * 1000)),
        "sys/class/hwmon/hwmon1/temp3_label": "Tccd1", "sys/class/hwmon/hwmon1/temp3_input": str(int((57 + 6 * wave) * 1000)),
        "sys/class/hwmon/hwmon2/name": "acpitz", "sys/class/hwmon/hwmon2/temp1_input": "47000", "sys/class/hwmon/hwmon2/temp1_crit": "105000",
        "sys/class/hwmon/hwmon5/name": "asus",
        "sys/class/hwmon/hwmon5/fan1_label": "cpu_fan", "sys/class/hwmon/hwmon5/fan1_input": str(int(2700 + 600 * wave)),
        "sys/class/hwmon/hwmon5/fan2_label": "gpu_fan", "sys/class/hwmon/hwmon5/fan2_input": str(int(2500 + 500 * wave)),
        "sys/class/hwmon/hwmon6/name": "asus_custom_fan_curve",
        "sys/class/drm/card1/device/vendor": "0x1002",
        "sys/class/drm/card1/device/gpu_busy_percent": str(int(max(0, 34 + 26 * math.sin(phase / 2.0)))),
        "sys/class/drm/card1/device/mem_info_vram_used": str(1_380_000_000 + int(200_000_000 * wave)),
        "sys/class/drm/card1/device/mem_info_vram_total": "4294967296",
        "sys/class/drm/card1/device/pp_dpm_sclk": "0: 800Mhz\n1: 2100Mhz *\n2: 2700Mhz\n",
        "sys/class/drm/card1/device/hwmon/hwmon4/name": "amdgpu",
        "sys/class/drm/card1/device/hwmon/hwmon4/temp1_label": "edge", "sys/class/drm/card1/device/hwmon/hwmon4/temp1_input": str(int((53 + 5 * wave) * 1000)),
        "sys/class/drm/card1/device/hwmon/hwmon4/power1_average": str(int((16 + 6 * wave) * 1_000_000)),
        "sys/block/nvme0n1/device/model": "Samsung SSD 990 PRO 2TB",
        "sys/block/nvme0n1/device/hwmon2/name": "nvme",
        "sys/block/nvme0n1/device/hwmon2/temp1_label": "Composite", "sys/block/nvme0n1/device/hwmon2/temp1_input": "44850",
        "sys/block/nvme0n1/device/hwmon2/temp2_label": "Sensor 1", "sys/block/nvme0n1/device/hwmon2/temp2_input": "52850",
        "sys/block/nvme0n1/device/hwmon2/temp2_crit": "84850",
        "sys/block/nvme1n1/device/model": "WD_BLACK SN850X 1000GB",
        "sys/block/nvme1n1/device/hwmon3/name": "nvme",
        "sys/block/nvme1n1/device/hwmon3/temp1_label": "Composite", "sys/block/nvme1n1/device/hwmon3/temp1_input": "61850",
        "sys/block/sda/device/model": "WDC WD40EFRX-68N32N0",
        "sys/block/sda/device/hwmon/hwmon9/name": "drivetemp", "sys/block/sda/device/hwmon/hwmon9/temp1_input": "36000",
        "sys/firmware/acpi/platform_profile": "balanced",
        "sys/firmware/acpi/platform_profile_choices": "quiet balanced performance",
        "sys/devices/platform/asus-nb-wmi/throttle_thermal_policy": "0",
        "sys/class/leds/asus::kbd_backlight/brightness": "2", "sys/class/leds/asus::kbd_backlight/max_brightness": "3",
        "sys/class/dmi/id/sys_vendor": "ASUSTeK COMPUTER INC.", "sys/class/dmi/id/product_name": "ROG Zephyrus G14 GA402XV",
        "sys/class/power_supply/BAT0/type": "Battery", "sys/class/power_supply/BAT0/status": "Discharging",
        "sys/class/power_supply/BAT0/capacity": "82", "sys/class/power_supply/BAT0/power_now": str(int((14 + 3 * wave) * 1_000_000)),
    }
    for point, (t, p) in enumerate([(30, 0), (50, 40), (65, 110), (75, 160), (85, 215), (95, 255)], start=1):
        files[f"sys/class/hwmon/hwmon6/pwm1_auto_point{point}_temp"] = str(t)
        files[f"sys/class/hwmon/hwmon6/pwm1_auto_point{point}_pwm"] = str(p)
    for cpu in range(16):
        mhz = 2400 + 1600 * (0.5 + 0.5 * math.sin(phase / 2 + cpu * 0.7))
        files[f"sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_cur_freq"] = str(int(mhz * 1000))
    busy = int(1000 * phase * (0.25 + 0.2 * (1 + wave)))
    files["proc/stat"] = f"cpu  {10000 + busy} 0 {5000 + busy // 3} {400000 + int(3000 * phase)} 0 0 0 0 0 0\n"
    for path, contents in files.items():
        write(os.path.join(root, path), contents + ("" if contents.endswith("\n") else "\n"))


def main():
    if len(sys.argv) > 3 and sys.argv[3] == "--animate":
        # Rewrite the hardware tree forever (the showcase script kills it).
        # CPU counters move every 100 ms so the 250 ms utilization sample
        # always sees work; the rest of the tree changes once a second.
        phase = 0.0
        started = time.time()
        busy_total = idle_total = 0.0
        stat_path = os.path.join(OUT, "sysroot", "proc", "stat")
        while True:
            sysroot(os.path.join(OUT, "sysroot"), phase)
            for _ in range(10):
                # Counters only ever grow, like the kernel's.
                load = 0.35 + 0.25 * math.sin((time.time() - started) / 5.0)
                busy_total += 100 * load
                idle_total += 100 * (1 - load)
                write(stat_path, f"cpu  {10000 + int(busy_total)} 0 5000 {400000 + int(idle_total)} 0 0 0 0 0 0\n")
                time.sleep(0.1)
            phase += 1.0
    for name, slug, _ in MODULES:
        write(report_path(slug), fenced(f"{name.upper()}", [("summary", f"{name}: complete\nhost cyberdeck, user operator\n")]))
    write(report_path("signals"), SIGNALS)
    write(report_path("packages"), PACKAGES)
    write(report_path("disks"), STORAGE)
    write(os.path.join(AUDIT, "CHANGES.md"), CHANGES)
    write(os.path.join(AUDIT, "SUMMARY.md"), "# 🛰️ Omniscient Audit\n\n**Health score:** 🟠 78/100\n")
    write(os.path.join(AUDIT, "SUGGESTIONS.md"), "# 🧰 Omniscient Fix Suggestions\n")
    run = os.path.join(OUT, "run", "omniscient")
    os.makedirs(run, exist_ok=True)
    os.chmod(os.path.join(OUT, "run"), 0o700)
    write(os.path.join(run, "snapshot.json"), json.dumps(snapshot()))
    write(os.path.join(run, "watch.json"), json.dumps(watch()))
    journal_tools()
    history()
    sysroot(os.path.join(OUT, "sysroot"), 0.0)
    write(os.path.join(OUT, "home", ".local", "bin", "omniscient"), "#!/bin/sh\n"
          f"export OMNISCIENT_SYSFS_ROOT='{OUT}/sysroot' OMNISCIENT_REPORT_DIR='{STATE}' PATH='{OUT}/bin:/usr/bin:/bin'\n"
          f"exec '{REAL}' \"$@\"\n", 0o755)


if __name__ == "__main__":
    main()
