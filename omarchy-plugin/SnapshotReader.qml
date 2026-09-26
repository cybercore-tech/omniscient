pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// A Quickshell Singleton, not an Item: an Item's own `state` property drives
// Qt's state machine, so writing "running" into it looked up a State that
// does not exist instead of storing the audit state.
Singleton {
  id: root

  readonly property string runtimePath: {
    var runtime = Quickshell.env("XDG_RUNTIME_DIR") || ""
    return runtime.length > 0 ? runtime + "/omniscient/snapshot.json" : ""
  }
  readonly property string fallbackPath: (Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state") + "/omniscient/snapshot.json"

  property bool available: false
  property string state: "offline"
  property int healthScore: -1
  property string updatedAt: ""
  property int selectedCount: 0
  property int completedCount: 0
  property string summaryPath: ""
  property string errorMessage: ""
  property string snapshotPath: ""
  property string message: ""
  property var modules: []
  property var reports: []
  property var suggestions: []
  property string suggestionsPath: ""

  // The snapshot is re-read on a timer, but it only changes when an audit
  // runs. Every assignment below re-evaluates the panel's bindings, and a new
  // array rebuilds every Repeater and ListView delegate that uses it, so an
  // unchanged snapshot must cause no assignments at all. Reading once per
  // second and replacing every array each time is what let the HUD churn the
  // whole shell.
  readonly property int maxSnapshotBytes: 1048576
  readonly property int maxListItems: 256
  readonly property int maxTextLength: 4096
  property string lastRaw: ""
  property int consumeCount: 0
  property int applyCount: 0

  function refresh() {
    if (!runtimeReader.running && !fallbackReader.running) runtimeReader.running = true
  }

  function boundedText(value) {
    var text = String(value === undefined || value === null ? "" : value)
    return text.length > root.maxTextLength ? text.substring(0, root.maxTextLength) : text
  }

  function boundedList(value) {
    return Array.isArray(value) ? value.slice(0, root.maxListItems) : []
  }

  function setIfChanged(name, value) {
    if (root[name] !== value) root[name] = value
  }

  function setListIfChanged(name, value) {
    if (JSON.stringify(root[name]) !== JSON.stringify(value)) root[name] = value
  }

  function goOffline(state, message) {
    root.lastRaw = ""
    root.setIfChanged("available", false)
    root.setIfChanged("state", state)
    root.setIfChanged("errorMessage", message)
  }

  function consume(raw) {
    root.consumeCount++
    raw = String(raw || "").trim()
    if (raw === root.lastRaw && raw.length) return
    if (!raw.length) {
      root.goOffline("offline", "SNAPSHOT UNAVAILABLE")
      return
    }
    if (raw.length > root.maxSnapshotBytes) {
      root.goOffline("error", "SNAPSHOT TOO LARGE")
      return
    }
    var value
    try {
      value = JSON.parse(raw)
    } catch (error) {
      root.goOffline("error", "INVALID SNAPSHOT JSON")
      return
    }
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
      root.goOffline("error", "INVALID SNAPSHOT JSON")
      return
    }
    root.lastRaw = raw
    root.applyCount++
    var score = value.health && value.health.score !== undefined ? Number(value.health.score) : -1
    root.setIfChanged("available", true)
    root.setIfChanged("state", root.boundedText(value.state || "ready"))
    root.setIfChanged("updatedAt", root.boundedText(value.updated_at))
    root.setIfChanged("selectedCount", Number(value.selected_count || 0) | 0)
    root.setIfChanged("completedCount", Number(value.completed_count || 0) | 0)
    root.setIfChanged("summaryPath", root.boundedText(value.summary_path))
    root.setIfChanged("errorMessage", root.boundedText(value.error))
    root.setIfChanged("message", root.boundedText(value.message))
    root.setIfChanged("snapshotPath", root.runtimePath.length > 0 ? root.runtimePath : root.fallbackPath)
    root.setListIfChanged("modules", root.boundedList(value.modules))
    root.setListIfChanged("reports", root.boundedList(value.reports))
    root.setListIfChanged("suggestions", root.boundedList(value.suggestions))
    root.setIfChanged("suggestionsPath", root.boundedText(value.suggestions_path))
    root.setIfChanged("healthScore", isFinite(score) ? Math.max(-1, Math.min(100, Math.round(score))) : -1)
  }

  function stateColor(value) {
    if (value === "complete") return "#c8e967"
    if (value === "running") return "#ff4f9a"
    if (value === "error" || value === "failed") return "#ff667d"
    if (value === "queued") return "#ffb454"
    if (value === "idle") return "#8290a4"
    return "#52e8ff"
  }

  function healthLabel(score) {
    if (score < 0) return "WAITING"
    if (score < 40) return "URGENT"
    if (score < 70) return "WARNING"
    if (score < 85) return "WATCH"
    return "HEALTHY"
  }

  function healthColor(score) {
    if (score < 0) return "#52e8ff"
    if (score < 40) return "#ff667d"
    if (score < 70) return "#ff8f70"
    if (score < 85) return "#ffb454"
    return "#c8e967"
  }

  function severityColor(value) {
    if (value === "urgent") return "#ff667d"
    if (value === "warning") return "#ff8f70"
    if (value === "attention") return "#ffb454"
    if (value === "watch") return "#ffb454"
    if (value === "healthy") return "#c8e967"
    return "#52e8ff"
  }

  Process {
    id: runtimeReader
    command: root.runtimePath.length
      ? ["/usr/bin/head", "-c", String(root.maxSnapshotBytes + 1), "--", root.runtimePath]
      : ["/usr/bin/true"]
    stdout: StdioCollector {
      id: runtimeOutput
      waitForEnd: true
      onStreamFinished: if (text.trim().length) root.consume(text)
    }
    // Process.exited's QProcess::ExitStatus parameter type is not in
    // Quickshell's type description; the handler is valid at runtime.
    onExited: if (!runtimeOutput.text.trim().length) fallbackReader.running = true // qmllint disable signal-handler-parameters
  }

  Process {
    id: fallbackReader
    command: ["/usr/bin/head", "-c", String(root.maxSnapshotBytes + 1), "--", root.fallbackPath]
    stdout: StdioCollector {
      id: fallbackOutput
      waitForEnd: true
      onStreamFinished: root.consume(text)
    }
  }

  Timer {
    interval: 2000
    repeat: true
    running: true
    onTriggered: root.refresh()
  }

  Component.onCompleted: root.refresh()
}
