pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// PLATFORM tab: machine identity, power profile and CPU energy preference,
// vendor features (ASUS, Lenovo ideapad), batteries, and the control tools
// present. Omniscient only reads; control stays with those tools.
Flickable {
  id: view

  required property var reading

  readonly property var platform: view.reading ? view.reading.platform : null
  readonly property var asus: view.platform ? view.platform.asus : null
  readonly property var ideapad: view.platform ? view.platform.ideapad : null
  readonly property var batteries: view.platform ? (view.platform.batteries || []) : []
  readonly property var notes: view.reading ? (view.reading.notes || []) : []

  function onOff(value, on, off) {
    return value === null || value === undefined ? "—" : (value ? on : off)
  }

  clip: true
  contentWidth: width
  contentHeight: column.implicitHeight
  boundsBehavior: Flickable.StopAtBounds
  ScrollBar.vertical: HudScrollBar {}

  component ChoiceRow: Flow {
    id: choiceRow
    property var choices: []
    property string current: ""
    Layout.fillWidth: true
    spacing: 6
    Repeater {
      model: choiceRow.choices
      delegate: Rectangle {
        id: chip
        required property var modelData
        readonly property bool selected: choiceRow.current === String(chip.modelData)
        width: chipLabel.implicitWidth + 16
        height: 24
        radius: 3
        color: chip.selected ? "#1a2940" : SensorStyle.panel
        border.width: 1
        border.color: chip.selected ? SensorStyle.accent : SensorStyle.line
        Text {
          id: chipLabel
          anchors.centerIn: parent
          text: String(chip.modelData).toUpperCase().replace(/_/g, " ")
          color: chip.selected ? SensorStyle.accent : SensorStyle.muted
          font.family: "monospace"; font.pixelSize: 10; font.bold: chip.selected
        }
      }
    }
  }

  component Section: Rectangle {
    id: section
    default property alias content: sectionColumn.data
    property color edge: SensorStyle.line
    Layout.fillWidth: true
    implicitHeight: sectionColumn.implicitHeight + 24
    color: SensorStyle.card
    border.width: 1
    border.color: section.edge
    ColumnLayout {
      id: sectionColumn
      anchors.fill: parent
      anchors.margins: 12
      spacing: 6
    }
  }

  component Heading: Text {
    color: SensorStyle.accent
    font.family: "monospace"
    font.pixelSize: 11
    font.bold: true
  }

  component Line: Text {
    Layout.fillWidth: true
    color: SensorStyle.text
    font.family: "monospace"
    font.pixelSize: 11
    wrapMode: Text.Wrap
  }

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

    Section {
      visible: view.platform !== null
      Text {
        text: view.platform ? (view.platform.vendor + " " + view.platform.product).toUpperCase() : ""
        color: SensorStyle.bright; font.family: "monospace"; font.pixelSize: 13; font.bold: true
      }
      Heading {
        text: "POWER PROFILE" + (view.platform && view.platform.profile_source === "power-profiles-daemon" ? " / POWER-PROFILES-DAEMON" : "")
      }
      Line {
        visible: view.platform !== null && view.platform.profile === null
        text: "No platform profile is exposed (no ACPI platform_profile and no power-profiles-daemon)."
        color: SensorStyle.muted
      }
      ChoiceRow {
        choices: view.platform ? view.platform.profile_choices : []
        current: view.platform && view.platform.profile ? view.platform.profile : ""
      }
      Heading {
        visible: view.platform !== null && view.platform.energy_preference !== null
        text: "CPU ENERGY PREFERENCE"
      }
      ChoiceRow {
        visible: view.platform !== null && view.platform.energy_preference !== null
        choices: view.platform ? view.platform.energy_preference_choices : []
        current: view.platform && view.platform.energy_preference ? view.platform.energy_preference : ""
      }
    }

    Section {
      visible: view.ideapad !== null
      edge: "#e2231a"
      Heading { text: "LENOVO IDEAPAD"; color: "#ff667d" }
      Line { text: "FAN MODE / " + (view.ideapad && view.ideapad.fan_mode ? view.ideapad.fan_mode.toUpperCase() : "—") }
      Line {
        text: "BATTERY CONSERVATION / " + view.onOff(view.ideapad ? view.ideapad.conservation_mode : null, "ON (charges to ~60%)", "OFF (charges to 100%)")
      }
      Line { text: "FN LOCK / " + view.onOff(view.ideapad ? view.ideapad.fn_lock : null, "ON", "OFF") }
      Line {
        text: "CAMERA POWER / " + view.onOff(view.ideapad ? view.ideapad.camera_power : null, "ON", "OFF (the webcam is switched off)")
        color: view.ideapad && view.ideapad.camera_power === false ? SensorStyle.warm : SensorStyle.text
      }
      Line {
        visible: view.ideapad !== null && view.ideapad.usb_charging !== null
        text: "ALWAYS-ON USB CHARGING / " + view.onOff(view.ideapad ? view.ideapad.usb_charging : null, "ON", "OFF")
      }
    }

    Section {
      visible: view.asus !== null
      edge: "#a56bff"
      Heading { text: "ASUS / ROG"; color: "#a56bff" }
      Line {
        text: "WMI " + (view.asus && view.asus.wmi ? "PRESENT" : "ABSENT")
              + (view.asus && view.asus.throttle_policy ? " · THERMAL POLICY " + view.asus.throttle_policy.toUpperCase() : "")
      }
      HudMeter {
        visible: view.asus !== null && view.asus.keyboard_max_brightness !== null
        Layout.fillWidth: true
        label: "KEYBOARD LIGHT"
        value: view.asus ? (view.asus.keyboard_brightness || 0) + " / " + (view.asus.keyboard_max_brightness || 0) : "—"
        fraction: view.asus && view.asus.keyboard_max_brightness ? view.asus.keyboard_brightness / view.asus.keyboard_max_brightness : 0
        tint: "#a56bff"
      }
      Line { text: "Fan curves appear on the SENSORS tab under asus_custom_fan_curve."; color: SensorStyle.muted; font.pixelSize: 10 }
    }

    Section {
      visible: view.batteries.length > 0
      Heading { text: "BATTERY" }
      Repeater {
        model: view.batteries
        delegate: HudMeter {
          required property var modelData
          Layout.fillWidth: true
          label: modelData.name + " / " + String(modelData.status).toUpperCase()
          labelWidth: 220
          value: (modelData.capacity_percent === null ? "—" : modelData.capacity_percent + "%")
                 + (modelData.power_watts !== null ? " · " + Number(modelData.power_watts).toFixed(1) + " W" : "")
          fraction: (modelData.capacity_percent || 0) / 100
          tint: modelData.capacity_percent !== null && modelData.capacity_percent < 20 ? SensorStyle.critical : SensorStyle.cool
        }
      }
    }

    Section {
      visible: view.platform !== null
      Heading { text: "CONTROL TOOLS" }
      Line {
        text: view.platform && view.platform.tools.length
          ? view.platform.tools.join("\n")
          : "none found (asusctl, rog-control-center, supergfxctl, openrgb, coolercontrol, fancontrol, nvidia-smi, powerprofilesctl)"
      }
      Line {
        text: "Omniscient is read-only: fan curves, profiles and RGB are changed with these tools, never by the HUD."
        color: SensorStyle.muted; font.pixelSize: 10
      }
    }

    Section {
      visible: view.notes.length > 0
      Heading { text: "HINTS" }
      Repeater {
        model: view.notes
        delegate: Line {
          required property var modelData
          text: "ℹ " + modelData
          color: SensorStyle.muted
          font.pixelSize: 10
        }
      }
    }
  }
}
