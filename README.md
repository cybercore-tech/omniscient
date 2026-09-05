# omniscient

A full-system audit tool for the CYBERDECK suite. A Rust rewrite of an
original fish shell function of the same name — same look, same output
locations, but with the rough edges fixed.

## Build

    cargo build --release

The `Cargo.toml` pins several dependencies (`dialoguer`, `chrono`,
`anyhow`, and a few transitive crates: `zeroize`, `getrandom`,
`fastrand`, `rustix`) to older versions. That pinning was only needed
to get this building on an old apt-provided Rust toolchain (1.75) in a
throwaway sandbox — on a real machine with a current Rust via
`rustup`/`mise`, try loosening or removing those pins first
(`cargo update`), and only re-pin anything that actually breaks.

## Use

    ./target/release/omniscient

Same flow as the original: header, a capability matrix showing which
tools this run can actually use, a multi-select picker (module names,
or "Full System Audit" for everything), then a single `sudo -v` prompt
up front before anything runs — not scattered mid-scan like the fish
version.

Reports land in `~/.arch-sys/system/omniscient/`, same as before:

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
- **Your actual CYBERGRID hex palette** (`src/palette.rs`) via
  true-color ANSI, not xterm-256 approximations.
- **No `fzf` dependency** — `dialoguer`'s multi-select instead.
- **No `which` crate** — `src/pathcheck.rs` is a ~15-line hand-rolled
  PATH search, since that's all this project ever needed from it.

## Layout

    src/lib.rs        — module declarations
    src/main.rs        — entry point: header, matrix, menu, execution
    src/modules.rs     — the AuditModule trait + all 9 modules
    src/health.rs      — scoring based on real signal
    src/report.rs      — SUMMARY.md generation
    src/hud.rs         — the scanning animation
    src/palette.rs     — CYBERGRID true-color helpers
    src/pathcheck.rs   — PATH lookup (replaces the `which` crate)
