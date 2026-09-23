#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/.cargo-target}"
RELEASE_VERSION="${RELEASE_VERSION:-v$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)}"
RELEASE_VERSION="${RELEASE_VERSION#v}"
RELEASE_TARGET="${RELEASE_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
ARCHIVE_NAME="omniscient-${RELEASE_VERSION}-${RELEASE_TARGET}"
DIST_DIR="$ROOT_DIR/dist"
TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/omniscient-package.XXXXXX")"
trap 'rm -rf "$TEMP_DIR"' EXIT

printf '[package] building %s for %s\n' "$ARCHIVE_NAME" "$RELEASE_TARGET"
cargo build --release --locked --target "$RELEASE_TARGET"

BIN_SRC="$CARGO_TARGET_DIR/$RELEASE_TARGET/release/omniscient"
if [[ ! -x "$BIN_SRC" ]]; then
    printf 'release binary not found: %s\n' "$BIN_SRC" >&2
    exit 1
fi

STAGE_DIR="$TEMP_DIR/$ARCHIVE_NAME"
mkdir -p "$STAGE_DIR"
install -m 755 "$BIN_SRC" "$STAGE_DIR/omniscient"
install -m 644 README.md "$STAGE_DIR/README.md"
install -m 644 LICENSE "$STAGE_DIR/LICENSE"
if [[ -d assets ]]; then
    cp -R assets "$STAGE_DIR/assets"
fi

mkdir -p "$DIST_DIR"
ARCHIVE_PATH="$DIST_DIR/$ARCHIVE_NAME.tar.gz"
CHECKSUM_PATH="$DIST_DIR/$ARCHIVE_NAME.sha256"
rm -f "$ARCHIVE_PATH" "$CHECKSUM_PATH"
tar -C "$TEMP_DIR" -czf "$ARCHIVE_PATH" "$ARCHIVE_NAME"
(cd "$DIST_DIR" && sha256sum "$(basename "$ARCHIVE_PATH")" > "$(basename "$CHECKSUM_PATH")")

printf '[package] archive: %s\n' "$ARCHIVE_PATH"
printf '[package] checksum: %s\n' "$CHECKSUM_PATH"
