#!/usr/bin/env bash
#
# ThroughWaves Release Builder
# Builds platform-specific installers/packages for distribution.
#
# Usage:
#   ./scripts/build-release.sh           # auto-detect current OS
#   ./scripts/build-release.sh macos     # macOS .dmg
#   ./scripts/build-release.sh windows   # Windows .msi (via cargo-wix)
#   ./scripts/build-release.sh linux     # Linux .AppImage + .deb
#   ./scripts/build-release.sh all       # all platforms (CI)

set -euo pipefail

VERSION="1.0.0"
APP_NAME="ThroughWaves"
BINARY="jamhub-app"
DIST_DIR="dist"

mkdir -p "$DIST_DIR"

echo "=== ThroughWaves Release Builder v${VERSION} ==="

build_release() {
    echo "[1/4] Building release binary..."
    cargo build --release --bin "$BINARY"
    echo "  Binary: target/release/$BINARY"
}

#
# ──── macOS ────
#
build_macos() {
    echo ""
    echo "=== Building macOS Package ==="
    build_release

    local APP_BUNDLE="${APP_NAME}.app"
    local DMG_NAME="${APP_NAME}-${VERSION}-macOS.dmg"

    # Create .app bundle
    echo "[2/4] Creating .app bundle..."
    mkdir -p "${APP_BUNDLE}/Contents/MacOS"
    mkdir -p "${APP_BUNDLE}/Contents/Resources"
    cp "target/release/${BINARY}" "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"
    chmod +x "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"

    # Info.plist already exists in repo
    echo "  Bundle: ${APP_BUNDLE}"

    # Create DMG
    echo "[3/4] Creating DMG installer..."
    local DMG_TMP="${DIST_DIR}/${APP_NAME}-tmp.dmg"
    local DMG_FINAL="${DIST_DIR}/${DMG_NAME}"

    # Create a temporary DMG
    rm -f "$DMG_TMP" "$DMG_FINAL"
    hdiutil create -size 200m -fs HFS+ -volname "$APP_NAME" "$DMG_TMP" -quiet

    # Mount, copy app, add Applications symlink
    local MOUNT_DIR=$(hdiutil attach "$DMG_TMP" -quiet | tail -1 | awk '{print $NF}')
    # Handle multi-word mount point
    MOUNT_DIR=$(hdiutil attach "$DMG_TMP" -quiet | grep "/Volumes" | sed 's/.*\/Volumes/\/Volumes/')
    cp -R "${APP_BUNDLE}" "${MOUNT_DIR}/"
    ln -s /Applications "${MOUNT_DIR}/Applications"

    # Unmount
    hdiutil detach "$MOUNT_DIR" -quiet 2>/dev/null || true
    sleep 1

    # Convert to compressed DMG
    hdiutil convert "$DMG_TMP" -format UDZO -o "$DMG_FINAL" -quiet
    rm -f "$DMG_TMP"

    local SIZE=$(du -sh "$DMG_FINAL" | awk '{print $1}')
    echo "[4/4] Done!"
    echo "  Installer: $DMG_FINAL ($SIZE)"
    echo ""
}

#
# ──── Windows ────
#
build_windows() {
    echo ""
    echo "=== Building Windows Package ==="
    build_release

    local EXE="target/release/${BINARY}.exe"
    local ZIP_NAME="${APP_NAME}-${VERSION}-Windows.zip"

    echo "[2/4] Packaging Windows executable..."

    # Create a distribution folder
    local WIN_DIR="${DIST_DIR}/windows/${APP_NAME}"
    rm -rf "${WIN_DIR}"
    mkdir -p "${WIN_DIR}"

    cp "$EXE" "${WIN_DIR}/${APP_NAME}.exe"

    # Create a simple launcher batch file
    cat > "${WIN_DIR}/README.txt" << 'WINEOF'
ThroughWaves - Professional Digital Audio Workstation

To start ThroughWaves, double-click ThroughWaves.exe

System Requirements:
  - Windows 10 or later (64-bit)
  - 4GB RAM minimum, 8GB recommended
  - Audio interface recommended for recording

For VST3 plugins, place .vst3 files in:
  C:\Program Files\Common Files\VST3\
WINEOF

    # Create ZIP
    echo "[3/4] Creating ZIP archive..."
    (cd "${DIST_DIR}/windows" && zip -r "../${ZIP_NAME}" "${APP_NAME}/")
    rm -rf "${DIST_DIR}/windows"

    local SIZE=$(du -sh "${DIST_DIR}/${ZIP_NAME}" | awk '{print $1}')
    echo "[4/4] Done!"
    echo "  Installer: ${DIST_DIR}/${ZIP_NAME} ($SIZE)"
    echo ""
}

