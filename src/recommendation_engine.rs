use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::innertube::TrackItem;
use crate::settings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackScore {
    pub track: TrackItem,
    pub play_count: u32,
    pub completion_score: f32,
    pub last_played_ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserTasteProfile {
    pub artist_scores: HashMap<String, f32>,
    pub track_scores: HashMap<String, TrackScore>,
    pub total_listens: u32,
}

pub struct RecommendationEngine {
    config_path: PathBuf,
    pub profile: UserTasteProfile,
}

impl RecommendationEngine {
    pub fn new() -> Self {
        let dir = settings::app_config_dir();
        settings::ensure_private_dir(&dir);
        let path = dir.join("user_taste.json");

        let profile = if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<UserTasteProfile>(&s).ok())
                .unwrap_or_default()
        } else {
            UserTasteProfile::default()
        };

        Self { config_path: path, profile }
    }

    pub fn save(&self) {
        if let Ok(data) = serde_json::to_string_pretty(&self.profile) {
            let _ = settings::write_private(&self.config_path, &data);
        }
    }

    /// Record listening activity & update algorithm score
    pub fn record_play(&mut self, track: TrackItem) {
        self.profile.total_listens += 1;

        let artist = track.artist.clone();
        if !artist.is_empty() {
            *self.profile.artist_scores.entry(artist).or_insert(0.0) += 1.0;
        }

        let id = track.media_id.clone();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = self.profile.track_scores.entry(id).or_insert_with(|| TrackScore {
            track: track.clone(),
            play_count: 0,
            completion_score: 1.0,
            last_played_ts: ts,
        });

        entry.play_count += 1;
        entry.last_played_ts = ts;

        self.save();
    }

    /// Get top #1 favorite artist derived from user listening taste
    pub fn get_top_artist(&self) -> Option<String> {
        let mut sorted: Vec<(&String, &f32)> = self.profile.artist_scores.iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.first().map(|(k, _)| (*k).clone())
    }

    /// Get top favorite artists derived from user listening taste
    pub fn get_top_artists(&self, limit: usize) -> Vec<String> {
        let mut sorted: Vec<(&String, &f32)> = self.profile.artist_scores.iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.into_iter().take(limit).map(|(k, _)| k.clone()).collect()
    }

    /// Get top heavy rotation tracks
    pub fn get_heavy_rotation(&self, limit: usize) -> Vec<TrackItem> {
        let mut sorted: Vec<&TrackScore> = self.profile.track_scores.values().collect();
        sorted.sort_by(|a, b| b.play_count.cmp(&a.play_count));
        sorted.into_iter().take(limit).map(|s| s.track.clone()).collect()
    }

    /// Filter tracks to guarantee zero duplicates across Home screen shelves
    pub fn filter_unique(tracks: &[TrackItem], seen: &mut HashSet<String>) -> Vec<TrackItem> {
        let mut result = Vec::new();
        for t in tracks {
            if seen.insert(t.media_id.clone()) {
                result.push(t.clone());
            }
        }
        result
    }
}
