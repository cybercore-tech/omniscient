# Omniscient / Omarchy Plugin

This is the Quickshell HUD client for Omniscient's versioned local snapshot
contract. It runs the Rust audit engine's `--hud` mode in the panel process
and renders the resulting report index and Markdown content in place.

## Current surface

- Bar widget with live health score when a snapshot is available.
- Expanded overlay panel with a run-in-place audit action, live health score,
  module states, privilege hints, report index, and Markdown report viewer.
- Reads XDG_RUNTIME_DIR/omniscient/snapshot.json, falling back to
  ~/.local/state/omniscient/snapshot.json.
- Refreshes once per second and degrades to a waiting state when the snapshot
  is absent or invalid. Privileged modules use the configured graphical
  `pkexec` backend when launched from the HUD.

## Local validation

Validate this folder with:

    omarchy plugin validate /path/to/omniscient/omarchy-plugin

If installing into a user-owned plugin directory for local testing, copy the
folder to:

    ~/.config/omarchy/plugins/io.github.cybercore-tech.omniscient

Then enable it with:

    omarchy plugin enable io.github.cybercore-tech.omniscient right

The plugin does not edit shell.json. The `RUN AUDIT HERE` action starts the
explicit user-requested `omniscient --hud` audit and leaves the existing
interactive terminal dashboard available as a separate mode.
