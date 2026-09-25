import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import qs.Commons

Item {
  id: root

  readonly property string selfId: "io.github.cybercore-tech.omniscient"
  property bool opened: false
  property var shell: null
  property string selectedReport: ""
  property string reportText: ""
  property string runnerMessage: ""

  function open(payloadJson) {
    root.opened = true
    SnapshotReader.refresh()
  }

  function close() {
    root.opened = false
  }

  function toggle() {
    root.opened ? root.close() : root.open("{}")
  }

  onShellChanged: {
    if (!root.opened && root.shell && root.shell.openPanelIds
        && root.shell.openPanelIds[root.selfId] === true)
      root.open("{}")
  }

  function stateLabel() {
    if (!SnapshotReader.available) return "WAITING FOR SNAPSHOT"
    return SnapshotReader.state.toUpperCase()
  }

  function moduleStateColor(state) {
    return SnapshotReader.stateColor(state)
  }

  function runAudit() {
    if (auditRunner.running) return
    root.runnerMessage = "AUDIT PROCESS STARTING"
    root.opened = true
    auditRunner.running = true
    SnapshotReader.refresh()
  }

  function openReport(path) {
    root.selectedReport = path
    root.reportText = "LOADING REPORT..."
    reportReader.running = true
  }

  Process {
    id: auditRunner
    command: ["sh", "-lc", "OMNISCIENT_AUTH=pkexec exec omniscient --hud"]
    stderr: StdioCollector { id: auditStderr; waitForEnd: true }
    onExited: function(exitCode) {
      SnapshotReader.refresh()
      if (exitCode !== 0) {
        root.runnerMessage = auditStderr.text.trim().length
          ? auditStderr.text.trim()
          : "AUDIT PROCESS EXITED / CODE " + exitCode
      } else {
        root.runnerMessage = "AUDIT PROCESS COMPLETE"
      }
    }
  }

  Process {
    id: reportReader
    command: root.selectedReport.length ? ["cat", root.selectedReport] : ["true"]
    stdout: StdioCollector {
      id: reportStdout
      waitForEnd: true
      onStreamFinished: root.reportText = text.trim()
    }
    stderr: StdioCollector {
      id: reportStderr
      waitForEnd: true
    }
    onExited: function(exitCode) {
      if (exitCode !== 0) {
        root.reportText = reportStderr.text.trim().length
          ? reportStderr.text.trim()
          : "REPORT COULD NOT BE READ"
      }
    }
  }

  PanelWindow {
    id: panelWindow
    visible: root.opened
    anchors { top: true; bottom: true; left: true; right: true }
    color: "transparent"
    WlrLayershell.namespace: "io-github-cybercore-tech-omniscient"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: root.opened ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    exclusionMode: ExclusionMode.Ignore

    MouseArea {
      anchors.fill: parent
      onClicked: root.close()
    }

    Rectangle {
      id: card
      width: Math.min(1100, parent.width - 36)
      height: Math.min(680, parent.height - 36)
      anchors.centerIn: parent
      radius: 7
      color: "#080b12"
      border.width: 1
      border.color: "#263445"

      MouseArea {
        anchors.fill: parent
        onClicked: mouse.accepted = true
      }

      ColumnLayout {
        anchors.fill: parent
        anchors.margins: 18
        spacing: 14

        RowLayout {
          Layout.fillWidth: true
          spacing: 7
          Rectangle { width: 9; height: 9; radius: 5; color: "#ff4f9a" }
          Rectangle { width: 9; height: 9; radius: 5; color: "#a56bff" }
          Rectangle { width: 9; height: 9; radius: 5; color: "#52e8ff" }
          Text {
            text: "OMNISCIENT / SYSTEM AUDIT"
            color: "#f2f5f7"
            font.family: "monospace"
            font.pixelSize: 12
            font.bold: true
            Layout.leftMargin: 7
          }
          Item { Layout.fillWidth: true }
          Text {
            text: stateLabel()
            color: SnapshotReader.stateColor(SnapshotReader.state)
            font.family: "monospace"
            font.pixelSize: 10
            font.bold: true
          }
          Text {
            text: "×"
            color: "#8290a4"
            font.pixelSize: 18
            Layout.leftMargin: 10
            MouseArea { anchors.fill: parent; onClicked: root.close() }
          }
        }

        Rectangle { Layout.fillWidth: true; height: 1; color: "#263445" }

        RowLayout {
          Layout.fillWidth: true
          spacing: 12

          Rectangle {
            Layout.preferredWidth: 180
            Layout.preferredHeight: 92
            color: "#111824"
            border.width: 1
            border.color: SnapshotReader.healthScore >= 0 ? "#c8e967" : "#263445"
            ColumnLayout {
              anchors.fill: parent
              anchors.margins: 12
              Text { text: "HEALTH SIGNAL"; color: "#8290a4"; font.family: "monospace"; font.pixelSize: 9 }
              Text {
                text: SnapshotReader.healthScore >= 0 ? SnapshotReader.healthScore + "/100" : "—/100"
                color: SnapshotReader.healthScore >= 0 ? "#c8e967" : "#52e8ff"
                font.family: "monospace"; font.pixelSize: 27; font.bold: true
              }
              Text {
                text: SnapshotReader.selectedCount + " SELECTED / " + SnapshotReader.completedCount + " COMPLETE"
                color: "#8290a4"; font.family: "monospace"; font.pixelSize: 8
              }
            }
          }

          ColumnLayout {
            Layout.fillWidth: true
            spacing: 5
            Text { text: "SNAPSHOT CONTRACT / V1"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: 9 }
            Text {
              Layout.fillWidth: true
              text: SnapshotReader.available
                ? (SnapshotReader.message.length ? SnapshotReader.message : "Atomic local state is available to the HUD.")
                : "Run the audit here to publish the first state snapshot."
              color: "#8290a4"; font.family: "monospace"; font.pixelSize: 11; wrapMode: Text.Wrap
            }
            Text {
              Layout.fillWidth: true
              text: SnapshotReader.errorMessage.length ? SnapshotReader.errorMessage : SnapshotReader.snapshotPath
              color: SnapshotReader.errorMessage.length ? "#ff667d" : "#8290a4"
              font.family: "monospace"; font.pixelSize: 8; elide: Text.ElideMiddle
            }
            Rectangle {
              Layout.preferredWidth: 170
              Layout.preferredHeight: 30
              radius: 4
              color: "#121c2b"
              border.width: 1
              border.color: "#ff4f9a"
              Text {
                anchors.centerIn: parent
                text: auditRunner.running ? "AUDIT RUNNING..." : "RUN AUDIT HERE"
                color: "#ff4f9a"
                font.family: "monospace"
                font.pixelSize: 9
                font.bold: true
              }
              MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.runAudit()
              }
            }
          }
        }

        Text { text: "MODULE REGISTRY"; color: "#8290a4"; font.family: "monospace"; font.pixelSize: 9 }

        GridLayout {
          Layout.fillWidth: true
          Layout.preferredHeight: 190
          columns: 3
          rowSpacing: 7
          columnSpacing: 7

          Repeater {
            model: SnapshotReader.modules
            delegate: Rectangle {
              Layout.fillWidth: true
              Layout.preferredHeight: 58
              color: "#111824"
              border.width: 1
              border.color: "#263445"
              ColumnLayout {
                anchors.fill: parent
                anchors.margins: 9
                spacing: 2
                Text { text: String(modelData.name || "").toUpperCase(); color: "#f2f5f7"; font.family: "monospace"; font.pixelSize: 9; elide: Text.ElideRight; Layout.fillWidth: true }
                Text { text: String(modelData.state || "unknown").toUpperCase(); color: root.moduleStateColor(String(modelData.state || "unknown")); font.family: "monospace"; font.pixelSize: 8 }
                Text { text: modelData.requires_sudo ? "ELEVATED" : "USER MODE"; color: "#8290a4"; font.family: "monospace"; font.pixelSize: 8 }
              }
            }
          }
        }

        RowLayout {
          Layout.fillWidth: true
          Layout.fillHeight: true
          spacing: 10

          Rectangle {
            Layout.preferredWidth: 250
            Layout.fillHeight: true
            color: "#0d1320"
            border.width: 1
            border.color: "#263445"
            ColumnLayout {
              anchors.fill: parent
              anchors.margins: 10
              spacing: 6
              Text { text: "REPORT INDEX"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: 9 }
              ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: SnapshotReader.reports
                delegate: Rectangle {
                  width: ListView.view.width
                  height: 28
                  color: root.selectedReport === String(modelData) ? "#1a2940" : "transparent"
                  Text {
                    anchors.fill: parent
                    anchors.margins: 6
                    text: String(modelData).split("/").pop()
                    color: root.selectedReport === String(modelData) ? "#c8e967" : "#8290a4"
                    font.family: "monospace"
                    font.pixelSize: 8
                    elide: Text.ElideMiddle
                  }
                  MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.openReport(String(modelData))
                  }
                }
                Text {
                  anchors.centerIn: parent
                  visible: SnapshotReader.reports.length === 0
                  text: "NO REPORTS YET"
                  color: "#8290a4"
                  font.family: "monospace"
                  font.pixelSize: 9
                }
              }
            }
          }

          Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: "#0d1320"
            border.width: 1
            border.color: "#263445"
            ColumnLayout {
              anchors.fill: parent
              anchors.margins: 10
              spacing: 6
              Text {
                text: root.selectedReport.length ? "REPORT VIEW / " + root.selectedReport.split("/").pop() : "REPORT VIEW"
                color: "#ff4f9a"
                font.family: "monospace"
                font.pixelSize: 9
                elide: Text.ElideMiddle
                Layout.fillWidth: true
              }
              Flickable {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentWidth: width
                contentHeight: reportBody.paintedHeight
                Text {
                  id: reportBody
                  width: parent.width
                  text: root.reportText.length ? root.reportText : "SELECT A REPORT TO VIEW IT HERE"
                  color: "#c8d2e8"
                  font.family: "monospace"
                  font.pixelSize: 9
                  wrapMode: Text.Wrap
                }
              }
            }
          }
        }

        RowLayout {
          Layout.fillWidth: true
          Text {
            Layout.fillWidth: true
            text: SnapshotReader.summaryPath.length ? "REPORT / " + SnapshotReader.summaryPath : "REPORT / awaiting completed audit"
            color: "#8290a4"
            font.family: "monospace"
            font.pixelSize: 8
            elide: Text.ElideMiddle
          }
          Text {
            text: SnapshotReader.updatedAt.length ? SnapshotReader.updatedAt : "NO UPDATE YET"
            color: "#52e8ff"
            font.family: "monospace"
            font.pixelSize: 8
          }
        }
      }
    }
  }
}
