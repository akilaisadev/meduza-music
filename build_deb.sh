#!/usr/bin/env bash
# Build a Debian/Ubuntu .deb bundle for Meduza Music.
# The bundle ships a current yt-dlp so outdated distro packages don't break
# YouTube stream resolution.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT_DIR"

VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
ARCH="$(dpkg --print-architecture)"
PKG="meduza-music_${VERSION}_${ARCH}"
BUILD="target/deb/${PKG}"
OUT="target/deb/${PKG}.deb"

echo "Building ${PKG} ..."

# 1. Release binary
cargo build --release --locked
BIN="target/release/meduza-ui"
command -v strip >/dev/null 2>&1 && strip "$BIN" || true

# 2. Staging tree
rm -rf "$BUILD"
mkdir -p \
  "$BUILD/DEBIAN" \
  "$BUILD/usr/bin" \
  "$BUILD/usr/lib/meduza-music" \
  "$BUILD/usr/share/applications" \
  "$BUILD/usr/share/icons/hicolor/512x512/apps" \
  "$BUILD/usr/share/icons/hicolor/128x128/apps" \
  "$BUILD/usr/share/doc/meduza-music"

cp "$BIN" "$BUILD/usr/lib/meduza-music/meduza-music"

# 3. Bundled yt-dlp (prefer a fresh download, fall back to a local install)
YOUTUBE_DLP="$BUILD/usr/lib/meduza-music/yt-dlp"
if command -v curl >/dev/null 2>&1; then
  if ! curl -fsSL --max-time 90 \
      https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux \
      -o "$YOUTUBE_DLP"; then
    cp "$HOME/.local/bin/yt-dlp" "$YOUTUBE_DLP"
  fi
elif [[ -x "$HOME/.local/bin/yt-dlp" ]]; then
  cp "$HOME/.local/bin/yt-dlp" "$YOUTUBE_DLP"
else
  echo "error: need curl or ~/.local/bin/yt-dlp to bundle yt-dlp" >&2
  exit 1
fi
chmod 755 "$YOUTUBE_DLP"

# 4. Wrapper keeps the bundled tools first on PATH for the GUI session
cat > "$BUILD/usr/bin/meduza-music" <<'EOF'
#!/bin/sh
export PATH="/usr/lib/meduza-music:$PATH"
exec /usr/lib/meduza-music/meduza-music "$@"
EOF
chmod 755 "$BUILD/usr/bin/meduza-music"

# 5. Desktop integration
cp org.meduzamusic.MeduzaMusic.desktop "$BUILD/usr/share/applications/"
cp assets/icon_512.png "$BUILD/usr/share/icons/hicolor/512x512/apps/org.meduzamusic.MeduzaMusic.png"
cp assets/icon_128.png "$BUILD/usr/share/icons/hicolor/128x128/apps/org.meduzamusic.MeduzaMusic.png"

# 6. Docs
cp RELEASE_NOTES.md "$BUILD/usr/share/doc/meduza-music/release-notes.md"
install -m 644 /dev/null "$BUILD/usr/share/doc/meduza-music/copyright"
cat > "$BUILD/usr/share/doc/meduza-music/copyright" <<'EOF'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: Meduza Music
Source: https://github.com/akilaisadev/meduza-music

Files: *
Copyright: 2026 Akila <akilawasalathilaka@gmail.com>
License: MIT
 Permission is hereby granted, free of charge, to any person obtaining a copy
 of this software and associated documentation files (the "Software"), to deal
 in the Software without restriction, including without limitation the rights
 to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 copies of the Software, and to permit persons to whom the Software is
 furnished to do so, subject to the following conditions:
 .
 The above copyright notice and this permission notice shall be included in
 all copies or substantial portions of the Software.
 .
 THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 THE SOFTWARE.
EOF

SIZE="$(du -sk "$BUILD" | cut -f1)"
cat > "$BUILD/DEBIAN/control" <<EOF
Package: meduza-music
Version: $VERSION
Section: sound
Priority: optional
Architecture: $ARCH
Installed-Size: $SIZE
Depends: mpv, nodejs
Maintainer: Akila <akilawasalathilaka@gmail.com>
Homepage: https://github.com/akilaisadev/meduza-music
Description: High-performance YouTube Music desktop client
 Meduza Music is a fast, privacy-friendly YouTube Music desktop player built
 with Rust and egui. It includes gapless playback, lyrics, MPRIS media keys,
 local playlists and an offline audio cache. A current yt-dlp is bundled so
 stream resolution keeps working on distributions with older packages.
EOF

# 7. Build
rm -f "$OUT"
dpkg-deb --build --root-owner-group "$BUILD" "$OUT"
echo "Done: $OUT"
