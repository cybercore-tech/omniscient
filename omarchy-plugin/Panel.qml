import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons

Item {
  id: root

  readonly property string selfId: "io.github.cybercore-tech.omniscient"
  readonly property string omniscientBinary: (Quickshell.env("HOME") || "") + "/.local/bin/omniscient"
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
  property bool confirmingHelp: false
  property bool helpLaunchLocked: false
  property string pendingHelpUrl: ""
  property string pendingHelpLabel: ""
  property bool fullReportView: false
  property bool fixCenterOpen: false
  property int selectedFixIndex: 0

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
    root.confirmingHelp = false
    root.pendingFixId = id
    root.confirmingFix = true
  }

  function openFixCenter() {
    root.fullReportView = false
    root.fixCenterOpen = true
    if (SnapshotReader.suggestions.length > 0 && root.selectedFixIndex >= SnapshotReader.suggestions.length)
      root.selectedFixIndex = 0
  }

  function closeFixCenter() {
    root.fixCenterOpen = false
    root.confirmingFix = false
    root.confirmingHelp = false
  }

  function selectedFix() {
    if (root.selectedFixIndex < 0 || root.selectedFixIndex >= SnapshotReader.suggestions.length)
      return ({})
    return SnapshotReader.suggestions[root.selectedFixIndex]
  }

  function requestHelp(url, label) {
    var value = String(url || "")
    if (root.helpLaunchLocked)
      return
    if (value.indexOf("https://") !== 0 && value.indexOf("http://") !== 0)
      return
    root.pendingHelpUrl = value
    root.pendingHelpLabel = String(label || "EXTERNAL REFERENCE")
    root.confirmingHelp = true
  }

  function openFixHelp(url, label) {
    root.requestHelp(url, label)
  }

  function allowHelp() {
    if (root.helpLaunchLocked)
      return
    root.confirmingHelp = false
    if (root.pendingHelpUrl.length > 0 && !helpLauncher.running) {
      root.helpLaunchLocked = true
      helpLauncher.running = true
      helpLaunchGuard.restart()
    }
  }

  function fixReportPath() {
    var marker = "FIX REPORT / "
    var value = String(root.fixMessage || "")
    var index = value.indexOf(marker)
    return index >= 0 ? value.substring(index + marker.length).trim() : ""
  }

  function openFixReport() {
    var path = root.fixReportPath()
    if (path.length > 0) {
      root.fixCenterOpen = false
      root.openReport(path)
    }
  }

  function applyFix() {
    if (!root.pendingFixId.length || fixRunner.running) return
    root.confirmingFix = false
    root.fixMessage = "FIX REQUEST STARTING"
    fixRunner.running = true
  }

  function openSuggestionsReport() {
    if (SnapshotReader.suggestionsPath.length) {
      root.fixCenterOpen = false
      root.openReport(SnapshotReader.suggestionsPath)
    }
  }

  function runAudit() {
    if (auditRunner.running) return
    root.runnerMessage = "AUDIT PROCESS STARTING"
    root.opened = true
    auditRunner.running = true
    SnapshotReader.refresh()
  }

  function openReport(path) {
    root.fixCenterOpen = false
    var value = String(path || "")
    // Storage Matrix historically emitted storage.md under the disks
    // directory while older snapshots indexed it as disks.md. Keep those
    // snapshots readable after the backend contract is corrected.
    if (value.endsWith("/disks.md"))
      value = value.substring(0, value.length - "/disks.md".length) + "/storage.md"
    if (!isSafeReportPath(value)) {
      root.reportText = "REPORT REJECTED / UNSAFE LOCAL PATH"
      return
    }
    root.selectedReport = value
    root.reportText = "LOADING REPORT..."
    reportReader.running = true
  }

  function isSafeReportPath(path) {
    var value = String(path || "")
    return value.startsWith("/")
      && value.endsWith(".md")
      && value.indexOf("/../") < 0
      && value.indexOf("/omniscient/") >= 0
  }

  function escapeHtml(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\"/g, "&quot;")
  }

  function inlineMarkdown(value) {
    var html = escapeHtml(value)
    html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, "<a href='$2'>$1</a>")
    html = html.replace(/\*\*(.*?)\*\*/g, "<b>$1</b>")
    html = html.replace(/`([^`]+)`/g, "<font color='#ffb454'><b>$1</b></font>")
    return html
  }

  function markdownToRichText(value) {
    var lines = String(value || "").split("\n")
    var html = []
    var inCode = false
    for (var i = 0; i < lines.length; i++) {
      var line = lines[i]
      if (line.indexOf("```") === 0) {
        inCode = !inCode
        html.push(inCode
          ? "<font color='#a56bff'><b>▌ CODE BLOCK</b></font><br>"
          : "<font color='#a56bff'><b>▌ END CODE</b></font><br>")
      } else if (inCode) {
        html.push("<font color='#7dd3fc'>" + escapeHtml(line) + "</font><br>")
      } else if (line.indexOf("### ") === 0) {
        html.push("<font color='#ffb454'><b>" + inlineMarkdown(line.substring(4)) + "</b></font><br>")
      } else if (line.indexOf("## ") === 0) {
        html.push("<font color='#52e8ff'><b>" + inlineMarkdown(line.substring(3)) + "</b></font><br>")
      } else if (line.indexOf("# ") === 0) {
        html.push("<font color='#ff4f9a'><b>" + inlineMarkdown(line.substring(2)) + "</b></font><br>")
      } else if (line.indexOf("- ") === 0) {
        html.push("<font color='#c8e967'>◆</font> " + inlineMarkdown(line.substring(2)) + "<br>")
      } else if (line.indexOf("> ") === 0) {
        html.push("<font color='#8290a4'>│ " + inlineMarkdown(line.substring(2)) + "</font><br>")
      } else if (line.trim().length === 0) {
        html.push("<br>")
      } else {
        html.push(inlineMarkdown(line) + "<br>")
      }
    }
    return html.join("")
  }

  function reportSeverity(path) {
    var value = String(path).toLowerCase()
    if (value.indexOf("suggestions") >= 0) {
      var highest = "healthy"
      for (var i = 0; i < SnapshotReader.suggestions.length; i++) {
        var severity = String(SnapshotReader.suggestions[i].severity || "attention")
        if (severity === "urgent") return "urgent"
        if (severity === "warning") highest = "warning"
        else if (severity === "attention" && highest === "healthy") highest = "attention"
      }
      return highest
    }
    return SnapshotReader.healthLabel(SnapshotReader.healthScore).toLowerCase()
  }

  function reportIcon(path) {
    var value = String(path).toLowerCase()
    if (value.indexOf("hardware") >= 0) return "🖥️"
    if (value.indexOf("storage") >= 0) return "💾"
    if (value.indexOf("snapshot") >= 0) return "📸"
    if (value.indexOf("network") >= 0) return "🌐"
    if (value.indexOf("container") >= 0) return "📦"
    if (value.indexOf("service") >= 0) return "⚙️"
    if (value.indexOf("log") >= 0) return "📜"
    if (value.indexOf("bluetooth") >= 0) return "📡"
    if (value.indexOf("device") >= 0) return "🔌"
    if (value.indexOf("suggestion") >= 0) return "🧰"
    if (value.indexOf("summary") >= 0) return "🛰️"
    return "📄"
  }

  Process {
    id: auditRunner
    command: ["/usr/bin/env", "OMNISCIENT_AUTH=pkexec", root.omniscientBinary, "--hud"]
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
      ? ["/usr/bin/env", "OMNISCIENT_AUTH=pkexec", root.omniscientBinary, "--fix", root.pendingFixId]
      : ["/usr/bin/true"]
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
    command: root.selectedReport.length ? ["/usr/bin/cat", root.selectedReport] : ["/usr/bin/true"]
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
        root.reportText = "REPORT UNAVAILABLE\n\nThe saved report path no longer exists or cannot be read:\n" + root.selectedReport
      }
    }
  }

  Process {
    id: helpLauncher
    command: root.pendingHelpUrl.length
      ? ["/usr/bin/xdg-open", root.pendingHelpUrl]
      : ["/usr/bin/true"]
  }

  Timer {
    id: helpLaunchGuard
    interval: 1500
    repeat: false
    onTriggered: root.helpLaunchLocked = false
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

        Flickable {
          id: auditBodyScroll
          Layout.fillWidth: true
          Layout.fillHeight: true
          Layout.minimumHeight: 0
          clip: true
          contentWidth: width
          contentHeight: auditBodyContent.implicitHeight

          ColumnLayout {
            id: auditBodyContent
            width: auditBodyScroll.width
            spacing: 10

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
          Layout.preferredHeight: Math.max(54, Math.ceil(SnapshotReader.modules.length / 3) * 61 - 7)
          Layout.minimumHeight: Math.max(54, Math.ceil(SnapshotReader.modules.length / 3) * 61 - 7)
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
              Rectangle {
                Layout.preferredWidth: 150
                Layout.preferredHeight: 28
                radius: 3
                color: "#182438"
                border.width: 1
                border.color: "#ffb454"
                Text { anchors.centerIn: parent; text: "OPEN FIX CENTER"; color: "#ffb454"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixCenter() }
              }
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
                      onLinkActivated: function(link) { root.requestHelp(link, "SUGGESTION REFERENCE") }
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
          Layout.preferredHeight: 300
          Layout.minimumHeight: 220
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
              Text { text: "REPORT INDEX / URGENCY"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }
              ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: SnapshotReader.reports
                delegate: Rectangle {
                  width: ListView.view.width
                  height: 32
                  color: root.selectedReport === String(modelData) ? "#1a2940" : "transparent"
                  Text {
                    anchors.fill: parent
                    anchors.margins: 6
                    text: root.reportIcon(String(modelData)) + "  " + String(modelData).split("/").pop()
                    color: SnapshotReader.severityColor(root.reportSeverity(String(modelData)))
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
              RowLayout {
                Layout.fillWidth: true
                Text {
                  text: root.selectedReport.length ? "REPORT VIEW / " + root.selectedReport.split("/").pop() : "REPORT VIEW"
                  color: "#ff4f9a"
                  font.family: "monospace"
                  font.pixelSize: root.fontSection
                  elide: Text.ElideMiddle
                  Layout.fillWidth: true
                }
                Rectangle {
                  Layout.preferredWidth: 112
                  Layout.preferredHeight: 30
                  color: "#121c2b"
                  border.width: 1
                  border.color: "#52e8ff"
                  Text { anchors.centerIn: parent; text: "FULL VIEW"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.fullReportView = true }
                }
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
                  text: root.markdownToRichText(root.reportText.length ? root.reportText : "SELECT A REPORT TO VIEW IT HERE")
                  color: "#c8d2e8"
                  font.family: "monospace"
                  font.pixelSize: root.fontBody
                  wrapMode: Text.Wrap
                  textFormat: Text.RichText
                  onLinkActivated: function(link) { root.requestHelp(link, "REPORT REFERENCE") }
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

          }
        }

        Rectangle {
          visible: root.confirmingFix
          anchors.fill: parent
          z: 40
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
                text: "This will run an allowlisted package repair with elevated permissions.\n\nFinding: " + String(root.selectedFix().title || root.pendingFixId) + "\nCommand: " + String(root.selectedFix().command || "not available") + "\n\nAllow only if you reviewed the explanation and proposed command. A fix report will be written after completion."
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

        Rectangle {
          visible: root.confirmingHelp
          anchors.fill: parent
          z: 45
          color: "#d9080b12"
          border.width: 1
          border.color: "#52e8ff"

          MouseArea { anchors.fill: parent }

          Rectangle {
            width: Math.min(600, parent.width - 48)
            height: 230
            anchors.centerIn: parent
            color: "#111824"
            border.width: 1
            border.color: "#52e8ff"

            ColumnLayout {
              anchors.fill: parent
              anchors.margins: 20
              spacing: 10
              Text {
                text: "ALLOW EXTERNAL REFERENCE"
                color: "#52e8ff"
                font.family: "monospace"
                font.pixelSize: root.fontTitle
                font.bold: true
              }
              Text {
                Layout.fillWidth: true
                text: "Open the " + root.pendingHelpLabel + " in your browser?\n\nThis is read-only guidance. It will not run a repair or grant permissions.\n\n" + root.pendingHelpUrl
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
                  Text { anchors.centerIn: parent; text: "DENY / CLOSE"; color: "#c8d2e8"; font.family: "monospace"; font.pixelSize: root.fontSmall }
                  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.confirmingHelp = false }
                }
                Rectangle {
                  Layout.preferredWidth: 145
                  Layout.preferredHeight: 36
                  color: "#12262f"
                  border.width: 1
                  border.color: "#52e8ff"
                  Text { anchors.centerIn: parent; text: "ALLOW / OPEN"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.allowHelp() }
                }
              }
            }
          }
        }

        Rectangle {
          visible: root.fullReportView && root.selectedReport.length > 0
          anchors.fill: parent
          z: 15
          color: "#080b12"
          border.width: 1
          border.color: "#52e8ff"

          ColumnLayout {
            anchors.fill: parent
            anchors.margins: 22
            spacing: 12

            RowLayout {
              Layout.fillWidth: true
              Text {
                text: root.reportIcon(root.selectedReport) + "  FULL REPORT / " + root.selectedReport.split("/").pop()
                color: "#52e8ff"
                font.family: "monospace"
                font.pixelSize: root.fontTitle
                font.bold: true
                elide: Text.ElideMiddle
                Layout.fillWidth: true
              }
              Rectangle {
                Layout.preferredWidth: 112
                Layout.preferredHeight: 34
                color: "#121c2b"
                border.width: 1
                border.color: "#ff4f9a"
                Text { anchors.centerIn: parent; text: "BACK"; color: "#ff4f9a"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.fullReportView = false }
              }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: "#263445" }

            Flickable {
              Layout.fillWidth: true
              Layout.fillHeight: true
              clip: true
              contentWidth: width
              contentHeight: fullReportBody.paintedHeight
              Text {
                id: fullReportBody
                width: parent.width
                text: root.markdownToRichText(root.reportText)
                color: "#c8d2e8"
                font.family: "monospace"
                font.pixelSize: root.fontBody
                wrapMode: Text.Wrap
                textFormat: Text.RichText
                onLinkActivated: function(link) { root.requestHelp(link, "REPORT REFERENCE") }
              }
            }
          }
        }

        Rectangle {
          visible: root.fixCenterOpen
          anchors.fill: parent
          z: 25
          color: "#080b12"
          border.width: 1
          border.color: "#ffb454"

          MouseArea { anchors.fill: parent; onClicked: mouse.accepted = true }

          ColumnLayout {
            anchors.fill: parent
            anchors.margins: 22
            spacing: 12

            RowLayout {
              Layout.fillWidth: true
              Text {
                text: "🧰  FIX CENTER / REPAIR CONTROL"
                color: "#ffb454"
                font.family: "monospace"
                font.pixelSize: root.fontTitle
                font.bold: true
                Layout.fillWidth: true
              }
              Text {
                text: SnapshotReader.suggestions.length + " ACTIONS"
                color: "#52e8ff"
                font.family: "monospace"
                font.pixelSize: root.fontSmall
                font.bold: true
              }
              Rectangle {
                Layout.preferredWidth: 104
                Layout.preferredHeight: 34
                color: "#121c2b"
                border.width: 1
                border.color: "#ff4f9a"
                Text { anchors.centerIn: parent; text: "CLOSE"; color: "#ff4f9a"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.closeFixCenter() }
              }
            }

            Text {
              Layout.fillWidth: true
              text: "Select a finding to review the command, man page, documentation, and repair mode. Manual repair only opens guidance; Auto Repair requires confirmation and one explicit authorization."
              color: "#c8d2e8"
              font.family: "monospace"
              font.pixelSize: root.fontBody
              wrapMode: Text.Wrap
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: "#263445" }

            RowLayout {
              Layout.fillWidth: true
              Layout.fillHeight: true
              spacing: 12

              Rectangle {
                Layout.preferredWidth: 350
                Layout.fillHeight: true
                color: "#0d1320"
                border.width: 1
                border.color: "#263445"

                ColumnLayout {
                  anchors.fill: parent
                  anchors.margins: 10
                  spacing: 7
                  Text { text: "REPAIR QUEUE"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }
                  ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: SnapshotReader.suggestions
                    spacing: 6
                    delegate: Rectangle {
                      width: ListView.view.width
                      height: 64
                      color: root.selectedFixIndex === index ? "#1c2b43" : "#111824"
                      border.width: 1
                      border.color: SnapshotReader.severityColor(String(modelData.severity || "attention"))
                      RowLayout {
                        anchors.fill: parent
                        anchors.margins: 9
                        spacing: 8
                        Text { text: String(modelData.severity || "attention").toUpperCase(); color: SnapshotReader.severityColor(String(modelData.severity || "attention")); font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                        ColumnLayout {
                          Layout.fillWidth: true
                          spacing: 2
                          Text { text: String(modelData.title || ""); color: "#f2f5f7"; font.family: "monospace"; font.pixelSize: root.fontSmall; elide: Text.ElideRight; Layout.fillWidth: true }
                          Text { text: modelData.auto_fix ? "AUTO REPAIR AVAILABLE" : "MANUAL REVIEW REQUIRED"; color: modelData.auto_fix ? "#c8e967" : "#8290a4"; font.family: "monospace"; font.pixelSize: root.fontMicro }
                        }
                      }
                      MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.selectedFixIndex = index }
                    }
                    Text {
                      anchors.centerIn: parent
                      visible: SnapshotReader.suggestions.length === 0
                      text: "NO FINDINGS / SYSTEM CLEAR"
                      color: "#c8e967"
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
                border.color: SnapshotReader.suggestions.length ? SnapshotReader.severityColor(String(root.selectedFix().severity || "attention")) : "#263445"

                Flickable {
                  id: fixDetailScroll
                  anchors.fill: parent
                  anchors.margins: 16
                  clip: true
                  contentWidth: width
                  contentHeight: fixDetailColumn.implicitHeight

                  ColumnLayout {
                    id: fixDetailColumn
                    width: fixDetailScroll.width
                    spacing: 10

                  Text {
                    Layout.fillWidth: true
                    text: SnapshotReader.suggestions.length ? String(root.selectedFix().title || "SELECT A FINDING") : "NO REPAIR ACTIONS"
                    color: SnapshotReader.suggestions.length ? SnapshotReader.severityColor(String(root.selectedFix().severity || "attention")) : "#c8e967"
                    font.family: "monospace"
                    font.pixelSize: root.fontTitle
                    font.bold: true
                    wrapMode: Text.Wrap
                  }
                  Text {
                    Layout.fillWidth: true
                    text: SnapshotReader.suggestions.length ? String(root.selectedFix().detail || "") : "The latest audit did not produce repair suggestions."
                    color: "#c8d2e8"
                    font.family: "monospace"
                    font.pixelSize: root.fontBody
                    wrapMode: Text.Wrap
                  }
                  Text {
                    visible: SnapshotReader.suggestions.length > 0
                    Layout.fillWidth: true
                    text: "EXPLANATION / IMPACT"
                    color: "#52e8ff"
                    font.family: "monospace"
                    font.pixelSize: root.fontSection
                    font.bold: true
                  }
                  Text {
                    visible: SnapshotReader.suggestions.length > 0
                    Layout.fillWidth: true
                    text: String(root.selectedFix().explanation || "No additional explanation was recorded for this finding.")
                    color: "#c8d2e8"
                    font.family: "monospace"
                    font.pixelSize: root.fontSmall
                    wrapMode: Text.Wrap
                  }
                  Text {
                    visible: SnapshotReader.suggestions.length > 0
                    Layout.fillWidth: true
                    text: "MANUAL CHECKLIST / REVIEW EACH STEP"
                    color: "#ffb454"
                    font.family: "monospace"
                    font.pixelSize: root.fontSection
                    font.bold: true
                  }
                  ColumnLayout {
                    visible: SnapshotReader.suggestions.length > 0
                    Layout.fillWidth: true
                    spacing: 3
                    Repeater {
                      model: SnapshotReader.suggestions.length > 0 ? (root.selectedFix().manual_steps || []) : []
                      delegate: Text {
                        Layout.fillWidth: true
                        text: "◆ " + String(modelData || "")
                        color: "#c8d2e8"
                        font.family: "monospace"
                        font.pixelSize: root.fontSmall
                        wrapMode: Text.Wrap
                      }
                    }
                  }
                  Text { visible: SnapshotReader.suggestions.length > 0; text: "PROPOSED COMMAND / INSPECTION"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontSection; font.bold: true }
                  Rectangle {
                    visible: SnapshotReader.suggestions.length > 0
                    Layout.fillWidth: true
                    Layout.preferredHeight: 52
                    color: "#080b12"
                    border.width: 1
                    border.color: "#263445"
                    Text { anchors.fill: parent; anchors.margins: 10; text: String(root.selectedFix().command || "NO AUTOMATIC COMMAND"); color: "#ffb454"; font.family: "monospace"; font.pixelSize: root.fontBody; wrapMode: Text.Wrap; verticalAlignment: Text.AlignVCenter }
                  }
                  Text {
                    visible: SnapshotReader.suggestions.length > 0
                    Layout.fillWidth: true
                    text: (root.selectedFix().auto_fix ? "AUTO REPAIR: AVAILABLE AFTER AUTHORIZATION / " : "AUTO REPAIR: NOT AVAILABLE / ") + String(root.selectedFix().auto_fix_reason || "Manual review is required.")
                    color: root.selectedFix().auto_fix ? "#c8e967" : "#ff8f70"
                    font.family: "monospace"
                    font.pixelSize: root.fontSmall
                    wrapMode: Text.Wrap
                  }
                  RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Rectangle {
                      Layout.preferredWidth: 112
                      Layout.preferredHeight: 32
                      color: "#121c2b"
                      border.width: 1
                      border.color: "#52e8ff"
                      Text { anchors.centerIn: parent; text: "OPEN MAN PAGE"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                      MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixHelp(root.selectedFix().man_url, "MAN PAGE") }
                    }
                    Rectangle {
                      Layout.preferredWidth: 124
                      Layout.preferredHeight: 32
                      color: "#121c2b"
                      border.width: 1
                      border.color: "#52e8ff"
                      Text { anchors.centerIn: parent; text: "OPEN DOCUMENTATION"; color: "#52e8ff"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                      MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixHelp(root.selectedFix().docs_url, "DOCUMENTATION") }
                    }
                    Rectangle {
                      Layout.preferredWidth: 152
                      Layout.preferredHeight: 32
                      color: "#121c2b"
                      border.width: 1
                      border.color: "#a56bff"
                      Text { anchors.centerIn: parent; text: "VIEW MD REPORT"; color: "#a56bff"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                      MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openSuggestionsReport() }
                    }
                  }
                  Text { visible: root.fixMessage.length > 0; Layout.fillWidth: true; text: root.fixMessage; color: root.fixMessage.indexOf("FAILED") >= 0 ? "#ff667d" : "#c8e967"; font.family: "monospace"; font.pixelSize: root.fontMicro; elide: Text.ElideMiddle }
                  RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Rectangle {
                      Layout.fillWidth: true
                      Layout.preferredHeight: 42
                      color: "#182438"
                      border.width: 1
                      border.color: "#8290a4"
                      Text { anchors.centerIn: parent; text: "MANUAL REPAIR / OPEN GUIDE"; color: "#c8d2e8"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                      MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixHelp(root.selectedFix().docs_url, "MANUAL REPAIR GUIDE") }
                    }
                    Rectangle {
                      visible: SnapshotReader.suggestions.length > 0 && Boolean(root.selectedFix().auto_fix)
                      Layout.fillWidth: true
                      Layout.preferredHeight: 42
                      color: "#2a2417"
                      border.width: 1
                      border.color: "#ffb454"
                      Text { anchors.centerIn: parent; text: "AUTO REPAIR / CONFIRM"; color: "#ffb454"; font.family: "monospace"; font.pixelSize: root.fontSmall; font.bold: true }
                      MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.requestFix(String(root.selectedFix().id || "")) }
                    }
                  }
                  Rectangle {
                    visible: root.fixReportPath().length > 0
                    Layout.preferredWidth: 180
                    Layout.preferredHeight: 30
                    color: "#121c2b"
                    border.width: 1
                    border.color: "#c8e967"
                    Text { anchors.centerIn: parent; text: "VIEW FIX RESULT"; color: "#c8e967"; font.family: "monospace"; font.pixelSize: root.fontMicro; font.bold: true }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.openFixReport() }
                  }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
