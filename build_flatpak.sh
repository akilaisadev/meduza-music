#!/usr/bin/env bash
# ===========================================================
# Meduza Music — Flatpak Build Script
# Builds the Flutter Linux release bundle and packages as Flatpak
# Usage: ./build_flatpak.sh
# ===========================================================
set -e

APP_ID="org.meduza.Music"
MANIFEST="org.meduza.Music.yml"
BUILD_DIR=".flatpak-build"
REPO_DIR=".flatpak-repo"
BUNDLE_NAME="MeduzaMusic.flatpak"

export PATH="$HOME/flutter/bin:$PATH"

echo "========================================"
echo " Meduza Music — Flatpak Build Pipeline"
echo "========================================"

# --- Step 1: Check prerequisites ---
echo ""
echo "→ Checking prerequisites..."

if ! command -v flutter &>/dev/null; then
  echo "ERROR: flutter not found in PATH. Make sure flutter/bin is on PATH."
  exit 1
fi

if ! command -v flatpak &>/dev/null; then
  echo "ERROR: flatpak not installed. Install with: sudo apt install flatpak"
  exit 1
fi

if ! command -v flatpak-builder &>/dev/null; then
  echo "ERROR: flatpak-builder not installed."
  echo "       Install with: sudo apt install flatpak-builder"
  exit 1
fi

echo "  ✓ flutter $(flutter --version --machine 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('frameworkVersion','?'))" 2>/dev/null || echo 'OK')"
echo "  ✓ flatpak $(flatpak --version)"
echo "  ✓ flatpak-builder $(flatpak-builder --version)"

# --- Step 2: Check Flatpak runtime is available ---
echo ""
echo "→ Ensuring Flathub remote is configured..."
# Add both system-wide and user-specific flathub repository options
flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo || true
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo || true

echo "→ Updating Flathub remote metadata refs..."
flatpak update --user -y || true

echo "→ Checking GNOME Platform runtime 46..."
if ! flatpak info org.gnome.Platform//46 &>/dev/null; then
  echo "  Installing org.gnome.Platform//46 from Flathub..."
  flatpak install --user -y flathub org.gnome.Platform//46 org.gnome.Sdk//46 || {
    echo "ERROR: Could not install GNOME Platform 46."
    echo "       Make sure Flathub remote is added:"
    echo "       flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo"
    exit 1
  }
fi
echo "  ✓ org.gnome.Platform//46 present"

# --- Step 3: Build Flutter Linux release ---
echo ""
echo "→ Building Flutter Linux release bundle..."
flutter clean
flutter pub get
flutter build linux --release

echo "  ✓ Release bundle at: build/linux/x64/release/bundle/"

# --- Step 4: Build Flatpak ---
echo ""
echo "→ Building Flatpak package..."
rm -rf "$BUILD_DIR" "$REPO_DIR"

flatpak-builder \
  --repo="$REPO_DIR" \
  --force-clean \
  --user \
  "$BUILD_DIR" \
  "$MANIFEST"

echo "  ✓ Flatpak built"

# --- Step 5: Export as installable .flatpak bundle ---
echo ""
echo "→ Exporting $BUNDLE_NAME..."
flatpak build-bundle \
  "$REPO_DIR" \
  "$BUNDLE_NAME" \
  "$APP_ID"

echo ""
echo "========================================"
echo " SUCCESS! 🎵 MeduzaMusic.flatpak built!"
echo "========================================"
echo ""
echo " Install on any distro with:"
echo "   flatpak install --user MeduzaMusic.flatpak"
echo ""
echo " Run with:"
echo "   flatpak run org.meduza.Music"
echo ""
echo " Or double-click MeduzaMusic.flatpak in your file manager."
echo "========================================"
