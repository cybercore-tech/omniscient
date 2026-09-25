import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons

Item {
  id: root

  readonly property string selfId: "io.github.cybercore-tech.omniscient"
  readonly property int fontMicro: 10
  readonly property int fontSmall: 11
  readonly property int fontBody: 12
  readonly property int fontSection: 12
  readonly property int fontTitle: 15
  readonly property int fontMetric: 34
  property bool opened: false
  property var shell: null
  property string selectedReport: ""
  property string reportText: ""
  property string runnerMessage: ""
  property string pendingFixId: ""
  property string fixMessage: ""
  property bool confirmingFix: false

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

  function healthColor() {
    return SnapshotReader.healthColor(SnapshotReader.healthScore)
  }

  function requestFix(id) {
    root.pendingFixId = id
    root.confirmingFix = true
  }

  function applyFix() {
    if (!root.pendingFixId.length || fixRunner.running) return
    root.confirmingFix = false
    root.fixMessage = "FIX REQUEST STARTING"
    fixRunner.running = true
  }

  function openSuggestionsReport() {
    if (SnapshotReader.suggestionsPath.length)
      root.openReport(SnapshotReader.suggestionsPath)
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
    id: fixRunner
    command: root.pendingFixId.length
      ? ["sh", "-lc", "OMNISCIENT_AUTH=pkexec exec omniscient --fix " + Util.shellQuote(root.pendingFixId)]
      : ["true"]
    stdout: StdioCollector { id: fixStdout; waitForEnd: true }
    stderr: StdioCollector { id: fixStderr; waitForEnd: true }
    onExited: function(exitCode) {
      SnapshotReader.refresh()
      if (exitCode !== 0) {
        root.fixMessage = fixStderr.text.trim().length
          ? fixStderr.text.trim()
          : "FIX FAILED / CODE " + exitCode
      } else {
        root.fixMessage = fixStdout.text.trim().length
          ? fixStdout.text.trim()
          : "FIX COMPLETE / REPORT WRITTEN"
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
      width: Math.min(1240, parent.width - 28)
      height: Math.min(780, parent.height - 28)
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
        anchors.margins: 20
        spacing: 10

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
            font.pixelSize: root.fontTitle
            font.bold: true
            Layout.leftMargin: 7
          }
          Item { Layout.fillWidth: true }
          Text {
            text: stateLabel()
            color: SnapshotReader.stateColor(SnapshotReader.state)
            font.family: "monospace"
            font.pixelSize: root.fontBody
            font.bold: true
          }
          Text {
            text: "×"
            color: "#8290a4"
            font.pixelSize: 22
            Layout.leftMargin: 10
            MouseArea { anchors.fill: parent; onClicked: root.close() }
          }
        }

        Rectangle { Layout.fillWidth: true; height: 1; color: "#263445" }

        RowLayout {
          Layout.fillWidth: true
          spacing: 12

          Rectangle {
            Layout.preferredWidth: 220
            Layout.preferredHeight: 110
            color: "#111824"
            border.width: 1
            border.color: root.healthColor()
            ColumnLayout {
              anchors.fill: parent
              anchors.margins: 12
              Text { text: "HEALTH SIGNAL / " + SnapshotReader.healthLabel(SnapshotReader.healthScore); color: root.healthColor(); font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
              Text {
                text: SnapshotReader.healthScore >= 0 ? SnapshotReader.healthScore + "/100" : "—/100"
                color: root.healthColor()
                font.family: "monospace"; font.pixelSize: root.fontMetric; font.bold: true
              }
              Text {
                text: SnapshotReader.selectedCount + " SELECTED / " + SnapshotReader.completedCount + " COMPLETE"
                color: "#8290a4"; font.family: "monospace"; font.pixelSize: root.fontMicro
              }
            }
          }

          ColumnLayout {
            Layout.fillWidth: true
            spacing: 5
            Text { text: "SNAPSHOT CONTRACT / V1"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }
            Text {
              Layout.fillWidth: true
              text: SnapshotReader.available
                ? (SnapshotReader.message.length ? SnapshotReader.message : "Atomic local state is available to the HUD.")
                : "Run the audit here to publish the first state snapshot."
              color: "#c8d2e8"; font.family: "monospace"; font.pixelSize: root.fontBody; wrapMode: Text.Wrap
            }
            Text {
              Layout.fillWidth: true
              text: SnapshotReader.errorMessage.length ? SnapshotReader.errorMessage : SnapshotReader.snapshotPath
              color: SnapshotReader.errorMessage.length ? "#ff667d" : "#8290a4"
              font.family: "monospace"; font.pixelSize: root.fontMicro; elide: Text.ElideMiddle
            }
            Rectangle {
              Layout.preferredWidth: 200
              Layout.preferredHeight: 36
              radius: 4
              color: "#121c2b"
              border.width: 1
              border.color: "#ff4f9a"
              Text {
                anchors.centerIn: parent
                text: auditRunner.running ? "AUDIT RUNNING..." : "RUN AUDIT HERE"
                color: "#ff4f9a"
                font.family: "monospace"
                font.pixelSize: root.fontSmall
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

        Text { text: "MODULE REGISTRY"; color: "#8290a4"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }

        GridLayout {
          Layout.fillWidth: true
          Layout.preferredHeight: 180
          columns: 3
          rowSpacing: 7
          columnSpacing: 7

          Repeater {
            model: SnapshotReader.modules
            delegate: Rectangle {
              Layout.fillWidth: true
              Layout.preferredHeight: 54
              color: "#111824"
              border.width: 1
              border.color: "#263445"
              ColumnLayout {
                anchors.fill: parent
                anchors.margins: 9
                spacing: 2
                Text { text: String(modelData.name || "").toUpperCase(); color: "#f2f5f7"; font.family: "monospace"; font.pixelSize: root.fontSmall; elide: Text.ElideRight; Layout.fillWidth: true }
                Text { text: String(modelData.state || "unknown").toUpperCase(); color: root.moduleStateColor(String(modelData.state || "unknown")); font.family: "monospace"; font.pixelSize: root.fontMicro }
                Text { text: modelData.requires_sudo ? "ELEVATED" : "USER MODE"; color: "#8290a4"; font.family: "monospace"; font.pixelSize: root.fontMicro }
              }
            }
          }
        }

        Rectangle {
          Layout.fillWidth: true
          Layout.preferredHeight: 132
          color: "#0d1320"
          border.width: 1
          border.color: SnapshotReader.suggestions.length ? "#ffb454" : "#263445"

          ColumnLayout {
            anchors.fill: parent
            anchors.margins: 12
            spacing: 6

            RowLayout {
              Layout.fillWidth: true
              Text {
                text: "FIX SUGGESTIONS / " + SnapshotReader.suggestions.length
                color: SnapshotReader.suggestions.length ? "#ffb454" : "#52e8ff"
                font.family: "monospace"
                font.pixelSize: root.fontSection
                font.bold: true
              }
              Item { Layout.fillWidth: true }
              Text {
                visible: SnapshotReader.suggestionsPath.length > 0
                text: "VIEW SUGGESTIONS REPORT"
                color: "#52e8ff"
                font.family: "monospace"
                font.pixelSize: root.fontMicro
                MouseArea {
                  anchors.fill: parent
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.openSuggestionsReport()
                }
              }
            }

            Text {
              visible: root.fixMessage.length > 0
              text: root.fixMessage
              color: root.fixMessage.indexOf("FAILED") >= 0 ? "#ff667d" : "#c8e967"
              font.family: "monospace"
              font.pixelSize: root.fontMicro
              elide: Text.ElideMiddle
              Layout.fillWidth: true
            }

            ListView {
              Layout.fillWidth: true
              Layout.fillHeight: true
              clip: true
              model: SnapshotReader.suggestions
              spacing: 5

              delegate: Rectangle {
                width: ListView.view.width
                height: 62
                color: "#111824"
                border.width: 1
                border.color: SnapshotReader.severityColor(String(modelData.severity || "attention"))

                RowLayout {
                  anchors.fill: parent
                  anchors.margins: 8
                  spacing: 10

                  ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                      text: String(modelData.severity || "attention").toUpperCase() + " / " + String(modelData.title || "")
                      color: SnapshotReader.severityColor(String(modelData.severity || "attention"))
                      font.family: "monospace"
                      font.pixelSize: root.fontSmall
                      font.bold: true
                      elide: Text.ElideRight
                      Layout.fillWidth: true
                    }
                    Text {
                      text: String(modelData.detail || "")
                      color: "#c8d2e8"
                      font.family: "monospace"
                      font.pixelSize: root.fontMicro
                      elide: Text.ElideRight
                      Layout.fillWidth: true
                    }
                    Text {
                      text: "<a href='" + String(modelData.man_url || "") + "'>MAN</a>  <a href='" + String(modelData.docs_url || "") + "'>DOCS</a>"
                      textFormat: Text.RichText
                      color: "#52e8ff"
                      font.family: "monospace"
                      font.pixelSize: root.fontMicro
                      onLinkActivated: function(link) { Qt.openUrlExternally(link) }
                    }
                  }

                  Rectangle {
                    visible: Boolean(modelData.auto_fix)
                    Layout.preferredWidth: 118
                    Layout.preferredHeight: 32
                    radius: 3
                    color: "#1d2634"
                    border.width: 1
                    border.color: "#ffb454"
                    Text {
                      anchors.centerIn: parent
                      text: "REVIEW / APPLY"
                      color: "#ffb454"
                      font.family: "monospace"
                      font.pixelSize: root.fontMicro
                      font.bold: true
                    }
                    MouseArea {
                      anchors.fill: parent
                      cursorShape: Qt.PointingHandCursor
                      onClicked: root.requestFix(String(modelData.id || ""))
                    }
                  }
                }
              }

              Text {
                anchors.centerIn: parent
                visible: SnapshotReader.suggestions.length === 0
                text: "NO REPAIR ACTIONS SUGGESTED"
                color: "#c8e967"
                font.family: "monospace"
                font.pixelSize: root.fontSmall
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
              Text { text: "REPORT INDEX"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }
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
                    font.pixelSize: root.fontMicro
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
                  font.pixelSize: root.fontSmall
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
                font.pixelSize: root.fontSection
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
                  font.pixelSize: root.fontBody
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
            font.pixelSize: root.fontMicro
            elide: Text.ElideMiddle
          }
          Text {
            text: SnapshotReader.updatedAt.length ? SnapshotReader.updatedAt : "NO UPDATE YET"
            color: "#52e8ff"
            font.family: "monospace"
            font.pixelSize: root.fontMicro
          }
        }

        Rectangle {
          visible: root.confirmingFix
          anchors.fill: parent
          z: 20
          color: "#d9080b12"
          border.width: 1
          border.color: "#ffb454"

          MouseArea { anchors.fill: parent }

          Rectangle {
            width: Math.min(560, parent.width - 48)
            height: 190
            anchors.centerIn: parent
            color: "#111824"
            border.width: 1
            border.color: "#ffb454"

            ColumnLayout {
              anchors.fill: parent
              anchors.margins: 20
              spacing: 10
              Text {
                text: "CONFIRM REPAIR ACTION"
                color: "#ffb454"
                font.family: "monospace"
                font.pixelSize: root.fontTitle
                font.bold: true
              }
              Text {
                Layout.fillWidth: true
                text: "This will run an allowlisted package repair with elevated permissions:\n" + root.pendingFixId + "\n\nReview the generated fix report after completion."
                color: "#c8d2e8"
                font.family: "monospace"
                font.pixelSize: root.fontBody
                wrapMode: Text.Wrap
              }
              RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                Rectangle {
                  Layout.preferredWidth: 110
                  Layout.preferredHeight: 36
                  color: "#1d2634"
                  border.width: 1
                  border.color: "#8290a4"
                  Text { anchors.centerIn: parent; text: "CANCEL"; color: "#c8d2e8"; font.family: "monospace"; font.pixelSize: root.fontSmall }
                  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.confirmingFix = false }
                }
                Rectangle {
                  Layout.preferredWidth: 150
                  Layout.preferredHeight: 36
                  color: "#2a2417"
                  border.width: 1
                  border.color: "#ffb454"
                  Text { anchors.centerIn: parent; text: "AUTHORIZE / APPLY"; color: "#ffb454"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.applyFix() }
                }
              }
            }
          }
        }
      }
    }
  }
}
