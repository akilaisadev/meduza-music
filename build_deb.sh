#!/usr/bin/env bash
set -e

echo "=== Building Meduza Music Release Binary ==="
cargo build --release

DEB_ROOT="target/debian/meduza-music_0.2.0_amd64"
rm -rf "$DEB_ROOT"
mkdir -p "$DEB_ROOT/DEBIAN"
mkdir -p "$DEB_ROOT/usr/bin"
mkdir -p "$DEB_ROOT/usr/share/applications"
mkdir -p "$DEB_ROOT/usr/share/icons/hicolor/256x256/apps"

echo "=== Copying Binary & Assets ==="
cp target/release/meduza-music "$DEB_ROOT/usr/bin/meduza-music"
chmod +x "$DEB_ROOT/usr/bin/meduza-music"

if [ -f "assets/icon.png" ]; then
    cp assets/icon.png "$DEB_ROOT/usr/share/icons/hicolor/256x256/apps/meduza-music.png"
fi

cat << 'EOF' > "$DEB_ROOT/DEBIAN/control"
Package: meduza-music
Version: 0.2.0
Section: sound
Priority: optional
Architecture: amd64
Depends: mpv, python3, ffmpeg
Maintainer: Akila <akilaisadev@github.com>
Description: Meduza Music Player
 Premium desktop music player built in Rust.
 Features zero-data audio caching, taste recommendation engine, and continuous radio playback.
EOF

cat << 'EOF' > "$DEB_ROOT/usr/share/applications/meduza-music.desktop"
[Desktop Entry]
Name=Meduza Music
Comment=Premium Desktop Music Player
Exec=meduza-music
Icon=meduza-music
Terminal=false
Type=Application
Categories=AudioVideo;Audio;Player;Music;
Keywords=Music;Player;Streaming;Meduza;
EOF

chmod 644 "$DEB_ROOT/usr/share/applications/meduza-music.desktop"

echo "=== Building Debian Package (.deb) ==="
dpkg-deb --build --root-owner-group "$DEB_ROOT" target/meduza-music_0.2.0_amd64.deb

echo "=== SUCCESS! ==="
echo "Debian package created at: target/meduza-music_0.2.0_amd64.deb"
echo "To install on any Debian/Ubuntu system, run:"
echo "  sudo dpkg -i target/meduza-music_0.2.0_amd64.deb"
