#!/usr/bin/env bash
# Omarchy plugin gate: strict static analysis, then a runtime harness that
# loads the real plugin into a nested, memory-capped compositor.
#
#   scripts/plugin-gate.sh           lint + runtime (fails if runtime cannot run)
#   scripts/plugin-gate.sh lint      static analysis only
#
# Environment:
#   OMNI_TEST_WAYLAND_DISPLAY  reuse an existing (nested) compositor socket
#                              instead of starting a nested Hyprland
#   OMNI_SOAK_SECONDS          soak duration (default 60)
#   OMNI_PLUGIN_DIR            plugin to test (default: omarchy-plugin)
#
# The runtime pass never touches the live desktop shell: it runs a separate
# quickshell process with its own XDG_RUNTIME_DIR, HOME and state, on a
# nested compositor, inside a systemd scope capped at 1 GiB.
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:-full}"
PLUGIN_DIR="${OMNI_PLUGIN_DIR:-$ROOT_DIR/omarchy-plugin}"
SHELL_DIR="${OMARCHY_PATH:-/usr/share/omarchy}/shell"
QMLLINT="${QMLLINT:-/usr/lib/qt6/bin/qmllint}"
MEMORY_MAX="1G"
# The fixed plugin peaks near 250 MiB on the 75 MB fixture; reading the
# whole report instead of a bounded prefix peaks near 490 MiB.
PEAK_RSS_LIMIT_KB=$((384 * 1024))
SOAK_GROWTH_LIMIT_KB=$((64 * 1024))

fail() {
    printf 'plugin gate: FAIL / %s\n' "$*" >&2
    exit 1
}

for tool in "$QMLLINT" quickshell python3 systemd-run; do
    command -v "$tool" >/dev/null 2>&1 || fail "missing tool: $tool"
done
[[ -d "$SHELL_DIR/Commons" && -d "$SHELL_DIR/Ui" ]] || fail "Omarchy shell not found at $SHELL_DIR"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/omni-plugin-gate.XXXXXX")"
NESTED_UNIT=""
cleanup() {
    if [[ -n "$NESTED_UNIT" ]]; then
        systemctl --user stop "$NESTED_UNIT" >/dev/null 2>&1 || true
    fi
    rm -rf -- "$WORK"
}
trap cleanup EXIT

