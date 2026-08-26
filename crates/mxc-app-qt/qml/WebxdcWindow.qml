import QtQuick
import QtQuick.Controls
import QtWebEngine

// A running WebXDC mini-app: a WebEngineView on the private webxdc:// scheme (served by the
// C++ shim from the extracted .xdc). Created DYNAMICALLY from main.qml on `webxdcReady` —
// never instantiate it statically, so the main app still loads when QtWebEngine is missing.
Window {
    id: win
    property string thread: ""
    width: 420
    height: 640
    title: qsTr("WebXDC app")

    // Incoming state from the backend (forwarded by main.qml's Connections).
    function pushUpdates(items) {
        web.runJavaScript("window.__webxdcPushUpdates([" + items + "]);")
    }
    function pushRealtime(b64) {
        web.runJavaScript("window.__webxdcRealtimeData('" + b64 + "');")
    }

    onClosing: backend.closeWebxdc()

    WebEngineView {
        id: web
        anchors.fill: parent
        url: "webxdc://app/index.html"
        backgroundColor: "white"
        settings.playbackRequiresUserGesture: false
        settings.webGLEnabled: true
        settings.localContentCanAccessRemoteUrls: false
        settings.screenCaptureEnabled: false

        // Keep the app offline: only our private scheme may navigate; real links open in the
        // user's browser instead (matches monocles Android + the GTK client).
        onNavigationRequested: (request) => {
            const u = request.url.toString()
            if (!u.startsWith("webxdc:") && u !== "about:blank") {
                request.action = WebEngineNavigationRequest.IgnoreRequest
                if (u.startsWith("http://") || u.startsWith("https://"))
                    Qt.openUrlExternally(request.url)
            }
        }
        onNewWindowRequested: (request) => {
            const u = request.requestedUrl.toString()
            if (u.startsWith("http://") || u.startsWith("https://"))
                Qt.openUrlExternally(request.requestedUrl)
        }
    }
}
