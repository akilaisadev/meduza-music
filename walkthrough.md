# Meduza Music — latency, UI, responsive & flagship intelligence fixes

## 1. Flagship User Intelligence System (Active Learning)
- **Persistent Taste Profile Database:** Built file load/save serialization to `~/.config/meduza/taste_profile.json` so the user's learned preference state is preserved across app restarts.
- **Implicit Feedback Tracking:**
  - **Successful Listens:** If a track runs for over 20 seconds, the engine registers a positive play score, reinforcing artist and style keywords.
  - **Early Skips Penalization:** If a track is skipped within 20 seconds of starting, the engine logs a skip penalty. Repeated early skips decay artist play affinity and penalize style keyword weights negatively (allowing active learning of user dislikes).
- **Style Keyword Extraction & Genre Affinity:** Implemented an NLP-style keyword tokenizer that cleans track titles and artist names (filtering out common video/audio stopwords) to map user preferences to specific sub-genre concepts (e.g., learning terms like "lofi", "synthwave", "acoustic").
- **Skip-to-Play Ratio Penalty:** Aggressively suppresses suggestions of tracks that are frequently skipped by the user, dynamically multiplying the final ranking score.
- **Widescreen Diversity Window:** Shuffling scales track weight penalties based on recent artists in a 5-track sliding window, preventing repetitive artist clusters.

## 2. Immersive Blur Application Theme
- **Cross-Fade Album Art Background:** Overhauled the main application frame background to render the currently playing track's artwork as a full-bleed background, utilizing a smooth `AnimatedSwitcher` to cross-fade between album art updates.
- **Backdrop Blurring:** Applied a heavy `BackdropFilter` (90px blur) and dark overlay over the background image, creating an atmospheric, organic color glow that responds dynamically to each song's style.
- **Frosted Glass Sidebar:** Refactored the left navigation sidebar to be a transparent panel with a `BackdropFilter` (30px blur), allowing the dynamic colored background to bleed through beautifully.

## 3. Personalization: Real Favorites Grid on Home
- **Dynamic Liked Songs Grid:** The Home screen now watches the user's Liked Songs playlist via `PlaylistManager`.
- **Your Favorites Row:** If the user has liked tracks, a personalized row titled **"Your Favorites"** is loaded in the grid container. Liking/unliking a song in the player instantly updates the home grid. Falls back to default Quick Picks if empty.

## 4. Advanced Hover Micro-Animations & Design Polish
- **Gradient Shader Greeting:** The Home screen greeting ("Good Evening") now uses a vibrant linear text gradient mask running from white to the accent glow color.
- **Lifty Translation & Glow Shadow Cards:**
  - **Quick Picks Grid:** Hovering translates items upward slightly (`-3px`) and paints a soft accent glow shadow.
  - **Music Cards:** Hovering lifts cards (`-5px`), scales borders, and adds a larger neon hover shadow. Shows a floating play button overlay.

---
Static analysis has been run: **No compilation issues found!** 
Press `R` in your active terminal to Hot Restart and try out the learning intelligence system!
