pragma Singleton
import QtQuick
import Quickshell

// Colour scales and formatting shared by the sensor tabs.
Singleton {
  id: style

  readonly property color cool: "#c8e967"
  readonly property color warm: "#ffb454"
  readonly property color hot: "#ff8f70"
  readonly property color critical: "#ff667d"
  readonly property color accent: "#52e8ff"
  readonly property color muted: "#8290a4"
  readonly property color text: "#c8d2e8"
  readonly property color bright: "#f2f5f7"
  readonly property color panel: "#0d1320"
  readonly property color card: "#111824"
  readonly property color line: "#263445"

  // CPU, GPU and board temperatures; `crit` (when the chip reports one)
  // pulls the red band down to 5 °C below it.
  function temperature(celsius, crit) {
    if (celsius === null || celsius === undefined) return style.muted
    var red = crit ? Math.min(90, crit - 5) : 90
    if (celsius >= red) return style.critical
    if (celsius >= 75) return style.hot
    if (celsius >= 60) return style.warm
    return style.cool
  }

  // Drives run cooler than CPUs; NVMe tolerates more than SATA.
  function driveTemperature(celsius, kind) {
    if (celsius === null || celsius === undefined) return style.muted
    var nvme = kind === "nvme"
    if (celsius >= (nvme ? 70 : 55)) return style.critical
    if (celsius >= (nvme ? 60 : 45)) return style.warm
    return style.cool
  }

  function load(percent) {
    if (percent === null || percent === undefined) return style.muted
    if (percent >= 90) return style.critical
    if (percent >= 70) return style.hot
    if (percent >= 40) return style.warm
    return style.cool
  }

  function celsius(value) {
    return value === null || value === undefined ? "—" : Number(value).toFixed(1) + "°C"
  }

  function number(value, unit, digits) {
    return value === null || value === undefined ? "—" : Number(value).toFixed(digits || 0) + unit
  }
}
