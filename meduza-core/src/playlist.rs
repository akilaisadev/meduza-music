//! Local playlist storage (issue #9).
//!
//! Playlists are stored as a single private JSON file next to
//! `liked_songs.json`. Import/export uses the same JSON shape so a playlist
//! file can be moved between machines.

use crate::innertube::TrackItem;
use crate::settings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub tracks: Vec<TrackItem>,
    #[serde(default)]
    pub created_at: u64,
}

impl Playlist {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into().trim().to_string();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        Self {
            id: format!("pl-{}", now.as_nanos()),
            name,
            tracks: Vec::new(),
            created_at: now.as_secs(),
        }
    }

    pub fn contains(&self, media_id: &str) -> bool {
        self.tracks.iter().any(|t| t.media_id == media_id)
    }

    pub fn add_track(&mut self, track: TrackItem) -> bool {
        if self.contains(&track.media_id) {
            return false;
        }
        self.tracks.push(track);
        true
    }

    pub fn remove_track(&mut self, media_id: &str) {
        self.tracks.retain(|t| t.media_id != media_id);
    }
}

pub fn store_path() -> PathBuf {
    settings::app_config_dir().join("playlists.json")
}

/// Load the playlist library. Migrates the legacy "directory at file path"
/// artefact if an older build ever created one.
pub fn load() -> Vec<Playlist> {
    let path = store_path();
    if path.is_dir() {
        let _ = std::fs::remove_dir(&path);
    }
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new(); };
    match serde_json::from_str::<Vec<Playlist>>(&text) {
        Ok(playlists) => playlists,
        Err(e) => {
            // Keep the unreadable file: the next save would otherwise overwrite
            // it and every playlist in it would be lost for good.
            eprintln!("[Playlists] playlists.json is unreadable ({}); keeping a backup.", e);
            let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
            Vec::new()
        }
    }
}

pub fn save(playlists: &[Playlist]) {
    if let Ok(text) = serde_json::to_string_pretty(playlists) {
        let _ = settings::write_private(&store_path(), &text);
    }
}

/// Read a single exportable playlist from an arbitrary JSON file.
pub fn import_from_file(path: &Path) -> Result<Vec<Playlist>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let imported: Vec<Playlist> = serde_json::from_str(&text)
        .or_else(|_| serde_json::from_str::<Playlist>(&text).map(|p| vec![p]))
        .map_err(|e| format!("Invalid playlist JSON: {}", e))?;
    if imported.iter().any(|p| p.name.trim().is_empty()) {
        return Err("Playlist name cannot be empty".to_string());
    }
    Ok(imported)
}

pub fn export_to_file(playlists: &[Playlist], path: &Path) -> Result<(), String> {
    let text = serde_json::to_string_pretty(playlists).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track(id: &str) -> TrackItem {
        TrackItem {
            title: "Song".to_string(),
            artist: "Artist".to_string(),
            media_id: id.to_string(),
            thumbnail_url: "https://i.ytimg.com/vi/abc/hqdefault.jpg".to_string(),
            duration_seconds: 180,
        }
    }

    #[test]
    fn add_is_deduplicated_and_removal_works() {
        let mut p = Playlist::new("Roadtrip");
        assert!(p.add_track(sample_track("abcdefghijk")));
        assert!(!p.add_track(sample_track("abcdefghijk")));
        assert_eq!(p.tracks.len(), 1);
        p.remove_track("abcdefghijk");
        assert!(p.tracks.is_empty());
    }

    #[test]
    fn json_roundtrip_preserves_tracks() {
        let mut p = Playlist::new("Favourites");
        p.add_track(sample_track("abcdefghijk"));
        let json = serde_json::to_string(&vec![p.clone()]).unwrap();
        let decoded: Vec<Playlist> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, vec![p]);
    }
}
