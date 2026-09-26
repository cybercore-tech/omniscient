import QtQuick
import QtQuick.Layouts

// One labelled reading with a proportional bar.
RowLayout {
  id: meter

  property string label: ""
  property string value: ""
  // 0..1; values outside are clamped, NaN draws an empty bar.
  property real fraction: 0
  property color tint: "#52e8ff"
  property int labelWidth: 150

  spacing: 10

  Text {
    Layout.preferredWidth: meter.labelWidth
    text: meter.label
    color: "#c8d2e8"
    font.family: "monospace"
    font.pixelSize: 11
    elide: Text.ElideRight
  }

  Rectangle {
    Layout.fillWidth: true
    Layout.preferredHeight: 8
    radius: 3
    color: "#0b111b"
    border.width: 1
    border.color: "#263445"

    Rectangle {
      width: parent.width * (isNaN(meter.fraction) ? 0 : Math.max(0, Math.min(1, meter.fraction)))
      height: parent.height
      radius: 3
      color: meter.tint
    }
  }

  Text {
    Layout.preferredWidth: 96
    horizontalAlignment: Text.AlignRight
    text: meter.value
    color: meter.tint
    font.family: "monospace"
    font.pixelSize: 11
    font.bold: true
  }
}
