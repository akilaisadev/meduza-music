#!/usr/bin/env bash
set -e

echo "=== Building Flatpak for Meduza Music ==="

# Ensure flatpak-builder is installed
if ! command -v flatpak-builder &> /dev/null; then
    echo "Error: flatpak-builder is not installed."
    echo "Please install it via your package manager:"
    echo "  Ubuntu/Debian: sudo apt install flatpak-builder"
    echo "  Fedora:        sudo dnf install flatpak-builder"
    echo "  Arch:          sudo pacman -S flatpak-builder"
    exit 1
fi

# Build and install locally
flatpak-builder --user --install --force-clean --disable-rofiles-fuse --install-deps-from=flathub build-dir org.meduzamusic.MeduzaMusic.yml

echo "=== Build Complete! ==="
echo "You can run Meduza Music via Flatpak using:"
echo "  flatpak run org.meduzamusic.MeduzaMusic"
