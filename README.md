# Meduza Music 🎵

A premium, Spotify-inspired music player for Linux, built with Flutter. Streams from YouTube Music with dynamic recommendations, smart playlists, and a stunning glassmorphism UI.

## Features

- 🎧 **Stream any song** from YouTube Music
- 🏠 **Smart Home Feed** — 25+ dynamic rows refreshed based on your taste
- 🔍 **Instant Search** — results appear as you type, play immediately
- 📻 **Radio Mode** — auto-fetches related tracks so the music never stops
- 💾 **Home Cache** — loads in 0ms from local cache between sessions
- 🎨 **Dynamic Theming** — UI color changes with each track (13 preset hues)
- 🖥️ **Full-Screen Player** — click the album art to open the cinematic view
- 📋 **Playlists & Liked Songs**
- ⚡ **Fast Track Switching** — generation-based cancellation, no stale audio
- 🔁 Loop / 🔀 Shuffle / ⏭ Queue management

## Running (Development)

```bash
export PATH="$HOME/flutter/bin:$PATH"
flutter pub get
flutter run -d linux
```

## Building a Release

```bash
flutter build linux --release
# Bundle is at: build/linux/x64/release/bundle/
```

## Installing as Flatpak

Install `flatpak-builder` first:
```bash
sudo apt install flatpak-builder  # Debian/Ubuntu
# or
sudo dnf install flatpak-builder  # Fedora
```

Add Flathub (if not already added):
```bash
flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
```

Then build and install:
```bash
# Build Flutter release first
flutter build linux --release

# Run the Flatpak build script
./build_flatpak.sh

# Install the generated bundle
flatpak install --user MeduzaMusic.flatpak

# Launch
flatpak run org.meduza.Music
```

## Requirements

- **Network access** — streams from YouTube
- **PulseAudio / PipeWire** — for audio output
- **Wayland or X11** — for display

## Architecture

| File | Role |
|------|------|
| `lib/main.dart` | App shell, PlayerBar, window controls |
| `lib/playback_manager.dart` | Queue management, fast track switching with generation tokens |
| `lib/youtube_fetcher.dart` | YouTube search + stream URL resolution with 25-track LRU cache |
| `lib/discover_view.dart` | Home screen — lazy-loaded category rows |
| `lib/search_view.dart` | Search with debounced live results |
| `lib/full_screen_player_view.dart` | Cinematic full-screen player |
| `lib/playlist_view.dart` | Playlist detail view |
| `lib/library_view.dart` | Library + Recently Played |
| `lib/theme_engine.dart` | HSL-derived dynamic color system |
| `lib/home_cache_manager.dart` | JSON-based local recommendations cache |
