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
    run_gate cargo fmt --all -- --check
    run_gate cargo check --locked
    run_gate cargo test --locked
    run_gate cargo clippy --locked --all-targets -- -D warnings
    run_gate bash -n install.sh scripts/gate.sh scripts/package.sh
    run_gate git diff --check
}

case "$MODE" in
    quick)
        run_gate cargo fmt --all -- --check
        run_gate cargo check --locked
        run_gate cargo test --locked
        ;;
    full)
        quality_gates
        ;;
    release)
        quality_gates
        run_gate cargo build --release --locked
        ;;
    *)
        printf 'Usage: %s [quick|full|release]\n' "$0" >&2
        exit 2
        ;;
esac

printf '\n[gate] PASS / %s\n' "$MODE"
