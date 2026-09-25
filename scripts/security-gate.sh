#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# The plugin must not turn user-controlled strings into shell source.
if rg -n --glob '*.qml' --glob '*.rs' \
    '(^|[[:space:]])(sh|bash)[[:space:]]+-([lc])[[:space:]]|eval[[:space:]]*\(' \
    src omarchy-plugin; then
    echo "security gate: shell-mediated plugin or Rust execution detected" >&2
    exit 1
fi

# Keep system mutation limited to the reviewed package-install path.
if rg -n --glob '*.rs' \
    'systemctl[[:space:]]+(enable|start|restart|stop|disable)|sudoers|pacman[^\n]*-[Rr]' \
    src; then
    echo "security gate: unreviewed system mutation capability detected" >&2
    exit 1
fi

echo "security gate: PASS"
