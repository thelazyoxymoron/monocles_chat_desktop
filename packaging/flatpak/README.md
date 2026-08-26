# Flatpak for monocles chat (Qt)

Local build + install:

```sh
./packaging/flatpak/build-flatpak.sh
flatpak run de.monocles.chat.qt
```

Uses `org.kde.Platform//6.10` (bundles Qt 6 + QML modules, QtSvg, the WebP image plugin
and a GStreamer set) on top of `io.qt.qtwebengine.BaseApp//6.10` (QtWebEngine for WebXDC —
the KDE runtime doesn't ship it). This is a **network build**: rustup fetches the pinned nightly
(`rust-toolchain.toml`, required by the vendored libsignal), cargo fetches crates, and a
prebuilt `protoc` is downloaded (libsignal's `spqr` compiles protobufs in its build.rs).

Quirks handled in the manifest (don't remove):

- `deps/libsignal` is a symlink out of the repo; flatpak-builder preserves symlinks, so the
  manifest skips it and copies the real tree via a second `dir` source (only the Rust
  workspace — java/node/swift/target are skipped).
- The machine-specific `.cargo/config.toml` (DATABASE_URL) is skipped; the sqlx
  compile-time DB is built in-sandbox from `crates/mxc-store/migrations/*.sql`.
- `QMAKE` is exported so CXX-Qt finds the SDK's Qt.
- QtWebEngine lives under `/app` (BaseApp), invisible to the SDK's qmake — `CXXFLAGS`/
  `RUSTFLAGS` add `/app/include` + `/app/lib` so the WebXDC shim compiles and links;
  `QTWEBENGINEPROCESS_PATH` in finish-args points at the BaseApp's renderer process, and
  `/app/cleanup-BaseApp.sh` strips the BaseApp's dev files from the final app.

## Group calls (Muji) + patched libnice

The manifest builds a **patched libnice** (`libnice` module) into `/app` and sets
`--env=MONOCLES_MUJI_TCP_RELAY=1`. Stock libnice aborts the process in a group-call mesh once a
slow TCP-relay candidate pair is in flight (the `priv_conn_check_tick_stream_nominate` assertion);
the patch (`packaging/libnice/patch-conncheck.py`) turns that into a graceful skip. `/app/lib`
precedes the runtime's `/usr/lib`, so the runtime's `libgstnice.so` (webrtcbin) loads the bundled
`libnice.so.10` — same SONAME, no plugin rebuild. This is what lets group calls work even for a
participant whose network blocks UDP (forcing TCP/TLS relay). Remove the module + the env var once
the runtime ships a libnice with the upstream fix. See `packaging/libnice/README.md`.

After building, verify the calls stack inside the sandbox:

```sh
flatpak run --command=gst-inspect-1.0 de.monocles.chat.qt webrtcbin
# confirm the bundled libnice is the one loaded:
flatpak run --command=sh de.monocles.chat.qt -c \
  'ldd $(gst-inspect-1.0 nice | awk "/Filename/{print \$2}") | grep libnice'
```

**Flathub TODO** (offline build): vendor cargo sources via flatpak-cargo-generator, commit
`.sqlx/` (`cargo sqlx prepare`) + `SQLX_OFFLINE=true`, bundle protoc as a source, and drop
`--share=network`.
