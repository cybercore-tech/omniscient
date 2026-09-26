#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/.cargo-target}"
MODE="${1:-full}"

run_gate() {
    printf '\n[gate] %s\n' "$*"
    "$@"
}

quality_gates() {
    run_gate git diff --check
    run_gate cargo fmt --all -- --check
    run_gate cargo check --locked --all-targets --all-features
    # Cargo.toml denies clippy::pedantic; -D warnings makes every other lint fatal.
    run_gate cargo clippy --locked --all-targets --all-features -- -D warnings
    run_gate cargo test --locked --all-targets --all-features
    run_gate cargo test --locked --doc --all-features
    RUSTDOCFLAGS="-D warnings" run_gate cargo doc --locked --no-deps --all-features
    run_gate bash -n install.sh scripts/gate.sh scripts/package.sh scripts/security-gate.sh scripts/plugin-gate.sh
    run_gate bash scripts/security-gate.sh
    plugin_gate
}

# The plugin gate needs Qt 6 qmllint, Quickshell, the Omarchy shell and a
# Wayland session, so it runs on an Omarchy workstation, not on CI runners.
# CI sets OMNISCIENT_SKIP_PLUGIN_GATE=1 and says so in its log.
plugin_gate() {
    if [[ "${OMNISCIENT_SKIP_PLUGIN_GATE:-0}" == 1 ]]; then
        printf '\n[gate] plugin gate SKIPPED (OMNISCIENT_SKIP_PLUGIN_GATE=1)\n'
        return
    fi
    run_gate bash scripts/plugin-gate.sh full
}

case "$MODE" in
    quick)
        run_gate cargo fmt --all -- --check
        run_gate cargo check --locked --all-targets
        run_gate cargo test --locked
        ;;
    plugin)
        run_gate bash scripts/plugin-gate.sh full
        ;;
    full)
        quality_gates
        ;;
    release)
        quality_gates
        run_gate cargo build --release --locked
        ;;
    *)
        printf 'Usage: %s [quick|full|release|plugin]\n' "$0" >&2
        exit 2
        ;;
esac

printf '\n[gate] PASS / %s\n' "$MODE"
