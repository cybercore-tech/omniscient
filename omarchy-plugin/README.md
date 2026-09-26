# Omniscient / Omarchy Plugin

This is the Quickshell HUD client for Omniscient's versioned local snapshot
contract. It runs the Rust audit engine's `--hud` mode in the panel process
and renders the resulting report index and Markdown content in place.

## Current surface

- Bar widget with live health score when a snapshot is available.
- Expanded overlay panel with separate `RUN FULL AUDIT` and `PACKAGE SCAN`
  actions, live health score, module states, privilege hints, report index,
  Markdown report viewer, and score-colored repair suggestions.
- `RUN FULL AUDIT` covers the 16 operational modules. Package Integrity is
  intentionally opt-in through `PACKAGE SCAN` because file verification can
  be materially slower than the normal system pass.
- Package Integrity has its own focused scan path. The report reader exposes
  `ALL`, `ARCH OFFICIAL`, `OMARCHY`, `BLACKARCH`, `CHAOTIC AUR`, and
  `AUR / FOREIGN` category buttons. Package versions and `CURRENT` /
  `UPDATE AVAILABLE` status tokens are color-coded in the reader.
- Module cards and report-index rows are clickable. Hover states, alternating
  rows, urgency borders, semantic Markdown colors, and a full-window report
  reader make the saved evidence navigable without leaving the HUD.
- Larger Cybercore typography and health levels: HEALTHY, WATCH, WARNING, and
  URGENT.
- Audit-generated `SUGGESTIONS.md` reports include commands, man pages, and
  documentation links. Allowlisted package installs require an explicit in-panel
  confirmation and produce a `fixes/` report after completion.
- Reads XDG_RUNTIME_DIR/omniscient/snapshot.json, falling back to
  ~/.local/state/omniscient/snapshot.json.
- Polls the snapshot every two seconds but updates the panel only when its
  contents change, and degrades to a waiting state when the snapshot is
  absent, invalid, or larger than 1 MiB.
- Reads at most 512 KiB of a report and renders it through virtualized list
  views, so opening a report costs the same whatever its size; a longer report
  says where the complete file is. Privileged modules use the configured graphical
  `pkexec` backend when launched from the HUD.

## Security model

The plugin runs inside Omarchy's unsandboxed shell process. Read the
[repository security model](../SECURITY.md) before enabling it. The HUD uses
direct process arguments (no shell interpolation), requests one explicit
Polkit authorization for a scan, and exposes only the allowlisted package
repair actions documented there. It does not install services, edit sudoers,
or contact a network service at runtime.

## Local validation

Validate this folder with:

    omarchy plugin validate /path/to/omniscient/omarchy-plugin

If installing into a user-owned plugin directory for local testing, copy the
folder to:

    ~/.config/omarchy/plugins/io.github.cybercore-tech.omniscient

Then enable it with:

    omarchy plugin enable io.github.cybercore-tech.omniscient right

The plugin does not edit shell.json. `RUN FULL AUDIT` starts the explicit
user-requested `omniscient --hud` audit. `PACKAGE SCAN` starts
`omniscient --packages`, which selects only Package Integrity so long package
verification does not surprise users who asked for the normal audit. Both
actions leave the existing interactive terminal dashboard available as a
separate mode and publish the same versioned snapshot contract.