#
# ──── Linux ────
#
build_linux() {
    echo ""
    echo "=== Building Linux Packages ==="
    build_release

    local BIN="target/release/${BINARY}"
    local DEB_NAME="${APP_NAME,,}-${VERSION}-linux-amd64.deb"
    local APPIMAGE_NAME="${APP_NAME}-${VERSION}-linux-x86_64.AppImage"
    local TAR_NAME="${APP_NAME}-${VERSION}-linux-x86_64.tar.gz"

    # ── .deb package ──
    echo "[2/4] Creating .deb package..."
    local DEB_DIR="${DIST_DIR}/deb"
    rm -rf "$DEB_DIR"
    mkdir -p "${DEB_DIR}/DEBIAN"
    mkdir -p "${DEB_DIR}/usr/bin"
    mkdir -p "${DEB_DIR}/usr/share/applications"
    mkdir -p "${DEB_DIR}/usr/share/icons/hicolor/256x256/apps"

    cp "$BIN" "${DEB_DIR}/usr/bin/jamhub"
    chmod +x "${DEB_DIR}/usr/bin/jamhub"

    # Control file
    cat > "${DEB_DIR}/DEBIAN/control" << DEBEOF
Package: jamhub
Version: ${VERSION}
Section: sound
Priority: optional
Architecture: amd64
Depends: libasound2, libgl1, libx11-6, libxcursor1, libxrandr2, libxi6
Maintainer: ThroughWaves Team <hello@throughwaves.app>
Description: Professional Digital Audio Workstation
 ThroughWaves is a full-featured DAW with VST3 hosting, MIDI sequencing,
 AI stem separation, built-in effects, live jam sessions, and a
 collaborative music platform.
Homepage: https://throughwaves.app
DEBEOF

    # Desktop entry
    cat > "${DEB_DIR}/usr/share/applications/jamhub.desktop" << DSKEOF
[Desktop Entry]
Type=Application
Name=ThroughWaves
GenericName=Digital Audio Workstation
Comment=Professional DAW with collaboration features
Exec=jamhub
Icon=jamhub
Terminal=false
Categories=AudioVideo;Audio;Music;Sequencer;Midi;
Keywords=daw;audio;music;recording;midi;vst;
MimeType=audio/wav;audio/flac;audio/ogg;audio/mpeg;
DSKEOF

    dpkg-deb --build "$DEB_DIR" "${DIST_DIR}/${DEB_NAME}" 2>/dev/null || echo "  (dpkg-deb not available — skipping .deb)"
    rm -rf "$DEB_DIR"

    # ── AppImage structure ──
    echo "[3/4] Creating AppImage structure..."
    local AI_DIR="${DIST_DIR}/AppDir"
    rm -rf "$AI_DIR"
    mkdir -p "${AI_DIR}/usr/bin"
    mkdir -p "${AI_DIR}/usr/share/applications"
    mkdir -p "${AI_DIR}/usr/share/icons/hicolor/256x256/apps"

    cp "$BIN" "${AI_DIR}/usr/bin/jamhub"
    chmod +x "${AI_DIR}/usr/bin/jamhub"
    cp "${DEB_DIR}/usr/share/applications/jamhub.desktop" "${AI_DIR}/usr/share/applications/" 2>/dev/null || true
    cp "${AI_DIR}/usr/share/applications/jamhub.desktop" "${AI_DIR}/" 2>/dev/null || true

    # AppRun script
    cat > "${AI_DIR}/AppRun" << 'APPEOF'
#!/bin/bash
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/jamhub" "$@"
APPEOF
    chmod +x "${AI_DIR}/AppRun"

    # Create tar.gz as universal Linux package
    echo "[3/4] Creating tar.gz archive..."
    local TAR_DIR="${DIST_DIR}/tar/${APP_NAME}"
    rm -rf "${DIST_DIR}/tar"
    mkdir -p "$TAR_DIR"
    cp "$BIN" "${TAR_DIR}/jamhub"
    chmod +x "${TAR_DIR}/jamhub"
    cat > "${TAR_DIR}/README.txt" << 'TAREOF'
ThroughWaves - Professional Digital Audio Workstation

To start ThroughWaves:
  chmod +x jamhub
  ./jamhub

System Requirements:
  - Linux x86_64 (Ubuntu 22.04+, Fedora 38+, Arch)
  - ALSA or PulseAudio
  - 4GB RAM minimum, 8GB recommended

For VST3 plugins, place .vst3 bundles in:
  ~/.vst3/
  /usr/lib/vst3/
TAREOF
    (cd "${DIST_DIR}/tar" && tar -czf "../${TAR_NAME}" "${APP_NAME}/")
    rm -rf "${DIST_DIR}/tar" "$AI_DIR"

    echo "[4/4] Done!"
    echo "  Packages in ${DIST_DIR}/"
    echo ""
}

# ──── Detect platform or use argument ────
PLATFORM="${1:-auto}"

if [ "$PLATFORM" = "auto" ]; then
    case "$(uname -s)" in
        Darwin*) PLATFORM="macos" ;;
        Linux*)  PLATFORM="linux" ;;
        MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
        *) echo "Unknown platform: $(uname -s)"; exit 1 ;;
    esac
fi

case "$PLATFORM" in
    macos)   build_macos ;;
    windows) build_windows ;;
    linux)   build_linux ;;
    all)     build_macos; build_linux; build_windows ;;
    *)       echo "Usage: $0 [macos|windows|linux|all]"; exit 1 ;;
esac

echo "=== Release build complete ==="
ls -lh "${DIST_DIR}/"*.{dmg,zip,deb,tar.gz,AppImage} 2>/dev/null || true
