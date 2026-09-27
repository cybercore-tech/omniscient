import QtQuick

// A small line chart: `values` oldest first. Scales to the data unless
// `floor`/`ceiling` are set; NaN and non-numbers are skipped.
Canvas {
  id: spark

  property var values: []
  property color stroke: "#52e8ff"
  property real floor: NaN
  property real ceiling: NaN
  property real lineWidth: 1.5

  implicitHeight: 40
  onValuesChanged: spark.requestPaint()
  onWidthChanged: spark.requestPaint()
  onHeightChanged: spark.requestPaint()

  onPaint: {
    var ctx = spark.getContext("2d")
    ctx.reset()
    var points = (spark.values || []).filter(function(v) { return typeof v === "number" && isFinite(v) })
    if (points.length < 2) return
    var low = isNaN(spark.floor) ? Math.min.apply(null, points) : spark.floor
    var high = isNaN(spark.ceiling) ? Math.max.apply(null, points) : spark.ceiling
    if (high - low < 1e-9) { high = low + 1; low = low - 1 }
    var pad = spark.lineWidth + 1
    var w = spark.width - pad * 2
    var h = spark.height - pad * 2
    var x = function(i) { return pad + w * i / (points.length - 1) }
    var y = function(v) { return pad + h - h * (Math.max(low, Math.min(high, v)) - low) / (high - low) }

    ctx.beginPath()
    ctx.moveTo(x(0), spark.height)
    for (var i = 0; i < points.length; i++) ctx.lineTo(x(i), y(points[i]))
    ctx.lineTo(x(points.length - 1), spark.height)
    ctx.closePath()
    ctx.fillStyle = Qt.rgba(spark.stroke.r, spark.stroke.g, spark.stroke.b, 0.12)
    ctx.fill()

    ctx.beginPath()
    for (var j = 0; j < points.length; j++) {
      if (j === 0) ctx.moveTo(x(j), y(points[j]))
      else ctx.lineTo(x(j), y(points[j]))
    }
    ctx.strokeStyle = spark.stroke
    ctx.lineWidth = spark.lineWidth
    ctx.stroke()

    ctx.beginPath()
    ctx.arc(x(points.length - 1), y(points[points.length - 1]), spark.lineWidth + 1, 0, 2 * Math.PI)
    ctx.fillStyle = spark.stroke
    ctx.fill()
  }
}
