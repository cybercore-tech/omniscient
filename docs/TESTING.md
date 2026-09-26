# 🧪 Testing Omniscient

![gate](https://img.shields.io/badge/gate-scripts%2Fgate.sh%20full-52e8ff)
![clippy](https://img.shields.io/badge/clippy-pedantic%20denied-a56bff)
![qmllint](https://img.shields.io/badge/qmllint-all%20categories%2C%200%20warnings-c8e967)
![harness](https://img.shields.io/badge/plugin%20harness-54%20checks-ffb454)

Omniscient has two halves that fail in different ways: a Rust audit engine
that runs system commands, and a Quickshell plugin that runs **inside the
Omarchy desktop shell**. A bug in the second one does not crash a program; it
freezes the desktop. The gates are built around that.

## 🚦 One command

```bash
./scripts/gate.sh full
```

| Layer | What it enforces |
| --- | --- |
| `git diff --check`, rustfmt | clean whitespace and formatting |
| `cargo check --all-targets --all-features` | everything compiles, tests included |
| Clippy, `-D warnings`, `clippy::pedantic` denied | the strictest standard lint set; exceptions use `#[expect(..., reason = ...)]` |
| `cargo test --all-targets`, doctests | unit and regression tests |
| rustdoc with `-D warnings` | public docs build cleanly, `# Errors` sections included |
| `scripts/security-gate.sh` | no shell-mediated execution, no unreviewed system mutation |
| `scripts/plugin-gate.sh` | plugin lint and runtime harness (below) |

CI runs the same Rust gate plus `cargo audit` and `cargo deny`. It cannot run
the plugin gate (no Wayland session or Omarchy shell on a runner), so that one
is run locally before pushing.

## 🧩 Plugin gate

```bash
./scripts/gate.sh plugin
# reuse an already running nested compositor, shorter soak:
OMNI_TEST_WAYLAND_DISPLAY=wayland-2 OMNI_SOAK_SECONDS=20 ./scripts/plugin-gate.sh
```

<details>
<summary><b>Static: Qt 6 qmllint at its strictest</b></summary>

Every warning category `qmllint` knows is raised to `warning`, and
`--max-warnings 0` makes any finding fatal. The plugin uses
`pragma ComponentBehavior: Bound` with typed `required` delegate properties.
The gate also requires every plugin `.qml` file to be a manifest entry point
or registered in `qmldir`: the folder is a declared module, so an unlisted
helper type makes the panel fail to load at runtime, which qmllint misses.
Three line-level `// qmllint disable` directives remain, each for a proven
tooling gap rather than a code problem, and each is commented at its site:
`PanelWindow` (only its interface is in Quickshell's type description),
`Process.exited` (its `QProcess::ExitStatus` parameter type is not exported),
and `Bar.run()` (the host declares `bar` as a plain `QtObject`).

</details>

<details>
<summary><b>Runtime: the real plugin in a nested, capped compositor</b></summary>

The gate starts a nested Hyprland (or reuses `OMNI_TEST_WAYLAND_DISPLAY`) and
runs a separate `quickshell` with its own `HOME`, `XDG_RUNTIME_DIR` and state,
inside a systemd scope capped at 1 GiB with no swap. The live desktop shell is
never touched. `tests/plugin/make-fixtures.py` generates synthetic fixtures;
no real audit data is used or committed.

The harness (`tests/plugin/shell.qml`) checks, among 54 assertions:

- an unchanged snapshot causes **no** reassignment (the old reader replaced
  every list every second, rebuilding every delegate);
- changed, invalid, non-object, oversized (> 1 MiB) and hostile snapshots
  (10,000 modules, 100k-character strings, `1e9` health) are handled and capped;
- a **75 MB** dense, multibyte report is read only up to 512 KiB, the cut is
  detected in UTF-8 bytes and explained, only on-screen chunks get delegates,
  and the UI thread never stalls past 600 ms;
- the hidden full-report view holds no model until opened, and releases it;
- code highlighting never alters text: for tricky lines (journal
  timestamps, paths, versions, URLs, HTML-like text, quotes) stripping the
  tags and decoding entities must give back the original line, and the
  markup must be balanced;
- report HTML is escaped, `javascript:`/`file:` links are ignored, and
  `https:` links require confirmation;
- report paths outside Omniscient, relative paths and `..` traversal are refused;
- package categories filter correctly, and a failing audit surfaces its stderr;
- a soak rewrites the snapshot every 3 s with the largest report open.

The script then fails on any runtime QML warning from the plugin, a peak RSS
over 384 MiB, or more than 64 MiB of growth across the soak.

</details>

## 🧬 Mutation testing

The tests are only as good as the bugs they catch, so each safety guard was
deliberately broken to confirm a test fails:

- [x] Rust: timeout does not kill, unbounded retention, inherited stdin,
  ANSI/control characters kept, byte cap removed, UTF-8 boundary ignored,
  code fences not defused, dropped bytes not reported, report budget never
  spent, section heading not sanitized — **all caught**. (Removing the
  reader's explicit "done" message was an *equivalent* mutant: dropping the
  sender already signals it, so the redundant send was removed.)
- [x] Plugin: snapshot churn reintroduced, unbounded report read, hidden full
  view kept rendered, report not chunked, UTF-16 length used for truncation,
  long lines not capped, chunk code state ignored, HTML not escaped, list cap
  removed, traversal check removed — **all caught**.

Two early survivors (unbounded `cat` and a line cap whose notice still
printed) led to the peak-memory limit and the delegate/stall assertions.

## 🖥️ Host checks

Some behavior depends on the machine and stays out of the default suite:

```bash
cargo test --release -- --ignored real_omarchy --nocapture
```

This runs the real Omarchy module. On the development laptop it turns the
74.8 MB `omarchy-debug` output into a 261 KB section that states how many
bytes were omitted.
