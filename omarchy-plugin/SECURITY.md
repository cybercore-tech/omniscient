# Omniscient plugin security

This directory is the Omarchy plugin surface. It runs inside the unsandboxed
Omarchy shell process, so review the exact source commit before enabling it.

The plugin launches the installed Omniscient binary with direct argv arrays;
it does not interpolate values into a shell. Snapshot and Markdown reads are
direct, size-bounded `head -c` operations (1 MiB for the snapshot, 512 KiB for
a report) restricted to local Omniscient report paths, so no file can be pulled
into the shell process whole. A scan
uses one explicit Polkit authorization. The normal HUD scan and the focused
`PACKAGE SCAN` both launch the local binary with direct arguments; the focused
path only inventories packages and verifies local package files. It does not
install, remove, upgrade, downgrade, or repair packages. Repair buttons
require confirmation and can only request the compiled `install-tool:<tool>`
allowlist; they cannot run arbitrary commands, remove packages, edit sudoers,
manage services, or contact a network service.

The complete privilege/path/dependency model is documented in the repository
root's [SECURITY.md](../SECURITY.md).
