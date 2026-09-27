pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Quickshell.Io

// TRENDS tab: 30-day series from `omniscient --trends` (health per audit,
// hourly watch alerts and CPU temperature, shell memory, battery capacity).
// Read when the tab opens and once a minute while it stays open.
Flickable {
  id: view

  required property string binary
  property bool active: false
  property var series: []
  property string error: ""
  property int reads: 0

  function reload() {
    if (!reader.running) {
      view.reads++
      reader.running = true
    }
  }

  function accept(raw) {
    var text = String(raw || "").trim()
    if (!text.length || text.length > 4000000) {
      view.error = "UNEXPECTED TRENDS OUTPUT"
      return
    }
    try {
      var value = JSON.parse(text)
      if (value === null || typeof value !== "object" || value.version !== 1 || !Array.isArray(value.series)) {
        view.error = "UNEXPECTED TRENDS OUTPUT"
        return
      }
      view.series = value.series.slice(0, 32)
      view.error = ""
    } catch (e) {
      view.error = "INVALID TRENDS OUTPUT"
    }
  }

  function stats(points, higherIsBetter) {
    var values = (points || []).map(function(p) { return Number(p[1]) })
    if (!values.length) return null
    var first = values[0]
    var last = values[values.length - 1]
    var change = last - first
    var good = higherIsBetter ? change > 0 : change < 0
    return {
      last: last,
      low: Math.min.apply(null, values),
      high: Math.max.apply(null, values),
      change: change,
      tint: Math.abs(change) < 1e-9 ? "#8290a4" : (good ? "#c8e967" : "#ff8f70"),
      values: values
    }
  }

  function since(points) {
    if (!points || !points.length) return ""
    var d = new Date(points[0][0] * 1000)
    return "since " + (d.getMonth() + 1) + "/" + d.getDate()
  }

  onActiveChanged: if (view.active) view.reload()

  Process {
    id: reader
    command: [view.binary, "--trends"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: view.accept(text)
    }
  }

  Timer {
    interval: 60000
    repeat: true
    running: view.active
    onTriggered: view.reload()
  }

  clip: true
  contentWidth: width
  contentHeight: column.implicitHeight
  boundsBehavior: Flickable.StopAtBounds
  ScrollBar.vertical: HudScrollBar {}

  ColumnLayout {
    id: column
    width: view.width - 14
    spacing: 10

    Text {
      visible: view.error.length > 0
      text: view.error
      color: "#ff667d"
      font.family: "monospace"
      font.pixelSize: 11
    }

    Repeater {
      model: view.series
      delegate: Rectangle {
        id: card
        required property var modelData
        readonly property var summary: view.stats(card.modelData.points, card.modelData.higher_is_better)
        Layout.fillWidth: true
        implicitHeight: cardColumn.implicitHeight + 22
        color: "#111824"
        border.width: 1
        border.color: "#263445"

        ColumnLayout {
          id: cardColumn
          anchors.fill: parent
          anchors.margins: 11
          spacing: 6
          RowLayout {
            Layout.fillWidth: true
            Text {
              Layout.fillWidth: true
              text: card.modelData.label
              color: "#52e8ff"
              font.family: "monospace"
              font.pixelSize: 11
              font.bold: true
            }
            Text {
              visible: card.summary !== null
              text: card.summary ? Number(card.summary.last).toFixed(card.modelData.unit === "°C" ? 1 : 0) + card.modelData.unit : ""
              color: "#f2f5f7"
              font.family: "monospace"
              font.pixelSize: 16
              font.bold: true
            }
          }
          Sparkline {
            visible: card.modelData.points.length >= 2
            Layout.fillWidth: true
            Layout.preferredHeight: 54
            values: card.summary ? card.summary.values : []
            stroke: card.summary ? card.summary.tint : "#52e8ff"
          }
          Text {
            Layout.fillWidth: true
            text: card.modelData.points.length < 2
              ? "collecting… (" + card.modelData.points.length + " sample" + (card.modelData.points.length === 1 ? "" : "s") + " so far)"
              : card.modelData.points.length + " samples " + view.since(card.modelData.points)
                + " · low " + Number(card.summary.low).toFixed(1) + " · high " + Number(card.summary.high).toFixed(1)
                + " · change " + (card.summary.change >= 0 ? "+" : "") + Number(card.summary.change).toFixed(1) + card.modelData.unit
            color: card.summary && card.modelData.points.length >= 2 ? card.summary.tint : "#8290a4"
            font.family: "monospace"
            font.pixelSize: 10
          }
        }
      }
    }
  }
}
