use std::fs;
use std::path::PathBuf;
use std::thread;

pub struct DataSaver {
    cache_dir: PathBuf,
}

impl DataSaver {
    pub fn new() -> Self {
        let base_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("meduza-music")
            .join("audio");
        let _ = fs::create_dir_all(&base_dir);
        Self { cache_dir: base_dir }
    }

    /// Check if track audio is cached locally. Returns local file path if present.
    pub fn get_cached_file(&self, media_id: &str) -> Option<String> {
        for ext in &["webm", "m4a", "opus", "mp3"] {
            let path = self.cache_dir.join(format!("{}.{}", media_id, ext));
            if path.exists() {
                if let Ok(meta) = fs::metadata(&path) {
                    if meta.len() > 100_000 {
                        return Some(path.to_string_lossy().to_string());
                    }
                }
            }
        }
        None
    }

    /// Cache stream asynchronously in the background for 0-data replays.
    pub fn cache_stream_in_bg(&self, media_id: String, stream_url: String) {
        let dir = self.cache_dir.clone();
        thread::spawn(move || {
            let ext = if stream_url.contains("mime=audio%2Fwebm") || stream_url.contains(".webm") {
                "webm"
            } else if stream_url.contains("mime=audio%2Fmp4") || stream_url.contains(".m4a") {
                "m4a"
            } else {
                "webm"
            };
            let target_path = dir.join(format!("{}.{}", media_id, ext));
            if target_path.exists() {
                return;
            }

            println!("[DataSaver] Caching audio for {} in background...", media_id);
            let temp_path = dir.join(format!("{}.tmp", media_id));

            if let Ok(resp) = ureq::get(&stream_url).timeout(std::time::Duration::from_secs(180)).call() {
                if let Ok(mut file) = fs::File::create(&temp_path) {
                    let mut reader = resp.into_reader();
                    if std::io::copy(&mut reader, &mut file).is_ok() {
                        let _ = fs::rename(&temp_path, &target_path);
                        println!("[DataSaver] Cached {} (0-data on future replays!).", media_id);
                    }
                }
            }
            let _ = fs::remove_file(&temp_path);
        });
    }
}
