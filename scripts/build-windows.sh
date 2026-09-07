#!/usr/bin/env bash
set -euo pipefail

# build-windows.sh - Build Maolan Editor on a Windows host and fetch the setup file.
#
# Usage:
#   ./scripts/build-windows.sh CONNECTION
#
# Arguments:
#   CONNECTION    SSH connection target: IP, hostname, or user@IP.
#
# The script prepares C:\maolan\editor on the Windows host, runs the editor
# PowerShell build script there, and copies the generated installer into ./dist.

if [[ $# -ne 1 ]]; then
    sed -n '4,11p' "$0" | sed -e 's/^# //' -e 's/^#$//'
    exit 2
fi

CONNECTION="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_DIR="$(dirname "$SCRIPT_DIR")"
DIST_DIR="$SOURCE_DIR/dist"
REMOTE_ROOT='C:\maolan'
REMOTE_EDITOR='C:\maolan\editor'
REMOTE_REPO='https://github.com/maolan/editor.git'

mkdir -p "$DIST_DIR"

remote_ps() {
    local script="$1"
    ssh "$CONNECTION" powershell -NoProfile -ExecutionPolicy Bypass -Command "$script"
}

echo "Preparing Windows host: $CONNECTION"
remote_ps "\$ErrorActionPreference = 'Stop'
function Test-Command([string]\$Name) {
    return [bool](Get-Command \$Name -ErrorAction SilentlyContinue)
}
if (-not (Test-Command 'git')) {
    Write-Host 'Installing Git...'
    \$installer = Join-Path \$env:TEMP 'Git-installer.exe'
    if (-not (Test-Path \$installer)) {
        Invoke-WebRequest -Uri 'https://github.com/git-for-windows/git/releases/download/v2.49.0.windows.1/Git-2.49.0-64-bit.exe' -OutFile \$installer
    }
    Start-Process -FilePath \$installer -ArgumentList '/VERYSILENT','/NORESTART' -Wait
    \$env:PATH = \"\$env:ProgramFiles\Git\cmd;\$env:PATH\"
}
if (-not (Test-Path '$REMOTE_ROOT')) {
    New-Item -ItemType Directory -Force '$REMOTE_ROOT' | Out-Null
}
if (-not (Test-Path '$REMOTE_EDITOR')) {
    git clone '$REMOTE_REPO' '$REMOTE_EDITOR'
} else {
    Push-Location '$REMOTE_EDITOR'
    git reset --hard
    git clean -fdx
    git pull --ff-only
    Pop-Location
}"

echo "Building Maolan Editor on Windows..."
remote_ps "\$ErrorActionPreference = 'Stop'
Push-Location '$REMOTE_EDITOR'
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build.ps1
Pop-Location"

echo "Fetching installer..."
REMOTE_SETUP="$(remote_ps "\$ErrorActionPreference = 'Stop'
\$dist = Join-Path '$REMOTE_EDITOR' 'dist'
\$setup = Get-ChildItem -Path \$dist -Filter 'maolan-editor-*.windows.amd64.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not \$setup) {
    throw \"No maolan-editor Windows setup file found in \$dist\"
}
\$setup.FullName -replace '\\', '/'" | tr -d '\r' | tail -n1)"

scp "$CONNECTION:$REMOTE_SETUP" "$DIST_DIR/"

echo "Done: $DIST_DIR/$(basename "$REMOTE_SETUP")"
