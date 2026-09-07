#!/usr/bin/env bash
set -euo pipefail

# build-fedora.sh - Build a .rpm package for Maolan Editor on Fedora.
#
# Usage:
#   ./scripts/build-fedora.sh [OPTIONS]
#
# Options:
#   -s, --source-dir DIR     Path to maolan-editor source directory (default: parent of this script)
#   -o, --output-dir DIR     Where to write the .rpm file (default: ./dist)
#   -v, --version VERSION    Override package version (default: read from Cargo.toml)
#   -t, --target-dir DIR     Local target directory (useful when source is on NFS)
#   -h, --help               Show this help message
#
# The script installs build dependencies via dnf, installs Rust if missing,
# builds the release binary, and produces a .rpm package using rpmbuild.

. /etc/os-release

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$SOURCE_DIR/dist"
OVERRIDE_VERSION=""
TARGET_DIR=""

usage() {
    sed -n '4,14p' "$0" | sed -e 's/^# //' -e 's/^#$//'
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

RPM_ARCH="$(uname -m)"
PKG_NAME="maolan-editor"
RPM_NAME="${PKG_NAME}-${PKG_VERSION}-1.fc${VERSION_ID}.${RPM_ARCH}.rpm"

classic_jack_devel_installed() {
    rpm -q jack-audio-connection-kit-devel &>/dev/null
}

pipewire_jack_devel_installed() {
    rpm -q pipewire-jack-audio-connection-kit-devel &>/dev/null
}

pipewire_installed() {
    rpm -q pipewire &>/dev/null
}

if classic_jack_devel_installed; then
    JACK_DEVEL_PACKAGE="jack-audio-connection-kit-devel"
    JACK_RUNTIME_REQUIRE="jack-audio-connection-kit"
    JACK_PROVIDER_LABEL="JACK"
elif pipewire_jack_devel_installed || pipewire_installed; then
    JACK_DEVEL_PACKAGE="pipewire-jack-audio-connection-kit-devel"
    JACK_RUNTIME_REQUIRE="pipewire-jack-audio-connection-kit"
    JACK_PROVIDER_LABEL="PipeWire JACK"
else
    JACK_DEVEL_PACKAGE="jack-audio-connection-kit-devel"
    JACK_RUNTIME_REQUIRE="jack-audio-connection-kit"
    JACK_PROVIDER_LABEL="JACK"
fi

echo "========================================"
echo "Building Maolan Editor .rpm package"
echo "Version: $PKG_VERSION"
echo "Architecture: $RPM_ARCH"
echo "Source: $SOURCE_DIR"
echo "Output: $OUTPUT_DIR/$RPM_NAME"
echo "JACK provider: $JACK_PROVIDER_LABEL"
echo "========================================"

echo ""
echo "[1/6] Installing build dependencies..."
sudo dnf install -y \
    pkgconf-pkg-config \
    gcc \
    gcc-c++ \
    "$JACK_DEVEL_PACKAGE" \
    alsa-lib-devel \
    libxkbcommon-devel \
    git \
    rpm-build \
    curl \
    ca-certificates

echo ""
echo "[2/6] Checking Rust toolchain..."
if ! command -v cargo &>/dev/null; then
    echo "Rust not found. Installing from distribution packages..."
    sudo dnf install -y rust cargo
else
    echo "Rust already installed: $(rustc --version)"
fi

echo ""
echo "[3/6] Building release binary..."
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
echo "[4/6] Preparing RPM package structure..."

SPEC_DIR="$(mktemp -d)"
trap "rm -rf '$SPEC_DIR'" EXIT

mkdir -p "$SPEC_DIR"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

STAGING_DIR="$SPEC_DIR/staging"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/share/applications"
mkdir -p "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$STAGING_DIR/usr/share/doc/$PKG_NAME"

cp "$BIN_DIR/maolan-editor" "$STAGING_DIR/usr/bin/"
strip "$STAGING_DIR/usr/bin/maolan-editor"
chmod 755 "$STAGING_DIR/usr/bin/maolan-editor"

cat > "$STAGING_DIR/usr/share/applications/maolan-editor.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Maolan Editor
Comment=Simple audio editor for Maolan audio files
GenericName=Audio Editor
Exec=/usr/bin/maolan-editor %f
Icon=maolan-editor
Terminal=false
Categories=AudioVideo;Audio;AudioVideoEditing;
MimeType=audio/wav;audio/flac;audio/mpeg;audio/ogg;
Keywords=audio;editor;waveform;maolan;
StartupNotify=true
EOF
chmod 644 "$STAGING_DIR/usr/share/applications/maolan-editor.desktop"

ICON_SOURCE="$SOURCE_DIR/../maolan/assets/images/maolan-icon.svg"
if [[ -f "$ICON_SOURCE" ]]; then
    cp "$ICON_SOURCE" "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps/maolan-editor.svg"
else
    cat > "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps/maolan-editor.svg" <<EOF
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect width="128" height="128" rx="24" fill="#151515"/><path d="M24 82h80v14H24zM32 58h14v18H32zM54 34h14v42H54zM76 48h14v28H76z" fill="#f2c94c"/></svg>
EOF
fi
chmod 644 "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps/maolan-editor.svg"

cp "$SOURCE_DIR/README.md" "$STAGING_DIR/usr/share/doc/$PKG_NAME/"
cp "$SOURCE_DIR/LICENSE" "$STAGING_DIR/usr/share/doc/$PKG_NAME/"

cd "$STAGING_DIR"
tar czf "$SPEC_DIR/SOURCES/maolan-editor-files.tar.gz" .

cat > "$SPEC_DIR/SPECS/maolan-editor.spec" <<EOF
Name:           $PKG_NAME
Version:        $PKG_VERSION
Release:        1.fedora
Summary:        Simple audio editor for Maolan audio files
License:        BSD-2-Clause
URL:            https://github.com/maolan/edit
Source0:        maolan-editor-files.tar.gz
BuildArch:      $RPM_ARCH

Requires:       $JACK_RUNTIME_REQUIRE, alsa-lib, libxkbcommon

%description
Maolan Editor opens audio files, displays their waveforms, and saves or
exports audio using the same decode and encode paths as Maolan.

%prep
# No source preparation needed for binary build

%build
# No build needed; binaries are already built.

%install
mkdir -p %{buildroot}
cd %{buildroot}
tar xzf %{SOURCE0}

%files
%defattr(-,root,root,-)
/usr/bin/maolan-editor
/usr/share/applications/maolan-editor.desktop
/usr/share/icons/hicolor/scalable/apps/maolan-editor.svg
%doc /usr/share/doc/maolan-editor/README.md
%license /usr/share/doc/maolan-editor/LICENSE

%changelog
* Mon Sep 07 2026 Maolan Team <meka@sys.it.com> - $PKG_VERSION-1
- Initial RPM package.
EOF

echo ""
echo "[5/6] Building .rpm package..."
cd "$SPEC_DIR"
rpmbuild --define "_topdir $SPEC_DIR" --bb "$SPEC_DIR/SPECS/maolan-editor.spec"

mkdir -p "$OUTPUT_DIR"
BUILT_RPM="$(ls "$SPEC_DIR/RPMS/$RPM_ARCH/"*.rpm | head -n1)"
cp "$BUILT_RPM" "$OUTPUT_DIR/$RPM_NAME"

echo ""
echo "[6/6] Verifying package..."
rpm -qip "$OUTPUT_DIR/$RPM_NAME"
rpm -qlp "$OUTPUT_DIR/$RPM_NAME"

echo ""
echo "========================================"
echo "Package built successfully:"
echo "  $OUTPUT_DIR/$RPM_NAME"
echo "========================================"
