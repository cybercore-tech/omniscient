pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// SENSORS tab: CPU, memory, GPUs and every hwmon chip. Read-only.
Flickable {
  id: view

  required property var reading

  readonly property var cpu: view.reading ? view.reading.cpu : null
  readonly property var memory: view.reading ? view.reading.memory : null
  readonly property var gpus: view.reading ? (view.reading.gpus || []) : []
  // Chips that report something worth showing.
  readonly property var chips: view.reading
    ? (view.reading.chips || []).filter(function(chip) {
        return chip.temps.length || chip.fans.length || chip.pwms.length || chip.curves.length
      })
    : []
  readonly property real maxMhz: {
    var top = 1
    var cores = view.cpu ? view.cpu.cores : []
    for (var i = 0; i < cores.length; i++) top = Math.max(top, cores[i].mhz)
    return top
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
      visible: !view.reading
      text: "READING SENSORS…"
      color: SensorStyle.muted
      font.family: "monospace"
      font.pixelSize: 12
    }

    // ---- CPU ------------------------------------------------------------
    Rectangle {
      visible: view.cpu !== null
      Layout.fillWidth: true
      implicitHeight: cpuColumn.implicitHeight + 24
      color: SensorStyle.card
      border.width: 1
      border.color: SensorStyle.temperature(view.cpu ? view.cpu.package_celsius : null, 0)

      ColumnLayout {
        id: cpuColumn
        anchors.fill: parent
        anchors.margins: 12
        spacing: 6

        RowLayout {
          Layout.fillWidth: true
          ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            Text { text: "CPU / " + (view.cpu ? view.cpu.model : ""); color: SensorStyle.bright; font.family: "monospace"; font.pixelSize: 12; font.bold: true; elide: Text.ElideRight; Layout.fillWidth: true }
            Text {
              text: view.cpu
                ? [view.cpu.driver, view.cpu.governor,
                   view.cpu.boost === null ? "" : (view.cpu.boost ? "BOOST ON" : "BOOST OFF"),
                   view.cpu.amd_pstate ? "AMD-PSTATE " + view.cpu.amd_pstate.toUpperCase() : ""]
                    .filter(function(part) { return part && part.length }).join(" · ").toUpperCase()
                : ""
              color: SensorStyle.muted; font.family: "monospace"; font.pixelSize: 10
            }
          }
          ColumnLayout {
            spacing: 0
            Text {
              Layout.alignment: Qt.AlignRight
              text: SensorStyle.celsius(view.cpu ? view.cpu.package_celsius : null)
              color: SensorStyle.temperature(view.cpu ? view.cpu.package_celsius : null, 0)
              font.family: "monospace"; font.pixelSize: 28; font.bold: true
            }
            Text { Layout.alignment: Qt.AlignRight; text: view.cpu ? view.cpu.package_source : ""; color: SensorStyle.muted; font.family: "monospace"; font.pixelSize: 9 }
          }
        }

        HudMeter {
          Layout.fillWidth: true
          label: "UTILIZATION"
          value: SensorStyle.number(view.cpu ? view.cpu.utilization_percent : null, "%", 1)
          fraction: view.cpu && view.cpu.utilization_percent !== null ? view.cpu.utilization_percent / 100 : 0
          tint: SensorStyle.load(view.cpu ? view.cpu.utilization_percent : null)
        }
        Text {
          text: "PACKAGE POWER / " + (view.cpu && view.cpu.package_watts !== null ? Number(view.cpu.package_watts).toFixed(1) + " W" : "needs the elevated audit")
          color: SensorStyle.text; font.family: "monospace"; font.pixelSize: 11
        }

        Text { text: "CORE CLOCKS"; color: SensorStyle.accent; font.family: "monospace"; font.pixelSize: 11; font.bold: true }
        GridLayout {
          Layout.fillWidth: true
          columns: 2
          columnSpacing: 14
          rowSpacing: 3
          Repeater {
            model: view.cpu ? view.cpu.cores : []
            delegate: HudMeter {
              required property var modelData
              Layout.fillWidth: true
              labelWidth: 60
              label: "CPU" + modelData.cpu
              value: Math.round(modelData.mhz) + " MHz"
              fraction: modelData.mhz / view.maxMhz
              tint: SensorStyle.accent
            }
          }
        }
      }
    }

    // ---- Memory ---------------------------------------------------------
    Rectangle {
      visible: view.memory !== null && view.memory.total_mib > 0
      Layout.fillWidth: true
      implicitHeight: memoryColumn.implicitHeight + 24
      color: SensorStyle.card
      border.width: 1
      border.color: SensorStyle.line
      ColumnLayout {
        id: memoryColumn
        anchors.fill: parent
        anchors.margins: 12
        spacing: 4
        Text { text: "MEMORY"; color: SensorStyle.accent; font.family: "monospace"; font.pixelSize: 11; font.bold: true }
        HudMeter {
          Layout.fillWidth: true
          readonly property real used: view.memory ? view.memory.total_mib - view.memory.available_mib : 0
          label: "RAM"
          value: view.memory ? (used / 1024).toFixed(1) + " / " + (view.memory.total_mib / 1024).toFixed(1) + " GiB" : "—"
          fraction: view.memory && view.memory.total_mib ? used / view.memory.total_mib : 0
          tint: SensorStyle.load(view.memory && view.memory.total_mib ? used * 100 / view.memory.total_mib : null)
        }
        HudMeter {
          visible: view.memory !== null && view.memory.swap_total_mib > 0
          Layout.fillWidth: true
          readonly property real used: view.memory ? view.memory.swap_total_mib - view.memory.swap_free_mib : 0
          label: "SWAP"
          value: view.memory ? (used / 1024).toFixed(1) + " / " + (view.memory.swap_total_mib / 1024).toFixed(1) + " GiB" : "—"
          fraction: view.memory && view.memory.swap_total_mib ? used / view.memory.swap_total_mib : 0
          tint: SensorStyle.load(view.memory && view.memory.swap_total_mib ? used * 100 / view.memory.swap_total_mib : null)
        }
      }
    }

    // ---- GPUs -----------------------------------------------------------
    Repeater {
      model: view.gpus
      delegate: Rectangle {
        id: gpuCard
        required property var modelData
        Layout.fillWidth: true
        implicitHeight: gpuColumn.implicitHeight + 24
        color: SensorStyle.card
        border.width: 1
        border.color: SensorStyle.line
        ColumnLayout {
          id: gpuColumn
          anchors.fill: parent
          anchors.margins: 12
          spacing: 4
          Text {
            text: "GPU / " + [gpuCard.modelData.vendor, gpuCard.modelData.driver].filter(function(part) { return part.length }).join(" ")
                  + " (" + gpuCard.modelData.card + ")"
                  + (gpuCard.modelData.clock_mhz !== null ? " · " + Math.round(gpuCard.modelData.clock_mhz) + " MHz" : "")
                  + (gpuCard.modelData.power_watts !== null ? " · " + Number(gpuCard.modelData.power_watts).toFixed(1) + " W" : "")
                  + (gpuCard.modelData.fan_rpm !== null ? " · FAN " + gpuCard.modelData.fan_rpm + " RPM" : "")
            color: SensorStyle.bright; font.family: "monospace"; font.pixelSize: 12; font.bold: true
            elide: Text.ElideRight; Layout.fillWidth: true
          }
          HudMeter {
            visible: gpuCard.modelData.busy_percent !== null
            Layout.fillWidth: true
            label: "BUSY"
            value: SensorStyle.number(gpuCard.modelData.busy_percent, "%", 0)
            fraction: (gpuCard.modelData.busy_percent || 0) / 100
            tint: SensorStyle.load(gpuCard.modelData.busy_percent)
          }
          HudMeter {
            visible: gpuCard.modelData.vram_total_mib !== null
            Layout.fillWidth: true
            label: "VRAM"
            value: Math.round(gpuCard.modelData.vram_used_mib || 0) + " / " + Math.round(gpuCard.modelData.vram_total_mib || 0) + " MiB"
            fraction: gpuCard.modelData.vram_total_mib ? gpuCard.modelData.vram_used_mib / gpuCard.modelData.vram_total_mib : 0
            tint: SensorStyle.accent
          }
          Repeater {
            model: gpuCard.modelData.temps
            delegate: HudMeter {
              required property var modelData
              Layout.fillWidth: true
              label: modelData.label.toUpperCase()
              value: SensorStyle.celsius(modelData.celsius)
              fraction: modelData.celsius / (modelData.crit_celsius || 110)
              tint: SensorStyle.temperature(modelData.celsius, modelData.crit_celsius)
            }
          }
        }
      }
    }

    // ---- hwmon chips ----------------------------------------------------
    Repeater {
      model: view.chips
      delegate: Rectangle {
        id: chipCard
        required property var modelData
        Layout.fillWidth: true
        implicitHeight: chipColumn.implicitHeight + 24
        color: SensorStyle.card
        border.width: 1
        border.color: SensorStyle.line
        ColumnLayout {
          id: chipColumn
          anchors.fill: parent
          anchors.margins: 12
          spacing: 4
          Text {
            text: chipCard.modelData.name.toUpperCase() + (chipCard.modelData.power_watts !== null ? " · " + Number(chipCard.modelData.power_watts).toFixed(1) + " W" : "")
            color: SensorStyle.accent; font.family: "monospace"; font.pixelSize: 11; font.bold: true
          }
          Repeater {
            model: chipCard.modelData.temps
            delegate: HudMeter {
              required property var modelData
              Layout.fillWidth: true
              label: modelData.label
              value: SensorStyle.celsius(modelData.celsius)
              fraction: modelData.celsius / (modelData.crit_celsius || modelData.max_celsius || 110)
              tint: SensorStyle.temperature(modelData.celsius, modelData.crit_celsius)
            }
          }
          Repeater {
            model: chipCard.modelData.fans
            delegate: HudMeter {
              required property var modelData
              Layout.fillWidth: true
              label: "FAN " + modelData.label
              value: modelData.rpm + " RPM"
              fraction: modelData.rpm / 6000
              tint: modelData.rpm === 0 ? SensorStyle.muted : SensorStyle.accent
            }
          }
          Repeater {
            model: chipCard.modelData.pwms
            delegate: HudMeter {
              required property var modelData
              Layout.fillWidth: true
              label: "DUTY " + modelData.label
              value: Number(modelData.percent).toFixed(0) + "%"
              fraction: modelData.percent / 100
              tint: SensorStyle.load(modelData.percent)
            }
          }
          Repeater {
            model: chipCard.modelData.curves
            delegate: Text {
              required property var modelData
              Layout.fillWidth: true
              text: "CURVE " + modelData.fan + " / " + modelData.points.map(function(point) {
                return Math.round(point[0]) + "°C→" + Math.round(point[1]) + "%"
              }).join("  ")
              color: SensorStyle.text; font.family: "monospace"; font.pixelSize: 10
              wrapMode: Text.Wrap
            }
          }
        }
      }
    }
  }
}
