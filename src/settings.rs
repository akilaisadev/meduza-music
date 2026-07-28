use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioQuality {
    DataSaver,
    Normal,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub audio_quality: AudioQuality,
    pub enable_cache: bool,
    pub gapless_playback: bool,
    pub autoplay_radio: bool,
    pub preload_next_track: bool,
    pub normalize_volume: bool,
    pub low_end_mode: bool,
    pub max_cache_size_mb: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            audio_quality: AudioQuality::High,
            enable_cache: true,
            gapless_playback: true,
            autoplay_radio: true,
            preload_next_track: true,
            normalize_volume: true,
            low_end_mode: false,
            max_cache_size_mb: 500,
        }
    }
}

impl AppSettings {
    fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("meduza-music");
        fs::create_dir_all(&path).ok();
        path.push("settings.json");
        path
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str(&data) {
                return settings;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(data) = serde_json::to_string_pretty(self) {
            fs::write(path, data).ok();
        }
    }
}
