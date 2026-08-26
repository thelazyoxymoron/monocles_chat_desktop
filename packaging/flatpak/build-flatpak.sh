#!/usr/bin/env bash
# Build + install the monocles chat (Qt) Flatpak locally.
#
#   ./packaging/flatpak/build-flatpak.sh
#
# Prerequisites (see README.md):
#   - flatpak + flatpak-builder
#   - KDE runtime/SDK 6.10 from Flathub (installed automatically below)
set -euo pipefail
cd "$(dirname "$0")"

MANIFEST="de.monocles.chat.qt.yml"
BUILD_DIR="build-dir"
RUNTIME_VERSION="6.10"

if ! command -v flatpak-builder >/dev/null 2>&1; then
  echo "error: flatpak-builder not found."
  echo "  Debian:  sudo apt install flatpak-builder"
  echo "  Fedora:  sudo dnf install flatpak-builder"
  echo "  Or:      flatpak install -y flathub org.flatpak.Builder"
  exit 1
fi

# Ensure Flathub + the KDE runtime/SDK + the QtWebEngine BaseApp (WebXDC) are available
# (user install). The KDE runtime does not ship QtWebEngine; the BaseApp provides it.
flatpak remote-add --user --if-not-exists flathub \
  https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y flathub \
  "org.kde.Platform//${RUNTIME_VERSION}" "org.kde.Sdk//${RUNTIME_VERSION}" \
  "io.qt.qtwebengine.BaseApp//${RUNTIME_VERSION}"

# Build, install into the per-user installation (and export to a local OSTree repo so a
# distributable single-file bundle can be made), cleaning any prior state.
flatpak-builder --user --install --repo=repo --force-clean "${BUILD_DIR}" "${MANIFEST}"

# Single-file bundle for passing around (recipients double-click it or
# `flatpak install ./monocles-chat-qt-<version>.flatpak`; --runtime-repo lets their
# flatpak fetch the KDE runtime from Flathub automatically).
VERSION="$(grep -m1 '^version' ../../Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
BUNDLE="../../target/monocles-chat-qt-${VERSION}.flatpak"
flatpak build-bundle repo "${BUNDLE}" de.monocles.chat.qt master \
  --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo

echo
echo "Installed. Run with:  flatpak run de.monocles.chat.qt"
echo "Shareable bundle:     ${BUNDLE}"
echo "Check calls deps:     flatpak run --command=gst-inspect-1.0 de.monocles.chat.qt webrtcbin"
