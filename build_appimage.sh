#!/bin/bash
set -e

APP_NAME="MeduzaMusic"
EXEC_NAME="meduza-music"
VERSION="1.2.0"

echo "Building AppImage for $APP_NAME v$VERSION..."

# Ensure we have linuxdeploy
if [ ! -f "linuxdeploy-x86_64.AppImage" ]; then
    echo "Downloading linuxdeploy..."
    wget -c -nv "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
    chmod +x linuxdeploy-x86_64.AppImage
fi

# Prepare AppDir
APPDIR="AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/512x512/apps"

# Build the release binary
echo "Compiling Rust binary..."
cargo build --release

# Copy binary and strip it to make the AppImage as small as possible
cp "target/release/$EXEC_NAME" "$APPDIR/usr/bin/"
strip "$APPDIR/usr/bin/$EXEC_NAME"

# Copy desktop file and icon
cp "org.meduzamusic.MeduzaMusic.desktop" "$APPDIR/usr/share/applications/"
cp "logo.png" "$APPDIR/usr/share/icons/hicolor/512x512/apps/org.meduzamusic.MeduzaMusic.png"

# Set version
export VERSION="$VERSION"

# Run linuxdeploy
./linuxdeploy-x86_64.AppImage --appdir "$APPDIR" --output appimage -d "org.meduzamusic.MeduzaMusic.desktop" -i "logo.png"

echo "AppImage created successfully!"
