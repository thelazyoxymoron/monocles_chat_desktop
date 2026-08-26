import QtQuick
import QtQuick.Controls

// Thin overlay scrollbar (GTK-style): a slim handle that fattens slightly on hover. The Material
// style's auto-hide fade animates both contentItem and background opacity, so the background must
// be replaced with a transparent stand-in (NOT null — that breaks the transition) to get rid of
// Material's 16px-wide grey track.
ScrollBar {
    id: control
    policy: ScrollBar.AsNeeded

    padding: 1

    contentItem: Rectangle {
        implicitWidth: control.hovered || control.pressed ? 6 : 3
        implicitHeight: control.hovered || control.pressed ? 6 : 3
        radius: width / 2
        color: control.pressed ? Qt.rgba(0.55, 0.55, 0.55, 0.9)
                               : Qt.rgba(0.55, 0.55, 0.55, 0.55)
        Behavior on implicitWidth { NumberAnimation { duration: 120 } }
    }

    background: Rectangle {
        implicitWidth: 6
        implicitHeight: 6
        color: "transparent"
    }
}
