pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

Item {
  id: root

  readonly property string selfId: "io.github.cybercore-tech.omniscient"
  readonly property string omniscientBinary: (Quickshell.env("HOME") || "") + "/.local/bin/omniscient"
  readonly property int fontMicro: 10
  readonly property int fontSmall: 11
  readonly property int fontBody: 12
  readonly property int fontSection: 12
  readonly property int fontTitle: 15
  readonly property int fontMetric: 34
  property bool opened: false
  property var shell: null
  property string selectedReport: ""
  property string reportText: ""
  property string runnerMessage: ""
  property string pendingFixId: ""
  property string fixMessage: ""
  property bool confirmingFix: false
  property bool confirmingHelp: false
  property bool helpLaunchLocked: false
  property bool packageScanFinished: false
  property string packageCategory: "ALL"
  property var packageCategories: ["ALL", "ARCH OFFICIAL", "OMARCHY", "BLACKARCH", "CHAOTIC AUR", "AUR / FOREIGN"]
  property string pendingHelpUrl: ""
  property string pendingHelpLabel: ""
  property bool fullReportView: false
  property bool fixCenterOpen: false
  property int selectedFixIndex: 0
  // Reports are rendered to rich text in JavaScript and laid out by Qt, both
  // on the shell's UI thread, so cost must not grow with report size. A
  // 75 MB report once froze the whole desktop. The reader keeps at most
  // maxReportBytes, and the text is split into chunks shown by ListViews,
  // which create (render and lay out) only the chunks on screen.
  readonly property int maxReportBytes: 524288
  readonly property int chunkLines: 40
  readonly property int maxLineChars: 2000
  property bool reportTruncated: false
  property var reportChunks: []
  // The full-window view gets a model only while it is open.
  readonly property var fullReportChunks: root.fullReportView ? root.reportChunks : []
  // Live chunk delegates across both views; the harness asserts it stays
  // small no matter how large the report is.
  property int liveChunkDelegates: 0

  // Tabs. AUDIT is the original surface; the sensor tabs show live,
  // read-only hardware readings from `omniscient --sensors`, polled only
  // while the panel is open on one of them.
  property string tab: "audit"
  readonly property var tabs: [
    { id: "audit", label: "AUDIT" },
    { id: "sensors", label: "SENSORS" },
    { id: "drives", label: "DRIVES" },
    { id: "platform", label: "PLATFORM" }
  ]
  readonly property int maxSensorBytes: 600000
  property var sensors: null
  property string sensorError: ""
  property int sensorPolls: 0
  readonly property bool sensorsWanted: root.opened && root.tab !== "audit"

  function acceptSensors(raw) {
    var text = String(raw || "").trim()
    if (text.length > root.maxSensorBytes) {
      root.sensorError = "SENSOR READING TOO LARGE"
      return
    }
    try {
      var value = JSON.parse(text)
      if (value !== null && typeof value === "object" && value.version === 1 && value.cpu) {
        root.sensors = value
        root.sensorError = ""
      } else {
        root.sensorError = "UNEXPECTED SENSOR READING"
      }
    } catch (error) {
      root.sensorError = "INVALID SENSOR READING"
    }
  }

  onReportTextChanged: root.chunkReport()
  onPackageCategoryChanged: root.chunkReport()

  function open(payloadJson) {
    root.opened = true
    SnapshotReader.refresh()
  }

  function close() {
    root.opened = false
  }

  function toggle() {
    root.opened ? root.close() : root.open("{}")
  }

  onShellChanged: {
    if (!root.opened && root.shell && root.shell.openPanelIds
        && root.shell.openPanelIds[root.selfId] === true)
      root.open("{}")
  }

  function stateLabel() {
    if (!SnapshotReader.available) return "WAITING FOR SNAPSHOT"
    return SnapshotReader.state.toUpperCase()
  }

  function moduleStateColor(state) {
    return SnapshotReader.stateColor(state)
  }

  function healthColor() {
    return SnapshotReader.healthColor(SnapshotReader.healthScore)
  }

  function requestFix(id) {
    root.confirmingHelp = false
    root.pendingFixId = id
    root.confirmingFix = true
  }

  function openFixCenter() {
    root.fullReportView = false
    root.fixCenterOpen = true
    if (SnapshotReader.suggestions.length > 0 && root.selectedFixIndex >= SnapshotReader.suggestions.length)
      root.selectedFixIndex = 0
  }

  function closeFixCenter() {
    root.fixCenterOpen = false
    root.confirmingFix = false
    root.confirmingHelp = false
  }

  function selectedFix() {
    if (root.selectedFixIndex < 0 || root.selectedFixIndex >= SnapshotReader.suggestions.length)
      return ({})
    return SnapshotReader.suggestions[root.selectedFixIndex]
  }

  function requestHelp(url, label) {
    var value = String(url || "")
    if (root.helpLaunchLocked)
      return
    if (value.indexOf("https://") !== 0 && value.indexOf("http://") !== 0)
      return
    root.pendingHelpUrl = value
    root.pendingHelpLabel = String(label || "EXTERNAL REFERENCE")
    root.confirmingHelp = true
  }

  function openFixHelp(url, label) {
    root.requestHelp(url, label)
  }

  function allowHelp() {
    if (root.helpLaunchLocked)
      return
    root.confirmingHelp = false
    if (root.pendingHelpUrl.length > 0 && !helpLauncher.running) {
      root.helpLaunchLocked = true
      helpLauncher.running = true
      helpLaunchGuard.restart()
    }
  }

  function fixReportPath() {
    var marker = "FIX REPORT / "
    var value = String(root.fixMessage || "")
    var index = value.indexOf(marker)
    return index >= 0 ? value.substring(index + marker.length).trim() : ""
  }

  function openFixReport() {
    var path = root.fixReportPath()
    if (path.length > 0) {
      root.fixCenterOpen = false
      root.openReport(path)
    }
  }

  function applyFix() {
    if (!root.pendingFixId.length || fixRunner.running) return
    root.confirmingFix = false
    root.fixMessage = "FIX REQUEST STARTING"
    fixRunner.running = true
  }

  function openSuggestionsReport() {
    if (SnapshotReader.suggestionsPath.length) {
      root.fixCenterOpen = false
      root.openReport(SnapshotReader.suggestionsPath)
    }
  }

  function runAudit() {
    if (auditRunner.running || packageRunner.running) return
    root.runnerMessage = "AUDIT PROCESS STARTING"
    root.packageCategory = "ALL"
    root.packageScanFinished = false
    root.opened = true
    auditRunner.running = true
    SnapshotReader.refresh()
  }

  function runPackageScan() {
    if (auditRunner.running || packageRunner.running) return
    root.runnerMessage = "PACKAGE SCAN STARTING"
    root.packageCategory = "ALL"
    root.packageScanFinished = false
    root.opened = true
    packageRunner.running = true
    SnapshotReader.refresh()
  }

  function openReport(path) {
    root.fixCenterOpen = false
    root.fullReportView = false
    var value = String(path || "")
    // Storage Matrix historically emitted storage.md under the disks
    // directory while older snapshots indexed it as disks.md. Keep those
    // snapshots readable after the backend contract is corrected.
    if (value.endsWith("/disks.md"))
      value = value.substring(0, value.length - "/disks.md".length) + "/storage.md"
    if (!isSafeReportPath(value)) {
      root.reportText = "REPORT REJECTED / UNSAFE LOCAL PATH"
      return
    }
    root.selectedReport = value
    if (!root.isPackageReport(value)) root.packageCategory = "ALL"
    root.reportText = "LOADING REPORT..."
    reportReader.running = false
    reportReaderStartTimer.start()
    reportRevealTimer.start()
  }

  function isPackageReport(path) {
    return String(path || "").endsWith("/packages.md")
  }

  function reportForModule(slug) {
    var valueSlug = String(slug || "")
    var reportName = valueSlug === "disks" ? "storage.md" : valueSlug + ".md"
    for (var i = 0; i < SnapshotReader.reports.length; i++) {
      var value = String(SnapshotReader.reports[i] || "")
      if (value.endsWith("/" + reportName)) return value
    }
    return ""
  }

  function openModuleReport(slug) {
    var path = root.reportForModule(slug)
    if (path.length > 0) {
      root.openReport(path)
    } else {
      root.runnerMessage = "REPORT NOT AVAILABLE / RUN THE MODULE FIRST"
    }
  }

  function packageReportPath() {
    for (var i = 0; i < SnapshotReader.reports.length; i++) {
      var value = String(SnapshotReader.reports[i] || "")
      if (root.isPackageReport(value)) return value
    }
    return ""
  }

  function openPackageReport() {
    var path = root.packageReportPath()
    if (path.length > 0) root.openReport(path)
  }

  function activateReportLink(link) {
    var value = String(link || "")
    if (value.indexOf("#category:") === 0) {
      root.selectPackageCategory(value.substring("#category:".length))
      return
    }
    root.requestHelp(value, "REPORT REFERENCE")
  }

  function packageCategoryLine(line) {
    var value = String(line || "")
    for (var i = 1; i < root.packageCategories.length; i++) {
      if (value.indexOf(root.packageCategories[i] + " /") === 0) return true
    }
    return false
  }

  function packageReportView() {
    if (!root.isPackageReport(root.selectedReport) || root.packageCategory === "ALL")
      return root.reportText

    var lines = String(root.reportText || "").split("\n")
    var selected = []
    var inside = false
    for (var i = 0; i < lines.length; i++) {
      var line = lines[i]
      if (!inside && line.indexOf(root.packageCategory + " /") === 0) {
        inside = true
        selected.push(line)
        continue
      }
      if (inside && (root.packageCategoryLine(line) || line.indexOf("## ") === 0))
        break
      if (inside) selected.push(line)
    }
    if (!selected.length) return "# PACKAGE CATEGORY / " + root.packageCategory + "\n\nNo packages were detected in this category."
    return "# PACKAGE CATEGORY / " + root.packageCategory + "\n\n```\n" + selected.join("\n") + "\n```"
  }

  function selectPackageCategory(category) {
    root.packageCategory = String(category || "ALL")
    reportBodyList.positionViewAtBeginning()
  }

  function isSafeReportPath(path) {
    var value = String(path || "")
    return value.startsWith("/")
      && value.endsWith(".md")
      && value.indexOf("/../") < 0
      && value.indexOf("/omniscient/") >= 0
  }

  function escapeHtml(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\"/g, "&quot;")
      .replace(/'/g, "&#39;")
  }

  function inlineMarkdown(value) {
    var html = escapeHtml(value)
    html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, "<a href='$2'>$1</a>")
    html = html.replace(/\*\*(.*?)\*\*/g, "<b>$1</b>")
    html = html.replace(/`([^`]+)`/g, "<font color='#ffb454'><b>$1</b></font>")
    return html
  }

  function colored(color, text, bold) {
    var body = root.escapeHtml(text)
    return "<font color='" + color + "'>" + (bold ? "<b>" + body + "</b>" : body) + "</font>"
  }

  // Colours the tokens of plain text in ONE pass and escapes every piece
  // exactly once. Chained regex replacements over already-built HTML used to
  // match inside the tags earlier rules had inserted (a path rule turned
  // "</b>" into "/b>" text), mangling every code block.
  function highlightTokens(text, baseColor) {
    var pattern = /(https?:\/\/[^\s<>"']+)|(\/\/[A-Za-z0-9._-]+|\/[A-Za-z0-9._~:@%+\-]+(?:\/[A-Za-z0-9._~:@%+\-]+)*)|\b(UPDATE AVAILABLE)\b|\b(CURRENT|PASS|COMPLETE|READY|HEALTHY)\b|\b(FAILED|ERROR|WARNING|WARN)\b|\b([Uu]navailable|UNAVAILABLE|[Nn]ot installed|NOT INSTALLED|[Ss]kipped|SKIPPED|[Uu]nknown|UNKNOWN|N\/A)\b|\b([0-9]+(?::[0-9]+)?(?:\.[0-9A-Za-z]+)+(?:[-+][0-9A-Za-z.+:~_-]+)?)\b/g
    var out = []
    var last = 0
    var match
    var plain = function(part) {
      if (part.length) out.push(baseColor ? root.colored(baseColor, part, false) : root.escapeHtml(part))
    }
    while ((match = pattern.exec(text)) !== null) {
      if (match[0].length === 0) { pattern.lastIndex++; continue }
      plain(text.substring(last, match.index))
      if (match[1] || match[2]) out.push(root.colored("#52e8ff", match[0], false))
      else if (match[3]) out.push(root.colored("#ffb454", match[0], true))
      else if (match[4]) out.push(root.colored("#c8e967", match[0], true))
      else if (match[5]) out.push(root.colored("#ff667d", match[0], true))
      else if (match[6]) out.push(root.colored("#ff8f70", match[0], true))
      else out.push(root.colored("#ffb454", match[0], false))
      last = match.index + match[0].length
    }
    plain(text.substring(last))
    return out.join("")
  }

  function highlightCode(value) {
    var line = String(value)
    var m = /^(ARCH OFFICIAL|OMARCHY|BLACKARCH|CHAOTIC AUR|AUR \/ FOREIGN)(\s*\/.*)$/.exec(line)
    if (m) return root.colored("#52e8ff", m[1], true) + root.colored("#8290a4", m[2], false)
    m = /^(\s*)(lscpu|lsblk|pacman|systemctl|journalctl|dmesg|flatpak|snap|findmnt|btrfs|smartctl)(\b.*)$/.exec(line)
    if (m) return root.escapeHtml(m[1]) + root.colored("#52e8ff", m[2], true) + root.highlightTokens(m[3], "#c8d2e8")
    m = /^(\s*)([├└│─┬┌┐┘▶▸◆●•]+)(.*)$/.exec(line)
    if (m) return root.escapeHtml(m[1]) + root.colored("#a56bff", m[2], true) + root.highlightTokens(m[3], "#c8d2e8")
    // "Label: value" lines; the colon must be followed by a space or the end,
    // so timestamps such as "Sep 25 20:14:00" are not taken for labels.
    m = /^(\s*)([A-Za-z][A-Za-z0-9 _()\/.+-]{0,38}:)(\s.*|)$/.exec(line)
    if (m) return root.escapeHtml(m[1]) + root.colored("#52e8ff", m[2], true) + root.highlightTokens(m[3], "#f2f5f7")
    return root.highlightTokens(line, "")
  }

  function markdownToRichText(value, startsInCode) {
    var lines = String(value || "").split("\n")
    var html = []
    var inCode = Boolean(startsInCode)
    for (var i = 0; i < lines.length; i++) {
      var line = lines[i]
      if (line.length > root.maxLineChars)
        line = line.substring(0, root.maxLineChars) + " …[line truncated]"
      if (line.indexOf("```") === 0) {
        inCode = !inCode
        html.push(inCode
          ? "<font color='#a56bff'><b>▌ CODE BLOCK</b></font><br>"
          : "<font color='#a56bff'><b>▌ END CODE</b></font><br>")
      } else if (inCode) {
        html.push("<font color='#7dd3fc'>" + highlightCode(line) + "</font><br>")
      } else if (line.indexOf("### ") === 0) {
        html.push("<font color='#ffb454'><b>" + inlineMarkdown(line.substring(4)) + "</b></font><br>")
      } else if (line.indexOf("## ") === 0) {
        html.push("<font color='#52e8ff'><b>" + inlineMarkdown(line.substring(3)) + "</b></font><br>")
      } else if (line.indexOf("# ") === 0) {
        html.push("<font color='#ff4f9a'><b>" + inlineMarkdown(line.substring(2)) + "</b></font><br>")
      } else if (line.indexOf("- ") === 0) {
        html.push("<font color='#c8e967'>◆</font> " + inlineMarkdown(line.substring(2)) + "<br>")
      } else if (line.indexOf("> ") === 0) {
        html.push("<font color='#8290a4'>│ " + inlineMarkdown(line.substring(2)) + "</font><br>")
      } else if (line.trim().length === 0) {
        html.push("<br>")
      } else {
        html.push(inlineMarkdown(line) + "<br>")
      }
    }
    return html.join("")
  }

  // Splits the visible report into chunks of chunkLines lines. Only string
  // splitting happens here; rich-text rendering is per visible chunk.
  function chunkReport() {
    var text = root.reportText.length ? root.packageReportView() : "SELECT A REPORT TO VIEW IT HERE"
    var lines = String(text).split("\n")
    var chunks = []
    var inCode = false
    for (var start = 0; start < lines.length; start += root.chunkLines) {
      var part = lines.slice(start, start + root.chunkLines)
      chunks.push({ text: part.join("\n"), code: inCode })
      for (var i = 0; i < part.length; i++) {
        if (part[i].indexOf("```") === 0) inCode = !inCode
      }
    }
    root.reportChunks = chunks
  }

  function chunkHtml(chunk) {
    return chunk ? root.markdownToRichText(chunk.text, chunk.code) : ""
  }

  // The report text with the package-category filter applied, as displayed.
  function displayedReport() {
    var parts = []
    for (var i = 0; i < root.reportChunks.length; i++) parts.push(root.reportChunks[i].text)
    return parts.join("\n")
  }

  // UTF-8 length of a string. `head -c` bounds the read in bytes while
  // JavaScript counts UTF-16 units, so multibyte reports need this to tell
  // whether the read was cut.
  function utf8Length(value) {
    var bytes = 0
    for (var i = 0; i < value.length; i++) {
      var code = value.charCodeAt(i)
      if (code < 0x80) bytes += 1
      else if (code < 0x800) bytes += 2
      else if (code >= 0xd800 && code <= 0xdbff) { bytes += 4; i++ }
      else bytes += 3
    }
    return bytes
  }

  function acceptReport(raw) {
    var value = String(raw || "")
    root.reportTruncated = root.utf8Length(value) > root.maxReportBytes
    if (root.reportTruncated) {
      value = value.substring(0, root.maxReportBytes)
      var lastLine = value.lastIndexOf("\n")
      if (lastLine > 0) value = value.substring(0, lastLine)
      value += "\n\n> REPORT TRUNCATED / showing the first " + Math.round(root.maxReportBytes / 1024)
        + " KiB. The complete report is on disk:\n> " + root.selectedReport
    }
    root.reportText = value.trim()
  }

  function reportSeverity(path) {
    var value = String(path).toLowerCase()
    if (value.indexOf("suggestions") >= 0) {
      var highest = "healthy"
      for (var i = 0; i < SnapshotReader.suggestions.length; i++) {
        var severity = String(SnapshotReader.suggestions[i].severity || "attention")
        if (severity === "urgent") return "urgent"
        if (severity === "warning") highest = "warning"
        else if (severity === "attention" && highest === "healthy") highest = "attention"
      }
      return highest
    }
    return SnapshotReader.healthLabel(SnapshotReader.healthScore).toLowerCase()
  }

  function reportIcon(path) {
    var value = String(path).toLowerCase()
    if (value.indexOf("signals") >= 0) return "🧠"
    if (value.indexOf("changes") >= 0) return "🔀"
    if (value.indexOf("hardware") >= 0) return "🖥️"
    if (value.indexOf("storage") >= 0) return "💾"
    if (value.indexOf("snapshot") >= 0) return "📸"
    if (value.indexOf("network") >= 0) return "🌐"
    if (value.indexOf("container") >= 0) return "📦"
    if (value.indexOf("service") >= 0) return "⚙️"
    if (value.indexOf("log") >= 0) return "📜"
    if (value.indexOf("bluetooth") >= 0) return "📡"
    if (value.indexOf("device") >= 0) return "🔌"
    if (value.indexOf("security") >= 0) return "🛡️"
    if (value.indexOf("account") >= 0) return "🔐"
    if (value.indexOf("persistence") >= 0) return "🧬"
    if (value.indexOf("package") >= 0) return "🧾"
    if (value.indexOf("recovery") >= 0) return "🧰"
    if (value.indexOf("reliability") >= 0) return "📈"
    if (value.indexOf("performance") >= 0) return "⚡"
    if (value.indexOf("omarchy") >= 0) return "🖥️"
    if (value.indexOf("suggestion") >= 0) return "🧰"
    if (value.indexOf("summary") >= 0) return "🛰️"
    return "📄"
  }

  Process {
    id: auditRunner
    command: ["/usr/bin/env", "OMNISCIENT_AUTH=pkexec", root.omniscientBinary, "--hud"]
    stderr: StdioCollector { id: auditStderr; waitForEnd: true }
    onExited: function(exitCode) { // qmllint disable signal-handler-parameters
      SnapshotReader.refresh()
      if (exitCode !== 0) {
        root.runnerMessage = auditStderr.text.trim().length
          ? auditStderr.text.trim()
          : "AUDIT PROCESS EXITED / CODE " + exitCode
      } else {
        root.runnerMessage = "AUDIT PROCESS COMPLETE"
      }
    }
  }

  Process {
    id: packageRunner
    command: ["/usr/bin/env", "OMNISCIENT_AUTH=sudo", root.omniscientBinary, "--packages"]
    stderr: StdioCollector { id: packageStderr; waitForEnd: true }
    onExited: function(exitCode) { // qmllint disable signal-handler-parameters
      SnapshotReader.refresh()
      root.packageScanFinished = exitCode === 0
      if (exitCode !== 0) {
        root.runnerMessage = packageStderr.text.trim().length
          ? packageStderr.text.trim()
          : "PACKAGE SCAN EXITED / CODE " + exitCode
      } else {
        root.runnerMessage = "PACKAGE SCAN COMPLETE"
      }
    }
  }

  Process {
    id: fixRunner
    command: root.pendingFixId.length
      ? ["/usr/bin/env", "OMNISCIENT_AUTH=pkexec", root.omniscientBinary, "--fix", root.pendingFixId]
      : ["/usr/bin/true"]
    stdout: StdioCollector { id: fixStdout; waitForEnd: true }
    stderr: StdioCollector { id: fixStderr; waitForEnd: true }
    onExited: function(exitCode) { // qmllint disable signal-handler-parameters
      SnapshotReader.refresh()
      if (exitCode !== 0) {
        root.fixMessage = fixStderr.text.trim().length
          ? fixStderr.text.trim()
          : "FIX FAILED / CODE " + exitCode
      } else {
        root.fixMessage = fixStdout.text.trim().length
          ? fixStdout.text.trim()
          : "FIX COMPLETE / REPORT WRITTEN"
      }
    }
  }

  Process {
    id: reportReader
    command: root.selectedReport.length
      ? ["/usr/bin/head", "-c", String(root.maxReportBytes + 1), "--", root.selectedReport]
      : ["/usr/bin/true"]
    stdout: StdioCollector {
      id: reportStdout
      waitForEnd: true
      onStreamFinished: root.acceptReport(text)
    }
    stderr: StdioCollector {
      id: reportStderr
      waitForEnd: true
    }
    onExited: function(exitCode) { // qmllint disable signal-handler-parameters
      if (exitCode !== 0) {
        root.reportText = "REPORT UNAVAILABLE\n\nThe saved report path no longer exists or cannot be read:\n" + root.selectedReport
      }
    }
  }

  Process {
    id: helpLauncher
    command: root.pendingHelpUrl.length
      ? ["/usr/bin/xdg-open", root.pendingHelpUrl]
      : ["/usr/bin/true"]
  }

  Timer {
    id: helpLaunchGuard
    interval: 1500
    repeat: false
    onTriggered: root.helpLaunchLocked = false
  }

  Timer {
    id: reportRevealTimer
    interval: 80
    repeat: false
    onTriggered: auditBodyScroll.contentY = Math.max(0, auditBodyScroll.contentHeight - auditBodyScroll.height)
  }

  Timer {
    id: reportReaderStartTimer
    interval: 1
    repeat: false
    onTriggered: reportReader.running = true
  }

  Process {
    id: sensorReader
    command: [root.omniscientBinary, "--sensors"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.acceptSensors(text)
    }
    onExited: function(exitCode) { // qmllint disable signal-handler-parameters
      if (exitCode !== 0) root.sensorError = "SENSOR READING FAILED / CODE " + exitCode
    }
  }

  Timer {
    interval: 2000
    repeat: true
    running: root.sensorsWanted
    triggeredOnStart: true
    onTriggered: {
      if (sensorReader.running) return
      root.sensorPolls++
      sensorReader.running = true
    }
  }

  Component {
    id: reportChunkDelegate

    Text {
      id: chunkText
      required property var modelData
      width: ListView.view ? ListView.view.width - 12 : 0
      text: root.chunkHtml(chunkText.modelData)
      color: "#c8d2e8"
      font.family: "monospace"
      font.pixelSize: root.fontBody
      wrapMode: Text.Wrap
      textFormat: Text.RichText
      onLinkActivated: function(link) { root.activateReportLink(link) }
      Component.onCompleted: root.liveChunkDelegates++
      Component.onDestruction: root.liveChunkDelegates--
    }
  }

  // Quickshell's type description exports only the PanelWindow interface;
  // the Wayland implementation is supplied at runtime, so the linter cannot
  // see that it is creatable. The same applies to QProcess::ExitStatus in
  // Process.exited, which is why those handlers carry a line-level directive.
  PanelWindow { // qmllint disable uncreatable-type
    id: panelWindow
    visible: root.opened
    anchors { top: true; bottom: true; left: true; right: true }
    color: "transparent"
    WlrLayershell.namespace: "io-github-cybercore-tech-omniscient"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: root.opened ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    exclusionMode: ExclusionMode.Ignore

    MouseArea {
      anchors.fill: parent
      onClicked: root.close()
    }

    Rectangle {
      id: card
      width: Math.min(1240, parent.width - 28)
      height: Math.min(780, parent.height - 28)
      anchors.centerIn: parent
      radius: 7
      color: "#080b12"
      border.width: 1
      border.color: "#263445"

      MouseArea {
        anchors.fill: parent
        onClicked: function(mouse) { mouse.accepted = true }
      }

      ColumnLayout {
        anchors.fill: parent
        anchors.margins: 20
        spacing: 10

        RowLayout {
          Layout.fillWidth: true
          spacing: 7
          Rectangle { implicitWidth: 9; implicitHeight: 9; radius: 5; color: "#ff4f9a" }
          Rectangle { implicitWidth: 9; implicitHeight: 9; radius: 5; color: "#a56bff" }
          Rectangle { implicitWidth: 9; implicitHeight: 9; radius: 5; color: "#52e8ff" }
          Text {
            text: "OMNISCIENT / SYSTEM AUDIT"
            color: "#f2f5f7"
            font.family: "monospace"
            font.pixelSize: root.fontTitle
            font.bold: true
            Layout.leftMargin: 7
          }
          Item { Layout.fillWidth: true }
          Text {
            text: root.stateLabel()
            color: SnapshotReader.stateColor(SnapshotReader.state)
            font.family: "monospace"
            font.pixelSize: root.fontBody
            font.bold: true
          }
          Text {
            text: "×"
            color: "#8290a4"
            font.pixelSize: 22
            Layout.leftMargin: 10
            MouseArea { anchors.fill: parent; onClicked: root.close() }
          }
        }

        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: "#263445" }

        RowLayout {
          Layout.fillWidth: true
          spacing: 6
          Repeater {
            model: root.tabs
            delegate: Rectangle {
              id: tabButton
              required property var modelData
              readonly property bool current: root.tab === tabButton.modelData.id
              property bool hovered: false
              Layout.preferredWidth: tabLabel.implicitWidth + 28
              Layout.preferredHeight: 28
              radius: 3
              color: tabButton.current ? "#1a2940" : (tabButton.hovered ? "#142033" : "#0d1320")
              border.width: 1
              border.color: tabButton.current ? "#52e8ff" : "#263445"
              Text {
                id: tabLabel
                anchors.centerIn: parent
                text: tabButton.modelData.label
                color: tabButton.current ? "#52e8ff" : "#8290a4"
                font.family: "monospace"
                font.pixelSize: root.fontSmall
                font.bold: tabButton.current
              }
              MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: tabButton.hovered = true
                onExited: tabButton.hovered = false
                onClicked: root.tab = tabButton.modelData.id
              }
            }
          }
          Item { Layout.fillWidth: true }
          Text {
            visible: root.tab !== "audit"
            text: root.sensorError.length ? root.sensorError : "LIVE / READ-ONLY / EVERY 2 S"
            color: root.sensorError.length ? "#ff667d" : "#8290a4"
            font.family: "monospace"
            font.pixelSize: root.fontMicro
          }
        }

        Flickable {
          boundsBehavior: Flickable.StopAtBounds
          ScrollBar.vertical: HudScrollBar {}
          id: auditBodyScroll
          visible: root.tab === "audit"
          Layout.fillWidth: true
          Layout.fillHeight: true
          Layout.minimumHeight: 0
          clip: true
          contentWidth: width
          contentHeight: auditBodyContent.implicitHeight

          ColumnLayout {
            id: auditBodyContent
            width: auditBodyScroll.width - 14
            spacing: 10

        RowLayout {
          Layout.fillWidth: true
          spacing: 12

          Rectangle {
            Layout.preferredWidth: 220
            Layout.preferredHeight: 110
            color: "#111824"
            border.width: 1
            border.color: root.healthColor()
            ColumnLayout {
              anchors.fill: parent
              anchors.margins: 12
              Text { text: "HEALTH SIGNAL / " + SnapshotReader.healthLabel(SnapshotReader.healthScore); color: root.healthColor(); font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
              Text {
                text: SnapshotReader.healthScore >= 0 ? SnapshotReader.healthScore + "/100" : "—/100"
                color: root.healthColor()
                font.family: "monospace"; font.pixelSize: root.fontMetric; font.bold: true
              }
              Text {
                text: SnapshotReader.selectedCount + " SELECTED / " + SnapshotReader.completedCount + " COMPLETE"
                color: "#8290a4"; font.family: "monospace"; font.pixelSize: root.fontMicro
              }
            }
          }

          ColumnLayout {
            Layout.fillWidth: true
            spacing: 5
            Text { text: "SNAPSHOT CONTRACT / V1"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }
            Text {
              Layout.fillWidth: true
              text: SnapshotReader.available
                ? (SnapshotReader.message.length ? SnapshotReader.message : "Atomic local state is available to the HUD.")
                : "Run the audit here to publish the first state snapshot."
              color: "#c8d2e8"; font.family: "monospace"; font.pixelSize: root.fontBody; wrapMode: Text.Wrap
            }
            Text {
              Layout.fillWidth: true
              text: SnapshotReader.errorMessage.length ? SnapshotReader.errorMessage : SnapshotReader.snapshotPath
              color: SnapshotReader.errorMessage.length ? "#ff667d" : "#8290a4"
              font.family: "monospace"; font.pixelSize: root.fontMicro; elide: Text.ElideMiddle
            }
            RowLayout {
              Layout.fillWidth: true
              spacing: 8
              Rectangle {
                Layout.preferredWidth: 180
                Layout.preferredHeight: 36
                radius: 4
                property bool hovered: false
                color: hovered ? "#2a1935" : (auditRunner.running ? "#24172a" : "#121c2b")
                border.width: 1
                border.color: "#ff4f9a"
                Text {
                  anchors.centerIn: parent
                  text: auditRunner.running ? "AUDIT RUNNING..." : "RUN FULL AUDIT"
                  color: "#ff4f9a"
                  font.family: "monospace"
                  font.pixelSize: root.fontSmall
                  font.bold: true
                }
                MouseArea {
                  anchors.fill: parent
                  cursorShape: Qt.PointingHandCursor
                  hoverEnabled: true
                  onEntered: parent.hovered = true
                  onExited: parent.hovered = false
                  onClicked: root.runAudit()
                }
              }
              Rectangle {
                Layout.preferredWidth: 180
                Layout.preferredHeight: 36
                radius: 4
                property bool hovered: false
                color: hovered ? "#3a2d18" : (packageRunner.running ? "#2a2417" : "#121c2b")
                border.width: 1
                border.color: "#ffb454"
                Text {
                  anchors.centerIn: parent
                  text: packageRunner.running ? "PACKAGE SCANNING..." : "PACKAGE SCAN"
                  color: "#ffb454"
                  font.family: "monospace"
                  font.pixelSize: root.fontSmall
                  font.bold: true
                }
                MouseArea {
                  anchors.fill: parent
                  cursorShape: Qt.PointingHandCursor
                  hoverEnabled: true
                  onEntered: parent.hovered = true
                  onExited: parent.hovered = false
                  onClicked: root.runPackageScan()
                }
              }
            }
            Rectangle {
              visible: packageRunner.running || (root.packageScanFinished && root.packageReportPath().length > 0)
              Layout.fillWidth: true
              Layout.preferredHeight: 64
              color: packageRunner.running ? "#191522" : "#132019"
              border.width: 1
              border.color: packageRunner.running ? "#ffb454" : "#c8e967"
              RowLayout {
                anchors.fill: parent
                anchors.margins: 10
                spacing: 10
                ColumnLayout {
                  Layout.fillWidth: true
                  spacing: 2
                  Text {
                    text: packageRunner.running ? "PACKAGE SCAN / ACTIVE" : "PACKAGE SCAN / COMPLETE"
                    color: packageRunner.running ? "#ffb454" : "#c8e967"
                    font.family: "monospace"
                    font.pixelSize: root.fontSmall
                    font.bold: true
                  }
                  Text {
                    Layout.fillWidth: true
                    text: packageRunner.running
                      ? (SnapshotReader.message.length ? SnapshotReader.message : "Package Integrity is collecting read-only evidence…")
                      : "Package report is ready. Review categories, versions, and status markers."
                    color: "#c8d2e8"
                    font.family: "monospace"
                    font.pixelSize: root.fontMicro
                    elide: Text.ElideRight
                  }
                }
                Rectangle {
                  visible: !packageRunner.running && root.packageReportPath().length > 0
                  Layout.preferredWidth: 150
                  Layout.preferredHeight: 32
                  color: "#182438"
                  border.width: 1
                  border.color: "#52e8ff"
                  Text { anchors.centerIn: parent; text: "OPEN PACKAGE REPORT"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openPackageReport() }
                }
              }
            }
          }
        }

        Text { text: "MODULE REGISTRY"; color: "#8290a4"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }

        GridLayout {
          Layout.fillWidth: true
          Layout.preferredHeight: Math.max(54, Math.ceil(SnapshotReader.modules.length / 3) * 61 - 7)
          Layout.minimumHeight: Math.max(54, Math.ceil(SnapshotReader.modules.length / 3) * 61 - 7)
          columns: 3
          rowSpacing: 7
          columnSpacing: 7

          Repeater {
            model: SnapshotReader.modules
            delegate: Rectangle {
              id: moduleCard
              required property var modelData
              required property int index
              property bool hovered: false
              Layout.fillWidth: true
              Layout.preferredHeight: 54
              color: hovered ? "#1a2940" : (moduleCard.index % 2 ? "#0f1725" : "#111824")
              border.width: 1
              border.color: hovered ? "#52e8ff" : "#263445"
              ColumnLayout {
                anchors.fill: parent
                anchors.margins: 9
                spacing: 2
                Text { text: String(moduleCard.modelData.name || "").toUpperCase(); color: "#f2f5f7"; font.family: "monospace"; font.pixelSize: root.fontSmall; elide: Text.ElideRight; Layout.fillWidth: true }
                Text { text: String(moduleCard.modelData.state || "unknown").toUpperCase(); color: root.moduleStateColor(String(moduleCard.modelData.state || "unknown")); font.family: "monospace"; font.pixelSize: root.fontMicro }
                Text { text: moduleCard.modelData.requires_sudo ? "ELEVATED" : "USER MODE"; color: "#8290a4"; font.family: "monospace"; font.pixelSize: root.fontMicro }
              }
              MouseArea {
                anchors.fill: parent
                z: 10
                hoverEnabled: true
                preventStealing: true
                acceptedButtons: Qt.NoButton
                cursorShape: root.reportForModule(String(moduleCard.modelData.slug || "")).length ? Qt.PointingHandCursor : Qt.ArrowCursor
                onEntered: parent.hovered = true
                onExited: parent.hovered = false
              }
              TapHandler {
                onTapped: root.openModuleReport(String(moduleCard.modelData.slug || ""))
              }
            }
          }
        }

        Rectangle {
          Layout.fillWidth: true
          Layout.preferredHeight: 132
          color: "#0d1320"
          border.width: 1
          border.color: SnapshotReader.suggestions.length ? "#ffb454" : "#263445"

          ColumnLayout {
            anchors.fill: parent
            anchors.margins: 12
            spacing: 6

            RowLayout {
              Layout.fillWidth: true
              Text {
                text: "FIX SUGGESTIONS / " + SnapshotReader.suggestions.length
                color: SnapshotReader.suggestions.length ? "#ffb454" : "#52e8ff"
                font.family: "monospace"
                font.pixelSize: root.fontSection
                font.bold: true
              }
              Item { Layout.fillWidth: true }
              Rectangle {
                Layout.preferredWidth: 150
                Layout.preferredHeight: 28
                radius: 3
                color: "#182438"
                border.width: 1
                border.color: "#ffb454"
                Text { anchors.centerIn: parent; text: "OPEN FIX CENTER"; color: "#ffb454"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixCenter() }
              }
              Text {
                visible: SnapshotReader.suggestionsPath.length > 0
                text: "VIEW SUGGESTIONS REPORT"
                color: "#52e8ff"
                font.family: "monospace"
                font.pixelSize: root.fontMicro
                MouseArea {
                  anchors.fill: parent
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.openSuggestionsReport()
                }
              }
            }

            Text {
              visible: root.fixMessage.length > 0
              text: root.fixMessage
              color: root.fixMessage.indexOf("FAILED") >= 0 ? "#ff667d" : "#c8e967"
              font.family: "monospace"
              font.pixelSize: root.fontMicro
              elide: Text.ElideMiddle
              Layout.fillWidth: true
            }

            ListView {
              boundsBehavior: Flickable.StopAtBounds
              ScrollBar.vertical: HudScrollBar {}
              Layout.fillWidth: true
              Layout.fillHeight: true
              clip: true
              model: SnapshotReader.suggestions
              spacing: 5

              delegate: Rectangle {
                id: suggestionRow
                required property var modelData
                required property int index
                property bool hovered: false
                width: ListView.view.width - 12
                height: 62
                color: hovered ? "#1a2940" : (suggestionRow.index % 2 ? "#0f1725" : "#111824")
                border.width: 1
                border.color: hovered ? "#52e8ff" : SnapshotReader.severityColor(String(suggestionRow.modelData.severity || "attention"))

                RowLayout {
                  anchors.fill: parent
                  anchors.margins: 8
                  spacing: 10

                  ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                      text: String(suggestionRow.modelData.severity || "attention").toUpperCase() + " / " + String(suggestionRow.modelData.title || "")
                      color: SnapshotReader.severityColor(String(suggestionRow.modelData.severity || "attention"))
                      font.family: "monospace"
                      font.pixelSize: root.fontSmall
                      font.bold: true
                      elide: Text.ElideRight
                      Layout.fillWidth: true
                    }
                    Text {
                      text: String(suggestionRow.modelData.detail || "")
                      color: "#c8d2e8"
                      font.family: "monospace"
                      font.pixelSize: root.fontMicro
                      elide: Text.ElideRight
                      Layout.fillWidth: true
                    }
                    Text {
                      text: "<a href='" + String(suggestionRow.modelData.man_url || "") + "'>MAN</a>  <a href='" + String(suggestionRow.modelData.docs_url || "") + "'>DOCS</a>"
                      textFormat: Text.RichText
                      color: "#52e8ff"
                      font.family: "monospace"
                      font.pixelSize: root.fontMicro
                      onLinkActivated: function(link) { root.requestHelp(link, "SUGGESTION REFERENCE") }
                    }
                  }

                  Rectangle {
                    visible: Boolean(suggestionRow.modelData.auto_fix)
                    Layout.preferredWidth: 118
                    Layout.preferredHeight: 32
                    radius: 3
                    color: "#1d2634"
                    border.width: 1
                    border.color: "#ffb454"
                    Text {
                      anchors.centerIn: parent
                      text: "REVIEW / APPLY"
                      color: "#ffb454"
                      font.family: "monospace"
                      font.pixelSize: root.fontMicro
                      font.bold: true
                    }
                    MouseArea {
                      anchors.fill: parent
                      cursorShape: Qt.PointingHandCursor
                      onClicked: root.requestFix(String(suggestionRow.modelData.id || ""))
                    }
                  }
                }
                MouseArea {
                  anchors.fill: parent
                  hoverEnabled: true
                  acceptedButtons: Qt.NoButton
                  onEntered: parent.hovered = true
                  onExited: parent.hovered = false
                }
              }

              Text {
                anchors.centerIn: parent
                visible: SnapshotReader.suggestions.length === 0
                text: "NO REPAIR ACTIONS SUGGESTED"
                color: "#c8e967"
                font.family: "monospace"
                font.pixelSize: root.fontSmall
              }
            }
          }
        }

        RowLayout {
          Layout.fillWidth: true
          Layout.preferredHeight: 300
          Layout.minimumHeight: 220
          spacing: 10

          Rectangle {
            Layout.preferredWidth: 250
            Layout.fillHeight: true
            color: "#0d1320"
            border.width: 1
            border.color: "#263445"
            ColumnLayout {
              anchors.fill: parent
              anchors.margins: 10
              spacing: 6
              Text { text: "REPORT INDEX / URGENCY"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }
              ListView {
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: HudScrollBar {}
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: SnapshotReader.reports
                delegate: Rectangle {
                  id: reportRow
                  required property var modelData
                  required property int index
                  property bool hovered: false
                  width: ListView.view.width - 12
                  height: 32
                  color: hovered || root.selectedReport === String(reportRow.modelData)
                    ? "#1a2940"
                    : (reportRow.index % 2 ? "#0f1725" : "transparent")
                  border.width: hovered ? 1 : 0
                  border.color: "#52e8ff"
                  Text {
                    anchors.fill: parent
                    anchors.margins: 6
                    text: root.reportIcon(String(reportRow.modelData)) + "  " + String(reportRow.modelData).split("/").pop()
                    color: SnapshotReader.severityColor(root.reportSeverity(String(reportRow.modelData)))
                    font.family: "monospace"
                    font.pixelSize: root.fontMicro
                    elide: Text.ElideMiddle
                  }
                  MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onEntered: parent.hovered = true
                    onExited: parent.hovered = false
                    onClicked: root.openReport(String(reportRow.modelData))
                  }
                }
                Text {
                  anchors.centerIn: parent
                  visible: SnapshotReader.reports.length === 0
                  text: "NO REPORTS YET"
                  color: "#8290a4"
                  font.family: "monospace"
                  font.pixelSize: root.fontSmall
                }
              }
            }
          }

          Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: "#0d1320"
            border.width: 1
            border.color: "#263445"
            ColumnLayout {
              anchors.fill: parent
              anchors.margins: 10
              spacing: 6
              RowLayout {
                Layout.fillWidth: true
                Text {
                  text: root.selectedReport.length ? "REPORT VIEW / " + root.selectedReport.split("/").pop() : "REPORT VIEW"
                  color: "#ff4f9a"
                  font.family: "monospace"
                  font.pixelSize: root.fontSection
                  elide: Text.ElideMiddle
                  Layout.fillWidth: true
                }
                Rectangle {
                  Layout.preferredWidth: 112
                  Layout.preferredHeight: 30
                  color: "#121c2b"
                  border.width: 1
                  border.color: "#52e8ff"
                  Text { anchors.centerIn: parent; text: "FULL VIEW"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.fullReportView = true }
                }
              }
              RowLayout {
                visible: root.isPackageReport(root.selectedReport)
                Layout.fillWidth: true
                spacing: 5
                Text {
                  text: "CATEGORIES"
                  color: "#8290a4"
                  font.family: "monospace"
                  font.pixelSize: root.fontMicro
                  font.bold: true
                }
                Repeater {
                  model: root.packageCategories
                  delegate: Rectangle {
                    id: categoryChip
                    required property var modelData
                    property bool hovered: false
                    Layout.preferredHeight: 24
                    Layout.preferredWidth: Math.max(42, packageCategoryLabel.implicitWidth + 16)
                    radius: 3
                    color: hovered || root.packageCategory === categoryChip.modelData ? "#1a2940" : "#111824"
                    border.width: 1
                    border.color: root.packageCategory === categoryChip.modelData ? "#52e8ff" : (hovered ? "#ffb454" : "#263445")
                    Text {
                      id: packageCategoryLabel
                      anchors.centerIn: parent
                      text: String(categoryChip.modelData)
                      color: root.packageCategory === categoryChip.modelData ? "#52e8ff" : "#c8d2e8"
                      font.family: "monospace"
                      font.pixelSize: root.fontMicro
                      font.bold: root.packageCategory === categoryChip.modelData
                    }
                    MouseArea {
                      anchors.fill: parent
                      hoverEnabled: true
                      cursorShape: Qt.PointingHandCursor
                      onEntered: parent.hovered = true
                      onExited: parent.hovered = false
                      onClicked: root.selectPackageCategory(String(categoryChip.modelData))
                    }
                  }
                }
              }
              ListView {
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: HudScrollBar {}
                id: reportBodyList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                reuseItems: false
                cacheBuffer: 400
                model: root.reportChunks
                delegate: reportChunkDelegate
              }
            }
          }
        }

        RowLayout {
          Layout.fillWidth: true
          Text {
            Layout.fillWidth: true
            text: SnapshotReader.summaryPath.length ? "REPORT / " + SnapshotReader.summaryPath : "REPORT / awaiting completed audit"
            color: "#8290a4"
            font.family: "monospace"
            font.pixelSize: root.fontMicro
            elide: Text.ElideMiddle
          }
          Text {
            text: SnapshotReader.updatedAt.length ? SnapshotReader.updatedAt : "NO UPDATE YET"
            color: "#52e8ff"
            font.family: "monospace"
            font.pixelSize: root.fontMicro
          }
        }

          }
        }

        SensorsView {
          visible: root.tab === "sensors"
          Layout.fillWidth: true
          Layout.fillHeight: true
          reading: root.sensors
        }

        DrivesView {
          visible: root.tab === "drives"
          Layout.fillWidth: true
          Layout.fillHeight: true
          reading: root.sensors
        }

        PlatformView {
          visible: root.tab === "platform"
          Layout.fillWidth: true
          Layout.fillHeight: true
          reading: root.sensors
        }
      }

      Rectangle {
        visible: root.confirmingFix
        anchors.fill: parent
        z: 40
        color: "#d9080b12"
        border.width: 1
        border.color: "#ffb454"

        MouseArea { anchors.fill: parent }

        Rectangle {
          width: Math.min(560, parent.width - 48)
          height: 190
          anchors.centerIn: parent
          color: "#111824"
          border.width: 1
          border.color: "#ffb454"

          ColumnLayout {
            anchors.fill: parent
            anchors.margins: 20
            spacing: 10
            Text {
              text: "CONFIRM REPAIR ACTION"
              color: "#ffb454"
              font.family: "monospace"
              font.pixelSize: root.fontTitle
              font.bold: true
            }
            Text {
              Layout.fillWidth: true
              text: "This will run an allowlisted package repair with elevated permissions.\n\nFinding: " + String(root.selectedFix().title || root.pendingFixId) + "\nCommand: " + String(root.selectedFix().command || "not available") + "\n\nAllow only if you reviewed the explanation and proposed command. A fix report will be written after completion."
              color: "#c8d2e8"
              font.family: "monospace"
              font.pixelSize: root.fontBody
              wrapMode: Text.Wrap
            }
            RowLayout {
              Layout.fillWidth: true
              Item { Layout.fillWidth: true }
              Rectangle {
                Layout.preferredWidth: 110
                Layout.preferredHeight: 36
                color: "#1d2634"
                border.width: 1
                border.color: "#8290a4"
                Text { anchors.centerIn: parent; text: "CANCEL"; color: "#c8d2e8"; font.family: "monospace"; font.pixelSize: root.fontSmall }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.confirmingFix = false }
              }
              Rectangle {
                Layout.preferredWidth: 150
                Layout.preferredHeight: 36
                color: "#2a2417"
                border.width: 1
                border.color: "#ffb454"
                Text { anchors.centerIn: parent; text: "AUTHORIZE / APPLY"; color: "#ffb454"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.applyFix() }
              }
            }
          }
        }
      }

      Rectangle {
        visible: root.confirmingHelp
        anchors.fill: parent
        z: 45
        color: "#d9080b12"
        border.width: 1
        border.color: "#52e8ff"

        MouseArea { anchors.fill: parent }

        Rectangle {
          width: Math.min(600, parent.width - 48)
          height: 230
          anchors.centerIn: parent
          color: "#111824"
          border.width: 1
          border.color: "#52e8ff"

          ColumnLayout {
            anchors.fill: parent
            anchors.margins: 20
            spacing: 10
            Text {
              text: "ALLOW EXTERNAL REFERENCE"
              color: "#52e8ff"
              font.family: "monospace"
              font.pixelSize: root.fontTitle
              font.bold: true
            }
            Text {
              Layout.fillWidth: true
              text: "Open the " + root.pendingHelpLabel + " in your browser?\n\nThis is read-only guidance. It will not run a repair or grant permissions.\n\n" + root.pendingHelpUrl
              color: "#c8d2e8"
              font.family: "monospace"
              font.pixelSize: root.fontBody
              wrapMode: Text.Wrap
            }
            RowLayout {
              Layout.fillWidth: true
              Item { Layout.fillWidth: true }
              Rectangle {
                Layout.preferredWidth: 110
                Layout.preferredHeight: 36
                color: "#1d2634"
                border.width: 1
                border.color: "#8290a4"
                Text { anchors.centerIn: parent; text: "DENY / CLOSE"; color: "#c8d2e8"; font.family: "monospace"; font.pixelSize: root.fontSmall }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.confirmingHelp = false }
              }
              Rectangle {
                Layout.preferredWidth: 145
                Layout.preferredHeight: 36
                color: "#12262f"
                border.width: 1
                border.color: "#52e8ff"
                Text { anchors.centerIn: parent; text: "ALLOW / OPEN"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.allowHelp() }
              }
            }
          }
        }
      }

      Rectangle {
        visible: root.fullReportView && root.selectedReport.length > 0
        anchors.fill: parent
        z: 15
        color: "#080b12"
        border.width: 1
        border.color: "#52e8ff"

        ColumnLayout {
          anchors.fill: parent
          anchors.margins: 22
          spacing: 12

          RowLayout {
            Layout.fillWidth: true
            Text {
              text: root.reportIcon(root.selectedReport) + "  FULL REPORT / " + root.selectedReport.split("/").pop()
              color: "#52e8ff"
              font.family: "monospace"
              font.pixelSize: root.fontTitle
              font.bold: true
              elide: Text.ElideMiddle
              Layout.fillWidth: true
            }
            Rectangle {
              Layout.preferredWidth: 112
              Layout.preferredHeight: 34
              color: "#121c2b"
              border.width: 1
              border.color: "#ff4f9a"
              Text { anchors.centerIn: parent; text: "BACK"; color: "#ff4f9a"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
              MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.fullReportView = false }
            }
          }

          Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: "#263445" }

          ListView {
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: HudScrollBar {}
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            cacheBuffer: 400
            model: root.fullReportChunks
            delegate: reportChunkDelegate
          }
        }
      }

      Rectangle {
        visible: root.fixCenterOpen
        anchors.fill: parent
        z: 25
        color: "#080b12"
        border.width: 1
        border.color: "#ffb454"

        MouseArea { anchors.fill: parent; onClicked: function(mouse) { mouse.accepted = true } }

        ColumnLayout {
          anchors.fill: parent
          anchors.margins: 22
          spacing: 12

          RowLayout {
            Layout.fillWidth: true
            Text {
              text: "🧰  FIX CENTER / REPAIR CONTROL"
              color: "#ffb454"
              font.family: "monospace"
              font.pixelSize: root.fontTitle
              font.bold: true
              Layout.fillWidth: true
            }
            Text {
              text: SnapshotReader.suggestions.length + " ACTIONS"
              color: "#52e8ff"
              font.family: "monospace"
              font.pixelSize: root.fontSmall
              font.bold: true
            }
            Rectangle {
              Layout.preferredWidth: 104
              Layout.preferredHeight: 34
              color: "#121c2b"
              border.width: 1
              border.color: "#ff4f9a"
              Text { anchors.centerIn: parent; text: "CLOSE"; color: "#ff4f9a"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
              MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.closeFixCenter() }
            }
          }

          Text {
            Layout.fillWidth: true
            text: "Select a finding to review the command, man page, documentation, and repair mode. Manual repair only opens guidance; Auto Repair requires confirmation and one explicit authorization."
            color: "#c8d2e8"
            font.family: "monospace"
            font.pixelSize: root.fontBody
            wrapMode: Text.Wrap
          }

          Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: "#263445" }

          RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 12

            Rectangle {
              Layout.preferredWidth: 350
              Layout.fillHeight: true
              color: "#0d1320"
              border.width: 1
              border.color: "#263445"

              ColumnLayout {
                anchors.fill: parent
                anchors.margins: 10
                spacing: 7
                Text { text: "REPAIR QUEUE"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }
                ListView {
                  boundsBehavior: Flickable.StopAtBounds
                  ScrollBar.vertical: HudScrollBar {}
                  Layout.fillWidth: true
                  Layout.fillHeight: true
                  clip: true
                  model: SnapshotReader.suggestions
                  spacing: 6
                  delegate: Rectangle {
                    id: fixRow
                    required property var modelData
                    required property int index
                    width: ListView.view.width - 12
                    height: 64
                    color: root.selectedFixIndex === fixRow.index ? "#1c2b43" : "#111824"
                    border.width: 1
                    border.color: SnapshotReader.severityColor(String(fixRow.modelData.severity || "attention"))
                    RowLayout {
                      anchors.fill: parent
                      anchors.margins: 9
                      spacing: 8
                      Text { text: String(fixRow.modelData.severity || "attention").toUpperCase(); color: SnapshotReader.severityColor(String(fixRow.modelData.severity || "attention")); font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                      ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text { text: String(fixRow.modelData.title || ""); color: "#f2f5f7"; font.family: "monospace"; font.pixelSize: root.fontSmall; elide: Text.ElideRight; Layout.fillWidth: true }
                        Text { text: fixRow.modelData.auto_fix ? "AUTO REPAIR AVAILABLE" : "MANUAL REVIEW REQUIRED"; color: fixRow.modelData.auto_fix ? "#c8e967" : "#8290a4"; font.family: "monospace"; font.pixelSize: root.fontMicro }
                      }
                    }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.selectedFixIndex = fixRow.index }
                  }
                  Text {
                    anchors.centerIn: parent
                    visible: SnapshotReader.suggestions.length === 0
                    text: "NO FINDINGS / SYSTEM CLEAR"
                    color: "#c8e967"
                    font.family: "monospace"
                    font.pixelSize: root.fontSmall
                  }
                }
              }
            }

            Rectangle {
              Layout.fillWidth: true
              Layout.fillHeight: true
              color: "#0d1320"
              border.width: 1
              border.color: SnapshotReader.suggestions.length ? SnapshotReader.severityColor(String(root.selectedFix().severity || "attention")) : "#263445"

              Flickable {
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: HudScrollBar {}
                id: fixDetailScroll
                anchors.fill: parent
                anchors.margins: 16
                clip: true
                contentWidth: width
                contentHeight: fixDetailColumn.implicitHeight

                ColumnLayout {
                  id: fixDetailColumn
                  width: fixDetailScroll.width - 14
                  spacing: 10

                Text {
                  Layout.fillWidth: true
                  text: SnapshotReader.suggestions.length ? String(root.selectedFix().title || "SELECT A FINDING") : "NO REPAIR ACTIONS"
                  color: SnapshotReader.suggestions.length ? SnapshotReader.severityColor(String(root.selectedFix().severity || "attention")) : "#c8e967"
                  font.family: "monospace"
                  font.pixelSize: root.fontTitle
                  font.bold: true
                  wrapMode: Text.Wrap
                }
                Text {
                  Layout.fillWidth: true
                  text: SnapshotReader.suggestions.length ? String(root.selectedFix().detail || "") : "The latest audit did not produce repair suggestions."
                  color: "#c8d2e8"
                  font.family: "monospace"
                  font.pixelSize: root.fontBody
                  wrapMode: Text.Wrap
                }
                Text {
                  visible: SnapshotReader.suggestions.length > 0
                  Layout.fillWidth: true
                  text: "EXPLANATION / IMPACT"
                  color: "#52e8ff"
                  font.family: "monospace"
                  font.pixelSize: root.fontSection
                  font.bold: true
                }
                Text {
                  visible: SnapshotReader.suggestions.length > 0
                  Layout.fillWidth: true
                  text: String(root.selectedFix().explanation || "No additional explanation was recorded for this finding.")
                  color: "#c8d2e8"
                  font.family: "monospace"
                  font.pixelSize: root.fontSmall
                  wrapMode: Text.Wrap
                }
                Text {
                  visible: SnapshotReader.suggestions.length > 0
                  Layout.fillWidth: true
                  text: "MANUAL CHECKLIST / REVIEW EACH STEP"
                  color: "#ffb454"
                  font.family: "monospace"
                  font.pixelSize: root.fontSection
                  font.bold: true
                }
                ColumnLayout {
                  visible: SnapshotReader.suggestions.length > 0
                  Layout.fillWidth: true
                  spacing: 3
                  Repeater {
                    model: SnapshotReader.suggestions.length > 0 ? (root.selectedFix().manual_steps || []) : []
                    delegate: Text {
                      id: stepLine
                      required property var modelData
                      Layout.fillWidth: true
                      text: "◆ " + String(stepLine.modelData || "")
                      color: "#c8d2e8"
                      font.family: "monospace"
                      font.pixelSize: root.fontSmall
                      wrapMode: Text.Wrap
                    }
                  }
                }
                Text { visible: SnapshotReader.suggestions.length > 0; text: "PROPOSED COMMAND / INSPECTION"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }
                Rectangle {
                  visible: SnapshotReader.suggestions.length > 0
                  Layout.fillWidth: true
                  Layout.preferredHeight: 52
                  color: "#080b12"
                  border.width: 1
                  border.color: "#263445"
                  Text { anchors.fill: parent; anchors.margins: 10; text: String(root.selectedFix().command || "NO AUTOMATIC COMMAND"); color: "#ffb454"; font.family: "monospace"; font.pixelSize: root.fontBody; wrapMode: Text.Wrap; verticalAlignment: Text.AlignVCenter }
                }
                Text {
                  visible: SnapshotReader.suggestions.length > 0
                  Layout.fillWidth: true
                  text: (root.selectedFix().auto_fix ? "AUTO REPAIR: AVAILABLE AFTER AUTHORIZATION / " : "AUTO REPAIR: NOT AVAILABLE / ") + String(root.selectedFix().auto_fix_reason || "Manual review is required.")
                  color: root.selectedFix().auto_fix ? "#c8e967" : "#ff8f70"
                  font.family: "monospace"
                  font.pixelSize: root.fontSmall
                  wrapMode: Text.Wrap
                }
                RowLayout {
                  Layout.fillWidth: true
                  spacing: 8
                  Rectangle {
                    Layout.preferredWidth: 112
                    Layout.preferredHeight: 32
                    color: "#121c2b"
                    border.width: 1
                    border.color: "#52e8ff"
                    Text { anchors.centerIn: parent; text: "OPEN MAN PAGE"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixHelp(root.selectedFix().man_url, "MAN PAGE") }
                  }
                  Rectangle {
                    Layout.preferredWidth: 124
                    Layout.preferredHeight: 32
                    color: "#121c2b"
                    border.width: 1
                    border.color: "#52e8ff"
                    Text { anchors.centerIn: parent; text: "OPEN DOCUMENTATION"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixHelp(root.selectedFix().docs_url, "DOCUMENTATION") }
                  }
                  Rectangle {
                    Layout.preferredWidth: 152
                    Layout.preferredHeight: 32
                    color: "#121c2b"
                    border.width: 1
                    border.color: "#a56bff"
                    Text { anchors.centerIn: parent; text: "VIEW MD REPORT"; color: "#a56bff"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openSuggestionsReport() }
                  }
                }
                Text { visible: root.fixMessage.length > 0; Layout.fillWidth: true; text: root.fixMessage; color: root.fixMessage.indexOf("FAILED") >= 0 ? "#ff667d" : "#c8e967"; font.family: "monospace"; font.pixelSize: root.fontMicro; elide: Text.ElideMiddle }
                RowLayout {
                  Layout.fillWidth: true
                  spacing: 8
                  Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 42
                    color: "#182438"
                    border.width: 1
                    border.color: "#8290a4"
                    Text { anchors.centerIn: parent; text: "MANUAL REPAIR / OPEN GUIDE"; color: "#c8d2e8"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixHelp(root.selectedFix().docs_url, "MANUAL REPAIR GUIDE") }
                  }
                  Rectangle {
                    visible: SnapshotReader.suggestions.length > 0 && Boolean(root.selectedFix().auto_fix)
                    Layout.fillWidth: true
                    Layout.preferredHeight: 42
                    color: "#2a2417"
                    border.width: 1
                    border.color: "#ffb454"
                    Text { anchors.centerIn: parent; text: "AUTO REPAIR / CONFIRM"; color: "#ffb454"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.requestFix(String(root.selectedFix().id || "")) }
                  }
                }
                Rectangle {
                  visible: root.fixReportPath().length > 0
                  Layout.preferredWidth: 180
                  Layout.preferredHeight: 30
                  color: "#121c2b"
                  border.width: 1
                  border.color: "#c8e967"
                  Text { anchors.centerIn: parent; text: "VIEW FIX RESULT"; color: "#c8e967"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixReport() }
                }
                }
              }
            }
          }
        }
      }
    }
  }
}
