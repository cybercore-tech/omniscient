#!/usr/bin/env bash
set -euo pipefail

echo "Building omniscient (release)..."
TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cargo-target}"
CARGO_TARGET_DIR="$TARGET_DIR" cargo build --release --locked
BIN_SRC="${TARGET_DIR}/release/omniscient"

if [[ ! -f "$BIN_SRC" ]]; then
    echo "Could not find built binary at: $BIN_SRC"
    echo "Check your cargo target-dir configuration."
    exit 1
fi

BIN_DST="$HOME/.local/bin/omniscient"
mkdir -p "$HOME/.local/bin"
# Replace atomically so a currently running instance does not trigger
# ETXTBSY ("Text file busy") while the new release is installed.
BIN_TMP="$(mktemp "${BIN_DST}.new.XXXXXX")"
trap 'rm -f "$BIN_TMP"' EXIT
install -m 755 "$BIN_SRC" "$BIN_TMP"
mv -f "$BIN_TMP" "$BIN_DST"

echo "Installed to $BIN_DST"

if command -v omniscient >/dev/null 2>&1; then
    echo "Done — run 'omniscient' to start the Cybercore dashboard."
else
    echo "Installed, but ~/.local/bin isn't on your \$PATH yet."
    echo "Add this to your shell config, then restart your shell:"
    echo ""
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
fi
