#!/usr/bin/env bash
# Run monocles chat (Qt) against the patched libnice built by
# packaging/libnice/build-patched-libnice.sh, with TCP/TLS TURN relays enabled for group calls.
#
# Uses `cargo run` so it ALWAYS builds the current sources (defaults to the debug profile, exactly
# like `cargo run`; pass --release for a release build). Anything after the profile flag is passed
# through to the app.
#
# Usage:
#   scripts/run-patched.sh [--debug|--release] [-- extra app args...]
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local/monocles-libnice}"
if [ ! -e "$PREFIX/lib/libnice.so.10" ]; then
  echo "Patched libnice not found at $PREFIX/lib/libnice.so.10"
  echo "Build it first:  packaging/libnice/build-patched-libnice.sh"
  exit 1
fi

PROFILE_FLAG=""   # debug by default (matches `cargo run`)
case "${1:-}" in
  --release) PROFILE_FLAG="--release"; shift ;;
  --debug)   shift ;;
esac
# Allow an explicit `--` separator before app args.
[ "${1:-}" = "--" ] && shift

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# cxx-qt-build 0.9 auto-detects Qt by matching a PATH qmake against a version range and, on a
# miss, tries to DOWNLOAD Qt. Point QMAKE at the system qmake6 so the installed Qt is used
# directly (the packaging scripts and .cargo/config.toml do the same).
export QMAKE="${QMAKE:-$(command -v qmake6 || command -v qmake || true)}"
export LD_LIBRARY_PATH="$PREFIX/lib:${LD_LIBRARY_PATH:-}"
export MONOCLES_MUJI_TCP_RELAY=1
echo "==> Using patched libnice: $PREFIX/lib  (MONOCLES_MUJI_TCP_RELAY=1)"
echo "==> cargo run ${PROFILE_FLAG:-(debug)} -p mxc-app-qt"
exec cargo run $PROFILE_FLAG -p mxc-app-qt -- "$@"
