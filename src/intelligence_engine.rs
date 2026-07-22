use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use directories::ProjectDirs;
use rand::Rng;
use crate::youtube_fetcher::TrackItem;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MoodTag {
    Upbeat,
    Chill,
    Epic,
    Ambient,
    Dance,
    Romantic,
    Melancholy,
    Focus,
    Party,
    Unknown,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct TasteProfile {
    pub artist_play_counts: HashMap<String, u32>,
    pub track_play_counts: HashMap<String, u32>,
    pub track_skips: HashMap<String, u32>,
    pub genre_affinity: HashMap<String, f64>,
    pub recently_played_ids: Vec<String>,
    pub liked_track_ids: HashSet<String>,
}

pub struct MeduzaIntelligenceEngine {
    pub profile: TasteProfile,
}

impl MeduzaIntelligenceEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            profile: TasteProfile::default(),
        };
        engine.load_profile();
        engine
    }

    fn get_profile_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("org", "meduza", "Meduza") {
            let config_dir = proj_dirs.config_dir();
            fs::create_dir_all(config_dir).unwrap_or(());
            config_dir.join("taste_profile.json")
        } else {
            PathBuf::from("taste_profile.json")
        }
    }

    pub fn load_profile(&mut self) {
        let path = Self::get_profile_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(parsed) = serde_json::from_str::<TasteProfile>(&content) {
                    self.profile = parsed;
                    return;
                }
            }
        }
        self.profile = TasteProfile::default();
    }

    pub fn save_profile(&self) {
        let path = Self::get_profile_path();
        if let Ok(content) = serde_json::to_string_pretty(&self.profile) {
            if let Ok(mut file) = File::create(path) {
                let _ = file.write_all(content.as_bytes());
            }
        }
    }

    pub fn record_play(&mut self, track: &TrackItem) {
        let artist_key = track.artist.to_lowercase().trim().to_string();
        let track_id = track.media_id.clone();

        *self.profile.artist_play_counts.entry(artist_key.clone()).or_insert(0) += 1;
        *self.profile.track_play_counts.entry(track_id.clone()).or_insert(0) += 1;

        // Add keyword genre affinity
        let keywords = Self::extract_keywords(&format!("{} {}", track.title, track.artist));
        for kw in keywords {
            let aff = self.profile.genre_affinity.entry(kw).or_insert(0.0);
            *aff = (*aff + 0.15).min(10.0);
        }

        // Add to recently played (limit to last 20)
        self.profile.recently_played_ids.retain(|id| id != &track_id);
        self.profile.recently_played_ids.push(track_id);
        if self.profile.recently_played_ids.len() > 20 {
            self.profile.recently_played_ids.remove(0);
        }

        self.save_profile();
    }

    pub fn record_skip(&mut self, track: &TrackItem) {
        let artist_key = track.artist.to_lowercase().trim().to_string();
        let track_id = track.media_id.clone();

        *self.profile.track_skips.entry(track_id).or_insert(0) += 1;

        // Penalize keywords
        let keywords = Self::extract_keywords(&format!("{} {}", track.title, track.artist));
        for kw in keywords {
            let aff = self.profile.genre_affinity.entry(kw).or_insert(0.0);
            *aff = (*aff - 0.12).max(-5.0);
        }

        // Decay artist count
        if let Some(plays) = self.profile.artist_play_counts.get_mut(&artist_key) {
            if *plays > 0 {
                *plays -= 1;
            }
        }

        self.save_profile();
    }

    pub fn is_liked(&self, media_id: &str) -> bool {
        self.profile.liked_track_ids.contains(media_id)
    }

    pub fn toggle_like(&mut self, track: &TrackItem) {
        let media_id = track.media_id.clone();
        if self.profile.liked_track_ids.contains(&media_id) {
            self.profile.liked_track_ids.remove(&media_id);
        } else {
            self.profile.liked_track_ids.insert(media_id);
        }
        self.save_profile();
    }

    fn extract_keywords(text: &str) -> Vec<String> {
        let stopwords: HashSet<&str> = [
            "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for", "with", "by",
            "of", "is", "are", "was", "were", "it", "you", "me", "him", "her", "them", "us",
            "my", "your", "his", "its", "their", "our", "this", "that", "these", "those", "lyrics",
            "audio", "official", "video", "music", "full", "hd", "hq", "remix", "feat", "ft"
        ].iter().cloned().collect();

        text.split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase())
            .filter(|w| w.len() > 2 && !stopwords.contains(w.as_str()))
            .collect()
    }

    pub fn score_track(&self, track: &TrackItem, hour_of_day: Option<u32>) -> f64 {
        let mut score = 1.0;
        let artist_key = track.artist.to_lowercase().trim().to_string();
        let track_id = &track.media_id;

        // Signal 1: Artist Play Affinity
        let artist_plays = *self.profile.artist_play_counts.get(&artist_key).unwrap_or(&0) as f64;
        let max_count = self.profile.artist_play_counts.values().max().cloned().unwrap_or(1).max(1) as f64;
        let affinity_score = (artist_plays / max_count).sqrt();
        score += affinity_score * 2.0;

        // Signal 2: Keyword / Genre Affinity
        let keywords = Self::extract_keywords(&format!("{} {}", track.title, track.artist));
        let mut genre_score = 0.0;
        for kw in keywords {
            genre_score += self.profile.genre_affinity.get(&kw).cloned().unwrap_or(0.0);
        }
        score += genre_score * 1.5;

        // Signal 3: Skip-to-Play Ratio Penalty
        let plays = *self.profile.track_play_counts.get(track_id).unwrap_or(&0) as f64;
        let skips = *self.profile.track_skips.get(track_id).unwrap_or(&0) as f64;
        if skips > 0.0 {
            let ratio = skips / (plays + skips);
            score *= 1.0 - ratio * 0.85;
        }

        // Signal 3b: Explicit Favorites Boost
        if self.profile.liked_track_ids.contains(track_id) {
            score += 6.5;
        }

        // Signal 4: Recency Penalty
        if self.profile.recently_played_ids.contains(track_id) {
            score *= 0.15;
        }

        // Signal 5: Time of Day Energy Arc Boost
        let preferred_tags = Self::get_energy_arc_tags(hour_of_day);
        let track_tags = Self::detect_mood_tags(&track.title, &track.artist);
        let overlap = preferred_tags.intersection(&track_tags).count() as f64;
        score += overlap * 0.4;

        // Signal 6: Mild randomness
        let mut rng = rand::thread_rng();
        score += rng.gen::<f64>() * 0.35;

        score.max(0.001)
    }

    pub fn shuffle_with_intelligence(&self, items: &[TrackItem], hour_of_day: Option<u32>) -> Vec<usize> {
        if items.len() <= 1 {
            return (0..items.len()).collect();
        }

        let mut remaining: Vec<usize> = (0..items.len()).collect();
        let mut result = Vec::new();
        let mut recent_artists: Vec<String> = Vec::new();
        let mut rng = rand::thread_rng();

        while !remaining.is_empty() {
            let mut scores = Vec::new();
            for &idx in &remaining {
                let item = &items[idx];
                let mut final_score = self.score_track(item, hour_of_day);

                // Diversity window penalty
                let artist_key = item.artist.to_lowercase().trim().to_string();
                let appearances = recent_artists.iter().filter(|&a| a == &artist_key).count() as f64;
                if appearances > 0.0 {
                    final_score *= (-appearances * 1.2).exp();
                }
                scores.push(final_score);
            }

            let total_weight: f64 = scores.iter().sum();
            let mut pick = rng.gen::<f64>() * total_weight;
            let mut chosen_pos = 0;

            for (i, &score) in scores.iter().enumerate() {
                pick -= score;
                if pick <= 0.0 {
                    chosen_pos = i;
                    break;
                }
            }

            let chosen_orig_idx = remaining.remove(chosen_pos);
            result.push(chosen_orig_idx);

            let chosen_artist = items[chosen_orig_idx].artist.to_lowercase().trim().to_string();
            if !chosen_artist.is_empty() {
                recent_artists.push(chosen_artist);
                if recent_artists.len() > 5 {
                    recent_artists.remove(0);
                }
            }
        }

        result
    }

    pub fn detect_mood_tags(title: &str, artist: &str) -> HashSet<MoodTag> {
        let combined = format!("{} {}", title, artist).to_lowercase();
        let mut tags = HashSet::new();

        let upbeat = ["dance", "party", "club", "hit", "pop", "feel good", "summer", "happy", "fun", "bop"];
        let chill = ["chill", "lofi", "lo-fi", "relax", "slow", "calm", "easy", "coffee", "night drive", "bedroom"];
        let epic = ["epic", "power", "anthem", "rise", "fire", "hype", "battle", "boss", "strong", "warrior"];
        let ambient = ["ambient", "space", "dream", "sleep", "meditation", "wave", "ocean", "forest", "rain", "ethereal"];
        let dance = ["edm", "techno", "electro", "house", "bass", "drop", "remix", "rave", "synthwave", "disco"];
        let romantic = ["love", "heart", "kiss", "romance", "soul", "tender", "forever", "darling", "sweetheart", "adore"];
        let melancholy = ["sad", "broken", "cry", "alone", "tears", "miss", "lost", "goodbye", "hurt", "empty"];
        let focus = ["study", "focus", "work", "productivity", "concentrate", "instrumental", "piano", "classical", "jazz"];
        let party = ["turn up", "lit", "shot", "drunk", "weekend", "friday", "saturday", "crowd", "loud", "anthem"];

        if upbeat.iter().any(|&k| combined.contains(k)) { tags.insert(MoodTag::Upbeat); }
        if chill.iter().any(|&k| combined.contains(k)) { tags.insert(MoodTag::Chill); }
        if epic.iter().any(|&k| combined.contains(k)) { tags.insert(MoodTag::Epic); }
        if ambient.iter().any(|&k| combined.contains(k)) { tags.insert(MoodTag::Ambient); }
        if dance.iter().any(|&k| combined.contains(k)) { tags.insert(MoodTag::Dance); }
        if romantic.iter().any(|&k| combined.contains(k)) { tags.insert(MoodTag::Romantic); }
        if melancholy.iter().any(|&k| combined.contains(k)) { tags.insert(MoodTag::Melancholy); }
        if focus.iter().any(|&k| combined.contains(k)) { tags.insert(MoodTag::Focus); }
        if party.iter().any(|&k| combined.contains(k)) { tags.insert(MoodTag::Party); }

        if tags.is_empty() {
            tags.insert(MoodTag::Unknown);
        }
        tags
    }

    pub fn get_energy_arc_tags(hour_of_day: Option<u32>) -> HashSet<MoodTag> {
        let hour = hour_of_day.unwrap_or(chrono::Local::now().time().hour);
        let mut tags = HashSet::new();
        if hour >= 5 && hour <= 8 {
            tags.insert(MoodTag::Upbeat); tags.insert(MoodTag::Focus); tags.insert(MoodTag::Chill);
        } else if hour >= 9 && hour <= 11 {
            tags.insert(MoodTag::Focus); tags.insert(MoodTag::Epic); tags.insert(MoodTag::Upbeat);
        } else if hour >= 12 && hour <= 13 {
            tags.insert(MoodTag::Upbeat); tags.insert(MoodTag::Dance); tags.insert(MoodTag::Party);
        } else if hour >= 14 && hour <= 17 {
            tags.insert(MoodTag::Upbeat); tags.insert(MoodTag::Dance); tags.insert(MoodTag::Romantic);
        } else if hour >= 18 && hour <= 20 {
            tags.insert(MoodTag::Chill); tags.insert(MoodTag::Romantic); tags.insert(MoodTag::Melancholy);
        } else if hour >= 21 && hour <= 23 {
            tags.insert(MoodTag::Ambient); tags.insert(MoodTag::Chill); tags.insert(MoodTag::Melancholy);
        } else {
            tags.insert(MoodTag::Ambient); tags.insert(MoodTag::Focus); tags.insert(MoodTag::Chill);
        }
        tags
    }
}

// Chrono hour replacement for std environment:
mod chrono {
    pub struct Local;
    pub struct Time {
        pub hour: u32,
    }
    impl Local {
        pub fn now() -> Self { Local }
        pub fn time(&self) -> Time {
            use std::time::{SystemTime, UNIX_EPOCH};
            let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            // Simple hour estimation (UTC for simplicity)
            let hour = ((secs % 86400) / 3600) as u32;
            Time { hour }
        }
    }
}
