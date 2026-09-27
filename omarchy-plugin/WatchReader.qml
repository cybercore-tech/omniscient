pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Latest `omniscient --watch` result (watch.json next to the snapshot), for
// the bar widget and the AUDIT tab. Read on file change, bounded like the
// snapshot; an unchanged file causes no updates.
Singleton {
  id: root

  readonly property string path: {
    var runtime = Quickshell.env("XDG_RUNTIME_DIR") || ""
    return runtime.length > 0 ? runtime + "/omniscient/watch.json" : ""
  }
  readonly property int maxBytes: 262144

  property bool available: false
  property string updatedAt: ""
  property string worst: ""
  property int urgent: 0
  property int warning: 0
  property int watchCount: 0
  property int newCount: 0
  property var findings: []
  property string lastRaw: ""
  property int applyCount: 0

  function refresh() {
    if (root.path.length && !reader.running) reader.running = true
  }

  function consume(raw) {
    raw = String(raw || "").trim()
    if (raw === root.lastRaw) return
    root.lastRaw = raw
    var value = null
    try {
      value = raw.length && raw.length <= root.maxBytes ? JSON.parse(raw) : null
    } catch (error) {
      value = null
    }
    if (value === null || typeof value !== "object" || value.version !== 1 || !Array.isArray(value.findings)) {
      root.available = false
      return
    }
    root.applyCount++
    var counts = value.counts || {}
    root.available = true
    root.updatedAt = String(value.updated_at || "")
    root.worst = String(value.worst || "")
    root.urgent = Number(counts.urgent) | 0
    root.warning = Number(counts.warning) | 0
    root.watchCount = Number(counts.watch) | 0
    root.newCount = Number(value.new_count) | 0
    root.findings = value.findings.slice(0, 20).map(function(f) {
      return {
        key: String(f.key || "").substring(0, 200),
        severity: String(f.severity || "watch"),
        title: String(f.title || "").substring(0, 300),
        detail: String(f.detail || "").substring(0, 600),
        isNew: Boolean(f.new),
        unit: String(f.unit || "").substring(0, 200)
      }
    })
  }

  function clock() {
    var d = new Date(root.updatedAt)
    if (isNaN(d.getTime())) return "—"
    var pad = function(n) { return (n < 10 ? "0" : "") + n }
    return pad(d.getHours()) + ":" + pad(d.getMinutes())
  }

  function severityColor(value) {
    if (value === "urgent") return "#ff667d"
    if (value === "warning") return "#ff8f70"
    if (value === "watch") return "#ffb454"
    return "#c8e967"
  }

  Process {
    id: reader
    command: ["/usr/bin/head", "-c", String(root.maxBytes + 1), "--", root.path]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.consume(text)
    }
  }

  FileView {
    path: root.path
    preload: false
    watchChanges: root.path.length > 0
    printErrors: false
    onFileChanged: settle.restart()
  }

  Timer {
    id: settle
    interval: 150
    onTriggered: root.refresh()
  }

  Component.onCompleted: root.refresh()
}
