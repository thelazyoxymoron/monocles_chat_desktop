import QtQuick
import QtQuick.Effects

// Renders a (white) monochrome SVG icon tinted to `color`. The source SVGs are authored
// white so MultiEffect colorization (which scales by source lightness) yields the full
// target colour.
//
// IMPORTANT: when used as a Control's `contentItem` (e.g. a Material ToolButton), the control
// resizes this item to fill its content area (which has a ~40px touch-target floor), so the
// outer item's size does NOT reflect the desired glyph size. We therefore draw the glyph in a
// fixed-size holder, centred — sized from `implicitWidth/Height` when set (the contentItem case),
// else the explicit `width/height` (free-standing usage like the bubble react button).
Item {
    id: root
    property url source
    property color color: "white"

    readonly property real iconW: implicitWidth > 0 ? implicitWidth : width
    readonly property real iconH: implicitHeight > 0 ? implicitHeight : height

    Item {
        id: holder
        width: root.iconW
        height: root.iconH
        anchors.centerIn: parent

        Image {
            id: img
            anchors.fill: parent
            source: root.source
            sourceSize.width: width
            sourceSize.height: height
            fillMode: Image.PreserveAspectFit
            smooth: true
            visible: false
        }
        MultiEffect {
            anchors.fill: parent
            source: img
            colorization: 1.0
            colorizationColor: root.color
        }
    }
}
