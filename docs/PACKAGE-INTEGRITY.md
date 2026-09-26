# Package Integrity / Operator Contract

Package Integrity is Omniscient's conservative package-evidence module. It
answers four questions without changing the machine:

1. Which packages are installed, and which repository surface do they match?
2. Which installed packages have an update reported by pacman?
3. Which packages are explicit, orphan candidates, foreign, or locally built?
4. Do installed package files still match the package database?

The module also records Flatpak, Snap, and Omarchy package surfaces when the
corresponding tools or metadata are available. Missing optional tools are
shown in the report rather than hidden or converted into a false failure.

## Collection model

The module uses read-only commands and captures both output and exit status:

| Evidence | Command or source | Purpose |
| --- | --- | --- |
| Installed inventory | `pacman -Q` | Names and installed versions. |
| Repository origin | `pacman -Sl` sync metadata | Matches installed names to configured repository categories. |
| Update status | `pacman -Qu` | Identifies packages for which pacman reports an available update. |
| Explicit packages | `pacman -Qe` | Separates user-requested packages from dependencies. |
| Orphan candidates | `pacman -Qdtq` | Lists unrequired dependency packages for review. |
| Foreign packages | `pacman -Qm` | Lists packages not tracked by configured sync repositories. |
| File integrity | `pacman -Qkk` | Checks installed files against local package metadata. |
| Flatpak surface | `flatpak list` | Records installed Flatpak applications and versions when available. |
| Snap surface | `snap list` | Records installed Snap packages and versions when available. |
| Omarchy surface | Omarchy package query | Records the Omarchy-specific package layer when available. |

The output is evidence, not a recommendation to update. Omniscient never
automatically runs `pacman -Syu`, removes orphans, edits repositories, changes
keyrings, or repairs package files as part of this module.

## Repository categories

The report groups installed packages using the local sync database. Category
matching is explicit and deterministic:

- **ARCH OFFICIAL** — a package matched to the official Arch repositories.
- **OMARCHY** — a package matched to the Omarchy repository.
- **BLACKARCH** — a package matched to BlackArch metadata when configured.
- **CHAOTIC AUR** — a package matched to Chaotic-AUR metadata when configured.
- **AUR / FOREIGN** — an honest fallback for unlisted, locally built, or
  otherwise foreign packages. This label does not claim that every entry was
  built by the AUR.

Each installed package line has the form:

```text
package-name version — CURRENT
package-name version — UPDATE AVAILABLE
```

`CURRENT` means the package name was not returned by `pacman -Qu` at scan
time. `UPDATE AVAILABLE` means pacman reported the package as upgradeable.
Version comparison remains pacman's responsibility; Omniscient does not
reimplement repository version ordering.

## Focused scan behavior

The normal HUD action, `RUN FULL AUDIT`, selects all 17 modules. The separate
`PACKAGE SCAN` action runs:

```bash
omniscient --packages
```

The focused path selects only the `packages` module, publishes one selected
module in the snapshot, leaves other modules `idle`, and writes the same
Markdown/report contract as a regular run. It is intentionally opt-in because
`pacman -Qkk` can take materially longer than the other inventory checks.

The interactive terminal dashboard remains available as a separate mode. The
focused headless path exists for the Omarchy HUD and other non-terminal
surfaces that need progress and report paths through the versioned snapshot.

## Omarchy report navigation

When `packages.md` is selected, the panel displays a category rail:

```text
ALL | ARCH OFFICIAL | OMARCHY | BLACKARCH | CHAOTIC AUR | AUR / FOREIGN
```

Selecting a category filters the in-panel view only; the saved Markdown file
is never rewritten. The `FULL VIEW` control opens the complete report in the
larger reader. Package headings, versions, status tokens, warnings, URLs, and
fenced command output use semantic Cybercore colors. The report index and
module registry are clickable, use alternating rows, and expose hover states
so the operator can move from a finding to its evidence without leaving the
HUD.

## Interpreting gaps

An empty category is not automatically a problem. It can mean that a
repository is not configured, no installed package came from that source, or
the local sync metadata is unavailable. A `pacman` command failure remains
visible in the report with its exit status. Review the command output and the
local repository configuration before making a repair decision.

## Related references

- [README](../README.md) — installation, usage, modules, reports, and HUD
  integration.
- [SECURITY.md](../SECURITY.md) — privilege boundaries and repair allowlists.
- [Omarchy plugin guide](../omarchy-plugin/README.md) — plugin validation and
  the in-panel workflow.
- [CHANGELOG](../CHANGELOG.md) — release history and the focused scan entry.
