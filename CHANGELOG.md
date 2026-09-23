# Changelog

All notable changes to Omniscient are documented here.

## [Unreleased]

- Continue hardening cross-distribution audit behavior.

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
