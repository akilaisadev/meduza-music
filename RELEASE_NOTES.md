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

### 📦 Installation Guide (Linux Flatpak)

#### 1. Download the Bundle
Download `meduza-music-v1.0.0.flatpak` attached below.

#### 2. Install
```bash
flatpak install --user meduza-music-v1.0.0.flatpak
```

#### 3. Launch
```bash
flatpak run org.meduzamusic.MeduzaMusic
```
