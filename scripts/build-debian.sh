#!/usr/bin/env bash
set -euo pipefail

# build-debian.sh - Build a .deb package and an AppImage for Maolan Editor on Debian.
#
# Usage:
#   ./scripts/build-debian.sh [OPTIONS]
#
# Options:
#   -s, --source-dir DIR     Path to maolan-editor source directory (default: parent of this script)
#   -o, --output-dir DIR     Where to write package files (default: ./dist)
#   -v, --version VERSION    Override package version (default: read from Cargo.toml)
#   -t, --target-dir DIR     Local target directory (useful when source is on NFS)
#   -h, --help               Show this help message
#
# The script installs build dependencies via apt, installs Rust via rustup if missing,
# builds the release binary, and produces a .deb package and an AppImage using linuxdeploy.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$SOURCE_DIR/dist"
OVERRIDE_VERSION=""
TARGET_DIR=""

usage() {
    sed -n '4,15p' "$0" | sed -e 's/^# //' -e 's/^#$//'
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -s|--source-dir)
            SOURCE_DIR="$(realpath "$2")"
            shift 2
            ;;
        -o|--output-dir)
            OUTPUT_DIR="$(realpath "$2")"
            shift 2
            ;;
        -v|--version)
            OVERRIDE_VERSION="$2"
            shift 2
            ;;
        -t|--target-dir)
            TARGET_DIR="$(realpath "$2")"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

CARGO_TOML="$SOURCE_DIR/Cargo.toml"
if [[ ! -f "$CARGO_TOML" ]]; then
    echo "Error: Cargo.toml not found at $CARGO_TOML" >&2
    exit 1
fi

if [[ -n "$OVERRIDE_VERSION" ]]; then
    PKG_VERSION="$OVERRIDE_VERSION"
else
    PKG_VERSION="$(grep -m1 '^version' "$CARGO_TOML" | sed 's/.*= *"\(.*\)".*/\1/')"
fi

DEB_ARCH="$(dpkg --print-architecture)"
PKG_NAME="maolan-editor"
DEB_NAME="${PKG_NAME}-${PKG_VERSION}-debian.${DEB_ARCH}.deb"
APPIMAGE_NAME="${PKG_NAME}-${PKG_VERSION}-x86_64.AppImage"

pipewire_installed() {
    dpkg-query -W -f='${Status}' pipewire 2>/dev/null | grep -q "install ok installed"
}

if pipewire_installed; then
    JACK_RUNTIME_PACKAGE="pipewire-jack"
    JACK_BUILD_PACKAGES=(pipewire-jack libjack-jackd2-dev)
    JACK_PROVIDER_LABEL="PipeWire JACK"
else
    JACK_RUNTIME_PACKAGE="jackd2"
    JACK_BUILD_PACKAGES=(jackd2 libjack-jackd2-dev)
    JACK_PROVIDER_LABEL="JACK"
fi

if apt-cache show libfuse2t64 >/dev/null 2>&1; then
    FUSE_PACKAGE="libfuse2t64"
else
    FUSE_PACKAGE="libfuse2"
fi

if apt-cache show libasound2t64 >/dev/null 2>&1; then
    ALSA_RUNTIME_PACKAGE="libasound2t64"
else
    ALSA_RUNTIME_PACKAGE="libasound2"
fi

echo "========================================"
echo "Building Maolan Editor .deb package and AppImage"
echo "Version: $PKG_VERSION"
echo "Architecture: $DEB_ARCH"
echo "Source: $SOURCE_DIR"
echo "Deb output: $OUTPUT_DIR/$DEB_NAME"
echo "AppImage output: $OUTPUT_DIR/$APPIMAGE_NAME"
echo "JACK provider: $JACK_PROVIDER_LABEL"
echo "========================================"

echo ""
echo "[1/7] Installing build dependencies..."
sudo apt-get update
sudo apt-get install -y \
    pkg-config \
    build-essential \
    "${JACK_BUILD_PACKAGES[@]}" \
    libasound2-dev \
    libxkbcommon-dev \
    "$FUSE_PACKAGE" \
    fakeroot \
    curl \
    ca-certificates \
    git

echo ""
echo "[2/7] Checking Rust toolchain..."
if ! command -v cargo &>/dev/null; then
    echo "Rust not found. Installing via rustup..."
    export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
    export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$CARGO_HOME/env"
else
    echo "Rust already installed: $(rustc --version)"
fi

if [[ -f "${CARGO_HOME:-$HOME/.cargo}/env" ]]; then
    source "${CARGO_HOME:-$HOME/.cargo}/env"
fi

echo ""
echo "[3/7] Building release binary..."
cd "$SOURCE_DIR"

CARGO_ARGS=("--release" "--all-targets")
if [[ -n "$TARGET_DIR" ]]; then
    mkdir -p "$TARGET_DIR"
    CARGO_ARGS+=("--target-dir" "$TARGET_DIR")
    echo "Using local target directory: $TARGET_DIR"
fi

cargo build "${CARGO_ARGS[@]}"

if [[ -n "$TARGET_DIR" ]]; then
    BIN_DIR="$TARGET_DIR/release"
else
    BIN_DIR="$SOURCE_DIR/target/release"
fi

