import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

BarWidget {
  id: root
  moduleName: "io.github.cybercore-tech.omniscient"

  // Live status: the mark takes the colour of the worst finding from the
  // hourly watch, a badge counts urgent + warning findings, and hovering
  // takes one fresh sensor reading for the tooltip (nothing polls).
  readonly property int alertCount: WatchReader.urgent + WatchReader.warning
  readonly property color statusColor: SnapshotReader.state === "error" || WatchReader.worst === "urgent"
    ? Color.urgent
    : WatchReader.worst === "warning" ? "#ff8f70"
    : SnapshotReader.state === "running" ? "#ff4f9a" : "#c8e967"
  property string temps: ""
  property double tempsAt: 0

  function readTemps() {
    if (sensorReader.running || Date.now() - root.tempsAt < 10000) return
    sensorReader.running = true
  }

  function acceptTemps(raw) {
    var text = String(raw || "")
    if (!text.length || text.length > 600000) return
    try {
      var reading = JSON.parse(text)
      var parts = []
      if (reading.cpu && reading.cpu.package_celsius !== null) parts.push("CPU " + Math.round(reading.cpu.package_celsius) + "°C")
      var drives = (reading.drives || []).filter(function(d) { return d.celsius !== null })
      if (drives.length) {
        var hottest = drives.reduce(function(a, b) { return a.celsius >= b.celsius ? a : b })
        parts.push(hottest.name + " " + Math.round(hottest.celsius) + "°C")
      }
      if (reading.gpus && reading.gpus.length && reading.gpus[0].busy_percent !== null) parts.push("GPU " + reading.gpus[0].busy_percent + "%")
      root.temps = parts.join(" · ")
      root.tempsAt = Date.now()
    } catch (error) {
      root.temps = ""
    }
  }

  function tooltip() {
    var lines = [SnapshotReader.available
      ? "Omniscient / health " + SnapshotReader.healthScore + "/100"
      : "Omniscient / no audit yet"]
    if (WatchReader.available) {
      lines.push("Watch " + WatchReader.clock() + ": " + WatchReader.urgent + " urgent, " + WatchReader.warning + " warning"
                 + (WatchReader.newCount ? ", " + WatchReader.newCount + " new" : ""))
      if (WatchReader.findings.length) lines.push("▸ " + WatchReader.findings[0].title)
    }
    if (root.temps.length) lines.push(root.temps)
    return lines.join("\n")
  }

  Process {
    id: sensorReader
    command: [(Quickshell.env("HOME") || "") + "/.local/bin/omniscient", "--sensors"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.acceptTemps(text)
    }
  }

  HoverHandler {
    onHoveredChanged: if (hovered) root.readTemps()
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "OMNI"
    labelVisible: false
    tooltipText: root.tooltip()
    foreground: SnapshotReader.state === "error"
      ? Color.urgent
      : SnapshotReader.state === "running"
        ? Color.accent
        : Color.foreground
    fixedWidth: root.vertical ? -1 : Style.space(26)
    fixedHeight: root.vertical ? Style.space(26) : -1
    onPressed: function(b) {
      if (!root.bar) return
      // The host declares `bar` as a plain QtObject, so the linter cannot see
      // Bar.run(); stock Omarchy widgets call it the same way.
      root.bar.run("omarchy-shell shell toggle io.github.cybercore-tech.omniscient") // qmllint disable missing-property
    }

    Item {
      id: cybercoreMark
      z: 1
      width: Style.space(18)
      height: Style.space(18)
      anchors.centerIn: parent

      Rectangle {
        id: outerDiamond
        width: Style.space(13)
        height: Style.space(13)
        anchors.centerIn: parent
        rotation: 45
        radius: 4
        color: "#273142"
        border.width: 0
      }

      Rectangle {
        width: Style.space(5)
        height: Style.space(5)
        anchors.centerIn: parent
        rotation: 45
        color: root.statusColor
      }

      Rectangle {
        width: Style.space(7)
        height: 2
        anchors.centerIn: parent
        rotation: -45
        radius: 1
        color: "#111824"
      }

      Rectangle {
        width: Style.space(3)
        height: Style.space(3)
        anchors.top: outerDiamond.top
        anchors.right: outerDiamond.right
        radius: 2
        color: SnapshotReader.state === "error" ? Color.urgent : "#52e8ff"
      }

      // Urgent + warning findings from the latest watch.
      Rectangle {
        visible: root.alertCount > 0
        z: 2
        width: Math.max(Style.space(11), badgeText.implicitWidth + Style.space(4))
        height: Style.space(11)
        radius: height / 2
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.topMargin: -Style.space(4)
        anchors.rightMargin: -Style.space(6)
        color: root.statusColor
        Text {
          id: badgeText
          anchors.centerIn: parent
          text: root.alertCount > 9 ? "9+" : String(root.alertCount)
          color: "#080b12"
          font.pixelSize: Style.space(8)
          font.bold: true
        }
      }
    }
  }
}
