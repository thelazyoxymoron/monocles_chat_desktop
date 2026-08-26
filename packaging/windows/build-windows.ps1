# Build a Windows distribution of the monocles chat Qt client.
# RUN THIS ON WINDOWS in a "x64 Native Tools" / "Developer PowerShell for VS" prompt —
# see packaging/windows/README.md for the prerequisites (Qt, GStreamer, rustup, protoc).
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build-windows.ps1
#
# Output: target\windows\monocles-chat-qt-<version>-windows-x64.zip
# (the app folder inside is self-contained except for the GStreamer runtime — see README).
#
# Configuration via environment variables (defaults below):
#   QT_DIR        Qt for MSVC install dir, e.g. C:\Qt\6.8.2\msvc2022_64
#                 (needs the "Qt WebEngine" component selected in the Qt installer!)
#   GSTREAMER_DIR GStreamer MSVC x64 install dir (runtime + development installers),
#                 default: %GSTREAMER_1_0_ROOT_MSVC_X86_64% set by the GStreamer installer
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")   # repo root

$Version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' |
            Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "==> Building monocles chat $Version for Windows x64"

# --- 1. Locate the toolchain pieces -------------------------------------------------------
$QtDir = if ($env:QT_DIR) { $env:QT_DIR } else { "C:\Qt\6.8.2\msvc2022_64" }
if (-not (Test-Path "$QtDir\bin\qmake.exe")) {
    throw "Qt for MSVC not found at $QtDir (set QT_DIR). Install via the Qt online installer, INCLUDING the 'Qt WebEngine' component (needed for WebXDC)."
}
$GstDir = if ($env:GSTREAMER_DIR) { $env:GSTREAMER_DIR } else { $env:GSTREAMER_1_0_ROOT_MSVC_X86_64 }
if (-not $GstDir -or -not (Test-Path "$GstDir\lib\pkgconfig")) {
    throw "GStreamer MSVC x64 not found (set GSTREAMER_DIR). Install BOTH the runtime and the development MSVC installers from https://gstreamer.freedesktop.org/download/ (calls + voice messages need them)."
}
# A "Typical" install omits the webrtc dev files; gstreamer-webrtc-sys needs them.
if (-not (Test-Path "$GstDir\lib\pkgconfig\gstreamer-webrtc-1.0.pc")) {
    throw "GStreamer install at $GstDir is missing gstreamer-webrtc-1.0.pc - re-run BOTH GStreamer MSVC installers and choose the 'Complete' setup type (the calls stack builds against gstreamer-webrtc)."
}
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    throw "MSVC (cl.exe) not on PATH — run this from a 'x64 Native Tools' / 'Developer PowerShell for VS' prompt."
}
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
    throw "rustup not found — install from https://rustup.rs (the pinned nightly from rust-toolchain.toml is picked up automatically)."
}
if (-not (Get-Command protoc -ErrorAction SilentlyContinue)) {
    throw "protoc not found — 'winget install protobuf' or download from https://github.com/protocolbuffers/protobuf/releases (libsignal's build needs it)."
}
$Python = (Get-Command python -ErrorAction SilentlyContinue) ?? (Get-Command py -ErrorAction SilentlyContinue)
if (-not $Python) { throw "Python 3 not found — 'winget install Python.Python.3.12' (used to build the sqlx check database)." }

# gstreamer-rs locates GStreamer through pkg-config. Prefer a pkg-config shipped with
# GStreamer; otherwise one must be on PATH (e.g. 'choco install pkgconfiglite').
if (Test-Path "$GstDir\bin\pkg-config.exe") {
    $env:PKG_CONFIG = "$GstDir\bin\pkg-config.exe"
} elseif (-not (Get-Command pkg-config -ErrorAction SilentlyContinue)) {
    throw "pkg-config not found — 'choco install pkgconfiglite' (gstreamer-rs needs it to locate GStreamer)."
}
$env:PKG_CONFIG_PATH = "$GstDir\lib\pkgconfig"
$env:PATH = "$GstDir\bin;$QtDir\bin;$env:PATH"

# CXX-Qt locates Qt through qmake.
$env:QMAKE = "$QtDir\bin\qmake.exe"

# --- 2. sqlx compile-time check DB from the migrations ------------------------------------
Write-Host "==> Preparing sqlx compile-time database..."
$Db = Join-Path (Get-Location) ".windows-build.db"
Remove-Item $Db -ErrorAction SilentlyContinue
$mig = @"
import sqlite3, sys, glob
con = sqlite3.connect(sys.argv[1])
for m in sorted(glob.glob('crates/mxc-store/migrations/*.sql')):
    con.executescript(open(m).read())
con.commit(); con.close()
"@
& $Python.Source -c $mig $Db
$env:DATABASE_URL = "sqlite://$($Db -replace '\\','/')"

# --- 3. Release build ----------------------------------------------------------------------
Write-Host "==> Building release binary (libsignal + SQLCipher from source - takes a while)..."
cargo build --release --locked -p mxc-app-qt
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

# --- 4. Stage + deploy Qt runtime ----------------------------------------------------------
Write-Host "==> Staging and running windeployqt..."
$Stage = "target\windows\monocles-chat-qt"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item target\release\monocles-chat-qt.exe $Stage\

# Copies the Qt DLLs, the QML modules our UI imports (--qmldir scans them), the image
# format plugins, and the QtWebEngine pieces (QtWebEngineProcess.exe, resources, locales).
& "$QtDir\bin\windeployqt.exe" --release --qmldir crates\mxc-app-qt\qml "$Stage\monocles-chat-qt.exe"
if ($LASTEXITCODE -ne 0) { throw "windeployqt failed" }

# --- 5. Zip --------------------------------------------------------------------------------
$Zip = "target\windows\monocles-chat-qt-$Version-windows-x64.zip"
Remove-Item $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Stage -DestinationPath $Zip
Write-Host ""
Write-Host "Built: $Zip"
Write-Host ""
Write-Host "NOTE: the GStreamer runtime (calls + voice messages) is NOT bundled."
Write-Host "      Target machines need the GStreamer MSVC x64 *runtime* installer from"
Write-Host "      https://gstreamer.freedesktop.org/download/ (or ship its DLLs alongside)."