# ---------------------------------------------------------------- static ----
printf '\n[plugin-gate] qmllint: every category at warning, zero allowed\n'
mkdir -p "$WORK/lint/qs"
ln -s "$SHELL_DIR/Commons" "$WORK/lint/qs/Commons"
ln -s "$SHELL_DIR/Ui" "$WORK/lint/qs/Ui"
mapfile -t categories < <("$QMLLINT" --help 2>&1 | sed -nE 's/^[[:space:]]+(--[a-z-]+) <level>.*/\1/p')
((${#categories[@]} > 20)) || fail "could not read qmllint categories"
lint_args=()
for category in "${categories[@]}"; do
    lint_args+=("$category" warning)
done
for file in "$PLUGIN_DIR"/*.qml "$ROOT_DIR"/tests/plugin/*.qml; do
    if [[ "$file" == "$ROOT_DIR"/tests/plugin/* ]]; then
        # The harness imports the plugin as "plugin"; lint it in place.
        mkdir -p "$WORK/lint/harness"
        cp -- "$file" "$WORK/lint/harness/"
        ln -sfn "$PLUGIN_DIR" "$WORK/lint/harness/plugin"
        target="$WORK/lint/harness/$(basename -- "$file")"
    else
        target="$file"
    fi
    "$QMLLINT" "${lint_args[@]}" --max-warnings 0 \
        -I "$WORK/lint" -I /usr/lib/qt6/qml -I "$(dirname -- "$target")" "$target" \
        || fail "qmllint: $file"
    printf '  clean  %s\n' "${file#"$ROOT_DIR"/}"
done

printf '\n[plugin-gate] manifest and entry points\n'
python3 - "$PLUGIN_DIR" <<'PY' || fail "manifest"
import json, os, sys
root = sys.argv[1]
manifest = json.load(open(os.path.join(root, "manifest.json")))
for key in ("schemaVersion", "id", "name", "version", "kinds", "entryPoints"):
    assert key in manifest, f"manifest is missing {key}"
for kind, entry in manifest["entryPoints"].items():
    assert "/" not in entry and not entry.startswith("."), f"unsafe entry point {entry}"
    assert os.path.isfile(os.path.join(root, entry)), f"missing entry point {entry}"
# The plugin folder has a qmldir, which makes it a declared module: a QML
# file that is neither an entry point nor listed there cannot be used by
# name, and the panel fails to load at runtime (qmllint does not catch it).
entries = set(manifest["entryPoints"].values())
listed = set()
for line in open(os.path.join(root, "qmldir")):
    parts = line.split()
    if parts and parts[-1].endswith(".qml"):
        listed.add(parts[-1])
for name in sorted(os.listdir(root)):
    if name.endswith(".qml"):
        assert name in entries or name in listed, f"{name} is neither an entry point nor listed in qmldir"
for name in listed:
    assert os.path.isfile(os.path.join(root, name)), f"qmldir lists missing {name}"
print("  manifest ok:", manifest["id"], manifest["version"])
print("  qmldir ok:", ", ".join(sorted(listed)))
PY

[[ "$MODE" == lint ]] && { printf '\n[plugin-gate] PASS / lint\n'; exit 0; }
[[ "$MODE" == full ]] || fail "usage: $0 [full|lint]"

# --------------------------------------------------------------- runtime ----
printf '\n[plugin-gate] runtime harness\n'
if [[ -n "${OMNI_TEST_WAYLAND_DISPLAY:-}" ]]; then
    socket="$OMNI_TEST_WAYLAND_DISPLAY"
else
    [[ -n "${WAYLAND_DISPLAY:-}" && -n "${XDG_RUNTIME_DIR:-}" ]] \
        || fail "runtime harness needs a Wayland session (or OMNI_TEST_WAYLAND_DISPLAY)"
    command -v Hyprland >/dev/null 2>&1 || fail "runtime harness needs Hyprland for a nested compositor"
    cat >"$WORK/hyprland.conf" <<'CONF'
monitor=,1600x1000@60,0x0,1
misc {
  disable_hyprland_logo = true
  disable_splash_rendering = true
}
CONF
    before="$(ls "$XDG_RUNTIME_DIR")"
    NESTED_UNIT="omni-plugin-gate-hypr-$$"
    systemd-run --user --quiet --unit="$NESTED_UNIT" -p MemoryMax=1500M \
        -E WAYLAND_DISPLAY="$WAYLAND_DISPLAY" -E XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" -E HOME="$HOME" \
        Hyprland -c "$WORK/hyprland.conf"
    socket=""
    for _ in $(seq 1 100); do
        socket="$(comm -13 <(printf '%s\n' "$before" | sort) <(ls "$XDG_RUNTIME_DIR" | sort) \
            | grep -E '^wayland-[0-9]+$' | head -n 1 || true)"
        [[ -n "$socket" ]] && break
        sleep 0.1
    done
    [[ -n "$socket" ]] || fail "nested compositor did not start"
    # Best effort: move the nested compositor's window out of sight on a
    # Hyprland host so the run does not cover the user's workspace.
    if [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]] && command -v hyprctl >/dev/null 2>&1; then
        nested_pid="$(systemctl --user show -p MainPID --value "$NESTED_UNIT")"
        for _ in $(seq 1 20); do
            address="$(hyprctl clients -j 2>/dev/null \
                | python3 -c 'import json,sys; pid=int(sys.argv[1]); print(next((c["address"] for c in json.load(sys.stdin) if c.get("pid")==pid), ""))' "$nested_pid" || true)"
            if [[ -n "$address" ]]; then
                hyprctl dispatch "hl.dsp.window.move({ window = \"address:$address\", workspace = \"special:omni-plugin-gate\", follow = false })" >/dev/null 2>&1 \
                    || hyprctl dispatch movetoworkspacesilent "special:omni-plugin-gate,address:$address" >/dev/null 2>&1 || true
                break
            fi
            sleep 0.1
        done
    fi
fi
[[ "$socket" == /* ]] || socket="$XDG_RUNTIME_DIR/$socket"
[[ -S "$socket" ]] || fail "no Wayland socket at $socket"

harness="$WORK/harness"
mkdir -p "$harness"
cp -r -- "$PLUGIN_DIR" "$harness/plugin"
cp -- "$ROOT_DIR/tests/plugin/shell.qml" "$harness/shell.qml"
ln -s "$SHELL_DIR/Commons" "$harness/Commons"
ln -s "$SHELL_DIR/Ui" "$harness/Ui"
fixtures="$WORK/fixtures"
python3 "$ROOT_DIR/tests/plugin/make-fixtures.py" "$fixtures"

log="$WORK/harness.log"
samples="$WORK/rss.samples"
# systemd-run needs the real user bus; the harness itself gets a clean,
# fixture-only environment. Scope mode execs in place, so $! is quickshell.
systemd-run --user --scope --quiet -p MemoryMax="$MEMORY_MAX" -p MemorySwapMax=0 \
    env -i PATH=/usr/bin:/bin \
    HOME="$fixtures/home" \
    XDG_RUNTIME_DIR="$fixtures/run" \
    XDG_STATE_HOME="$fixtures/state" \
    WAYLAND_DISPLAY="$socket" \
    QT_QPA_PLATFORM=wayland \
    OMNI_FIXTURES="$fixtures" \
    OMNI_SOAK_SECONDS="${OMNI_SOAK_SECONDS:-60}" \
    quickshell -p "$harness" >"$log" 2>&1 &
qs_pid=$!
: >"$samples"
deadline=$((SECONDS + 300 + ${OMNI_SOAK_SECONDS:-60}))
while kill -0 "$qs_pid" 2>/dev/null; do
    rss="$(awk '/^VmRSS:/ {print $2}' "/proc/$qs_pid/status" 2>/dev/null || true)"
    [[ -n "$rss" ]] && printf '%s %s\n' "$(date +%s%3N)" "$rss" >>"$samples"
    if ((SECONDS > deadline)); then
        kill "$qs_pid" 2>/dev/null || true
        sed -E 's/\x1b\[[0-9;]*m//g' "$log" | tail -n 40
        fail "harness timed out"
    fi
    sleep 0.25
done
status=0
wait "$qs_pid" || status=$?

strip() { sed -E 's/\x1b\[[0-9;]*m//g' "$1"; }
strip "$log" | grep -E ' (STEP|PASS|FAIL|RESULT) ' | sed -E 's/^.*(STEP|PASS|FAIL|RESULT) /  \1 /'

problems=0
result="$(strip "$log" | sed -nE 's/.*RESULT ([0-9]+) failures.*/\1/p' | tail -n 1)"
[[ "$result" == 0 ]] || { printf 'harness result: %s failures (exit %s)\n' "${result:-missing}" "$status"; problems=1; }
if strip "$log" | grep -E 'plugin/[A-Za-z]+\.qml:[0-9]+' | grep -vE ' (PASS|FAIL|STEP|MARK|RESULT) '; then
    printf 'QML warnings or errors from the plugin at runtime (above)\n'
    problems=1
fi
if strip "$log" | grep -E 'ERROR|Failed to load'; then
    problems=1
fi

peak="$(awk 'max < $2 {max = $2} END {print max + 0}' "$samples")"
soak_start="$(strip "$log" | sed -nE 's/.*MARK soak-start ([0-9]+).*/\1/p')"
soak_end="$(strip "$log" | sed -nE 's/.*MARK soak-end ([0-9]+).*/\1/p')"
printf '  memory: peak RSS %d MiB (limit %d MiB)\n' $((peak / 1024)) $((PEAK_RSS_LIMIT_KB / 1024))
((peak > 0 && peak <= PEAK_RSS_LIMIT_KB)) || { printf 'peak RSS out of bounds\n'; problems=1; }
if [[ -n "$soak_start" && -n "$soak_end" ]]; then
    # Compare the settled first and last 10 s of the soak window.
    read -r early late < <(awk -v s="$soak_start" -v e="$soak_end" '
        $1 >= s && $1 <= s + 10000 { a += $2; n++ }
        $1 >= e - 10000 && $1 <= e { b += $2; m++ }
        END { printf "%d %d\n", n ? a / n : 0, m ? b / m : 0 }' "$samples")
    growth=$((late - early))
    printf '  memory: soak %d -> %d MiB (growth %d KiB, limit %d KiB)\n' \
        $((early / 1024)) $((late / 1024)) "$growth" "$SOAK_GROWTH_LIMIT_KB"
    ((growth <= SOAK_GROWTH_LIMIT_KB)) || { printf 'memory grew during soak\n'; problems=1; }
else
    printf 'soak markers missing\n'
    problems=1
fi

if ((problems)); then
    printf '\n--- harness log (%s) ---\n' "$log"
    strip "$log" | tail -n 60
    fail "runtime harness"
fi
printf '\n[plugin-gate] PASS / full\n'
