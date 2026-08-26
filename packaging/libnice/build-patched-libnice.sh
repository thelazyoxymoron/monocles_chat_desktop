#!/usr/bin/env bash
# Build a patched libnice for monocles chat (Qt) desktop group calls.
#
# WHY: stock libnice (0.1.22 in Debian trixie, also 0.1.23) aborts the whole process with
#   libnice:ERROR ../agent/conncheck.c: priv_conn_check_tick_stream_nominate:
#   assertion failed: (p->state == NICE_CHECK_SUCCEEDED)
# in a Muji (XEP-0272) group-call mesh: once several NiceAgents run ICE connectivity checks at
# once and a slow TCP-relay pair (TURN over TCP/TLS, used when a peer's network blocks UDP) is
# still handshaking when the controlling agent's nominate tick fires, libnice asserts and calls
# abort(). A native abort() can't be caught from the app.
#
# This builds libnice with that fatal assertion turned into a graceful skip (don't nominate a
# not-yet-succeeded pair this tick — reconsider it once it succeeds), so the desktop can use
# TCP/TLS TURN relays in group calls without crashing. Run the app against this libnice and set
# MONOCLES_MUJI_TCP_RELAY=1 (see packaging/libnice/README.md) to re-enable TCP relays for Muji.
#
# The system GStreamer `nice` plugin (libgstnice.so) DT_NEEDEDs libnice.so.10; pointing
# LD_LIBRARY_PATH at this build makes webrtcbin load the patched libnice — no plugin rebuild
# needed (same SONAME / ABI within libnice.so.10).
set -euo pipefail

LIBNICE_VERSION="${LIBNICE_VERSION:-0.1.23}"
PREFIX="${PREFIX:-$HOME/.local/monocles-libnice}"
REPO="${LIBNICE_REPO:-https://gitlab.freedesktop.org/libnice/libnice.git}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$(mktemp -d)"
trap 'rm -rf "$BUILD_DIR"' EXIT

echo "==> Building patched libnice $LIBNICE_VERSION -> $PREFIX"

missing=0
for t in git meson ninja pkg-config cc python3; do
  command -v "$t" >/dev/null 2>&1 || { echo "  missing build tool: $t"; missing=1; }
done
if [ "$missing" = 1 ]; then
  cat <<'EOF'
Install the build dependencies first, e.g. on Debian/Ubuntu:
  sudo apt install build-essential meson ninja-build pkg-config git \
       libglib2.0-dev libgnutls28-dev
EOF
  exit 1
fi

git clone --branch "$LIBNICE_VERSION" --depth 1 "$REPO" "$BUILD_DIR/libnice"
cd "$BUILD_DIR/libnice"

echo "==> Applying graceful-nomination patch to agent/conncheck.c"
# Centralised, idempotent patch (exits non-zero if it can't match → no unpatched build).
python3 "$SCRIPT_DIR/patch-conncheck.py" agent/conncheck.c

echo "==> Configuring + building (release)"
meson setup build \
  --prefix="$PREFIX" \
  --libdir=lib \
  --buildtype=release \
  -Dgstreamer=disabled \
  -Dtests=disabled \
  -Dexamples=disabled \
  -Dintrospection=disabled \
  -Dgtk_doc=disabled
ninja -C build
ninja -C build install

cat <<EOF

==> Done. Patched libnice installed to: $PREFIX/lib

Run the desktop app against it (re-enables TCP/TLS relays for group calls). Prefer the helper —
it uses 'cargo run' so it always builds the current sources:

  scripts/run-patched.sh              # debug (like 'cargo run'); --release for a release build

Equivalent manual command (note: build first so you don't run a stale binary):

  LD_LIBRARY_PATH="$PREFIX/lib:\${LD_LIBRARY_PATH:-}" \\
  MONOCLES_MUJI_TCP_RELAY=1 \\
  cargo run -p mxc-app-qt

Verify webrtcbin is using this libnice:
  LD_LIBRARY_PATH="$PREFIX/lib" ldd \$(gst-inspect-1.0 nice 2>/dev/null | awk '/Filename/{print \$2}') | grep libnice
EOF
