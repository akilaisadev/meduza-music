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

    /// Check total size of cached audio files in megabytes.
    pub fn get_cache_size_mb(&self) -> f32 {
        let mut total_bytes = 0u64;
        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    total_bytes += meta.len();
                }
            }
        }
        (total_bytes as f32) / (1024.0 * 1024.0)
    }

    /// Clear all cached audio files to free up disk space.
    pub fn clear_cache(&self) {
        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
        println!("[DataSaver] Cleared audio cache successfully!");
    }

    /// Enforce maximum disk cache size limit in MB (LRU auto-purge oldest files).
    pub fn enforce_max_cache_size(&self, max_size_mb: u64) {
        let max_bytes = max_size_mb * 1024 * 1024;
        let Ok(entries) = fs::read_dir(&self.cache_dir) else { return; };

        let mut files = Vec::new();
        let mut total_bytes = 0u64;

        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    let len = meta.len();
                    total_bytes += len;
                    let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    files.push((entry.path(), len, modified));
                }
            }
        }

        if total_bytes <= max_bytes { return; }

        files.sort_by_key(|f| f.2);

        println!("[DataSaver] Purging oldest cache files to enforce {} MB storage cap...", max_size_mb);
        for (path, len, _) in files {
            if total_bytes <= max_bytes { break; }
            if fs::remove_file(&path).is_ok() {
                total_bytes = total_bytes.saturating_sub(len);
            }
        }
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
    pub fn cache_stream_in_bg(&self, media_id: String, stream_url: String, max_cache_mb: u64) {
        let dir = self.cache_dir.clone();
        let self_dir = self.cache_dir.clone();
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

            let temp_path = dir.join(format!("{}.tmp", media_id));

            if let Ok(resp) = ureq::get(&stream_url).timeout(std::time::Duration::from_secs(180)).call() {
                if let Ok(mut file) = fs::File::create(&temp_path) {
                    let mut reader = resp.into_reader();
                    if std::io::copy(&mut reader, &mut file).is_ok() {
                        let _ = fs::rename(&temp_path, &target_path);
                        println!("[DataSaver] Cached {} (0-data on future replays!).", media_id);

                        // Enforce max cache size limit after new file save
                        let ds = DataSaver { cache_dir: self_dir };
                        ds.enforce_max_cache_size(max_cache_mb);
                    }
                }
            }
            let _ = fs::remove_file(&temp_path);
        });
    }
}
