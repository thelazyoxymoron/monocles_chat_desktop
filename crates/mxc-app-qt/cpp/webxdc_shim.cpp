// WebXDC C++ shim: the QtWebEngine pieces that have no Rust/QML bindings.
//
//  - mxc_webxdc_pre_app_init(): QtWebEngineQuick::initialize() (GL context sharing — must run
//    BEFORE QGuiApplication) + registration of the private `webxdc://` scheme. The scheme is
//    secure + CORS- and Fetch-enabled so apps get a secure context (crypto.subtle) and can
//    fetch()/module-import their own assets — like a real https origin, but entirely offline.
//    Deliberately NOT a "local" scheme: that gives a file://-like opaque origin, which breaks
//    apps' own same-origin module loads (lesson from the GTK client).
//
//  - mxc_webxdc_install(root, js): points the handler at an extracted app dir + the generated
//    webxdc.js, installing it on the default profile on first use (Qt thread only).
//
//  - The handler serves `webxdc://app/<path>` from the app dir, `/webxdc.js` from memory, and
//    forwards `/__bridge__` POST bodies (the JS API's messages) to Rust
//    (mxc_webxdc_bridge_message, defined in src/webxdc.rs — runs on a Chromium IO thread).

#include <QtWebEngineQuick/qtwebenginequickglobal.h>
#include <QtWebEngineQuick/QQuickWebEngineProfile>
#include <QtWebEngineCore/QWebEngineUrlScheme>
#include <QtWebEngineCore/QWebEngineUrlSchemeHandler>
#include <QtWebEngineCore/QWebEngineUrlRequestJob>

#include <QBuffer>
#include <QByteArray>
#include <QFile>
#include <QMutex>
#include <QMutexLocker>
#include <QString>
#include <QUrl>

extern "C" void mxc_webxdc_bridge_message(const char *data, size_t len);

namespace {

QMutex g_mutex;
QString g_root;   // extracted app dir currently served
QByteArray g_js;  // generated webxdc.js for the current instance

QByteArray mimeFor(const QString &path)
{
    const QString ext = path.section(QLatin1Char('.'), -1).toLower();
    if (ext == QLatin1String("html") || ext == QLatin1String("htm")) return QByteArrayLiteral("text/html");
    if (ext == QLatin1String("js") || ext == QLatin1String("mjs")) return QByteArrayLiteral("text/javascript");
    if (ext == QLatin1String("css")) return QByteArrayLiteral("text/css");
    if (ext == QLatin1String("json")) return QByteArrayLiteral("application/json");
    if (ext == QLatin1String("png")) return QByteArrayLiteral("image/png");
    if (ext == QLatin1String("jpg") || ext == QLatin1String("jpeg")) return QByteArrayLiteral("image/jpeg");
    if (ext == QLatin1String("gif")) return QByteArrayLiteral("image/gif");
    if (ext == QLatin1String("webp")) return QByteArrayLiteral("image/webp");
    if (ext == QLatin1String("svg")) return QByteArrayLiteral("image/svg+xml");
    if (ext == QLatin1String("wasm")) return QByteArrayLiteral("application/wasm");
    if (ext == QLatin1String("woff")) return QByteArrayLiteral("font/woff");
    if (ext == QLatin1String("woff2")) return QByteArrayLiteral("font/woff2");
    if (ext == QLatin1String("ttf")) return QByteArrayLiteral("font/ttf");
    if (ext == QLatin1String("ico")) return QByteArrayLiteral("image/x-icon");
    if (ext == QLatin1String("mp3")) return QByteArrayLiteral("audio/mpeg");
    if (ext == QLatin1String("wav")) return QByteArrayLiteral("audio/wav");
    if (ext == QLatin1String("ogg")) return QByteArrayLiteral("audio/ogg");
    return QByteArrayLiteral("application/octet-stream");
}

class WebxdcSchemeHandler : public QWebEngineUrlSchemeHandler
{
public:
    void requestStarted(QWebEngineUrlRequestJob *job) override
    {
        // QUrl::path() is already percent-decoded; query/fragment are excluded. Strip leading
        // slashes so root-absolute asset refs (`/assets/x.js`) resolve into the app dir.
        QString path = job->requestUrl().path();
        while (path.startsWith(QLatin1Char('/')))
            path.remove(0, 1);
        if (path.isEmpty())
            path = QStringLiteral("index.html");

        if (path == QLatin1String("__bridge__")) {
            // requestBody() hands over the device unopened — open it or readAll() returns
            // nothing ("QIODevice::read: device not open") and app clicks go nowhere.
            QIODevice *body = job->requestBody();
            QByteArray data;
            if (body) {
                if (!body->isOpen())
                    body->open(QIODevice::ReadOnly);
                data = body->readAll();
            }
            mxc_webxdc_bridge_message(data.constData(), static_cast<size_t>(data.size()));
            auto *buf = new QBuffer(job);
            buf->setData(QByteArrayLiteral("{}"));
            job->reply(QByteArrayLiteral("application/json"), buf);
            return;
        }

        QByteArray bytes;
        QByteArray mime;
        {
            QMutexLocker lock(&g_mutex);
            if (path == QLatin1String("webxdc.js")) {
                bytes = g_js;
                mime = QByteArrayLiteral("text/javascript");
            } else {
                QFile f(g_root + QLatin1Char('/') + path);
                if (f.open(QIODevice::ReadOnly))
                    bytes = f.readAll();
                // A missing asset serves empty (not an error page), like the GTK client.
                mime = mimeFor(path);
            }
        }
        auto *buf = new QBuffer(job);
        buf->setData(bytes);
        job->reply(mime, buf);
    }
};

WebxdcSchemeHandler *g_handler = nullptr;

} // namespace

extern "C" void mxc_webxdc_pre_app_init()
{
    QtWebEngineQuick::initialize();
    QWebEngineUrlScheme scheme(QByteArrayLiteral("webxdc"));
    scheme.setSyntax(QWebEngineUrlScheme::Syntax::Host);
    scheme.setFlags(QWebEngineUrlScheme::SecureScheme
                    | QWebEngineUrlScheme::CorsEnabled
                    | QWebEngineUrlScheme::FetchApiAllowed);
    QWebEngineUrlScheme::registerScheme(scheme);
}

extern "C" void mxc_webxdc_install(const char *root, const char *js)
{
    {
        QMutexLocker lock(&g_mutex);
        g_root = QString::fromUtf8(root);
        g_js = QByteArray(js);
    }
    if (!g_handler) {
        g_handler = new WebxdcSchemeHandler();
        QQuickWebEngineProfile::defaultProfile()->installUrlSchemeHandler(
            QByteArrayLiteral("webxdc"), g_handler);
    }
}
