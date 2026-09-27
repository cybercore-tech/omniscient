pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Quickshell.Io

// JOURNAL tab: filtered, collapsed journal lines with a live tail and the
// boot's top offenders. Read-only; every filter is validated again by
// `omniscient --journal` before it reaches journalctl (argv only).
Item {
  id: view

  required property string binary
  // True while the panel is open on this tab; nothing runs otherwise.
  property bool active: false

  readonly property var priorities: ["EMERG", "ALERT", "CRIT", "ERR", "WARNING", "NOTICE", "INFO", "DEBUG"]
  readonly property var sinceChoices: [
    { id: "boot", label: "THIS BOOT" }, { id: "today", label: "TODAY" },
    { id: "1h", label: "1 HOUR" }, { id: "15m", label: "15 MIN" }
  ]
  readonly property int maxEntries: 2000
  readonly property int maxOutputChars: 9000000

  property int priority: 4
  property string since: "boot"
  property string boot: "0"
  property string unit: ""
  property string grep: ""
  property bool follow: false
  property string cursor: ""
  property var boots: []
  property var offenders: []
  property bool offendersPartial: false
  property string error: ""
  property int loads: 0
  property int tails: 0
  property int expanded: -1
  property bool loadedOnce: false

  // Rows as a ListModel so the live tail appends without rebuilding views.
  ListModel { id: rows }
  readonly property alias rowCount: rows.count

  function argsFor(tail) {
    var args = [view.binary, "--journal", "--priority", String(view.priority), "--boot", view.boot, "--since", view.since]
    if (view.unit.length) args.push("--unit", view.unit)
    if (view.grep.length) args.push("--grep", view.grep)
    if (tail) args.push("--after-cursor", view.cursor, "--limit", "500")
    else args.push("--limit", "500", "--offenders")
    return args
  }

  function reload() {
    if (reader.running) {
      reader.pendingReload = true
      return
    }
    view.expanded = -1
    reader.tail = false
    reader.command = view.argsFor(false)
    view.loads++
    reader.running = true
  }

  function tailOnce() {
    if (reader.running || !view.cursor.length) return
    reader.tail = true
    reader.command = view.argsFor(true)
    view.tails++
    reader.running = true
  }

  function accept(raw, tail) {
    var text = String(raw || "").trim()
    if (!text.length) return
    if (text.length > view.maxOutputChars) {
      view.error = "JOURNAL VIEW TOO LARGE"
      return
    }
    var value
    try {
      value = JSON.parse(text)
    } catch (e) {
      view.error = "INVALID JOURNAL VIEW"
      return
    }
    if (value === null || typeof value !== "object" || value.version !== 1 || !Array.isArray(value.entries)) {
      view.error = "UNEXPECTED JOURNAL VIEW"
      return
    }
    view.error = String(value.error || "")
    if (!tail) rows.clear()
    for (var i = 0; i < value.entries.length; i++) {
      var e = value.entries[i]
      rows.append({
        time: Number(e.time) || 0,
        priority: Math.max(0, Math.min(7, Number(e.priority) || 6)),
        source: String(e.source || ""),
        pid: String(e.pid || ""),
        message: String(e.message || ""),
        count: Number(e.count) || 1,
        bootId: String(e.boot || "")
      })
    }
    if (rows.count > view.maxEntries) rows.remove(0, rows.count - view.maxEntries)
    if (value.cursor) view.cursor = String(value.cursor)
    if (!tail) {
      view.boots = Array.isArray(value.boots) ? value.boots.slice(0, 12) : []
      view.offenders = Array.isArray(value.offenders) ? value.offenders.slice(0, 25) : []
      view.offendersPartial = Boolean(value.offenders_partial)
      list.positionViewAtEnd()
    } else if (value.entries.length && list.atYEnd) {
      list.positionViewAtEnd()
    }
    view.loadedOnce = true
  }

  function priorityColor(p) {
    if (p <= 2) return "#ff4f9a"
    if (p === 3) return "#ff667d"
    if (p === 4) return "#ffb454"
    if (p === 5) return "#52e8ff"
    return "#8290a4"
  }

  function clock(micros) {
    if (!micros) return "--:--:--"
    var d = new Date(micros / 1000)
    var pad = function(n) { return (n < 10 ? "0" : "") + n }
    return pad(d.getHours()) + ":" + pad(d.getMinutes()) + ":" + pad(d.getSeconds())
  }

  function dayOf(micros) {
    if (!micros) return ""
    var d = new Date(micros / 1000)
    return (d.getMonth() + 1) + "/" + d.getDate() + " " + view.clock(micros).substring(0, 5)
  }

  onActiveChanged: if (view.active && !view.loadedOnce) view.reload()
  onPriorityChanged: if (view.active) view.reload()
  onSinceChanged: if (view.active) view.reload()
  onBootChanged: if (view.active) view.reload()
  onUnitChanged: if (view.active) view.reload()
  onGrepChanged: if (view.active) view.reload()

  Process {
    id: reader
    property bool tail: false
    property bool pendingReload: false
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: view.accept(text, reader.tail)
    }
    stderr: StdioCollector {
      id: readerErrors
      waitForEnd: true
    }
    onExited: function(exitCode) { // qmllint disable signal-handler-parameters
      if (exitCode !== 0) view.error = (readerErrors.text.trim() || ("JOURNAL READ FAILED / CODE " + exitCode)).substring(0, 300)
      if (reader.pendingReload) {
        reader.pendingReload = false
        view.reload()
      }
    }
  }

  Timer {
    interval: 2000
    repeat: true
    running: view.active && view.follow
    onTriggered: view.tailOnce()
  }

  component Chip: Rectangle {
    id: chip
    property string label: ""
    property bool selected: false
    property color tint: "#52e8ff"
    signal clicked()
    property bool hovered: false
    implicitWidth: chipText.implicitWidth + 14
    implicitHeight: 22
    radius: 3
    color: chip.selected ? "#1a2940" : (chip.hovered ? "#142033" : "#0d1320")
    border.width: 1
    border.color: chip.selected ? chip.tint : "#263445"
    Text {
      id: chipText
      anchors.centerIn: parent
      text: chip.label
      color: chip.selected ? chip.tint : "#8290a4"
      font.family: "monospace"
      font.pixelSize: 10
      font.bold: chip.selected
    }
    MouseArea {
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onEntered: chip.hovered = true
      onExited: chip.hovered = false
      onClicked: chip.clicked()
    }
  }

  ColumnLayout {
    anchors.fill: parent
    spacing: 8

    // ---- filters ---------------------------------------------------------
    Flow {
      Layout.fillWidth: true
      spacing: 5
      Repeater {
        model: view.priorities
        delegate: Chip {
          required property var modelData
          required property int index
          label: modelData
          selected: view.priority === index
          tint: view.priorityColor(index)
          onClicked: view.priority = index
        }
      }
      Item { width: 10; height: 1 }
      Repeater {
        model: view.sinceChoices
        delegate: Chip {
          required property var modelData
          label: modelData.label
          selected: view.since === modelData.id
          onClicked: view.since = modelData.id
        }
      }
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: 6
      Flow {
        Layout.fillWidth: true
        spacing: 5
        Repeater {
          model: view.boots.slice(0, 6)
          delegate: Chip {
            required property var modelData
            label: (modelData.index === 0 ? "CURRENT" : "BOOT " + modelData.index) + " · " + view.dayOf(modelData.first)
            selected: view.boot === String(modelData.index)
            tint: "#a56bff"
            onClicked: view.boot = String(modelData.index)
          }
        }
      }
      Chip {
        visible: view.unit.length > 0
        label: "UNIT " + view.unit + "  ×"
        selected: true
        tint: "#ffb454"
        onClicked: view.unit = ""
      }
      TextField {
        id: search
        Layout.preferredWidth: 220
        Layout.preferredHeight: 26
        placeholderText: "search (Enter)"
        maximumLength: 120
        color: "#f2f5f7"
        placeholderTextColor: "#5a6a80"
        font.family: "monospace"
        font.pixelSize: 11
        background: Rectangle { color: "#0b111b"; radius: 3; border.width: 1; border.color: search.activeFocus ? "#52e8ff" : "#263445" }
        onAccepted: view.grep = search.text.replace(/[\u0000-\u001f\u007f]/g, "")
      }
      Chip {
        label: view.follow ? "● FOLLOWING" : "FOLLOW"
        selected: view.follow
        tint: "#c8e967"
        onClicked: view.follow = !view.follow
      }
      Chip {
        label: "RELOAD"
        onClicked: view.reload()
      }
    }

    Text {
      visible: view.error.length > 0
      Layout.fillWidth: true
      text: view.error
      color: "#ff667d"
      font.family: "monospace"
      font.pixelSize: 10
      wrapMode: Text.Wrap
    }

    RowLayout {
      Layout.fillWidth: true
      Layout.fillHeight: true
      spacing: 10

      // ---- entries -------------------------------------------------------
      Rectangle {
        Layout.fillWidth: true
        Layout.fillHeight: true
        color: "#0d1320"
        border.width: 1
        border.color: "#263445"

        ListView {
          id: list
          anchors.fill: parent
          anchors.margins: 6
          clip: true
          model: rows
          boundsBehavior: Flickable.StopAtBounds
          cacheBuffer: 600
          ScrollBar.vertical: HudScrollBar {}

          delegate: Rectangle {
            id: row
            required property int index
            required property real time
            required property int priority
            required property string source
            required property string pid
            required property string message
            required property int count
            required property string bootId
            readonly property bool open: view.expanded === row.index
            width: ListView.view.width - 12
            height: rowColumn.implicitHeight + 6
            color: row.open ? "#1a2940" : (row.index % 2 ? "#0f1725" : "transparent")

            ColumnLayout {
              id: rowColumn
              anchors.left: parent.left
              anchors.right: parent.right
              anchors.verticalCenter: parent.verticalCenter
              anchors.leftMargin: 4
              spacing: 2
              RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Text { text: view.clock(row.time); color: "#5a6a80"; font.family: "monospace"; font.pixelSize: 10 }
                Rectangle {
                  implicitWidth: 44
                  implicitHeight: 14
                  radius: 2
                  color: "transparent"
                  border.width: 1
                  border.color: view.priorityColor(row.priority)
                  Text { anchors.centerIn: parent; text: view.priorities[row.priority].substring(0, 4); color: view.priorityColor(row.priority); font.family: "monospace"; font.pixelSize: 9; font.bold: true }
                }
                Text {
                  Layout.preferredWidth: 170
                  text: row.source
                  color: "#52e8ff"
                  font.family: "monospace"
                  font.pixelSize: 10
                  elide: Text.ElideRight
                  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: view.unit = row.source }
                }
                Text {
                  visible: row.count > 1
                  text: "×" + row.count
                  color: "#ffb454"
                  font.family: "monospace"
                  font.pixelSize: 10
                  font.bold: true
                }
                Text {
                  Layout.fillWidth: true
                  text: row.message
                  color: row.priority <= 3 ? "#f2f5f7" : "#c8d2e8"
                  font.family: "monospace"
                  font.pixelSize: 10
                  elide: row.open ? Text.ElideNone : Text.ElideRight
                  wrapMode: row.open ? Text.WrapAnywhere : Text.NoWrap
                  maximumLineCount: row.open ? 40 : 1
                }
              }
              Text {
                visible: row.open
                text: "pid " + (row.pid || "—") + " · boot " + (row.bootId.substring(0, 12) || "—") + (row.count > 1 ? " · collapsed " + row.count + " similar lines" : "")
                color: "#8290a4"
                font.family: "monospace"
                font.pixelSize: 9
              }
            }
            MouseArea {
              anchors.fill: parent
              z: -1
              onClicked: view.expanded = row.open ? -1 : row.index
            }
          }

          Text {
            anchors.centerIn: parent
            visible: rows.count === 0
            text: view.loadedOnce ? "NO MATCHING JOURNAL LINES" : (reader.running ? "READING JOURNAL…" : "")
            color: "#8290a4"
            font.family: "monospace"
            font.pixelSize: 11
          }
        }
      }

      // ---- offenders -----------------------------------------------------
      Rectangle {
        Layout.preferredWidth: 250
        Layout.fillHeight: true
        color: "#0d1320"
        border.width: 1
        border.color: "#263445"
        ColumnLayout {
          anchors.fill: parent
          anchors.margins: 8
          spacing: 4
          Text {
            text: "TOP OFFENDERS"
            color: "#ffb454"
            font.family: "monospace"
            font.pixelSize: 11
            font.bold: true
          }
          Text {
            Layout.fillWidth: true
            text: view.offendersPartial ? "warnings, newest 50,000 of this boot" : "warnings this boot"
            color: "#5a6a80"
            font.family: "monospace"
            font.pixelSize: 9
            wrapMode: Text.Wrap
          }
          ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: view.offenders
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: HudScrollBar {}
            delegate: Rectangle {
              id: offender
              required property var modelData
              required property int index
              property bool hovered: false
              width: ListView.view.width - 12
              height: 22
              color: offender.hovered || view.unit === offender.modelData.source ? "#1a2940" : (offender.index % 2 ? "#0f1725" : "transparent")
              RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 4
                anchors.rightMargin: 4
                Text { Layout.fillWidth: true; text: offender.modelData.source; color: "#c8d2e8"; font.family: "monospace"; font.pixelSize: 10; elide: Text.ElideRight }
                Text { text: offender.modelData.count; color: "#ffb454"; font.family: "monospace"; font.pixelSize: 10; font.bold: true }
              }
              MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: offender.hovered = true
                onExited: offender.hovered = false
                onClicked: view.unit = offender.modelData.source
              }
            }
          }
        }
      }
    }
  }
}
