import QtQuick
import qs.Commons
import qs.Ui

BarWidget {
  id: root
  moduleName: "io.github.cybercore-tech.omniscient"

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "OMNI"
    labelVisible: false
    tooltipText: SnapshotReader.available
      ? "Omniscient / health " + SnapshotReader.healthScore + "/100"
      : "Open Omniscient HUD / waiting for snapshot"
    foreground: SnapshotReader.state === "error"
      ? Color.urgent
      : SnapshotReader.state === "running"
        ? Color.accent
        : Color.foreground
    fixedWidth: root.bar && root.bar.vertical ? -1 : Style.space(26)
    fixedHeight: root.bar && root.bar.vertical ? Style.space(26) : -1
    onPressed: function(b) {
      if (!root.bar) return
      root.bar.run("omarchy-shell shell toggle io.github.cybercore-tech.omniscient")
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
        color: SnapshotReader.state === "error"
          ? Color.urgent
          : SnapshotReader.state === "running" ? "#ff4f9a" : "#c8e967"
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
    }
  }
}
