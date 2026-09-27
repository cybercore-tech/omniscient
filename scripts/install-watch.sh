#!/usr/bin/env bash
# Installs (or with --remove, removes) the hourly Omniscient watch as a
# systemd user timer. It runs `omniscient --watch` unprivileged, sends a
# desktop notification for each new problem, and publishes watch.json for the
# HUD bar widget.
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNITS=(omniscient-watch.service omniscient-watch.timer)

if [[ "${1:-}" == --remove ]]; then
    systemctl --user disable --now omniscient-watch.timer 2>/dev/null || true
    for unit in "${UNITS[@]}"; do
        rm -f -- "$UNIT_DIR/$unit"
    done
    systemctl --user daemon-reload
    echo "Omniscient watch removed."
    exit 0
fi

[[ -x "$HOME/.local/bin/omniscient" ]] || { echo "install omniscient first (./install.sh)" >&2; exit 1; }
mkdir -p "$UNIT_DIR"
for unit in "${UNITS[@]}"; do
    install -m 644 "$ROOT_DIR/systemd/$unit" "$UNIT_DIR/$unit"
done
systemd-analyze --user verify "$UNIT_DIR/omniscient-watch.service" "$UNIT_DIR/omniscient-watch.timer"
systemctl --user daemon-reload
systemctl --user enable --now omniscient-watch.timer
echo "Omniscient watch installed: hourly, unprivileged."
echo "Run now:   systemctl --user start omniscient-watch.service"
echo "Remove:    $0 --remove"
