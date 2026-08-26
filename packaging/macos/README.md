# macOS build for monocles chat (Qt)

> **Status: untested.** The Rust backend and the CXX-Qt UI are cross-platform, but this
> script has not yet been run on a real Mac. Expect to iterate on the first run.

```sh
./packaging/macos/build-macos.sh
```

Run it **on a Mac** (Apple Silicon or Intel; builds for the host arch). It installs the
toolchain via Homebrew + rustup, builds the release binary, assembles
`target/macos/monocles chat.app` and a compressed
`target/macos/monocles-chat-qt-<version>-macos-<arch>.dmg`.

## Prerequisites

- Xcode Command Line Tools: `xcode-select --install`
- [Homebrew](https://brew.sh)

Everything else is installed by the script: `qt` (full Qt 6 incl. QtWebEngine for WebXDC
and `macdeployqt`), `gstreamer` (calls + voice messages), `protobuf` (libsignal's `spqr`
compiles protobufs in its build.rs), `openssl@3`, `pkgconf`, `cmake`, and the pinned Rust
nightly from `rust-toolchain.toml` via rustup.

## Known gaps / porting notes

- **Secret storage**: passwords, the SQLCipher DB key and the PQ OMEMO2 identity keys are
  stored in the OS keychain. `mxc-store/src/secrets.rs` selects the backend at compile
  time: `oo7` (freedesktop Secret Service over D-Bus) on Linux, and the `keyring` crate
  (macOS Keychain Services / Windows Credential Manager) elsewhere. On macOS the first
  store/retrieve will pop the standard Keychain access prompt for the app. No system
  dependency is required — `keyring`'s `apple-native` backend links `Security.framework`,
  which the SDK already provides.
- **GStreamer plugins** are dlopen'd at runtime and not bundled by `macdeployqt`; the
  built `.app` works on machines with `brew install gstreamer`. For standalone
  distribution, bundle the official GStreamer.framework or vendor the plugin set.
- **Group calls on UDP-blocking networks**: the app uses Homebrew's libnice and by default
  gathers UDP-only TURN relays for group (Muji) calls — no crash, but a UDP-blocked peer can't
  join a *group* call (1:1 still works). Stock libnice aborts in a group-call mesh on a slow
  TCP-relay pair (`priv_conn_check_tick_stream_nominate` assertion). To enable TCP/TLS-relay
  group calls, build a patched libnice and run with it + `MONOCLES_MUJI_TCP_RELAY=1`:
  ```sh
  brew install meson ninja pkgconf glib gnutls
  git clone --branch 0.1.23 --depth 1 https://gitlab.freedesktop.org/libnice/libnice.git
  ( cd libnice && python3 ../packaging/libnice/patch-conncheck.py agent/conncheck.c \
    && meson setup _b --prefix="$PWD/_prefix" --libdir=lib --buildtype=release \
       -Dgstreamer=disabled -Dtests=disabled -Dexamples=disabled -Dintrospection=disabled -Dgtk_doc=disabled \
    && ninja -C _b install )
  DYLD_LIBRARY_PATH="$PWD/libnice/_prefix/lib" MONOCLES_MUJI_TCP_RELAY=1 ./target/release/monocles-chat-qt
  ```
  Note: macOS SIP ignores `DYLD_*` for system binaries but honours it for your own ad-hoc-signed
  build. For a bundled `.app`, ship the patched `libnice.dylib` in `Contents/Frameworks` and set
  the env var in a launcher. See `packaging/libnice/README.md`.
- **Signing**: the script applies an ad-hoc `codesign`. For distribution outside your
  machine you need a Developer ID Application certificate + notarization
  (`xcrun notarytool submit`).
