#!/usr/bin/env bash
# Renders the README / GitHub Pages screenshots from synthetic, privacy-safe
# data (scripts/showcase.py) in a nested Hyprland with a headless 2x output.
# Nothing touches the live desktop shell or real system data.
#
#   scripts/showcase.sh [out-dir]    (default: assets/screenshots)
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT_DIR/assets/screenshots}"
SHELL_DIR="${OMARCHY_PATH:-/usr/share/omarchy}/shell"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/.cargo-target}"

for tool in quickshell Hyprland hyprctl grim python3 systemd-run; do
    command -v "$tool" >/dev/null 2>&1 || { echo "missing tool: $tool" >&2; exit 1; }
done

# Exclusively created and unpredictable (mktemp), and under /tmp with a
# neutral name, because screenshots show data paths and must not carry the
# builder's username or home directory.
WORK="$(mktemp -d /tmp/omniscient-showcase.XXXXXX)"
UNIT="omni-showcase-hypr-$$"
ANIMATOR=""
cleanup() {
    [[ -n "$ANIMATOR" ]] && kill "$ANIMATOR" 2>/dev/null || true
    systemctl --user stop "$UNIT" >/dev/null 2>&1 || true
    rm -rf -- "$WORK"
}
trap cleanup EXIT

cargo build --release --locked --quiet
python3 "$ROOT_DIR/scripts/showcase.py" "$WORK/data" "$CARGO_TARGET_DIR/release/omniscient"

harness="$WORK/harness"
mkdir -p "$harness"
cp -r -- "$ROOT_DIR/omarchy-plugin" "$harness/plugin"
ln -s "$SHELL_DIR/Commons" "$harness/Commons"
ln -s "$SHELL_DIR/Ui" "$harness/Ui"
cat >"$harness/shell.qml" <<'QML'
import QtQuick
import Quickshell
import "plugin"
ShellRoot {
  id: stage
  readonly property string scene: Quickshell.env("OMNI_SCENE") || ""
  readonly property string audit: Quickshell.env("OMNI_AUDIT") || ""
  Loader { id: panel; source: "plugin/Panel.qml" }
  Timer {
    interval: 1500
    running: true
    onTriggered: {
      var p = panel.item
      p.open("{}")
      var report = function(name) { return stage.audit + "/" + name }
      if (stage.scene === "fix-center") p.openFixCenter()
      else if (stage.scene === "deep-signals") { p.openReport(report("signals-2026-09-26_20-14-02/signals.md")); p.fullReportView = true }
      else if (stage.scene === "changes") { p.openReport(report("CHANGES.md")); p.fullReportView = true }
      else if (stage.scene === "packages") p.openReport(report("packages-2026-09-26_20-14-02/packages.md"))
      else if (["sensors", "drives", "platform", "journal", "trends"].indexOf(stage.scene) >= 0) p.tab = stage.scene
    }
  }
}
QML

printf 'monitor=,1600x1000@60,0x0,1\nmisc {\n  disable_hyprland_logo = true\n  disable_splash_rendering = true\n}\ndebug {\n  suppress_errors = true\n}\n' >"$WORK/hyprland.conf"
before="$(ls "$XDG_RUNTIME_DIR" | sort)"
systemd-run --user --quiet --unit="$UNIT" -p MemoryMax=1500M \
    -E WAYLAND_DISPLAY="$WAYLAND_DISPLAY" -E XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" -E HOME="$HOME" \
    Hyprland -c "$WORK/hyprland.conf"
socket=""
for _ in $(seq 1 100); do
    socket="$(comm -13 <(printf '%s\n' "$before") <(ls "$XDG_RUNTIME_DIR" | sort) | grep -E '^wayland-[0-9]+$' | head -n 1 || true)"
    [[ -n "$socket" ]] && break
    sleep 0.1
done
[[ -n "$socket" ]] || { echo "nested compositor did not start" >&2; exit 1; }
sleep 1
nested_pid="$(systemctl --user show -p MainPID --value "$UNIT")"
address="$(hyprctl clients -j | python3 -c 'import json,sys; pid=int(sys.argv[1]); print(next((c["address"] for c in json.load(sys.stdin) if c.get("pid")==pid), ""))' "$nested_pid")"
[[ -n "$address" ]] && hyprctl dispatch "hl.dsp.window.move({ window = \"address:$address\", workspace = \"special:omni-showcase\", follow = false })" >/dev/null 2>&1 || true
instance=""
for dir in "$XDG_RUNTIME_DIR"/hypr/*/; do
    name="$(basename "$dir")"
    [[ "$(hyprctl -i "$name" -j monitors 2>/dev/null | python3 -c 'import json,sys; m=json.load(sys.stdin); print(m[0]["name"] if m else "")' 2>/dev/null)" == WAYLAND-1 ]] && instance="$name"
done
[[ -n "$instance" ]] || { echo "nested instance not found" >&2; exit 1; }
hyprctl -i "$instance" output create headless SHOWCASE >/dev/null
hyprctl -i "$instance" keyword monitor SHOWCASE,3200x2000@60,0x0,2 >/dev/null
hyprctl -i "$instance" keyword monitor WAYLAND-1,disable >/dev/null
sleep 1

data="$WORK/data"
audit="$data/state/omniscient/full_system_audit-2026-09-26_20-14-02"
mkdir -p "$OUT"
shoot() { # scene file wait
    local scene="$1" file="$2" wait="$3"
    systemd-run --user --scope --quiet -p MemoryMax=1G env -i PATH=/usr/bin:/bin \
        HOME="$data/home" XDG_RUNTIME_DIR="$data/run" XDG_STATE_HOME="$data/state" \
        WAYLAND_DISPLAY="$XDG_RUNTIME_DIR/$socket" QT_QPA_PLATFORM=wayland \
        OMNI_SCENE="$scene" OMNI_AUDIT="$audit" \
        quickshell -p "$harness" >"$WORK/$scene.log" 2>&1 &
    local pid=$!
    sleep "$wait"
    hyprctl -i "$instance" dismissnotify >/dev/null 2>&1 || true
    sleep 0.3
    WAYLAND_DISPLAY="$socket" timeout 20 grim -g "180,110 1240x780" "$OUT/$file"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    printf '  %-34s %s\n' "$file" "$(du -h "$OUT/$file" | cut -f1)"
}

echo "[showcase] rendering into ${OUT#"$ROOT_DIR"/}"
shoot audit        omniscient-01-audit-overview.png   6
shoot fix-center   omniscient-02-fix-center.png       6
shoot deep-signals omniscient-03-deep-signals.png     6
shoot changes      omniscient-04-changes.png          6
python3 "$ROOT_DIR/scripts/showcase.py" "$data" "$CARGO_TARGET_DIR/release/omniscient" --animate &
ANIMATOR=$!
shoot sensors      omniscient-05-sensors.png          45
kill "$ANIMATOR" 2>/dev/null || true
ANIMATOR=""
shoot drives       omniscient-06-drives.png           6
shoot platform     omniscient-07-platform.png         6
shoot journal      omniscient-08-journal.png          8
shoot trends       omniscient-09-trends.png           6
shoot packages     omniscient-10-package-integrity.png 6
echo "[showcase] done"
