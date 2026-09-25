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
    fixedWidth: root.bar && root.bar.vertical ? -1 : Style.space(38)
    fixedHeight: root.bar && root.bar.vertical ? Style.space(26) : -1
    onPressed: function(b) {
      if (!root.bar) return
      root.bar.run("omarchy-shell shell toggle io.github.cybercore-tech.omniscient")
    }

    Item {
      id: cybercoreMark
      z: 1
      width: Style.space(25)
      height: Style.space(25)
      anchors.centerIn: parent

      Rectangle {
        id: outerDiamond
        width: Style.space(17)
        height: Style.space(17)
        anchors.centerIn: parent
        rotation: 45
        radius: 4
        color: "transparent"
        border.width: 2
        border.color: SnapshotReader.state === "error"
          ? Color.urgent
          : SnapshotReader.state === "running" ? "#ff4f9a" : "#c8e967"
      }

      Rectangle {
        width: Style.space(7)
        height: Style.space(7)
        anchors.centerIn: parent
        rotation: 45
        color: "#111824"
        border.width: 1
        border.color: "#52e8ff"
      }

      Rectangle {
        width: Style.space(9)
        height: 2
        anchors.centerIn: parent
        rotation: -45
        radius: 1
        color: "#ff4f9a"
      }

      Rectangle {
        width: Style.space(4)
        height: Style.space(4)
        anchors.top: outerDiamond.top
        anchors.right: outerDiamond.right
        radius: 2
        color: "#52e8ff"
      }
    }
  }
}
