#!/usr/bin/env python3
"""Generates the synthetic fixtures the plugin harness reads.

Nothing here comes from a real system: the largest report reproduces the
shape of the 75 MB `omarchy debug` dump (a coredump stack trace repeated
~450k times) that once froze the desktop shell, with invented contents.
"""

import json
import os
import stat
import sys

MODULES = [
    "hardware", "disks", "snapshots", "network", "containers", "services", "logs",
    "bluetooth", "devices", "security", "accounts", "persistence", "packages",
    "recovery", "reliability", "performance", "omarchy", "signals",
]


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text)


def snapshot(root, **overrides):
    audit = os.path.join(root, "state", "omniscient", "audit")
    value = {
        "version": 1,
        "state": "complete",
        "updated_at": "2026-01-01T00:00:00Z",
        "selected_count": len(MODULES),
        "completed_count": len(MODULES),
        "summary_path": os.path.join(audit, "SUMMARY.md"),
        "suggestions_path": os.path.join(audit, "SUGGESTIONS.md"),
        "message": "fixture snapshot",
        "health": {"score": 90, "notes": []},
        "modules": [
            {"name": slug.title(), "slug": slug, "state": "complete", "selected": True,
             "requires_sudo": False}
            for slug in MODULES
        ],
        "reports": [os.path.join(audit, slug, f"{slug}.md") for slug in MODULES],
        "suggestions": [{
            "id": "install-tool:smartctl", "severity": "warning", "title": "smartctl missing",
            "detail": "SMART data unavailable", "explanation": "Install smartmontools.",
            "manual_steps": ["Review the package", "Install it"],
            "command": "pacman -S --needed smartmontools", "auto_fix": True,
            "auto_fix_reason": "allowlisted", "requires_auth": True,
            "man_url": "https://man.archlinux.org/man/smartctl.8",
            "docs_url": "https://wiki.archlinux.org/title/S.M.A.R.T.",
        }],
    }
    value.update(overrides)
    return json.dumps(value)


def main():
    root = os.path.abspath(sys.argv[1])
    audit = os.path.join(root, "state", "omniscient", "audit")
    for slug in MODULES:
        write(os.path.join(audit, slug, f"{slug}.md"), f"# {slug.upper()}\n\n## ok\n\n```\nfine\n```\n")

    # Dense, multibyte lines like real journals and inxi output: every line
    # has paths, versions and status words the highlighter colours, and the
    # emoji/box characters make UTF-8 bytes differ from UTF-16 length.
    frame = ("Sep 25 20:14:{n:02d} fixturehost kernel[{n}]: ├─ WARNING ✓ /usr/lib/libfixture.so.1.2.{n} "
             "0x000055d1c0ffee{n:02d} pacman 6.1.0-3 unavailable → https://example.invalid/{n}\n")
    trace = "".join(frame.format(n=n) for n in range(6))
    with open(os.path.join(audit, "omarchy", "omarchy.md"), "w", encoding="utf-8") as handle:
        handle.write("# 🖥️ OMARCHY SURFACE\n\n## omarchy debug\n\n```\n")
        chunk = ("Stack trace of thread 4242 ⚙️:\n" + trace + "\n") * 1000
        while handle.tell() < 75 * 1024 * 1024:  # tell() counts bytes
            handle.write(chunk)
        handle.write("```\n")

    write(os.path.join(audit, "logs", "logs.md"),
          "# LOGS\n\nINJECT <img src=x onerror=alert(1)> <script>alert(1)</script>\n"
          "[click](javascript:alert(1)) **bold** `code`\n")
    write(os.path.join(audit, "packages", "packages.md"),
          "# PACKAGE INTEGRITY\n\n## package repository report\n\n```\n"
          "ARCH OFFICIAL / 2 packages\nlinux-lts 6.18.49-3 CURRENT\nbash 5.3.3-1 CURRENT\n"
          "OMARCHY / 1 package\nomarchy-keyring 1.0-1 UPDATE AVAILABLE\n"
          "AUR / FOREIGN / 1 package\nyay 12.5.0-1 CURRENT\n```\n")

    run = os.path.join(root, "run", "omniscient")
    os.makedirs(run, exist_ok=True)
    os.chmod(os.path.join(root, "run"), 0o700)
    write(os.path.join(root, "snapshot.json"), snapshot(root))
    write(os.path.join(run, "snapshot.json"), snapshot(root))
    write(os.path.join(root, "snapshot-changed.json"),
          snapshot(root, state="running", health={"score": 42, "notes": ["changed"]}))
    write(os.path.join(root, "snapshot-invalid.json"), '{"state": "complete", "modules": [')
    write(os.path.join(root, "snapshot-array.json"), "[1, 2, 3]")
    write(os.path.join(root, "snapshot-oversized.json"), snapshot(root, message="x" * (1200 * 1024)))
    write(os.path.join(root, "snapshot-hostile.json"), snapshot(
        root,
        health={"score": 1e9},
        message="m" * 100_000,
        modules=[{"name": f"m{n}", "slug": f"m{n}", "state": "complete"} for n in range(10_000)],
    ))

    fake = os.path.join(root, "home", ".local", "bin", "omniscient")
    write(fake, "#!/bin/sh\necho 'fake audit failed' >&2\nexit 3\n")
    os.chmod(fake, os.stat(fake).st_mode | stat.S_IXUSR)


if __name__ == "__main__":
    main()
