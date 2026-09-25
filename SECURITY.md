# Security model and review notes

Omniscient is a local Linux audit tool with an optional Omarchy Quickshell
surface. The Omarchy plugin is unsandboxed by design because that is how the
Omarchy shell loads plugins. Treat the repository, release archive, and
installed binary as trusted code and review the exact commit before enabling
it.

## Privilege boundary

- A normal audit is read-only with respect to the operating system. It writes
  only its own Markdown reports and an atomic local JSON snapshot.
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

## Repair actions

Repair actions are deliberately narrow. The panel requires an explicit user
confirmation, then accepts only IDs in the `install-tool:<tool>` form. The
tool-to-package mapping is compiled into the binary. The only write operation
is an allowlisted `pacman -S --needed --noconfirm <package>` invocation; there
is no arbitrary command, package name, removal, upgrade, service-management,
or sudoers-editing interface. Every attempt writes a Markdown result report.

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

## Reporting

Please report security issues privately through the repository's GitHub
security contact rather than opening a public issue with exploit details.
