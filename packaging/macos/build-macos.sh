#!/usr/bin/env bash
# Build a macOS .app bundle + .dmg for the monocles chat Qt client.
# RUN THIS ON A MAC (Apple Silicon or Intel) — see packaging/macos/README.md.
#
#   ./packaging/macos/build-macos.sh
#
# Prerequisites (installed automatically below where possible):
#   - Xcode Command Line Tools:  xcode-select --install
#   - Homebrew (https://brew.sh)
# Everything else (Qt 6 incl. QtWebEngine, GStreamer, protoc, openssl, rustup with the
# pinned nightly from rust-toolchain.toml) is installed via brew/rustup by this script.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

APP_NAME="monocles chat"
BUNDLE_ID="de.monocles.chat.qt"
VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
echo "==> Building ${APP_NAME} ${VERSION} for macOS ($(uname -m))"

[ "$(uname -s)" = "Darwin" ] || { echo "error: this script must run on macOS"; exit 1; }
command -v brew >/dev/null 2>&1 || { echo "error: Homebrew not found — install from https://brew.sh"; exit 1; }

# 1. Build dependencies.
#    - qt: full Qt 6 incl. qtdeclarative + qtwebengine (WebXDC) + macdeployqt
#    - gstreamer: calls (webrtcbin/nice/opus/vpx) + voice messages; the brew formula
#      bundles the plugin sets
#    - protobuf: protoc for libsignal's `spqr` build.rs
#    - openssl@3: keg-only, exported below for openssl-sys
echo "==> Installing build dependencies (brew)…"
brew install qt gstreamer pkgconf protobuf openssl@3 cmake

QT_PREFIX="$(brew --prefix qt)"
export QMAKE="${QT_PREFIX}/bin/qmake"          # CXX-Qt locates Qt through qmake
export OPENSSL_DIR="$(brew --prefix openssl@3)" # keg-only, not on default search paths
export PKG_CONFIG_PATH="$(brew --prefix gstreamer)/lib/pkgconfig:${PKG_CONFIG_PATH:-}"

# Fail early if the GStreamer install lacks the webrtc dev files (gstreamer-webrtc-sys
# needs gstreamer-webrtc-1.0.pc; brew's monorepo formula ships it, but check anyway).
pkg-config --exists gstreamer-webrtc-1.0 || {
    echo "error: gstreamer-webrtc-1.0.pc not found via pkg-config — the calls stack builds"
    echo "       against gstreamer-webrtc. Check the brew gstreamer install ('brew info gstreamer')."
    exit 1; }

# 2. Rust toolchain — rust-toolchain.toml pins the exact nightly the vendored libsignal
#    needs, so `cargo` auto-installs/selects it. Install rustup if missing.
if ! command -v rustup >/dev/null 2>&1 && ! [ -x "$HOME/.cargo/bin/rustup" ]; then
    echo "==> Installing rustup…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
fi
export PATH="$HOME/.cargo/bin:$PATH"

# 3. sqlx compile-time check DB, built from the migrations (no machine-specific
#    DATABASE_URL, no committed .sqlx/ cache).
echo "==> Preparing sqlx compile-time database…"
DB="$PWD/.macos-build.db"
rm -f "$DB"
python3 - "$DB" crates/mxc-store/migrations/*.sql <<'PY'
import sqlite3, sys
db, *migrations = sys.argv[1:]
con = sqlite3.connect(db)
for m in sorted(migrations):
    con.executescript(open(m).read())
con.commit(); con.close()
PY
export DATABASE_URL="sqlite://$DB"

# 4. Release binary (QML + icons are embedded as Qt resources via build.rs).
echo "==> Building release binary (libsignal + SQLCipher from source — takes a while)…"
cargo build --release --locked -p mxc-app-qt

# 5. Assemble the .app bundle.
echo "==> Assembling ${APP_NAME}.app…"
OUT="target/macos"
APP="${OUT}/${APP_NAME}.app"
rm -rf "$OUT" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m0755 target/release/monocles-chat-qt "$APP/Contents/MacOS/monocles-chat-qt"

# .icns from the 512px PNG (sips + iconutil ship with macOS).
ICONSET="${OUT}/app.iconset"
mkdir -p "$ICONSET"
for s in 16 32 64 128 256 512; do
    sips -z $s $s data/icons/de.monocles.chat.qt.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    d=$((s * 2))
    sips -z $d $d data/icons/de.monocles.chat.qt.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/app.icns"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>        <string>monocles-chat-qt</string>
    <key>CFBundleIdentifier</key>        <string>${BUNDLE_ID}</string>
    <key>CFBundleName</key>              <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>       <string>${APP_NAME}</string>
    <key>CFBundleVersion</key>           <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundlePackageType</key>       <string>APPL</string>
    <key>CFBundleIconFile</key>          <string>app</string>
    <key>LSMinimumSystemVersion</key>    <string>12.0</string>
    <key>NSHighResolutionCapable</key>   <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>Audio calls and voice messages.</string>
    <key>NSCameraUsageDescription</key>
    <string>Video calls.</string>
</dict>
</plist>
EOF

# 6. Bundle the Qt frameworks, QML modules and linked dylibs into the .app.
#    -qmldir scans our QML for imports (QtQuick/Controls/WebEngine/…) so the matching
#    QML plugins get copied; macdeployqt also handles QtWebEngineProcess.app.
echo "==> Running macdeployqt…"
MACDEPLOYQT="${QT_PREFIX}/bin/macdeployqt"
[ -x "$MACDEPLOYQT" ] || MACDEPLOYQT="$(command -v macdeployqt)" || {
    echo "error: macdeployqt not found (expected in ${QT_PREFIX}/bin)"; exit 1; }
"$MACDEPLOYQT" "$APP" -qmldir=crates/mxc-app-qt/qml -verbose=1

# Ad-hoc signature so Gatekeeper lets it launch locally (replace "-" with a
# "Developer ID Application: …" identity + notarization for distribution).
codesign --force --deep --sign - "$APP"

# 7. Compressed .dmg for passing around.
echo "==> Creating .dmg…"
DMG="${OUT}/monocles-chat-qt-${VERSION}-macos-$(uname -m).dmg"
hdiutil create -volname "${APP_NAME}" -srcfolder "$APP" -ov -format UDZO "$DMG"

echo
echo "Built: $APP"
echo "       $DMG"
echo
echo "NOTE: GStreamer plugins are dlopen'd at runtime and are NOT bundled by macdeployqt."
echo "      On this machine the brew install covers them. For distribution to Macs without"
echo "      Homebrew, bundle GStreamer (gst_macos framework) or document 'brew install"
echo "      gstreamer' as a requirement — calls + voice messages need it."
