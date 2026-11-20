# 🎵 Meduza Music v1.3.0

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
