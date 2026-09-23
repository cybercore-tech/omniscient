# ⟦◈⟧ OMNISCIENT // CYBERCORE SYSTEM AUDIT

`omniscient` is the full-system audit console for the Cybercore family: a
Rust-powered, full-screen Ratatui dashboard that turns system inspection into
a readable, repeatable report.

It began as a fish-shell cyberdeck with an ASCII HUD, a scanning animation,
and an `fzf` module picker. The Rust release keeps that spirit while adding a
structured module registry, live progress, health scoring, capability
detection, privilege handling, and linked Markdown reports.

## ⟦◆⟧ INSTALLATION // BUILD + DEPLOY

### Requirements

- Linux with an interactive terminal
- Rust stable and Cargo
- `sudo` for privileged audit modules
- `systemd`, `lsblk`, and other audit tools are detected at runtime
- `pkexec` plus a graphical Polkit agent are optional

Clone and install:

```bash
git clone https://github.com/darkstardevx/omniscient.git
cd omniscient
./install.sh
```

The installer:

- builds a locked release binary;
- uses `~/.cargo-target/` by default;
- installs to `~/.local/bin/omniscient`;
- replaces an active binary atomically, so a running instance does not cause
  a `Text file busy` failure;
- checks whether `~/.local/bin` is available on your `PATH`.

To choose another Cargo build directory:

```bash
CARGO_TARGET_DIR="$HOME/.cache/omniscient-target" ./install.sh
```

## ⟦◇⟧ USAGE // RUN A SCAN

Start the dashboard from a real terminal:

```bash
omniscient
```

The application is intentionally interactive. It requires a TTY so it can
enter the full-screen dashboard and restore your terminal cleanly when it
exits.

### ⟦⌘⟧ Keyboard controls

| Key | Action |
| --- | --- |
| `↑` / `↓` or `j` / `k` | Move through audit modules |
| `Space` | Select or clear the highlighted module |
| `A` | Select all modules, or clear the current selection |
| `Enter` | Run the selected modules |
| `Tab` | Toggle capability matrix and module details |
| `R` | Reset the dashboard and selection |
| `Q` / `Esc` | Quit when an audit is not running |

Typical workflow:

1. Launch `omniscient`.
2. Select one or more modules with `Space`, or press `A` for a full audit.
3. Press `Enter`.
4. Authorize elevated modules if prompted.
5. Follow the live scan output and open the final `SUMMARY.md` path.

## ⟦⌬⟧ AUTH // PRIVILEGE MODES

The repository is distro-neutral. Terminal `sudo` is the default for every
user and every distribution:

```text
select privileged modules → dashboard pauses → sudo -v → dashboard resumes
```

Only the hardware, storage, Btrfs snapshot, and kernel-log modules request
elevated access. If authorization is canceled, the dashboard returns without
starting the audit.

Users with a graphical Polkit agent may opt in from their own shell:

```bash
export OMNISCIENT_AUTH=pkexec
omniscient
```

To force the portable terminal flow:

```bash
OMNISCIENT_AUTH=sudo omniscient
```

Any unsupported `OMNISCIENT_AUTH` value falls back to `sudo`. The project
does not impose an Omarchy or desktop-specific default on other users.

## ⟦▣⟧ MODULE GRID // WHAT GETS INSPECTED

- ⟦HW⟧ **Hardware Core** — CPU, hardware inventory, USB, and PCI data
- ⟦ST⟧ **Storage Matrix** — block devices and SMART health information
- ⟦BT⟧ **Btrfs Snapshots** — mounted Btrfs filesystems and subvolumes
- ⟦NW⟧ **Network Nexus** — interfaces and listening sockets
- ⟦CT⟧ **Container Realm** — Docker, Flatpak, and Snap inventory
- ⟦SV⟧ **Services & Daemons** — running and failed systemd services
- ⟦KL⟧ **Kernel Logs** — recent journal entries and kernel messages
- ⟦BD⟧ **Bluetooth Deep** — visible Bluetooth devices
- ⟦IO⟧ **Connected Devices** — displays and audio devices

The capability matrix marks tools as available, missing, optional, or
privileged before a scan starts. Missing optional tools are recorded as
skipped instead of being treated as a system failure.

## ⟦✦⟧ HEALTH // SIGNAL, NOT JUST INVENTORY

The health score starts at `100` and is adjusted using real signals:

- missing required tools;
- failed systemd units;
- disks reporting SMART health failure.

The score is a diagnostic signal, not a security certification or a warranty
that every system component is healthy.

## ⟦▤⟧ REPORTS // WHERE OUTPUT GOES

Reports are written beneath:

```text
~/.arch-sys/system/omniscient/
```

For a full scan:

```text
full_system_audit-YYYY-MM-DD_HH-MM-SS/
├── hardware-YYYY-MM-DD_HH-MM-SS/hardware.md
├── storage-YYYY-MM-DD_HH-MM-SS/storage.md
├── ...
└── SUMMARY.md
```

For a selected-module scan, reports are written directly beneath the same
Omniscient report root and the generated `SUMMARY.md` links to each result.
Command failures and unavailable tools remain visible in the Markdown output.

## ⟦⬡⟧ PROJECT MAP // DEVELOPMENT

```text
src/lib.rs         module declarations
src/main.rs        interactive entry point
src/tui.rs         dashboard, input, worker thread, and progress state
src/modules.rs     AuditModule trait and nine audit modules
src/elevation.rs   sudo default and pkexec opt-in backend selection
src/health.rs      health scoring from system signals
src/report.rs      SUMMARY.md generation
src/hud.rs         scanning animation helpers
src/pathcheck.rs   executable lookup without the which crate
install.sh         locked release build and atomic installation
LICENSE            MIT license
```

Run the local quality gates before publishing a change:

```bash
cargo fmt --all -- --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
bash -n install.sh
git diff --check
```

## ⟦⟐⟧ RELEASE STATUS

This is a Cybercore `0.1.x` release line: suitable for real local audits and
continued testing across Linux distributions. The tool reports what it can
observe and never silently treats unavailable commands as successful checks.

## ⟦©⟧ LICENSE

Released under the [MIT License](LICENSE).
