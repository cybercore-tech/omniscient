#!/usr/bin/env bash
# ./install.sh           install to ~/.local/bin (everything but elevated checks)
# ./install.sh --system  also install the root-owned copy the privileged
#                        helper runs from (/usr/local/bin/omniscient); asks
#                        sudo for that one step only
set -euo pipefail

SYSTEM=0
case "${1:-}" in
    "") ;;
    --system) SYSTEM=1 ;;
    *)
        echo "usage: ./install.sh [--system]" >&2
        exit 2
        ;;
esac

if [[ "$EUID" -eq 0 ]]; then
    echo "Run install.sh as your user, not root: it builds with your toolchain" >&2
    echo "and asks sudo only to install the root-owned helper (--system)." >&2
    exit 1
fi

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

if [[ "$SYSTEM" -eq 1 ]]; then
    # The helper refuses to run from any executable that is not owned and
    # writable only by root, so a user-writable copy can never be what
    # pkexec starts. Staged in the root-only directory, then renamed.
    SYSTEM_DST="/usr/local/bin/omniscient"
    echo "Installing the root-owned helper to $SYSTEM_DST (sudo)..."
    sudo install -D -o root -g root -m 755 "$BIN_SRC" "${SYSTEM_DST}.new"
    sudo mv -f "${SYSTEM_DST}.new" "$SYSTEM_DST"
    echo "Installed to $SYSTEM_DST"
fi

if command -v omniscient >/dev/null 2>&1; then
    echo "Done — run 'omniscient' to start the Cybercore dashboard."
else
    echo "Installed, but ~/.local/bin isn't on your \$PATH yet."
    echo "Add this to your shell config, then restart your shell:"
    echo ""
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
fi
