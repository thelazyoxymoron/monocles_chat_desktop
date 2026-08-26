# Windows build for monocles chat (Qt)

> **Status: untested.** The Rust backend and the CXX-Qt UI are cross-platform, but this
> script has not yet been run on a real Windows machine. Expect to iterate on the first run.

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\build-windows.ps1
```

Run it **in a "x64 Native Tools" / "Developer PowerShell for VS" prompt** so MSVC is on
PATH. Output: `target\windows\monocles-chat-qt-<version>-windows-x64.zip` containing the
app folder (exe + Qt DLLs + QML modules + QtWebEngine, deployed by `windeployqt`).

## Prerequisites (install once)

| What | How |
| --- | --- |
| Visual Studio 2022 Build Tools (C++ workload) | `winget install Microsoft.VisualStudio.2022.BuildTools` |
| Qt 6.8+ for MSVC 2022 x64, **with the "Qt WebEngine" component** (WebXDC) | [Qt online installer](https://www.qt.io/download-qt-installer); set `QT_DIR` if not `C:\Qt\6.8.2\msvc2022_64` |
| GStreamer MSVC x64 — **both** runtime and development installers, **"Complete" setup type** (a "Typical" install omits the webrtc dev files the calls stack builds against) | <https://gstreamer.freedesktop.org/download/>; the installer sets `GSTREAMER_1_0_ROOT_MSVC_X86_64` |
| rustup (picks up the pinned nightly from `rust-toolchain.toml` automatically) | `winget install Rustlang.Rustup` |
| protoc (libsignal's `spqr` compiles protobufs in its build.rs) | `winget install protobuf` |
| Python 3 (builds the sqlx compile-time check DB) | `winget install Python.Python.3.12` |
| pkg-config (only if GStreamer's install doesn't ship one) | `choco install pkgconfiglite` |

## Known gaps / porting notes

- **Secret storage**: passwords, the SQLCipher DB key and the PQ OMEMO2 identity keys are
  stored in the OS keychain. `mxc-store/src/secrets.rs` selects the backend at compile
  time: `oo7` (freedesktop Secret Service) on Linux, and the `keyring` crate on Windows
  (Windows Credential Manager, via `CredWriteW`/`CredReadW`) and macOS. No extra system
  dependency is needed — `keyring`'s `windows-native` backend links `advapi32` from the
  Windows SDK that the MSVC toolchain already provides.
- **GStreamer runtime is not bundled** in the zip; target machines need the GStreamer
  MSVC x64 *runtime* installer, or ship `<gstreamer>\bin` + `lib\gstreamer-1.0`
  alongside the exe with `GST_PLUGIN_PATH` set.
- **Group calls on UDP-blocking networks**: the app uses the GStreamer MSVC build's `libnice`
  (`nice.dll` / `gstnice.dll`) and by default gathers UDP-only TURN relays for group (Muji) calls
  — no crash, but a UDP-blocked peer can't join a *group* call (1:1 still works). Stock libnice
  aborts in a group-call mesh on a slow TCP-relay pair (`priv_conn_check_tick_stream_nominate`
  assertion). To enable TCP/TLS-relay group calls you'd need a patched `nice.dll`: build libnice
  0.1.23 with the Meson/MSVC toolchain after applying `packaging/libnice/patch-conncheck.py`
  (`python3 packaging\libnice\patch-conncheck.py agent\conncheck.c`), drop the resulting
  `nice.dll` next to the exe (ahead of the GStreamer one on `PATH`), and set
  `MONOCLES_MUJI_TCP_RELAY=1` before launching. See `packaging/libnice/README.md`. Remove once
  the shipped GStreamer's libnice has the upstream fix.
- **OpenSSL**: `openssl-sys` needs OpenSSL; if the build fails there, either
  `winget install ShiningLight.OpenSSL` and set `OPENSSL_DIR`, or enable the crate's
  `vendored` feature.
- No installer yet — the zip is portable. An Inno Setup / WiX installer and code signing
  can be added later.
