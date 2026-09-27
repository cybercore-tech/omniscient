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
  property var bar: barLoader.item
  property var trends: harness.panel ? harness.panel.trendsView : null
  property int failures: 0
  property int checks: 0
  property int stepIndex: 0
  property double stepStarted: 0
  property double lastTick: 0
  property double maxGap: 0
  property int soakRewrites: 0
  property int applyBefore: 0
  property int readsBefore: 0
  property int pollsBefore: 0
  property int rowsBefore: 0
  property int tailsBefore: 0
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

  // Installs a fixture the way the backend publishes a snapshot: write a
  // temporary file, then rename it into place.
  function install(name) {
    installer.command = ["/usr/bin/cp", "--", harness.fixtures + "/" + name, harness.runtimeSnapshot + ".tmp"]
    installer.running = true
  }

  function argsLog() {
    return argsReader.text()
  }

  function lastArgs() {
    var lines = harness.argsLog().trim().split("\n")
    for (var i = lines.length - 1; i >= 0; i--) {
      if (lines[i].indexOf("--list-boots") < 0 && lines[i].indexOf("--output-fields") < 0) return lines[i]
    }
    return ""
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
    { name: "initial snapshot", wait: 2000, run: function() {
      harness.check(SnapshotReader.available, "initial snapshot is read", SnapshotReader.errorMessage)
      harness.check(SnapshotReader.applyCount === 1, "initial snapshot applied once", SnapshotReader.applyCount)
      harness.check(SnapshotReader.modules.length === 18, "modules parsed", SnapshotReader.modules.length)
      harness.check(SnapshotReader.healthScore === 90, "health parsed", SnapshotReader.healthScore)
      harness.applyBefore = SnapshotReader.applyCount
      harness.panel.open("{}")
    } },
    { name: "idle window", wait: 6000, run: function() {
      // Opening the panel reads the snapshot once; idle starts after that.
      harness.readsBefore = SnapshotReader.consumeCount
    } },
    { name: "idle polling does not churn", wait: 3000, run: function() {
      harness.check(SnapshotReader.consumeCount === harness.readsBefore,
        "an idle panel reads nothing (file watching, not polling)", SnapshotReader.consumeCount - harness.readsBefore + " reads")
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
      burst.remaining = 20
      burst.running = true
    } },
    { name: "rapid rewrites", wait: 8000, until: function() { return !burst.running && !installer.running && !mover.running }, run: function() {} },
    { name: "rapid rewrites settle", wait: 1500, run: function() {} },
    { name: "rapid rewrites result", wait: 100, run: function() {
      // The burst ends on snapshot.json (health 90); whatever order reads and
      // process signals arrive in, the reader must end on the last write.
      harness.check(SnapshotReader.available && SnapshotReader.healthScore === 90 && SnapshotReader.state === "complete",
        "the last of 20 rapid rewrites wins", SnapshotReader.available + " / " + SnapshotReader.healthScore + " / " + SnapshotReader.state)
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
      harness.check(harness.panel.sensorPolls === 0, "no sensor polling while on the AUDIT tab", harness.panel.sensorPolls)
      harness.resetGap()
      harness.panel.tab = "sensors"
    } },
    { name: "sensors tab", wait: 8000, until: function() { return harness.panel.sensors !== null }, run: function() {} },
    { name: "sensor reading", wait: 2500, run: function() {
      var r = harness.panel.sensors
      harness.check(harness.panel.sensorError === "", "sensor reading has no error", harness.panel.sensorError)
      harness.check(r.cpu.package_celsius === 68.4 && r.cpu.package_source === "k10temp Tctl", "Ryzen package temperature", r.cpu.package_source)
      harness.check(r.cpu.amd_pstate === "active" && r.cpu.cores.length === 2, "amd-pstate and core clocks", r.cpu.cores.length)
      harness.check(r.gpus.length === 1 && r.gpus[0].vendor === "AMD" && r.gpus[0].busy_percent === 37, "amdgpu stats", JSON.stringify(r.gpus[0]))
      harness.check(r.chips.some(function(c) { return c.curves.length === 1 }), "ASUS fan curve read")
      harness.check(harness.maxGap < harness.stallBudget, "sensor tab stays responsive", Math.round(harness.maxGap) + " ms")
      harness.panel.tab = "drives"
    } },
    { name: "drives tab", wait: 1500, run: function() {
      // This step runs 2.5 s after the first reading: a second poll is due.
      harness.check(harness.panel.sensorPolls >= 2, "sensors are polled while the tab is open", harness.panel.sensorPolls)
      var drives = harness.panel.sensors.drives
      harness.check(drives.length === 2 && drives[0].celsius === 44.9 && drives[1].celsius === 36, "NVMe and drivetemp temperatures", JSON.stringify(drives.map(function(d) { return d.celsius })))
      harness.panel.tab = "platform"
    } },
    { name: "platform tab", wait: 1500, run: function() {
      var platform = harness.panel.sensors.platform
      harness.check(platform.profile === "balanced" && platform.profile_choices.length === 3, "platform profile", platform.profile)
      harness.check(platform.asus !== null && platform.asus.throttle_policy === "performance", "ASUS thermal policy", JSON.stringify(platform.asus))
      harness.check(platform.tools.length === 0, "tools are not probed on a fixture root")
      harness.panel.acceptSensors("{not json")
      harness.check(harness.panel.sensorError === "INVALID SENSOR READING" && harness.panel.sensors !== null, "a bad reading keeps the last good one")
      harness.panel.acceptSensors("x".repeat(harness.panel.maxSensorBytes + 1))
      harness.check(harness.panel.sensorError === "SENSOR READING TOO LARGE", "an oversized reading is refused")
      var p = harness.panel
      p.selectedFixIndex = 5
      p.requestFix("install-tool:smartctl")
      harness.check(p.confirmingFix && p.pendingFixTitle === "smartctl missing" && p.pendingFixCommand.indexOf("smartmontools") >= 0,
        "the confirmation describes the requested fix, not the fix-center selection", p.pendingFixTitle)
      p.confirmingFix = false
      p.requestFix("enable-sensor:drivetemp", "Enable drivetemp", "modprobe drivetemp")
      harness.check(p.pendingFixId === "enable-sensor:drivetemp" && p.pendingFixTitle === "Enable drivetemp",
        "a fix offered outside the list carries its own description", p.pendingFixTitle)
      p.applyFix()
      p.tab = "audit"
      harness.pollsBefore = p.sensorPolls
    } },
    { name: "fix result", wait: 5000, until: function() { return harness.panel.fixMessage === "fake audit failed" }, run: function() {} },
    { name: "fix reported", wait: 200, run: function() {
      harness.check(harness.panel.fixMessage === "fake audit failed", "a failed fix reports its error", harness.panel.fixMessage)
    } },
    { name: "journal tab", wait: 8000, until: function() { return harness.panel.journal.rowCount > 0 && harness.panel.journal.offenders.length > 0 }, run: function() {
      harness.panel.tab = "journal"
    } },
    { name: "journal loaded", wait: 300, run: function() {
      var j = harness.panel.journal
      harness.check(j.rowCount === 3, "five UFW lines collapse into one row (plus two failures)", j.rowCount)
      harness.check(j.offenders[0].source === "cyberdeck-diag-deck.service" && j.offenders[0].count === 3, "top offender ranked", JSON.stringify(j.offenders[0]))
      harness.check(j.boots.length === 2, "boots listed", j.boots.length)
      harness.check(harness.lastArgs().indexOf("-p 4 -b 0") >= 0, "default is warnings of the current boot", harness.lastArgs())
      j.priority = 3
    } },
    { name: "journal priority", wait: 2500, run: function() {} },
    { name: "journal priority applied", wait: 200, run: function() {
      harness.check(harness.argsLog().indexOf("-p 3 -b 0") >= 0, "priority chip becomes -p 3")
      harness.panel.journal.unit = "cyberdeck-diag-deck.service"
    } },
    { name: "journal unit", wait: 2500, run: function() {} },
    { name: "journal unit applied", wait: 200, run: function() {
      harness.check(harness.argsLog().indexOf("_SYSTEMD_UNIT=cyberdeck-diag-deck.service + _SYSTEMD_USER_UNIT=cyberdeck-diag-deck.service") >= 0,
        "unit filter becomes journal matches")
      harness.panel.journal.grep = "fail; rm -rf / $(x)"
    } },
    { name: "journal grep", wait: 2500, run: function() {} },
    { name: "journal grep applied", wait: 200, run: function() {
      harness.check(harness.argsLog().indexOf("-g fail; rm -rf / $(x)") >= 0, "a hostile search is passed as one argument, never run")
      harness.rowsBefore = harness.panel.journal.rowCount
      harness.panel.journal.follow = true
    } },
    { name: "journal follow", wait: 5000, until: function() { return harness.panel.journal.tails > 0 && harness.panel.journal.rowCount > harness.rowsBefore }, run: function() {} },
    { name: "journal followed", wait: 200, run: function() {
      harness.check(harness.argsLog().indexOf("--after-cursor s=fixture;i=11") >= 0, "the tail continues from the last cursor")
      harness.check(harness.panel.journal.rowCount > harness.rowsBefore, "tailed lines are appended", harness.panel.journal.rowCount)
      harness.panel.journal.follow = false
      harness.tailsBefore = harness.panel.journal.tails
      harness.panel.tab = "audit"
    } },
    { name: "journal idle", wait: 4500, run: function() {} },
    { name: "journal stopped", wait: 200, run: function() {
      harness.check(harness.panel.journal.tails === harness.tailsBefore, "no journal reads off the tab", harness.panel.journal.tails - harness.tailsBefore)
    } },
    { name: "trends tab", wait: 6000, until: function() { return harness.trends.series.length >= 5 }, run: function() {
      harness.check(harness.panel.sensorHistory.cpu.length >= 2 && harness.panel.sensorHistory.util.length >= 2,
        "live sensor history accumulates while on the sensor tabs", harness.panel.sensorHistory.cpu.length)
      harness.panel.tab = "trends"
    } },
    { name: "trends read", wait: 800, run: function() {
      var byId = {}
      harness.trends.series.forEach(function(s) { byId[s.id] = s })
      harness.check(byId.health && byId.health.points.length === 5 && byId.health.points[4][1] === 80, "health trend per audit", byId.health ? byId.health.points.length : "missing")
      harness.check(byId.cpu && byId.cpu.points.length === 6 && byId.alerts.points[5][1] === 6, "hourly watch trends", byId.cpu ? byId.cpu.points.length : "missing")
      harness.check(byId["battery:BAT0"] && byId["battery:BAT0"].points.length === 4, "battery trend")
      harness.check(byId.shell && byId.shell.points[3][1] === 439, "shell memory trend in MiB", byId.shell ? JSON.stringify(byId.shell.points[3]) : "missing")
      var s = harness.trends.stats(byId.health.points, true)
      harness.check(s.change === -12 && s.tint === "#ff8f70", "a falling health score is marked as worse", s.change + " " + s.tint)
      var t = harness.trends.stats(byId.cpu.points, false)
      harness.check(t.high === 71 && t.low === 48, "trend min and max", t.low + ".." + t.high)
      harness.panel.tab = "audit"
    } },
    { name: "watch published", wait: 4000, until: function() { return WatchReader.available }, run: function() {
      watchInstaller.running = true
    } },
    { name: "watch read", wait: 300, run: function() {
      harness.check(WatchReader.available && WatchReader.urgent === 1 && WatchReader.findings.length === 3, "watch.json is read", WatchReader.findings.length)
      harness.check(WatchReader.findings[0].unit === "cyberdeck-diag-deck.service" && WatchReader.findings[0].isNew, "finding unit and new flag")
      var bar = harness.bar
      harness.check(bar !== null, "bar widget loads", barLoader.status)
      harness.check(bar.alertCount === 2, "badge counts urgent + warning", bar.alertCount)
      harness.check(String(bar.statusColor) !== "#c8e967", "an urgent finding turns the mark away from healthy green", String(bar.statusColor))
      harness.check(bar.tooltip().indexOf("1 urgent, 1 warning, 1 new") >= 0, "tooltip summarizes the watch", bar.tooltip())
      var p = harness.panel
      p.open('{"tab":"journal","unit":"cyberdeck-diag-deck.service","priority":3}')
      harness.check(p.tab === "journal" && p.journal.unit === "cyberdeck-diag-deck.service" && p.journal.priority === 3, "a notification payload opens the journal on the unit", p.tab + " / " + p.journal.unit)
      p.journal.unit = ""
      p.journal.priority = 4
      p.open('{"tab":"../../etc","unit":"$(touch /tmp/x)","priority":99}')
      harness.check(p.tab === "journal" && p.journal.unit === "" && p.journal.priority === 4, "hostile payload fields are ignored", p.tab + " / " + p.journal.unit + " / " + p.journal.priority)
      p.open("{not json")
      p.open("x".repeat(5000))
      harness.check(p.opened, "malformed and oversized payloads are ignored")
      p.open('{"tab":"audit","watch":true}')
      harness.check(p.tab === "audit", "the watch payload opens the audit tab")
    } },
    { name: "polling stops", wait: 5000, run: function() {} },
    { name: "polling stopped", wait: 500, run: function() {
      harness.check(harness.panel.sensorPolls === harness.pollsBefore, "leaving the sensor tabs stops polling", harness.panel.sensorPolls - harness.pollsBefore)
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

  FileView {
    id: argsReader
    path: harness.fixtures + "/journalctl-args.log"
    watchChanges: true
    printErrors: false
    onFileChanged: argsReader.reload()
  }

  Loader {
    id: panelLoader
    source: "plugin/Panel.qml"
  }

  Process {
    id: watchInstaller
    command: ["/usr/bin/cp", "--", harness.fixtures + "/watch.json", (Quickshell.env("XDG_RUNTIME_DIR") || "") + "/omniscient/watch.json"]
  }

  Loader {
    id: barLoader
    source: "plugin/BarWidget.qml"
  }

  Process {
    id: installer
    onExited: mover.running = true // qmllint disable signal-handler-parameters
  }

  Process {
    id: mover
    command: ["/usr/bin/mv", "-f", "--", harness.runtimeSnapshot + ".tmp", harness.runtimeSnapshot]
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

  // Rewrites the snapshot rapidly, alternating fixtures, ending on the
  // normal snapshot.
  Timer {
    id: burst
    property int remaining: 0
    interval: 60
    repeat: true
    onTriggered: {
      if (installer.running || mover.running) return
      harness.install(burst.remaining % 2 ? "snapshot.json" : "snapshot-changed.json")
      burst.remaining--
      if (burst.remaining <= 0) running = false
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
