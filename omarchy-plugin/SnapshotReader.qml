pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Item {
  id: root
  visible: false

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

  function refresh() {
    if (!runtimeReader.running && !fallbackReader.running) runtimeReader.running = true
  }

  function consume(raw) {
    raw = String(raw || "").trim()
    if (!raw.length) {
      root.available = false
      root.state = "offline"
      root.errorMessage = "SNAPSHOT UNAVAILABLE"
      return
    }
    try {
      var value = JSON.parse(raw)
      root.available = true
      root.state = String(value.state || "ready")
      root.updatedAt = String(value.updated_at || "")
      root.selectedCount = Number(value.selected_count || 0)
      root.completedCount = Number(value.completed_count || 0)
      root.summaryPath = String(value.summary_path || "")
      root.errorMessage = String(value.error || "")
      root.message = String(value.message || "")
      root.snapshotPath = root.runtimePath.length > 0 ? root.runtimePath : root.fallbackPath
      root.modules = Array.isArray(value.modules) ? value.modules : []
      root.reports = Array.isArray(value.reports) ? value.reports : []
      root.suggestions = Array.isArray(value.suggestions) ? value.suggestions : []
      root.suggestionsPath = String(value.suggestions_path || "")
      root.healthScore = value.health && value.health.score !== undefined ? Number(value.health.score) : -1
    } catch (error) {
      root.available = false
      root.state = "error"
      root.errorMessage = "INVALID SNAPSHOT JSON"
    }
  }

  function stateColor(value) {
    if (value === "complete") return "#c8e967"
    if (value === "running") return "#ff4f9a"
    if (value === "error" || value === "failed") return "#ff667d"
    if (value === "queued") return "#ffb454"
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
    command: root.runtimePath.length ? ["/usr/bin/cat", root.runtimePath] : ["/usr/bin/true"]
    stdout: StdioCollector {
      id: runtimeOutput
      waitForEnd: true
      onStreamFinished: if (text.trim().length) root.consume(text)
    }
    onExited: if (!runtimeOutput.text.trim().length) fallbackReader.running = true
  }

  Process {
    id: fallbackReader
    command: ["/usr/bin/cat", root.fallbackPath]
    stdout: StdioCollector {
      id: fallbackOutput
      waitForEnd: true
      onStreamFinished: root.consume(text)
    }
  }

  Timer {
    interval: 1000
    repeat: true
    running: true
    onTriggered: root.refresh()
  }

  Component.onCompleted: root.refresh()
}
