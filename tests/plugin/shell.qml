// Runtime harness for the Omarchy plugin. scripts/plugin-gate.sh copies the
// plugin next to this file, generates fixtures, and runs this shell inside a
// nested, memory-capped compositor. Every check prints one PASS/FAIL line;
// the last line is "RESULT <failures>".
pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import "plugin"

ShellRoot {
  id: harness

  readonly property string fixtures: Quickshell.env("OMNI_FIXTURES") || ""
  readonly property string runtimeSnapshot: (Quickshell.env("XDG_RUNTIME_DIR") || "") + "/omniscient/snapshot.json"
  readonly property string reportDir: harness.fixtures + "/state/omniscient/audit"
  readonly property int soakSeconds: Number(Quickshell.env("OMNI_SOAK_SECONDS") || "60")
  property var panel: panelLoader.item
  property int failures: 0
  property int checks: 0
  property int stepIndex: 0
  property double stepStarted: 0
  property double lastTick: 0
  property double maxGap: 0
  property int soakRewrites: 0
  property int applyBefore: 0
  // Longest tolerated UI-thread stall. Rendering is virtualized, so this
  // holds for any report size; before virtualization a dense 512 KiB report
  // stalled this (slow, i3-8130U) machine for ~2.7 s.
  readonly property int stallBudget: Number(Quickshell.env("OMNI_STALL_BUDGET_MS") || "600")

  function check(ok, name, detail) {
    harness.checks++
    if (!ok) harness.failures++
    console.log((ok ? "PASS " : "FAIL ") + name + (detail === undefined ? "" : " / " + detail))
  }

  function mark(label) {
    console.log("MARK " + label + " " + Date.now())
  }

  function install(name) {
    installer.command = ["/usr/bin/cp", "--", harness.fixtures + "/" + name, harness.runtimeSnapshot]
    installer.running = true
  }

  function report(name) {
    return harness.reportDir + "/" + name
  }

  function resetGap() {
    harness.maxGap = 0
    harness.lastTick = Date.now()
  }

  // Each step runs once, then the next runs after `wait` milliseconds.
  // `until` (optional) is polled before moving on, up to `wait`.
  readonly property var steps: [
    { name: "load", wait: 3000, run: function() {
      harness.check(harness.panel !== null, "panel loads", panelLoader.status === Loader.Error ? "Loader.Error" : "")
      if (harness.panel === null) harness.stepIndex = harness.steps.length - 1
    } },
    { name: "initial snapshot", wait: 7000, run: function() {
      harness.check(SnapshotReader.available, "initial snapshot is read", SnapshotReader.errorMessage)
      harness.check(SnapshotReader.applyCount === 1, "initial snapshot applied once", SnapshotReader.applyCount)
      harness.check(SnapshotReader.modules.length === 17, "modules parsed", SnapshotReader.modules.length)
      harness.check(SnapshotReader.healthScore === 90, "health parsed", SnapshotReader.healthScore)
      harness.applyBefore = SnapshotReader.applyCount
      harness.panel.open("{}")
    } },
    { name: "idle polling does not churn", wait: 3000, run: function() {
      harness.check(SnapshotReader.consumeCount >= 4, "snapshot is polled", SnapshotReader.consumeCount)
      harness.check(SnapshotReader.applyCount === harness.applyBefore,
        "an unchanged snapshot causes no reassignment", SnapshotReader.applyCount)
      harness.check(harness.panel.opened, "panel opens")
      harness.install("snapshot-changed.json")
    } },
    { name: "changed snapshot", wait: 3000, run: function() {
      harness.check(SnapshotReader.applyCount === harness.applyBefore + 1, "a changed snapshot is applied once", SnapshotReader.applyCount)
      harness.check(SnapshotReader.healthScore === 42, "new health shown", SnapshotReader.healthScore)
      harness.check(SnapshotReader.state === "running", "state is stored, not treated as a Qt State", SnapshotReader.state)
      harness.install("snapshot-invalid.json")
    } },
    { name: "invalid snapshot", wait: 3000, run: function() {
      harness.check(!SnapshotReader.available && SnapshotReader.errorMessage === "INVALID SNAPSHOT JSON",
        "invalid JSON degrades to an error", SnapshotReader.errorMessage)
      harness.install("snapshot-array.json")
    } },
    { name: "non-object snapshot", wait: 3000, run: function() {
      harness.check(!SnapshotReader.available && SnapshotReader.state === "error", "a JSON array is refused", SnapshotReader.state)
      harness.install("snapshot-oversized.json")
    } },
    { name: "oversized snapshot", wait: 3000, run: function() {
      harness.check(SnapshotReader.errorMessage === "SNAPSHOT TOO LARGE", "an oversized snapshot is refused", SnapshotReader.errorMessage)
      harness.install("snapshot-hostile.json")
    } },
    { name: "hostile snapshot values", wait: 3000, run: function() {
      harness.check(SnapshotReader.available, "hostile but valid snapshot loads", SnapshotReader.errorMessage)
      harness.check(SnapshotReader.healthScore === 100, "health is clamped", SnapshotReader.healthScore)
      harness.check(SnapshotReader.modules.length === SnapshotReader.maxListItems, "module list is capped", SnapshotReader.modules.length)
      harness.check(SnapshotReader.message.length === SnapshotReader.maxTextLength, "text is capped", SnapshotReader.message.length)
      harness.install("snapshot.json")
    } },
    { name: "restore snapshot", wait: 1000, run: function() {
      harness.check(SnapshotReader.available && SnapshotReader.healthScore === 90, "valid snapshot restores", SnapshotReader.healthScore)
    } },
    { name: "path policy", wait: 500, run: function() {
      var p = harness.panel
      harness.check(p.isSafeReportPath("/home/u/.local/state/omniscient/a/b.md"), "report path accepted")
      harness.check(!p.isSafeReportPath("/home/u/.local/state/omniscient/../../.ssh/id.md"), "parent traversal refused")
      harness.check(!p.isSafeReportPath("/home/u/.local/state/omniscient/a/b.txt"), "non-Markdown refused")
      harness.check(!p.isSafeReportPath("relative/omniscient/a.md"), "relative path refused")
      harness.check(!p.isSafeReportPath("/etc/shadow.md"), "path outside omniscient refused")
      p.openReport("/etc/omniscient/../shadow.md")
      harness.check(p.reportText === "REPORT REJECTED / UNSAFE LOCAL PATH", "unsafe report is not read", p.reportText)
    } },
    { name: "huge report", wait: 15000, until: function() { return harness.panel.reportText !== "LOADING REPORT..." }, run: function() {
      harness.mark("huge-report-open")
      harness.resetGap()
      harness.panel.openReport(harness.report("omarchy/omarchy.md"))
    } },
    { name: "huge report is bounded", wait: 2000, run: function() {
      var p = harness.panel
      harness.mark("huge-report-loaded")
      harness.check(p.reportTruncated, "a 75 MB report is truncated")
      harness.check(p.reportText.length <= p.maxReportBytes + 1024, "report text is bounded", p.reportText.length)
      harness.check(p.reportText.indexOf("REPORT TRUNCATED") >= 0, "truncation is explained")
      harness.check(p.fullReportChunks.length === 0, "the hidden full view has no model")
      harness.check(p.reportChunks.length > 100, "the whole bounded report is available", p.reportChunks.length + " chunks")
      harness.check(p.liveChunkDelegates > 0 && p.liveChunkDelegates <= 12,
        "only on-screen chunks are rendered", p.liveChunkDelegates + " delegates")
      harness.check(harness.maxGap < harness.stallBudget, "UI thread keeps ticking while loading",
        Math.round(harness.maxGap) + " ms")
      harness.resetGap()
      p.fullReportView = true
    } },
    { name: "full view renders on demand", wait: 1500, run: function() {
      var p = harness.panel
      harness.check(p.fullReportChunks.length === p.reportChunks.length, "full view gets the report when opened")
      harness.check(p.liveChunkDelegates > 0 && p.liveChunkDelegates <= 30,
        "full view renders only on-screen chunks", p.liveChunkDelegates + " delegates")
      harness.check(harness.maxGap < harness.stallBudget, "full view render stays responsive", Math.round(harness.maxGap) + " ms")
      p.fullReportView = false
      harness.check(p.fullReportChunks.length === 0, "full view is released when closed")
      p.openReport(harness.report("logs/logs.md"))
    } },
    { name: "markup is escaped", wait: 2000, until: function() { return harness.panel.reportText.indexOf("INJECT") >= 0 }, run: function() {} },
    { name: "markup checks", wait: 500, run: function() {
      var p = harness.panel
      var html = p.chunkHtml(p.reportChunks[0])
      harness.check(html.indexOf("<img") < 0 && html.indexOf("&lt;img") >= 0, "HTML in a report is escaped")
      harness.check(html.indexOf("<script") < 0, "script tags are escaped")
      var long = p.markdownToRichText("x".repeat(p.maxLineChars * 5), false)
      harness.check(long.length < p.maxLineChars + 200, "an over-long line is capped", long.length)
      // Highlighting may colour text but must never change it: strip the
      // tags, decode the entities, and the original line must come back.
      var samples = [
        "Sep 25 20:14:00 host kernel[0]: ├─ WARNING ✓ /usr/lib/libx.so.1.2.0 0x55d1 pacman 6.1.0-3 unavailable → https://example.invalid/0",
        "  System:",
        "Kernel: 6.18.49-3-lts arch: x86_64 </b></font> <script>alert('x')</script> & \"quoted\"",
        "ARCH OFFICIAL / 2 packages",
        "OMARCHY / omarchy-keyring 1.0-1 UPDATE AVAILABLE",
        "pacman -Qkk: 0 missing files, N/A, CURRENT, FAILED",
        "│  ├── /etc/fstab // not installed skipped UNKNOWN 1:2.3.4+git~r1",
        "url http://a/b/c?x='1'&y=2 PASS COMPLETE READY HEALTHY ERROR WARN"
      ]
      var decode = function(html) {
        return html.replace(/<[^>]*>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">")
          .replace(/&quot;/g, "\"").replace(/&#39;/g, "'").replace(/&amp;/g, "&")
      }
      var altered = samples.filter(function(line) { return decode(p.highlightCode(line)) !== line })
      harness.check(altered.length === 0, "highlighting never alters the text", altered.join(" | "))
      var balanced = samples.every(function(line) {
        var h = p.highlightCode(line)
        return (h.match(/<font /g) || []).length === (h.match(/<\/font>/g) || []).length
          && (h.match(/<b>/g) || []).length === (h.match(/<\/b>/g) || []).length
      })
      harness.check(balanced, "highlighted markup is balanced")
      var timestamp = p.highlightCode(samples[0])
      harness.check(timestamp.indexOf("<b>Sep 25 20:</b>") < 0, "a timestamp is not taken for a label")
      var fenced = p.markdownToRichText("inside <b>", true)
      harness.check(fenced.indexOf("#7dd3fc") >= 0, "a chunk that starts inside a code block renders as code")
      p.activateReportLink("javascript:alert(1)")
      harness.check(!p.confirmingHelp, "non-http links are ignored")
      p.activateReportLink("file:///etc/passwd")
      harness.check(!p.confirmingHelp, "file links are ignored")
      p.activateReportLink("https://wiki.archlinux.org/")
      harness.check(p.confirmingHelp && p.pendingHelpUrl === "https://wiki.archlinux.org/", "https links need confirmation")
      p.confirmingHelp = false
      p.openReport(harness.report("packages/packages.md"))
    } },
    { name: "package report", wait: 2000, until: function() { return harness.panel.reportText.indexOf("OMARCHY /") >= 0 }, run: function() {} },
    { name: "package categories", wait: 500, run: function() {
      var p = harness.panel
      p.selectPackageCategory("OMARCHY")
      harness.check(p.displayedReport().indexOf("omarchy-keyring") >= 0, "selected category shown")
      harness.check(p.displayedReport().indexOf("linux-lts") < 0, "other categories hidden")
      p.selectPackageCategory("BLACKARCH")
      harness.check(p.displayedReport().indexOf("No packages were detected") >= 0, "empty category explained")
      p.selectPackageCategory("ALL")
      harness.check(p.displayedReport().indexOf("linux-lts") >= 0, "ALL restores the whole report")
      p.openModuleReport("does-not-exist")
      harness.check(p.runnerMessage === "REPORT NOT AVAILABLE / RUN THE MODULE FIRST", "missing module report explained", p.runnerMessage)
      p.runAudit()
    } },
    { name: "audit failure is reported", wait: 5000, until: function() { return harness.panel.runnerMessage === "fake audit failed" }, run: function() {} },
    { name: "audit result", wait: 500, run: function() {
      harness.check(harness.panel.runnerMessage === "fake audit failed", "audit stderr surfaces", harness.panel.runnerMessage)
      harness.panel.openReport(harness.report("omarchy/omarchy.md"))
      harness.mark("soak-start")
      harness.resetGap()
      soak.running = true
    } },
    { name: "soak", wait: harness.soakSeconds * 1000 + 10000, until: function() { return !soak.running }, run: function() {} },
    { name: "soak result", wait: 500, run: function() {
      harness.mark("soak-end")
      harness.check(harness.soakRewrites >= harness.soakSeconds / 3, "snapshot rewritten during soak", harness.soakRewrites)
      harness.check(harness.maxGap < harness.stallBudget, "UI thread stays responsive during soak", Math.round(harness.maxGap) + " ms")
      if (harness.panel !== null) harness.panel.close()
    } }
  ]

  Loader {
    id: panelLoader
    source: "plugin/Panel.qml"
  }

  Process {
    id: installer
  }

  // Alternates identical rewrites (must cause no reassignment) with real
  // changes while the panel is open on the largest report.
  Timer {
    id: soak
    property int elapsed: 0
    interval: 3000
    repeat: true
    onRunningChanged: if (running) elapsed = 0
    onTriggered: {
      elapsed += interval
      harness.soakRewrites++
      harness.install(harness.soakRewrites % 2 ? "snapshot.json" : "snapshot-changed.json")
      if (elapsed >= harness.soakSeconds * 1000) running = false
    }
  }

  Timer {
    interval: 16
    repeat: true
    running: true
    onTriggered: {
      var now = Date.now()
      if (harness.lastTick > 0) harness.maxGap = Math.max(harness.maxGap, now - harness.lastTick)
      harness.lastTick = now
    }
  }

  Timer {
    id: driver
    interval: 50
    repeat: true
    running: true
    onTriggered: {
      if (harness.stepIndex >= harness.steps.length) {
        running = false
        console.log("RESULT " + harness.failures + " failures / " + harness.checks + " checks")
        Qt.quit()
        return
      }
      var step = harness.steps[harness.stepIndex]
      var now = Date.now()
      if (harness.stepStarted === 0) {
        harness.stepStarted = now
        console.log("STEP " + step.name)
        step.run()
        return
      }
      var done = step.until ? step.until() : false
      if (done || now - harness.stepStarted >= step.wait) {
        if (step.until && !done) harness.check(false, step.name + " completes in time", step.wait + " ms")
        harness.stepIndex++
        harness.stepStarted = 0
      }
    }
  }
}
