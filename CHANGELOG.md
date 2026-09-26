# Changelog

All notable changes to Omniscient are documented here.

## [Unreleased]

### Added

- Cybercore scrollbars (`HudScrollBar.qml`) on every scrollable HUD view: the
  panel body, report index, report view, full report view, suggestions, fix
  queue and fix details. Shown only when content overflows.

### Fixed

- Code blocks in the report viewer were mangled (`20:/b>/font>14:00 …`): the
  highlighter chained regex replacements over its own HTML, so later rules
  matched inside earlier tags. It is now a single-pass tokenizer that escapes
  each piece once, `'` is escaped in links, and journal timestamps are no
  longer taken for `Label:` headings. A harness invariant (highlighting must
  never change the text) guards it.
- **The HUD could freeze the Omarchy desktop shell.** Opening a module report
  loaded the whole file into the shell and rendered it on the UI thread; a
  75 MB `omarchy.md` pushed the shell past 11 GB. The plugin now reads at most
  512 KiB of a report (`head -c`, truncation detected in UTF-8 bytes and
  explained), renders reports through virtualized list views (worst stall on
  real reports: 73 ms, down from 3.5 s), and renders the full-report view only
  while it is open.
- The snapshot reader no longer reassigns every property and list once a
  second; an unchanged snapshot causes no UI work. It polls every two seconds,
  reads at most 1 MiB, and caps lists and strings.
- The snapshot reader is a Quickshell `Singleton` instead of an `Item`, whose
  own `state` property was being driven as a Qt state machine.
- Overlay panels no longer sit inside a `ColumnLayout` (undefined layout
  behavior), and layout-managed items use implicit sizes.
- Module reports are bounded at the source: command capture has timeouts,
  per-stream memory limits, stdin from `/dev/null`, protection against
  grandchildren holding pipes, and terminal-escape/control-character
  cleaning; sections are capped at 256 KiB / 4,000 lines and reports at 2 MiB,
  with an explicit note of what was omitted. Journal queries are limited to
  500 entries.
- The Omarchy module calls `omarchy-debug` directly; the `omarchy debug`
  dispatcher route is rejected on current omarchy-dev builds.
- Health scoring saturates instead of overflowing on many failed units.

### Changed

- `clippy::pedantic` is denied crate-wide and `unsafe_code` is forbidden.
- `scripts/gate.sh full` adds all-target Clippy, doctests, strict rustdoc and
  the new plugin gate (`scripts/plugin-gate.sh`): Qt 6 `qmllint` at its
  strictest plus a runtime harness in a nested, memory-capped compositor. See
  [docs/TESTING.md](docs/TESTING.md).

- Continue hardening cross-distribution audit behavior.
- Added a dedicated `omniscient --packages` headless path and a separate
  Omarchy `PACKAGE SCAN` action so package verification is opt-in rather than
  part of every full HUD audit.
- Removed Package Integrity from the default headless full-audit selection;
  the standard HUD pass now scans the 16 operational modules and leaves the
  package module to its explicit action.
- Expanded Package Integrity reports with Arch Official, Omarchy, BlackArch,
  Chaotic AUR, and AUR/Foreign origin categories, installed versions, and
  `CURRENT` / `UPDATE AVAILABLE` status markers.
- Added clickable package-category filtering, report-index and module-card
  navigation, hover states, alternating rows, full-window report reading, and
  semantic Markdown/code highlighting to the Omarchy surface.
- Documented the package evidence model, focused scan behavior, report
  navigation, and read-only boundaries in the README, plugin guide, and
  security notes.

## [0.1.0] - 2026-09-23

### Added

- XDG-compatible per-user report paths with an explicit override variable.
- Full-screen Cybercore Ratatui dashboard.
- Nine selectable system-audit modules.
- Capability matrix with available, optional, missing, and privileged states.
- Health scoring from missing tools, failed systemd units, and SMART failures.
- Linked Markdown reports and a generated `SUMMARY.md`.
- Portable terminal `sudo` authorization with optional per-user Polkit support.
- Atomic local installer and reproducible Linux release packaging.
- CI quality gates and tag-triggered GitHub Releases.

### Fixed

- SMART checks no longer probe optical drives as disks.
- Optional desktop and package-manager tools no longer reduce the health score.
- Release installation no longer fails when an existing binary is running.
