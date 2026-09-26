# Security model and review notes

Omniscient is a local Linux audit tool with an optional Omarchy Quickshell
surface. The Omarchy plugin is unsandboxed by design because that is how the
Omarchy shell loads plugins. Treat the repository, release archive, and
installed binary as trusted code and review the exact commit before enabling
it.

## Privilege boundary

- A normal audit is read-only with respect to the operating system. It writes
  only its own Markdown reports and an atomic local JSON snapshot.
- The focused `omniscient --packages` pass is also read-only. It runs package
  inventory, update metadata, orphan detection, foreign-package discovery,
  and file-integrity inspection; it does not invoke package installation,
  removal, upgrade, downgrade, repository modification, or keyring changes.
- The terminal mode uses `sudo` only for fixed inspection commands that need
  device or kernel visibility.
- The graphical HUD uses one explicit `pkexec` authorization for a single
  audit child. It does not create a service, daemon, timer, sudoers rule, or
  persistent root process.
- The privileged child accepts only validated numeric owner IDs and absolute
  report/snapshot paths. Existing path components must not be symlinks, and
  the user-owned portion must be owned by the invoking user before the child
  is launched.
- Privileged executable lookup is restricted to system-owned directories. No
  privileged command is constructed through a shell.

## Resource bounds

A report is evidence, not a dump, and the HUD renders it inside the
unsandboxed desktop shell. An unbounded `omarchy debug` section once produced
a 75 MB report that froze the Omarchy shell when opened, so both sides are
bounded independently:

- **Collection** (`src/capture.rs`): every command a module runs has a
  timeout (5 minutes by default, 15 for `pacman -Qkk` and repairs), keeps at
  most 256 KiB of each output stream in memory while draining and counting
  the rest, reads stdin from `/dev/null`, and cannot be hung by a grandchild
  that keeps its pipes open.
- **Reports**: each section is cleaned of terminal escape and control
  characters, its code fences are defused, and it is capped at 256 KiB and
  4,000 lines; a whole module report is capped at 2 MiB. Every cut says how
  much was omitted.
- **HUD** (`omarchy-plugin/`): the snapshot is read with `head -c` (1 MiB
  limit) and reports with `head -c` (512 KiB limit), so an old or hostile file
  cannot be pulled into the shell whole. Snapshot lists and strings are
  capped, an unchanged snapshot causes no UI updates, and reports are shown
  through virtualized list views that render only the on-screen part.

## Sensors

`omniscient --sensors` only reads `/sys` and `/proc` (under
`OMNISCIENT_SYSFS_ROOT`, default `/`) and prints one JSON reading capped at
512 KiB; the HUD refuses larger or malformed readings and keeps the last good
one. It never writes to hwmon, PWM, platform-profile, LED or RGB interfaces:
hardware control is intentionally out of scope and left to the dedicated
tools the PLATFORM tab lists. The HUD polls it only while a sensor tab is open.

## Deep Signals

Deep Signals is read-only. It reads `/proc`, `/sys` and bounded command
output (`systemctl show`, `journalctl`, `coredumpctl`, `smartctl -j`,
`btrfs device stats|scrub status|filesystem usage|subvolume list`, `ss`,
`pacman -Q`). It writes only its report, `signals.json`, `CHANGES.md`, and
two small trend files under `<report root>/history/` (battery capacity and
omarchy-shell memory, at most 500 lines each, parsed defensively). Its
suggestions are manual guidance; the repair allowlist is unchanged.

Elevation never prompts from inside a module: the elevated HUD child runs
commands directly, and otherwise only `sudo -n` is used, which fails at once
unless the dashboard's single `sudo -v` credential is cached. Per-command
`pkexec` is never used.

## Repair actions

Repair actions are deliberately narrow. The panel requires an explicit user
confirmation (which names the exact fix and command, looked up by id), then
accepts only two compiled forms:

- `install-tool:<tool>`: an allowlisted
  `pacman -S --needed --noconfirm <package>`, with the tool-to-package
  mapping compiled into the binary;
- `enable-sensor:drivetemp`: `modprobe drivetemp` (the kernel's read-only
  SATA temperature driver) plus one file,
  `/etc/modules-load.d/omniscient-drivetemp.conf`, written atomically and
  never through a symlink, so it loads at boot. `drivetemp` is the only
  module on the allowlist.

There is no arbitrary command, package name, module name, removal, upgrade,
service-management, or sudoers-editing interface. Every attempt writes a
Markdown result report. To undo the sensor fix, delete that file.

## Network and persistence

The runtime does not fetch URLs, contact a remote service, install a daemon,
or update itself. Documentation links are opened only when the user selects
them in the panel. The build uses a locked crates.io dependency graph and
does not depend on a remote Git crate.

## Review controls

The repository checks formatting, compilation, tests, Clippy, shell syntax,
QML linting, RustSec advisories, dependency licenses/sources, and forbidden
shell-mediated execution in CI. `deny.toml` is intentionally explicit. Any
future change to privilege, package management, path handling, process
launching, or plugin entry points should receive a new security review.

The HUD keeps the package pass behind a separate button so a user can inspect
the scope before authorizing a longer operation. The package report's origin
labels are inferred from local pacman sync metadata; `AUR / FOREIGN` is an
honest fallback for packages not present in configured sync databases and is
not proof of a specific upstream origin.

## Reporting

Please report security issues privately through the repository's GitHub
security contact rather than opening a public issue with exploit details.
