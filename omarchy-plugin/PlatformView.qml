pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// PLATFORM tab: machine identity, power profile, ASUS state and the control
// tools present. Omniscient only reads; control stays with those tools.
Flickable {
  id: view

  required property var reading

  readonly property var platform: view.reading ? view.reading.platform : null
  readonly property var asus: view.platform ? view.platform.asus : null
  readonly property var notes: view.reading ? (view.reading.notes || []) : []

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
      visible: !view.platform
      text: "READING SENSORS…"
      color: SensorStyle.muted
      font.family: "monospace"
      font.pixelSize: 12
    }

    Rectangle {
      visible: view.platform !== null
      Layout.fillWidth: true
      implicitHeight: identity.implicitHeight + 24
      color: SensorStyle.card
      border.width: 1
      border.color: SensorStyle.line
      ColumnLayout {
        id: identity
        anchors.fill: parent
        anchors.margins: 12
        spacing: 6
        Text {
          text: view.platform ? (view.platform.vendor + " " + view.platform.product).toUpperCase() : ""
          color: SensorStyle.bright; font.family: "monospace"; font.pixelSize: 13; font.bold: true
        }
        Text {
          text: "POWER PROFILE"
          color: SensorStyle.accent; font.family: "monospace"; font.pixelSize: 11; font.bold: true
        }
        Text {
          visible: view.platform !== null && view.platform.profile === null
          text: "This firmware does not expose ACPI platform profiles."
          color: SensorStyle.muted; font.family: "monospace"; font.pixelSize: 10
        }
        Flow {
          Layout.fillWidth: true
          spacing: 6
          Repeater {
            model: view.platform ? view.platform.profile_choices : []
            delegate: Rectangle {
              id: chip
              required property var modelData
              readonly property bool current: view.platform !== null && view.platform.profile === chip.modelData
              width: chipLabel.implicitWidth + 16
              height: 24
              radius: 3
              color: chip.current ? "#1a2940" : SensorStyle.panel
              border.width: 1
              border.color: chip.current ? SensorStyle.accent : SensorStyle.line
              Text {
                id: chipLabel
                anchors.centerIn: parent
                text: String(chip.modelData).toUpperCase()
                color: chip.current ? SensorStyle.accent : SensorStyle.muted
                font.family: "monospace"; font.pixelSize: 10; font.bold: chip.current
              }
            }
          }
        }
      }
    }

    Rectangle {
      visible: view.asus !== null
      Layout.fillWidth: true
      implicitHeight: asusColumn.implicitHeight + 24
      color: SensorStyle.card
      border.width: 1
      border.color: "#a56bff"
      ColumnLayout {
        id: asusColumn
        anchors.fill: parent
        anchors.margins: 12
        spacing: 6
        Text { text: "ASUS / ROG"; color: "#a56bff"; font.family: "monospace"; font.pixelSize: 11; font.bold: true }
        Text {
          text: "WMI " + (view.asus && view.asus.wmi ? "PRESENT" : "ABSENT")
                + (view.asus && view.asus.throttle_policy ? " · THERMAL POLICY " + view.asus.throttle_policy.toUpperCase() : "")
          color: SensorStyle.text; font.family: "monospace"; font.pixelSize: 11
        }
        HudMeter {
          visible: view.asus !== null && view.asus.keyboard_max_brightness !== null
          Layout.fillWidth: true
          label: "KEYBOARD LIGHT"
          value: view.asus ? (view.asus.keyboard_brightness || 0) + " / " + (view.asus.keyboard_max_brightness || 0) : "—"
          fraction: view.asus && view.asus.keyboard_max_brightness ? view.asus.keyboard_brightness / view.asus.keyboard_max_brightness : 0
          tint: "#a56bff"
        }
        Text {
          Layout.fillWidth: true
          text: "Fan curves appear on the SENSORS tab under asus_custom_fan_curve."
          color: SensorStyle.muted; font.family: "monospace"; font.pixelSize: 10; wrapMode: Text.Wrap
        }
      }
    }

    Rectangle {
      visible: view.platform !== null
      Layout.fillWidth: true
      implicitHeight: toolColumn.implicitHeight + 24
      color: SensorStyle.card
      border.width: 1
      border.color: SensorStyle.line
      ColumnLayout {
        id: toolColumn
        anchors.fill: parent
        anchors.margins: 12
        spacing: 4
        Text { text: "CONTROL TOOLS"; color: SensorStyle.accent; font.family: "monospace"; font.pixelSize: 11; font.bold: true }
        Text {
          Layout.fillWidth: true
          text: view.platform && view.platform.tools.length
            ? view.platform.tools.join(" · ")
            : "none found (asusctl, rog-control-center, supergfxctl, openrgb, coolercontrol, fancontrol, nvidia-smi)"
          color: SensorStyle.text; font.family: "monospace"; font.pixelSize: 11; wrapMode: Text.Wrap
        }
        Text {
          Layout.fillWidth: true
          text: "Omniscient is read-only: fan curves, profiles and RGB are changed with these tools, never by the HUD."
          color: SensorStyle.muted; font.family: "monospace"; font.pixelSize: 10; wrapMode: Text.Wrap
        }
      }
    }

    Repeater {
      model: view.notes
      delegate: Text {
        required property var modelData
        Layout.fillWidth: true
        text: "▸ " + modelData
        color: SensorStyle.warm; font.family: "monospace"; font.pixelSize: 10; wrapMode: Text.Wrap
      }
    }
  }
}
