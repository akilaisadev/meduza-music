# 🎵 Meduza Music v1.4.0

## What's New
- **Redesigned interface**: layered dark surfaces, vector icons that look the same on every system, a queue drawer, toast notifications, right-click menus on tracks and playlists, and a shelf-based home page with quick picks and recently played.
- **Full-screen player**: tinted with the current track's artwork, with synced lyrics (click a line to seek).
- **Keyboard shortcuts**: `Space` play/pause, `Ctrl+←/→` previous/next, `←/→` seek 5 s, `Ctrl+K` search, `Esc` to close panels.

## Fixes
- Failed stream resolution no longer skips a track in the queue; Repeat-One stops instead of retrying forever.
- Gapless playback plays the track that was preloaded, including under shuffle.
- A missing or failing mpv is reported as an error instead of "Playing".
- Settings, playlists and likes are written atomically; unreadable files are kept as `*.json.corrupt`.
- Listening history no longer drops tracks that are also in heavy rotation.
- Seeking is ignored while the track duration is unknown.

## Packages
Releases ship a `.deb` (Debian/Ubuntu), an AppImage and a Flatpak bundle. Build them locally with `build_deb.sh`, `build_appimage.sh` and `build_flatpak.sh`.

## Installation (Debian / Ubuntu)
```bash
sudo apt install ./meduza-music_1.4.0_amd64.deb
```

The package bundles a recent `yt-dlp` and requires `mpv` plus a JavaScript runtime (`nodejs`).

---

## What's New
- **Modern UI refresh**: unified design tokens, collapsible icon-rail sidebar, raised active navigation pills, hover play affordances on album cards, recent-search chip, and an Appearance section with a one-click zoom reset.
- **Pinch / Ctrl+scroll zoom**: the whole UI scales from its comfortable 100% default; **Ctrl+0** restores the default, and **Ctrl+K** jumps to search.
- **Playback reliability**: bundled a current `yt-dlp` in the `.deb`, pass a JS runtime explicitly, and fixed the invalid mpv flag that prevented playback on mpv 0.37.
- **Streaming**: audio URLs now resolve reliably against current YouTube; cached CDN links are still validated and expired links are re-resolved.

## Installation (Debian / Ubuntu)
Download the `.deb` below and install it:

```bash
sudo apt install ./meduza-music_1.3.0_amd64.deb
```

The package bundles a recent `yt-dlp` and requires `mpv` plus a JavaScript runtime (`nodejs`), so streams keep working on distributions whose own `yt-dlp` is outdated.
