# 🧪 Testing Omniscient

![gate](https://img.shields.io/badge/gate-scripts%2Fgate.sh%20full-52e8ff)
![clippy](https://img.shields.io/badge/clippy-pedantic%20denied-a56bff)
![qmllint](https://img.shields.io/badge/qmllint-all%20categories%2C%200%20warnings-c8e967)
![harness](https://img.shields.io/badge/plugin%20harness-100%20checks-ffb454)
![kani](https://img.shields.io/badge/kani-4%20proofs-52e8ff)
![fuzz](https://img.shields.io/badge/fuzz-7%20targets-a56bff)
![unsafe](https://img.shields.io/badge/unsafe-forbidden-c8e967)

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

The harness (`tests/plugin/shell.qml`) checks, among 100 assertions:

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
- an idle panel performs **zero** snapshot reads (file watching, not
  polling), and fixtures are installed the way the backend publishes them
  (temporary file, then rename);
- the sensor tabs, end to end: the fixture HOME's `omniscient` forwards
  `--sensors` to the real release binary reading a fake Ryzen + Radeon + ASUS
  + NVMe + drivetemp sysfs tree (`OMNISCIENT_SYSFS_ROOT`); CPU, GPU, fan-curve,
  drive, profile and ASUS values are checked, bad and oversized readings are
  refused while the last good one is kept, polling happens only on a sensor
  tab and stops when leaving it;
- the JOURNAL tab against a fake `journalctl` on the harness PATH that logs
  its argv: collapsed rows, ranked offenders, boots, and that each chip,
  unit click and search becomes exactly the right arguments (a hostile
  search stays one inert argument), the tail continues from the cursor, and
  nothing runs off the tab;
- watch mode: `watch.json` read by `WatchReader`; the real `BarWidget.qml`
  loaded with its badge count, status colour and tooltip checked; panel
  payloads (a notification opening the JOURNAL on a unit) honoured, and
  hostile, malformed and oversized payloads ignored;
- trends: fixture history files read by the real `--trends`, each series'
  points, units and last values, stats and the "worse" colouring of a falling
  health score, and the live sensor ring buffer;
- 20 rapid atomic rewrites end on the last one written;
- the repair confirmation names the requested fix (by id, not the fix-center
  selection), and a failing fix reports its error;
- a soak rewrites the snapshot every 3 s with the largest report open.

Every check runs *after* its step's wait. Two early harness failures came
from checks that ran at the start of a step, before the asynchronous work
they tested had happened; `OMNI_HARNESS_LOG=<path>` keeps the full log for
tracing such cases.

The script then fails on any runtime QML warning from the plugin, a peak RSS
over 384 MiB, or more than 64 MiB of growth across the soak.

</details>

## 🔌 Process-level tests

`tests/cli.rs` runs the real binary: `--sensors` and `--journal` must exit
cleanly when the reader closes early (this used to abort with a core dump,
found through Deep Signals' own crash trends), and invalid journal filters
are refused before `journalctl` runs. Replacing `emit` with `println!` makes
the test fail.

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

- [x] Later additions: file watchers removed (changes missed), SMART, PSI,
  boot, btrfs, taint, battery, crash, restart-loop and timer assessors each
  have tests built from real output of the development laptop, including two
  false positives found on real data (orderly shutdowns whose journal closed
  before "Journal stopped", and locale archives counted as libraries).

Two early survivors (unbounded `cat` and a line cap whose notice still
printed) led to the peak-memory limit and the delegate/stall assertions.

## 🔬 Deep verification

```bash
./scripts/verify.sh quick   # audit, deny, semgrep, geiger, Kani   (~15 min)
./scripts/verify.sh full    # + every fuzz target and cargo-mutants (hours)
```

`gate.sh` proves the code is clean and tested. `verify.sh` targets the part
that runs as root, the privileged helper (`src/helper.rs`), and asks
whether any input can make it do something it shouldn't. It is too slow for
every commit, so it runs before releases and after any change to the helper,
`fix.rs`, `elevation.rs`, `pathcheck.rs` or `capture.rs`.

| Layer | Tool | What it establishes |
| --- | --- | --- |
| Supply chain | `cargo audit`, `cargo deny` | no advisories, allowed licenses, no duplicate or git-sourced crates |
| SAST | `semgrep` with `sast/semgrep.yml` | project rules (below), each one a real review finding |
| Unsafe census | `cargo geiger` | Omniscient has **0** unsafe and `#![forbid(unsafe_code)]`; unsafe exists only in audited dependencies (libc, memchr, hashbrown, time) |
| Model checking | Kani (`src/proofs.rs`) | properties proved for *every* input within bounds, not sampled |
| Fuzzing | `cargo fuzz` (`fuzz/`) | millions of hostile inputs per target, four checked differentially against independent references |
| Mutation testing | `cargo mutants` | the tests fail when the root code is broken |

<details>
<summary><b>Restriction lints on root code</b></summary>

`src/helper.rs` denies `unwrap_used`, `expect_used`, `panic`,
`indexing_slicing` and `arithmetic_side_effects` at module level, so the
normal clippy gate enforces them. Tests are exempt. A panic in the helper drops the
elevated session, and an unchecked index or overflow there is a root-side
crash on input shaped by the requester. The lints forced the mount-path
decoder to walk slices with checked arithmetic, and that is the code path
where the fuzzer had already found an overflow.

</details>

<details>
<summary><b>Semgrep rules</b></summary>

- `helper-spawns-only-allowlisted-programs`: the helper may only execute
  a program `decide` returned from the allowlist, or the elevator constant.
- `sudo-must-be-non-interactive`: `sudo` without `-n` is an error. The single
  deliberate prompt (the user pressing the authenticate key, dashboard
  suspended) carries a reviewed `nosemgrep` with its reason.
- `predictable-temp-path`: `temp_dir().join(..)` is refused. Tests use
  `crate::scratch::dir`, which creates an unguessable 0700 directory with
  `create_dir` (it fails rather than reusing a pre-planted path or symlink).
  The rule found nine such test paths.
- `shell-interpolation`, `download-to-shell`, `unpinned-cargo-git-install`,
  `qml-shell-command`: the Omarchy marketplace baseline, kept as rules.
- `no-unwrap-in-root-code`, `fixed-tmp-path-in-script`.

</details>

<details>
<summary><b>Kani proofs</b></summary>

| Harness | Property |
| --- | --- |
| `decode_mount_path_never_panics` | the `/proc/self/mounts` decoder's byte core never panics and never returns more bytes than it was given, for every byte string up to 8 bytes (every escape, partial escape and out-of-range value) |
| `accepted_disk_names_cannot_escape_dev` | for every ASCII string up to 14 bytes, an accepted disk is `/dev/` plus a known prefix (`sd`, `vd`, `hd`, `xvd`, `nvme`, `mmcblk`) and lowercase letters and digits only: no `/`, no `.`, no traversal |
| `allowlist_admits_only_fixed_commands` | for every program name up to 8 bytes and single argument up to 6, acceptance means exactly `lshw -short` or `smartctl` on a valid disk; multi-argument requests are covered by the `helper-decide` fuzzer |
| `busy_percent_is_a_percentage` | the HUD CPU meter is always in `[0, 100]`, for any pair of `/proc/stat` samples |

</details>

<details>
<summary><b>Fuzz targets</b></summary>

| Target | Checks |
| --- | --- |
| `helper-decide` | arbitrary request lines into `decide`: anything accepted is an allowlisted program with allowlisted flags on a real mount or disk, or one of the two allowlisted repairs |
| `mount-decode-diff` | differential: `decode_mount_path` against an independent u32 reference decoder |
| `disk-name-diff` | differential: `valid_disk_name` against a regex |
| `journal-args` | HUD filter argv never panics; accepted priority, limit, unit, cursor and search obey the grammar (differential against regexes) |
| `journal-entries` | journal JSON from any program: messages bounded and escape-free, collapse key differential against a regex implementation |
| `capture-bound` | `sanitize` / `bound_text`: no control characters, idempotent, within bounds, code fences defused, nothing cut when nothing needs cutting |
| `parsers` | signals, sensors and history parsers never panic on hostile text, and derived ratios and utilization stay in range |

Two real bugs, both fixed with regression tests:

- [x] **Root helper panic**: `\777` in a mount path overflowed u8 arithmetic
  in `decode_mount_path`, crashing the root helper on crafted
  `/proc/self/mounts` content. Now checked arithmetic: an out-of-range escape
  stays literal text.
- [x] **Off-by-one in `bound_text`**: output of exactly `max_lines` lines, or
  exactly `max_bytes`, was reported as truncated because the cut landed
  before the final newline.

A third hit was a bug in the `parsers` harness itself (splitting in the
middle of a UTF-8 character), fixed in the harness.

</details>

MUTANTS_PLACEHOLDER

## 🖥️ Host checks

Some behavior depends on the machine and stays out of the default suite:

```bash
cargo test --release -- --ignored real_omarchy --nocapture
```

`cargo test --release -- --ignored real_deep_signals --nocapture` runs Deep
Signals on the host, and `real_section_timings` times each check.

The Omarchy host test runs the real Omarchy module. On the development laptop it turns the
74.8 MB `omarchy-debug` output into a 261 KB section that states how many
bytes were omitted.
