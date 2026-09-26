import QtQuick
import QtQuick.Controls.Basic

// Cybercore scrollbar for the HUD's scrollable views. Shown only when the
// content overflows; the thumb brightens on hover and takes the accent
// colour while dragged. Built on the Basic style so the custom look does not
// fight a platform style.
ScrollBar {
  id: bar

  property color accent: "#52e8ff"

  policy: bar.size < 1.0 ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
  minimumSize: 0.08
  padding: 2
  implicitWidth: 10

  background: Rectangle {
    color: "#0b111b"
    radius: 3
  }

  contentItem: Rectangle {
    implicitWidth: 6
    radius: 3
    color: bar.pressed ? bar.accent : (bar.hovered ? "#5a7090" : "#34445c")
  }
}
