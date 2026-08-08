use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::thread;

use crate::settings::{self, Semaphore};

pub struct DataSaver {
    cache_dir: PathBuf,
}

// Bound concurrent background cache downloads so a large queue cannot spawn an
// unbounded number of network connections / disk writes (CPU & bandwidth overload).
static DL_SEM: Semaphore = Semaphore::new(2);

/// Keep only alphanumeric chars in media IDs used for filenames — prevents
/// path traversal / symlink tricks via a crafted video ID.
fn sanitize_id(media_id: &str) -> String {
    media_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(64)
        .collect()
}

impl DataSaver {
    pub fn new() -> Self {
        let base_dir = settings::app_cache_root().join("audio");
        settings::ensure_private_dir(&base_dir);
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
        let id = sanitize_id(media_id);
        if id.is_empty() { return None; }
        for ext in &["webm", "m4a", "opus", "mp3"] {
            let path = self.cache_dir.join(format!("{}.{}", id, ext));
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
    ///
    /// Downloads go through the shared `download` worker pool (bounded to a
    /// couple of threads) instead of spawning a raw thread per track. When
    /// `pace_downloads` is set (low-end mode) the read loop is staggered so a
    /// big file write never starves the audio currently being streamed.
    pub fn cache_stream_in_bg(&self, media_id: String, stream_url: String, max_cache_mb: u64, pace_downloads: bool) {
        if stream_url.starts_with('/') {
            return; // Already a local disk file
        }
        // SSRF defense: only cache URLs from known Google/YouTube CDN hosts.
        if !settings::host_is_allowed(&stream_url, settings::UrlKind::Stream) {
            println!("[DataSaver] Refusing to cache non-whitelisted stream host.");
            return;
        }

        let dir = self.cache_dir.clone();
        crate::workers::download().submit(move || {
            // Give mpv 4 seconds of uninterrupted network priority to build audio buffer
            thread::sleep(std::time::Duration::from_secs(4));

            let id = sanitize_id(&media_id);
            if id.is_empty() { return; }

            let ext = if stream_url.contains("mime=audio%2Fwebm") || stream_url.contains(".webm") {
                "webm"
            } else if stream_url.contains("mime=audio%2Fmp4") || stream_url.contains(".m4a") {
                "m4a"
            } else {
                "webm"
            };
            let target_path = dir.join(format!("{}.{}", id, ext));
            if target_path.exists() {
                return;
            }

            // BUG-07: Validate URL is still live before downloading
            // YouTube CDN URLs expire — check with a HEAD request first.
            // fetch_allowed re-validates the allowlist on every redirect hop.
            let url_still_valid = settings::fetch_allowed(
                &stream_url,
                settings::UrlKind::Stream,
                settings::FetchMethod::Head,
                std::time::Duration::from_secs(8),
            )
            .map(|r| r.status() == 200 || r.status() == 206)
            .unwrap_or(false);

            if !url_still_valid {
                println!("[DataSaver] Stream URL for {} has expired, skipping cache.", id);
                return;
            }

            let temp_path = dir.join(format!("{}.tmp", id));

            // Bound concurrent downloads: wait for a free slot.
            let _guard = DL_SEM.acquire();

            if let Ok(resp) = settings::fetch_allowed(
                &stream_url,
                settings::UrlKind::Stream,
                settings::FetchMethod::Get,
                std::time::Duration::from_secs(180),
            ) {
                if let Ok(mut file) = fs::File::create(&temp_path) {
                    use std::io::Read;
                    let mut reader = resp.into_reader();
                    let mut buf = vec![0u8; 512 * 1024];
                    let mut copied_ok = true;
                    loop {
                        match reader.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                if file.write_all(&buf[..n]).is_err() {
                                    copied_ok = false;
                                    break;
                                }
                                // Pace: small sleep per chunk so a big download
                                // yields the disk/network to the playing audio.
                                if pace_downloads {
                                    std::thread::sleep(std::time::Duration::from_millis(2));
                                }
                            }
                            Err(_) => {
                                copied_ok = false;
                                break;
                            }
                        }
                    }
                    if copied_ok {
                        let _ = fs::rename(&temp_path, &target_path);
                        println!("[DataSaver] Cached {} (0-data on future replays!).", id);

                        // Enforce max cache size limit after new file save
                        let ds = DataSaver { cache_dir: dir };
                        ds.enforce_max_cache_size(max_cache_mb);
                    }
                }
            }
            let _ = fs::remove_file(&temp_path);
        });
    }
}

