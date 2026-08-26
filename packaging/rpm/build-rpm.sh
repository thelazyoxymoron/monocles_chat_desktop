#!/usr/bin/env bash
# Build a Fedora .rpm for the monocles chat Qt client. RUN THIS ON A FEDORA MACHINE —
# the binary links Fedora's Qt/GStreamer libs, so it can't be cross-built from Debian.
#
#   ./packaging/rpm/build-rpm.sh
#
# It installs the build dependencies (via dnf), the pinned Rust nightly (via rustup), builds
# the release binary, then packages it with rpmbuild using packaging/rpm/monocles-chat-qt.spec.
# The result is copied to the repo root and printed at the end.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

PKG=monocles-chat-qt
VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
echo "==> Building $PKG $VERSION for Fedora"

# 1. Build dependencies. CXX-Qt needs the Qt6 dev headers + qmake; libsignal's PQ dep `spqr`
#    compiles protobufs in its build.rs (needs protoc); SQLCipher is bundled (needs only
#    openssl-devel). GStreamer dev: core + base + bad-free-devel — the latter provides
#    gstreamer-webrtc-1.0.pc, without which gstreamer-webrtc-sys's build script fails.
#    The GStreamer *runtime* plugins are pulled at install time via the spec's Requires.
#    lld: CXX-Qt's link step fails with GNU ld.bfd on Fedora ("'cc' linking failed"; Fedora
#    dropped ld.gold too) — its build probe auto-switches to `-fuse-ld=lld` when lld exists.
echo "==> Installing build dependencies (sudo dnf)…"
sudo dnf install -y \
    gcc gcc-c++ make cmake pkgconf-pkg-config lld \
    qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qtwebengine-devel \
    gstreamer1-devel gstreamer1-plugins-base-devel gstreamer1-plugins-bad-free-devel \
    openssl-devel protobuf-compiler python3 rpm-build desktop-file-utils curl

# 2. Rust toolchain — rust-toolchain.toml pins the exact nightly the vendored libsignal needs,
#    so `cargo` auto-installs/selects it. Install rustup if it's missing.
if ! command -v rustup >/dev/null 2>&1 && ! [ -x "$HOME/.cargo/bin/rustup" ]; then
    echo "==> Installing rustup…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
fi
export PATH="$HOME/.cargo/bin:$PATH"

# CXX-Qt locates Qt through qmake; Fedora installs it as qmake6.
export QMAKE="$(command -v qmake6 || command -v qmake)"

# Link with lld (installed above): GNU ld.bfd fails on Fedora for CXX-Qt binaries.
# Setting it via RUSTFLAGS (instead of relying on CXX-Qt's lld autodetection) also
# invalidates cargo's fingerprint, so a retry after a failed bfd link rebuilds cleanly
# instead of reusing the cached no-lld linker args.
export RUSTFLAGS="-C link-arg=-fuse-ld=lld ${RUSTFLAGS:-}"

# 3. sqlx compile-time check DB, built from the migrations (no machine-specific DATABASE_URL,
#    no committed .sqlx/ cache). Uses python3's sqlite3 module. Exported env wins over the
#    git-ignored .cargo/config.toml.
echo "==> Preparing sqlx compile-time database…"
DB="$PWD/.rpm-build.db"
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

# 4. Build the release binary (QML + icons are embedded as Qt resources via build.rs).
echo "==> Building release binary (this takes a while — libsignal + SQLCipher from source)…"
cargo build --release --locked -p mxc-app-qt
strip target/release/monocles-chat-qt 2>/dev/null || true

# 5. Stage the prebuilt binary + data into an rpmbuild tree and package.
echo "==> Assembling RPM…"
TOP="$PWD/target/rpmbuild"
rm -rf "$TOP"
mkdir -p "$TOP"/{SOURCES,SPECS,BUILD,BUILDROOT,RPMS,SRPMS}
install -m0755 target/release/monocles-chat-qt        "$TOP/SOURCES/monocles-chat-qt"
install -m0644 data/de.monocles.chat.qt.desktop       "$TOP/SOURCES/"
install -m0644 data/de.monocles.chat.qt.metainfo.xml  "$TOP/SOURCES/"
install -m0644 data/icons/de.monocles.chat.qt.svg     "$TOP/SOURCES/"
install -m0644 data/icons/de.monocles.chat.qt.png     "$TOP/SOURCES/"
install -m0644 packaging/rpm/monocles-chat-qt.spec    "$TOP/SPECS/"

# 5b. Bundle a patched libnice + launcher wrapper so group calls work over TCP/TLS relays without
#     the libnice mesh-nomination crash (set BUNDLE_LIBNICE=0 to skip → plain package on Fedora's
#     libnice, UDP-only group calls). Build deps: meson ninja-build gcc pkgconf glib2-devel
#     gnutls-devel git python3.
BUNDLE_LIBNICE="${BUNDLE_LIBNICE:-1}"
RPM_DEFINES=(--define "_topdir $TOP" --define "appversion $VERSION")
if [ "$BUNDLE_LIBNICE" = 1 ]; then
    echo "==> Building + staging patched libnice"
    sudo dnf install -y meson ninja-build glib2-devel gnutls-devel || true
    LNBUILD="$PWD/target/rpm-libnice-prefix"
    rm -rf "$LNBUILD"
    PREFIX="$LNBUILD" packaging/libnice/build-patched-libnice.sh
    mkdir -p "$TOP/SOURCES/libnice"
    cp -a "$LNBUILD/lib/"libnice.so.10* "$TOP/SOURCES/libnice/"
    # strip the bundled lib (no -debuginfo subpackage; debug_package is nil).
    strip --strip-unneeded "$TOP/SOURCES/libnice/"libnice.so.10.* 2>/dev/null || true
    cat > "$TOP/SOURCES/monocles-chat-qt.sh" <<'WRAP'
#!/bin/sh
# monocles chat (Qt) launcher: load the bundled patched libnice (graceful ICE nomination so group
# calls don't abort on TCP-relay pairs in a mesh) and enable TCP/TLS relays for group calls.
PRIV=/usr/lib64/monocles-chat-qt
[ -d "$PRIV" ] || PRIV=/usr/lib/monocles-chat-qt
export LD_LIBRARY_PATH="$PRIV:${LD_LIBRARY_PATH:-}"
export MONOCLES_MUJI_TCP_RELAY=1
exec "$PRIV/monocles-chat-qt" "$@"
WRAP
    chmod 0755 "$TOP/SOURCES/monocles-chat-qt.sh"
    RPM_DEFINES+=(--define "bundle_libnice 1")
fi

rpmbuild "${RPM_DEFINES[@]}" -bb "$TOP/SPECS/monocles-chat-qt.spec"

RPM_OUT="$(find "$TOP/RPMS" -name '*.rpm' | head -1)"
cp "$RPM_OUT" .
echo
echo "Built: $(basename "$RPM_OUT")  (copied to repo root)"
echo "Install with:  sudo dnf install ./$(basename "$RPM_OUT")"
