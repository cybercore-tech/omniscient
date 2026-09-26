pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// DRIVES tab: live temperature of every drive. Read-only.
Flickable {
  id: view

  required property var reading
  property string fixMessage: ""
  property bool fixRunning: false

  // Asks the panel to confirm and run an allowlisted fix.
  signal fixRequested(string id, string title, string command)

  readonly property var drives: view.reading ? (view.reading.drives || []) : []

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
      visible: view.drives.length === 0
      text: view.reading ? "NO DRIVES REPORTED" : "READING SENSORS…"
      color: SensorStyle.muted
      font.family: "monospace"
      font.pixelSize: 12
    }

    Repeater {
      model: view.drives
      delegate: Rectangle {
        id: driveCard
        required property var modelData
        Layout.fillWidth: true
        implicitHeight: driveColumn.implicitHeight + 24
        color: SensorStyle.card
        border.width: 1
        border.color: SensorStyle.driveTemperature(driveCard.modelData.celsius, driveCard.modelData.kind)

        ColumnLayout {
          id: driveColumn
          anchors.fill: parent
          anchors.margins: 12
          spacing: 4
          RowLayout {
            Layout.fillWidth: true
            ColumnLayout {
              Layout.fillWidth: true
              spacing: 2
              Text {
                text: "/dev/" + driveCard.modelData.name + " · " + driveCard.modelData.kind.toUpperCase()
                color: SensorStyle.bright; font.family: "monospace"; font.pixelSize: 12; font.bold: true
              }
              Text {
                Layout.fillWidth: true
                text: driveCard.modelData.model
                color: SensorStyle.muted; font.family: "monospace"; font.pixelSize: 10; elide: Text.ElideRight
              }
            }
            Text {
              text: SensorStyle.celsius(driveCard.modelData.celsius)
              color: SensorStyle.driveTemperature(driveCard.modelData.celsius, driveCard.modelData.kind)
              font.family: "monospace"; font.pixelSize: 26; font.bold: true
            }
          }
          Repeater {
            model: driveCard.modelData.temps
            delegate: HudMeter {
              required property var modelData
              Layout.fillWidth: true
              label: modelData.label
              value: SensorStyle.celsius(modelData.celsius)
              fraction: modelData.celsius / (modelData.crit_celsius || 85)
              tint: SensorStyle.driveTemperature(modelData.celsius, driveCard.modelData.kind)
            }
          }
          Text {
            visible: driveCard.modelData.hint.length > 0
            Layout.fillWidth: true
            text: driveCard.modelData.hint
            color: SensorStyle.muted; font.family: "monospace"; font.pixelSize: 10; wrapMode: Text.Wrap
          }
          Rectangle {
            id: fixButton
            visible: driveCard.modelData.fix === "enable-sensor:drivetemp"
            property bool hovered: false
            Layout.preferredWidth: fixLabel.implicitWidth + 28
            Layout.preferredHeight: 30
            radius: 3
            color: fixButton.hovered ? "#2a2417" : "#1d1a14"
            border.width: 1
            border.color: SensorStyle.warm
            Text {
              id: fixLabel
              anchors.centerIn: parent
              text: view.fixRunning ? "ENABLING…" : "ENABLE LIVE SATA TEMPERATURES"
              color: SensorStyle.warm; font.family: "monospace"; font.pixelSize: 11; font.bold: true
            }
            MouseArea {
              anchors.fill: parent
              enabled: !view.fixRunning
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onEntered: fixButton.hovered = true
              onExited: fixButton.hovered = false
              onClicked: view.fixRequested(
                "enable-sensor:drivetemp",
                "Enable the read-only drivetemp sensor driver (now and at every boot)",
                "modprobe drivetemp; echo drivetemp > /etc/modules-load.d/omniscient-drivetemp.conf")
            }
          }
        }
      }
    }

    Text {
      visible: view.fixMessage.length > 0
      Layout.fillWidth: true
      text: view.fixMessage
      color: view.fixMessage.indexOf("FAILED") >= 0 ? SensorStyle.critical : SensorStyle.cool
      font.family: "monospace"; font.pixelSize: 10; wrapMode: Text.Wrap
    }

    Text {
      Layout.fillWidth: true
      text: "Wear, spare and error counters come from SMART in the Deep Signals report (elevated audit)."
      color: SensorStyle.muted; font.family: "monospace"; font.pixelSize: 10; wrapMode: Text.Wrap
    }
  }
}
