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
    text: SnapshotReader.available && SnapshotReader.healthScore >= 0
      ? "OMNI " + SnapshotReader.healthScore
      : "OMNI"
    tooltipText: SnapshotReader.available
      ? "Omniscient / health " + SnapshotReader.healthScore + "/100"
      : "Omniscient / waiting for snapshot"
    foreground: SnapshotReader.state === "error"
      ? Color.urgent
      : SnapshotReader.state === "running"
        ? Color.accent
        : Color.foreground
    fixedWidth: root.bar && root.bar.vertical ? -1 : Style.space(64)
    fixedHeight: root.bar && root.bar.vertical ? Style.space(26) : -1
    onPressed: function(b) {
      if (!root.bar) return
      root.bar.run("omarchy-shell shell toggle io.github.cybercore-tech.omniscient")
    }
  }
}
