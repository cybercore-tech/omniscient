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
  readonly property string readScript: "for f in \"$1\" \"$2\"; do [ -s \"$f\" ] && { cat \"$f\"; exit 0; }; done; exit 1"

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

  function refresh() {
    if (!reader.running) reader.running = true
  }

  function stateColor(value) {
    if (value === "complete") return "#c8e967"
    if (value === "running") return "#ff4f9a"
    if (value === "error" || value === "failed") return "#ff667d"
    if (value === "queued") return "#ffb454"
    return "#52e8ff"
  }

  Process {
    id: reader
    command: ["sh", "-c", root.readScript, "omniscient-snapshot", root.runtimePath, root.fallbackPath]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var raw = text.trim()
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
          root.healthScore = value.health && value.health.score !== undefined ? Number(value.health.score) : -1
        } catch (error) {
          root.available = false
          root.state = "error"
          root.errorMessage = "INVALID SNAPSHOT JSON"
        }
      }
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
