# Meduza Music — feature walkthrough

Meduza Music is a native Rust/egui desktop client for YouTube Music. Audio is
played by a background `mpv` process controlled over a private Unix IPC socket.

## Getting around

- **Home** — a personalized feed assembled from InnerTube shelves (jump back in,
  heavy rotation, listening history, top artists/mixes). Images are fetched and
  decoded on worker threads, cached on disk, and throttled on low-end hardware.
- **Search** — instant InnerTube search with debounced suggestions; results can
  be played or added to the library.
- **Your Library** — liked songs plus local playlists. Create, rename, delete,
  import and export playlists as JSON; add any track with the `+` button.
- **Settings** — audio quality, cache size, gapless preload depth, low-end mode,
  and cache maintenance.
- **Full-screen player** — click the artwork to expand. Includes synchronized
  lyrics from LRCLIB (toggle with the `♫ Lyrics` button), mouse-wheel volume
  control over the speaker icon, and transport controls.

## Playback

- `mpv` runs idle with `--no-config --no-scripts` and a private IPC socket in a
  0700 runtime directory. The socket and log file are restricted to the owner.
- Stream URLs are resolved by `yt-dlp` and cached in memory and on disk. Cached
  URLs are validated against both the local TTL and YouTube's absolute `expire`
  parameter before use; stale URLs are dropped and re-resolved automatically.
- A background preloader resolves upcoming queue tracks ahead of time. The
  number of tracks resolved is configurable (`preload_buffer_tracks`).
- Shuffle, repeat Off/All/One, and endless radio refills are supported. Global
  media keys and MPRIS2 metadata work on Linux via the session bus.
- Likes, playlists, settings and history are written as private (0600) JSON
  files under the XDG config directory.

## Building and packaging

```bash
cargo check --workspace       # verify all crates
cargo test --workspace        # unit tests (LRC parsing, playlist persistence)
./build_appimage.sh           # build an AppImage
./build_flatpak.sh            # build/install the Flatpak
```

No GTK or system D-Bus development headers are required. The Flatpak manifest
grants access to the session bus so MPRIS2 works inside the sandbox.

## Architecture

| Crate | Responsibility |
|---|---|
| `meduza-core` | InnerTube client, settings, URL allowlists, lyrics, playlists, workers |
| `meduza-playback` | mpv IPC, stream resolution, queue, likes, preloader, persistence |
| `meduza-viewmodel` | Snapshots, background updater, MPRIS2 integration |
| `meduza-ui` | egui application, panels, full-screen player, dialogs |
