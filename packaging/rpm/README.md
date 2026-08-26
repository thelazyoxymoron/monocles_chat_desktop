# Fedora RPM for monocles chat (Qt)

Run **on a Fedora machine** (the binary links Fedora's Qt/GStreamer, so it can't be
cross-built from Debian):

```sh
./packaging/rpm/build-rpm.sh
```

The script installs build deps via dnf, the pinned Rust nightly via rustup (see
`rust-toolchain.toml` — required by the vendored libsignal), builds the release binary,
and packages it with `packaging/rpm/monocles-chat-qt.spec`. Result, e.g.:

```
monocles-chat-qt-0.1.0-1.fc42.x86_64.rpm
sudo dnf install ./monocles-chat-qt-0.1.0-1.fc42.x86_64.rpm
```

Note: `deps/libsignal` must contain the vendored libsignal checkout — clone the monocles
`pq-omemo-2` fork into it on the Fedora machine too (see the top-level README):
`git clone https://codeberg.org/monocles/pq-omemo-2.git deps/libsignal`.

To ship a new build of the same upstream version, bump `Release:` in the spec (plus a
`%changelog` entry); for a new version, bump `[workspace.package] version` in `Cargo.toml`.

## Group calls + bundled patched libnice

Stock libnice aborts in a group-call (Muji) mesh on a slow TCP-relay candidate pair
(`priv_conn_check_tick_stream_nominate` assertion), which is needed when a participant's network
blocks UDP. So **by default `build-rpm.sh` bundles a patched libnice**: it builds libnice 0.1.23
with a graceful-nomination fix (`packaging/libnice/patch-conncheck.py`), installs it into the
private libdir `%{_libdir}/monocles-chat-qt`, and ships `%{_bindir}/monocles-chat-qt` as a launcher
wrapper that puts it on `LD_LIBRARY_PATH` and sets `MONOCLES_MUJI_TCP_RELAY=1`. The system
`libnice-gstreamer1` plugin then loads the bundled `libnice.so.10` (same SONAME). The private copy
is excluded from RPM auto-Provides so it can't satisfy other packages' libnice deps.

Extra build deps for the bundled libnice (build-rpm.sh installs them via dnf):
`meson ninja-build gcc pkgconf glib2-devel gnutls-devel git python3`.

To build a plain package on Fedora's own libnice instead (group calls then use UDP-only relays —
no crash, but UDP-blocked peers can't join group calls):

```sh
BUNDLE_LIBNICE=0 ./packaging/rpm/build-rpm.sh
```

Drop the bundling once Fedora's libnice carries the upstream fix — see `packaging/libnice/README.md`.
