# 🎵 Meduza Music: The Ultimate Native Linux Music Player

<p align="center">
  <img src="assets/icon_512.png" width="128" alt="Meduza Music Logo" />
</p>

<p align="center">
  <b>A blazing fast, native, lightweight desktop YouTube Music player for Linux.</b><br />
  <i>Crafted with Rust, egui, and MPV for smooth audio streaming, zero lag, and minimal resource usage. The perfect Flatpak music player for your Linux desktop environment.</i>
</p>

<p align="center">
  <a href="https://github.com/akilaisadev/meduza-music/releases/latest"><img src="https://img.shields.io/github/v/release/akilaisadev/meduza-music?style=for-the-badge&color=1DB954" alt="Release" /></a>
  <a href="https://github.com/akilaisadev/meduza-music/stargazers"><img src="https://img.shields.io/github/stars/akilaisadev/meduza-music?style=for-the-badge&color=E95420" alt="Stars" /></a>
  <img src="https://img.shields.io/badge/Built%20With-Rust-000000?style=for-the-badge&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Audio-MPV%20IPC-044B94?style=for-the-badge" alt="MPV" />
  <img src="https://img.shields.io/badge/Platform-Linux%20%7C%20Flatpak-5C2D91?style=for-the-badge&logo=linux" alt="Linux Flatpak" />
</p>

---

## 📸 Interface Showcase

### 🏠 Home
Quick picks from your recent listening, sideways-scrolling shelves for your personalised feed, and skeleton placeholders while it loads.

![Meduza Music home screen](images/home_redesign.png)

### 🎧 Player, Queue & Full-Screen
A bottom bar with transport, seek and volume, plus a queue drawer you can jump around in. The full-screen player is tinted with the current track's artwork and shows synced lyrics. Click a lyric line to seek to it.

### 🔍 Search & 📚 Library
A large search field with live suggestions, a top result card and genre browsing. The library has a playlists grid, a liked-songs page and a playlist view with rename, delete, and JSON import/export.

### ⌨️ Keyboard & Right-Click
`Space` play/pause, `Ctrl+←` / `Ctrl+→` previous/next, `←` / `→` seek 5 s, `Ctrl+K` search, `Esc` close the full-screen player or queue, `Ctrl+0` reset zoom. Right-click any track for play next, add to queue, like, or add to a playlist.

---

## ✨ Features

- ⚡ **Extreme Native Performance**: Written 100% in Rust using `egui` and `tokio`. Consumes **<40 MB RAM** (vs 500 MB+ in Electron players).
- 🎧 **YouTube Music Engine**: Stream millions of songs, albums, and playlists directly via InnerTube API.
- 🏠 **Smart Home Feed**: Dynamically adapts to your top artists and genres with instant offline session caching.
- 🔊 **Low-Latency MPV Player**: Resolves upcoming stream URLs in the background for smooth gapless transitions (buffer size configurable via Low-End mode).
- 💾 **Offline Data Saver**: Option to cache played audio locally for replay without network access.
- 🎨 **Modern Dark UI**: Layered surfaces, vector icons that render the same on every system, artwork-driven accent colours, and toast notifications for playback errors and playlist actions.
- 📝 **Synchronized Lyrics**: Fetch time-synced lyrics from LRCLIB in the full-screen player.
- 📚 **Local Playlists**: Build custom playlists, with JSON import/export.
- 🎹 **Global Media Keys**: MPRIS2 integration for desktop media controls and metadata (Linux).

---

## 🚀 Installation & Usage

### Option 1: AppImage (Recommended)

Download the latest `MeduzaMusic-*-x86_64.AppImage` from the [Releases Page](https://github.com/akilaisadev/meduza-music/releases/latest) and run:

```bash
# 1. Make it executable
chmod +x Meduza_Music-*-x86_64.AppImage

# 2. Launch Meduza Music
./Meduza_Music-*-x86_64.AppImage
```

AppImages are self-contained and do not require installation. Requires `mpv`, `python3` with `yt_dlp`, and FUSE (already present on most modern distros). No GTK runtime is needed.

### Option 2: Build from Source (Cargo)

Prerequisites: `cargo`, `mpv`, `python3` + `yt-dlp`.

```bash
# 1. Clone the repository
git clone https://github.com/akilaisadev/meduza-music.git
cd meduza-music

# 2. Build and run in release mode
cargo run --release
```

### Option 3: Package an AppImage yourself

```bash
./build_appimage.sh
```

---

## 🛠 Tech Stack & Architecture

| Component | Technology | Role |
|---|---|---|
| **GUI Framework** | `eframe` / `egui` 0.26 | High-performance, GPU-accelerated immediate mode GUI |
| **Audio Backend** | `mpv` IPC | Native socket IPC control for low latency stream playback |
| **API Client** | `InnerTube` (Async) | YouTube Music API fetching with LRU image & track caching |
| **Async Runtime** | `tokio` 1.0 | Background pre-loading, audio stream resolving & caching |
| **Data Persistence** | `serde_json` + `dirs` | Local home feed & settings JSON persistence |

---

## ⚡ Performance Comparison

| Feature | Meduza Music (Rust) | Typical Electron Player |
|---|---|---|
| **RAM Usage** | **35 – 45 MB** | 450 – 800 MB |
| **Startup Time** | **< 0.2 seconds** | 2.5 – 4.0 seconds |
| **CPU Usage** | **< 1% idling** | 4 – 8% idling |
| **Package Size** | **~16 MB** | ~120 MB |

---

## 🤝 Contributing & License

Contributions, bug reports, and feature requests are welcome! Feel free to check out the [Issues](https://github.com/akilaisadev/meduza-music/issues).

Distributed under the MIT License. Crafted with ❤️ by [@akilaisadev](https://github.com/akilaisadev).
