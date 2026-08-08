# 🎵 Meduza Music v1.2.0 

## What's New
- **Lazy Loading Implementation**: The home feed grid now utilizes aggressive lazy loading and view-culling, drastically reducing GPU/CPU consumption and completely eliminating scrolling flicker on large libraries!
- **Library Saving Architecture Fix**: Liked tracks and your history are no longer cross-filtered. Saving a track correctly adds it to your "Your Library" view without latency.
- **Improved Background Threading**: "Like" toggling has been moved to a background thread to prevent UI freezing.
- **Robust Auto-Skipping**: Broken YouTube Music stream resolutions or dead tracks now automatically skip and gracefully remove themselves from your playback queue to prevent silent hang-ups.
- **Streamlined UI Layout**: The visual padding and home feed elements have been refined for a sleeker, more premium look.

## Installation
You can install this update by downloading the `.flatpak` bundle below and running:

```bash
flatpak install --user Meduza_Music-1.2.0-x86_64.flatpak
```
