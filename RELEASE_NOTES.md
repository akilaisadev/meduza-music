# 🎵 Meduza Music v1.0.0 Release

Welcome to the official **v1.0.0 release** of **Meduza Music**! Meduza Music is a fast, lightweight, and modern desktop music player built with Rust, `eframe`/`egui`, and InnerTube API integration with `mpv` audio backend.

---

### ✨ Features & Improvements

- **🏠 Interactive & Smooth Home Feed**:
  - Personalized music shelves (Jump back in, Heavy rotation, Listening history, Top Artists & mixes).
  - High-contrast, always-visible scrollbar UI for smooth navigation through discovery shelves.

- **🔍 YouTube Music Search & Discovery**:
  - Instant search suggestions and real-time results for tracks, artists, and mixes.
  - Dedicated browse categories and genre grid.

- **⚡ Instant Playback Engine**:
  - `mpv` IPC audio player integration with stream resolving and zero-latency background pre-loading.
  - Built-in Data Saver & offline replay caching engine.

- **🎨 Modern UI & System Tray Integration**:
  - Dark mode glassmorphic UI, album art caching, and spinning vinyl disc animation.
  - System tray icon support (minimize to tray).

---

### 📦 Installation Guide (Linux AppImage)

#### 1. Download
Download the latest `Meduza_Music-*-x86_64.AppImage` from the release assets.

#### 2. Make it executable & launch
```bash
chmod +x Meduza_Music-*.AppImage
./Meduza_Music-*.AppImage
```

#### 3. Run from anywhere
Move the AppImage to your `~/bin` folder (or any folder in `PATH`) so a
`meduza-music` shell alias launches it from the terminal.

> Note: AppImage is the only packaged format supported. For system-level
> installs, build from source with `cargo build --release`.
