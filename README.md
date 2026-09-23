# omniscient

A full-system audit tool for the CYBERDECK suite.

## Origin

`omniscient` started life as a fish shell function — an ASCII cyberdeck
header, a color-cycling "SCANNING..." HUD animation, an `fzf`-driven
module picker, and nine audit modules covering hardware, storage, btrfs
snapshots, network, containers, services, kernel logs, bluetooth, and
connected devices. It worked, and it looked good doing it.

This is the Rust rewrite: same look, same output locations, same
modules — but with real fixes for the rough edges the fish version
had accumulated. See [What changed](#what-changed-from-the-fish-version)
below.

## Install

    git clone https://github.com/darkstardevx/omniscient.git
    cd omniscient
    ./install.sh

Builds the release binary and drops it in `~/.local/bin/omniscient`.
On Omarchy, `~/.local/bin` is already on your `$PATH` — nothing else to
do. `install.sh` checks and tells you if it isn't.

## Use

    omniscient

Omniscient opens a full-screen Cybercore TUI with a capability matrix,
health score, module cards, live scan output, report paths, and a single
`sudo -v` prompt only when the selected modules need elevated access.

Keyboard controls:

- `↑` / `↓` — move through modules
- `Space` — select or clear the highlighted module
- `A` — select all modules / clear all
- `Enter` — run the selected modules
- `Tab` — switch between capability matrix and module details
- `R` — reset the dashboard
- `Q` / `Esc` — quit when no audit is running

The dashboard is always available in interactive terminals. For a
non-interactive environment, run the report modules from a real terminal
or use the underlying library interfaces.

Reports land in `~/.arch-sys/system/omniscient/`, same paths as the
original fish version:

    <slug>-<timestamp>/<slug>.md          — single module
    full_system_audit-<timestamp>/        — every module, plus SUMMARY.md

## What changed from the fish version

- **One sudo prompt**, not one per module.
- **No triple-duplicated switch/case** — every module implements a
  small `AuditModule` trait (`src/modules.rs`), and the menu,
  capability matrix, health score, and both audit paths all iterate
  the same list.
- **Real health scoring** (`src/health.rs`) — docks points for actual
  failed systemd units and SMART `FAILED` disks, not just missing
  tools.
- **No silent `2>/dev/null`** — a missing or failing command gets a
  visible note in the report instead of a blank section.
- **A generated `SUMMARY.md`** (`src/report.rs`) ties every module's
  report together with links. The fish version had no aggregation.
- **Your actual CYBERGRID hex palette** (`cybercore`) via
  true-color ANSI, not xterm-256 approximations.
- **Full-screen TUI** — `ratatui` + `crossterm` keep the dashboard
  responsive while audits run in a worker thread.
- **No `which` crate** — `src/pathcheck.rs` is a ~15-line hand-rolled
  PATH search, since that's all this project ever needed from it.

## Layout

    src/lib.rs        — module declarations
    src/main.rs        — entry point for the full-screen TUI
    src/tui.rs         — dashboard, keyboard controls, worker thread, and progress state
    src/modules.rs     — the AuditModule trait + all 9 modules
    src/health.rs      — scoring based on real signal
    src/report.rs      — SUMMARY.md generation
    src/hud.rs         — the scanning animation
    src/pathcheck.rs   — PATH lookup (replaces the `which` crate)

## Requirements

Rust (stable). `sudo` for the hardware/storage/snapshot modules.
Everything else the audit runs is optional — the capability matrix at
startup shows you exactly what's available on the machine you're
running it on, and missing tools just get a noted skip in the report
rather than an error.

## License

MIT