if [[ ! -f "$BIN_DIR/maolan-editor" ]]; then
    echo "Error: Binary '$BIN_DIR/maolan-editor' not found after build" >&2
    exit 1
fi

echo "Build completed successfully."

echo ""
echo "[4/7] Preparing Debian package structure..."

STAGING_DIR="$(mktemp -d)"
APPDIR_BASE="$(mktemp -d)"
trap "rm -rf '$STAGING_DIR' '$APPDIR_BASE'" EXIT

mkdir -p "$STAGING_DIR/DEBIAN"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/share/applications"
mkdir -p "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$STAGING_DIR/usr/share/doc/$PKG_NAME"

cp "$BIN_DIR/maolan-editor" "$STAGING_DIR/usr/bin/"
strip "$STAGING_DIR/usr/bin/maolan-editor"
chmod 755 "$STAGING_DIR/usr/bin/maolan-editor"

cp "$SOURCE_DIR/assets/desktop/maolan-editor-linux.desktop" "$STAGING_DIR/usr/share/applications/maolan-editor.desktop"
chmod 644 "$STAGING_DIR/usr/share/applications/maolan-editor.desktop"

cp "$SOURCE_DIR/assets/images/maolan-editor-icon.svg" "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps/maolan-editor-icon.svg"
chmod 644 "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps/maolan-editor-icon.svg"

cp "$SOURCE_DIR/README.md" "$STAGING_DIR/usr/share/doc/$PKG_NAME/"
cp "$SOURCE_DIR/LICENSE" "$STAGING_DIR/usr/share/doc/$PKG_NAME/"
gzip -9 -n -c > "$STAGING_DIR/usr/share/doc/$PKG_NAME/changelog.gz" /dev/null 2>/dev/null || true

cat > "$STAGING_DIR/DEBIAN/control" <<EOF
Package: $PKG_NAME
Version: $PKG_VERSION
Section: sound
Priority: optional
Architecture: $DEB_ARCH
Depends: $JACK_RUNTIME_PACKAGE, $ALSA_RUNTIME_PACKAGE, libxkbcommon0
Maintainer: Maolan Team <maolan@github.io>
Description: Simple audio editor for Maolan audio files
 Maolan Editor opens audio files, displays their waveforms, and saves or
 exports audio using the same decode and encode paths as Maolan.
EOF

cat > "$STAGING_DIR/DEBIAN/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: Maolan Editor
Source: https://github.com/maolan/edit

Files: *
Copyright: Maolan Team
License: BSD-2-Clause
EOF

echo ""
echo "[5/7] Building .deb package..."
mkdir -p "$OUTPUT_DIR"
fakeroot dpkg-deb --build "$STAGING_DIR" "$OUTPUT_DIR/$DEB_NAME"

echo ""
echo "[6/7] Building AppImage..."

APPDIR="$APPDIR_BASE/AppDir"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/scalable/apps"

cp "$BIN_DIR/maolan-editor" "$APPDIR/usr/bin/"
sed \
    -e 's#Exec=/usr/bin/maolan-editor#Exec=maolan-editor#' \
    -e 's#Icon=/usr/share/icons/hicolor/scalable/apps/maolan-editor-icon.svg#Icon=maolan-editor-icon#' \
    "$SOURCE_DIR/assets/desktop/maolan-editor-linux.desktop" > "$APPDIR/usr/share/applications/maolan-editor.desktop"
cp "$SOURCE_DIR/assets/images/maolan-editor-icon.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/maolan-editor-icon.svg"

LINUXDEPLOY_CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/maolan"
LINUXDEPLOY="$LINUXDEPLOY_CACHE/linuxdeploy-x86_64.AppImage"
mkdir -p "$LINUXDEPLOY_CACHE"
if [[ ! -f "$LINUXDEPLOY" ]]; then
    echo "Downloading linuxdeploy..."
    curl -L -o "$LINUXDEPLOY" "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
    chmod +x "$LINUXDEPLOY"
fi

cd "$APPDIR_BASE"
"$LINUXDEPLOY" --appimage-extract-and-run \
    --appdir "$APPDIR" \
    --desktop-file "$APPDIR/usr/share/applications/maolan-editor.desktop" \
    --icon-file "$APPDIR/usr/share/icons/hicolor/scalable/apps/maolan-editor-icon.svg" \
    --executable "$APPDIR/usr/bin/maolan-editor" \
    --output appimage

BUILT_APPIMAGE=("$APPDIR_BASE"/*.AppImage)
if [[ ! -f "${BUILT_APPIMAGE[0]}" ]]; then
    echo "Error: No AppImage was produced in $APPDIR_BASE" >&2
    exit 1
fi
mv "${BUILT_APPIMAGE[0]}" "$OUTPUT_DIR/$APPIMAGE_NAME"
chmod +x "$OUTPUT_DIR/$APPIMAGE_NAME"

echo ""
echo "[7/7] Verifying packages..."
dpkg-deb --info "$OUTPUT_DIR/$DEB_NAME"
dpkg-deb --contents "$OUTPUT_DIR/$DEB_NAME"
ls -lh "$OUTPUT_DIR/$APPIMAGE_NAME"
echo ""
echo "========================================"
echo "Packages built successfully:"
echo "  $OUTPUT_DIR/$DEB_NAME"
echo "  $OUTPUT_DIR/$APPIMAGE_NAME"
echo "========================================"
