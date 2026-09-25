# Omniscient / Omarchy Plugin

This is the thin Quickshell client for Omniscient's versioned local snapshot
contract. It stays separate from the Rust audit engine and never performs
privileged inspection inside the Omarchy shell.

## Current surface

- Bar widget with live health score when a snapshot is available.
- Overlay panel with audit state, health score, module states, privilege hints,
  report path, and snapshot freshness.
- Reads XDG_RUNTIME_DIR/omniscient/snapshot.json, falling back to
  ~/.local/state/omniscient/snapshot.json.
- Refreshes once per second and degrades to a waiting state when the snapshot
  is absent or invalid.

## Local validation

Validate this folder with:

    omarchy plugin validate /path/to/omniscient/omarchy-plugin

If installing into a user-owned plugin directory for local testing, copy the
folder to:

    ~/.config/omarchy/plugins/io.github.cybercore-tech.omniscient

Then enable it with:

    omarchy plugin enable io.github.cybercore-tech.omniscient right

The plugin intentionally does not edit shell.json or start privileged scans.
