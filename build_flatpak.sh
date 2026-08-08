#!/bin/bash
set -e

echo "Building Flatpak for Meduza Music..."
flatpak-builder --force-clean --repo=repo build-dir org.meduzamusic.MeduzaMusic.yml
echo "Exporting .flatpak bundle..."
flatpak build-bundle repo Meduza_Music-1.2.0-x86_64.flatpak org.meduzamusic.MeduzaMusic
echo "Done! You can install it using: flatpak install --user Meduza_Music-1.2.0-x86_64.flatpak"
