#!/usr/bin/env bash
# Deep verification, beyond scripts/gate.sh: supply chain, SAST, unsafe
# census, model checking, fuzzing and mutation testing. See docs/TESTING.md.
#
#   scripts/verify.sh quick   audit, deny, semgrep, geiger, Kani   (~15 min)
#   scripts/verify.sh full    quick + every fuzz target + mutants  (hours)
#
# FUZZ_SECONDS (default 60) is the per-target fuzzing budget in full mode.
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:-quick}"
FUZZ_SECONDS="${FUZZ_SECONDS:-60}"
# Root code first: everything that runs as root or decides what root runs.
MUTANT_FILES=(src/helper.rs src/fix.rs src/elevation.rs src/pathcheck.rs src/capture.rs src/journal.rs)

step() {
    printf '\n[verify] %s\n' "$*"
    "$@"
}

need() {
    local tool="$1" hint="$2"
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'verify: %s not found (%s)\n' "$tool" "$hint" >&2
        exit 1
    fi
}

geiger() {
    # geiger reads the build's dep-info files; a shared target dir holding
    # stale ones from other checkouts makes it fail on files that no longer
    # exist, so it gets a target dir of its own.
    local target="${XDG_CACHE_HOME:-$HOME/.cache}/omniscient-geiger-target"
    local report
    report="$(CARGO_TARGET_DIR="$target" cargo geiger --all-features --output-format Ratio 2>/dev/null || true)"
    local line
    line="$(printf '%s\n' "$report" | command grep -E ' omniscient [0-9]+\.[0-9]+\.[0-9]+$' | head -n1)"
    printf '%s\n' "$line"
    # ":)" means no unsafe and #![forbid(unsafe_code)] at the crate root.
    if [[ "$line" != *':) omniscient '* ]]; then
        echo "verify: omniscient is not unsafe-free and forbid(unsafe_code)" >&2
        return 1
    fi
}

kani_all() {
    local harnesses
    mapfile -t harnesses < <(command grep -A3 '#\[kani::proof\]' src/proofs.rs |
        sed -n 's/^fn \([a-z_0-9]*\)().*/\1/p')
    if [[ ${#harnesses[@]} -eq 0 ]]; then
        echo "verify: no Kani harnesses found in src/proofs.rs" >&2
        return 1
    fi
    for harness in "${harnesses[@]}"; do
        step cargo kani --harness "$harness" --output-format terse
    done
}

fuzz_all() {
    local target
    for target in $(cargo +nightly fuzz list); do
        step cargo +nightly fuzz run "$target" -- -max_total_time="$FUZZ_SECONDS"
    done
}

mutants() {
    local args=()
    for file in "${MUTANT_FILES[@]}"; do
        args+=(-f "$file")
    done
    # Unset (not empty): each mutant copy builds in its own target dir so it
    # never contends with a shared one.
    step env -u CARGO_TARGET_DIR cargo mutants "${args[@]}" --timeout 180 -- --lib
}

quick() {
    need cargo-audit "cargo install --locked cargo-audit"
    need cargo-deny "cargo install --locked cargo-deny"
    need semgrep "pipx install semgrep"
    need cargo-geiger "cargo install --locked cargo-geiger"
    need cargo-kani "cargo install --locked kani-verifier && cargo kani setup"
    step cargo audit --deny warnings
    step cargo deny check advisories licenses bans sources
    step semgrep scan --config sast/semgrep.yml --error --metrics=off --quiet
    step geiger
    kani_all
}

case "$MODE" in
    quick)
        quick
        ;;
    full)
        quick
        need cargo-fuzz "cargo install --locked cargo-fuzz (needs a nightly toolchain)"
        need cargo-mutants "cargo install --locked cargo-mutants"
        fuzz_all
        mutants
        ;;
    *)
        echo "usage: scripts/verify.sh [quick|full]" >&2
        exit 2
        ;;
esac

printf '\n[verify] %s: PASS\n' "$MODE"
