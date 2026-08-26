import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Effects
import QtQuick.Window

// Circular contact avatar: cached photo if available, otherwise a coloured circle with the
// name's initial. A presence dot is overlaid bottom-right when the status is known/online.
Item {
    id: root
    property string name: ""
    property string avatarPath: ""
    property string presence: ""   // online/away/xa/dnd/offline (empty = none)
    implicitWidth: 40
    implicitHeight: 40

    // Whether the cached file is a real animation. AnimatedImage is QMovie-backed and ERRORS
    // on static JPEG/PNG (the avatar cache is extension-less), so the static path must use a
    // plain Image; only genuine GIF/animated-WebP files go through AnimatedImage.
    readonly property bool animated: root.avatarPath.length > 0
                                     && backend.isAnimatedImage(root.avatarPath)
    readonly property bool photoReady: root.animated ? animImg.status === Image.Ready
                                                     : img.status === Image.Ready

    // Fallback: coloured circle + initial (shown until/unless a photo is ready).
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: Material.accent
        visible: !root.photoReady
        Label {
            anchors.centerIn: parent
            text: root.name.length > 0 ? root.name.charAt(0).toUpperCase() : "?"
            color: "white"
            font.pixelSize: parent.height * 0.45
            font.bold: true
        }
    }

    // Static photo, decoded at DEVICE pixels (logical size × the screen's scale factor) so it
    // stays sharp on hiDPI; mipmap smooths photos larger than that.
    Image {
        id: img
        anchors.fill: parent
        source: !root.animated && root.avatarPath.length > 0 ? ("file://" + root.avatarPath) : ""
        fillMode: Image.PreserveAspectCrop
        visible: false
        asynchronous: true
        // Cached: list-model resets recreate delegates constantly; without the cache every
        // reset re-decodes from disk and the avatars visibly flash. A republished avatar
        // changes the path's ?m=<mtime> cache-buster (see session::avatar_path_for).
        cache: true
        smooth: true
        mipmap: true
        sourceSize.width: width * Screen.devicePixelRatio
        sourceSize.height: height * Screen.devicePixelRatio
    }
    // Animated avatar (GIF / animated WebP published raw, like monocles Android).
    AnimatedImage {
        id: animImg
        anchors.fill: parent
        source: root.animated ? ("file://" + root.avatarPath) : ""
        fillMode: Image.PreserveAspectCrop
        visible: false
        playing: true
        asynchronous: true
        cache: true
        smooth: true
    }
    Item {
        id: circleMask
        anchors.fill: parent
        layer.enabled: true
        layer.smooth: true
        visible: false
        Rectangle { anchors.fill: parent; radius: width / 2; color: "black"; antialiasing: true }
    }
    MultiEffect {
        anchors.fill: parent
        source: root.animated ? animImg : img
        maskEnabled: true
        maskSource: circleMask
        // Sample the mask's antialiased edge as a soft alpha ramp instead of a hard cutoff —
        // without this the circle edge is binary per pixel and looks jagged.
        maskThresholdMin: 0.5
        maskSpreadAtMin: 1.0
        visible: root.photoReady
    }

    // Presence dot.
    Rectangle {
        visible: root.presence.length > 0 && root.presence !== "offline"
        width: 12
        height: 12
        radius: 6
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        border.width: 2
        border.color: root.Material.background
        color: root.presence === "online" ? "#2ec27e"
             : root.presence === "away" ? "#e5a50a"
             : root.presence === "xa" ? "#ff7800"
             : root.presence === "dnd" ? "#e01b24"
             : "#8b949e"
    }
}
