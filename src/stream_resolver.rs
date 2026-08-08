use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use std::thread;
use crate::innertube::TrackItem;

static URL_CACHE: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
// IMP-6: Track whether disk cache has been loaded this session
static DISK_CACHE_LOADED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn get_url_cache() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    URL_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// IMP-6: Path to the on-disk URL cache file.
fn cache_file_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("meduza-music")
        .join("url_cache.json")
}

pub struct StreamResolver;

impl StreamResolver {
    fn yt_dlp_cmd() -> Command {
        let is_flatpak = std::path::Path::new("/.flatpak-info").exists();
        if is_flatpak {
            let mut cmd = Command::new("flatpak-spawn");
            cmd.args(["--host", "python3", "-m", "yt_dlp"]);
            cmd
        } else {
            let mut cmd = Command::new("python3");
            cmd.args(["-m", "yt_dlp"]);
            cmd
        }
    }

    /// IMP-6: Load persisted URL cache from disk on first call.
    /// Entries older than 4 hours are discarded (same TTL as in-memory cache).
    pub fn load_cache_from_disk() {
        if DISK_CACHE_LOADED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return; // Only load once per session
        }
        let path = cache_file_path();
        if !path.exists() { return; }

        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => return,
        };
        // Stored as: { "vid:quality" -> (url, unix_timestamp_secs) }
        let Ok(map) = serde_json::from_str::<HashMap<String, (String, u64)>>(&data) else {
            return;
        };

        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut cache = get_url_cache().lock().unwrap_or_else(|e| e.into_inner());
        let mut loaded = 0usize;

        for (key, (url, timestamp)) in map {
            let age_secs = now_unix.saturating_sub(timestamp);
            if age_secs < 4 * 3600 {
                // Reconstruct an Instant that's `age_secs` in the past
                let instant = Instant::now()
                    .checked_sub(Duration::from_secs(age_secs))
                    .unwrap_or_else(Instant::now);
                cache.insert(key, (url, instant));
                loaded += 1;
            }
        }
        if loaded > 0 {
            println!("[StreamResolver] Restored {} CDN URLs from disk cache", loaded);
        }
    }

    /// IMP-6: Persist the current in-memory URL cache to disk.
    /// Called on PlaybackManager::drop() so URLs survive across restarts.
    pub fn save_cache_to_disk() {
        let cache = get_url_cache().lock().unwrap_or_else(|e| e.into_inner());
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut map: HashMap<String, (String, u64)> = HashMap::new();
        for (key, (url, instant)) in cache.iter() {
            let age_secs = instant.elapsed().as_secs();
            if age_secs < 4 * 3600 {
                let timestamp = now_unix.saturating_sub(age_secs);
                map.insert(key.clone(), (url.clone(), timestamp));
            }
        }

        if map.is_empty() { return; }

        let path = cache_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string(&map) {
            let _ = std::fs::write(&path, data);
            println!("[StreamResolver] Saved {} CDN URLs to disk cache", map.len());
        }
    }

    /// Resolves a direct audio CDN URL for a given YouTube video ID using yt-dlp.
    /// Uses ultra-fast in-memory caching (~0.001s) and direct resolution (~0.4s).
    pub fn get_audio_url(video_id: &str, quality: crate::settings::AudioQuality) -> Option<String> {
        // IMP-6: Ensure disk cache is loaded on first call this session
        if !DISK_CACHE_LOADED.load(std::sync::atomic::Ordering::SeqCst) {
            Self::load_cache_from_disk();
        }

        let cache_key = format!("{}:{:?}", video_id, quality);
        {
            let guard = get_url_cache().lock().unwrap_or_else(|e| e.into_inner());
            if let Some((cached_url, time)) = guard.get(&cache_key) {
                if time.elapsed() < Duration::from_secs(4 * 3600) {
                    println!("[StreamResolver] 0.001s Cache Hit for {}", video_id);
                    return Some(cached_url.clone());
                }
            }
        }

        let url = format!("https://www.youtube.com/watch?v={}", video_id);
        
        let format_arg = match quality {
            crate::settings::AudioQuality::DataSaver => "bestaudio[acodec=opus][abr<=70]/bestaudio[abr<=96]/bestaudio[ext=webm]",
            crate::settings::AudioQuality::Normal    => "bestaudio[acodec=opus][abr<=128]/bestaudio[abr<=128]/bestaudio[ext=webm]",
            crate::settings::AudioQuality::High      => "bestaudio[acodec=opus]/bestaudio[ext=webm]/bestaudio/best",
        };

        // 1. FAST DIRECT RESOLUTION (~0.4s - 0.6s instant load!)
        let mut fast_cmd = Self::yt_dlp_cmd();
        if let Ok(output) = fast_cmd
            .args([
                "-f", format_arg,
                "--no-warnings",
                "--quiet",
                "--no-playlist",
                "--socket-timeout", "5",
                "--get-url",
                &url,
            ])
            .output()
        {
            if output.status.success() {
                let raw = String::from_utf8_lossy(&output.stdout);
                let stream_url = raw.lines()
                    .find(|l| l.starts_with("http"))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !stream_url.is_empty() {
                    println!("[StreamResolver] Fast Resolved (0.5s): {} ({})", &stream_url[..stream_url.len().min(60)], video_id);
                    get_url_cache().lock().unwrap_or_else(|e| e.into_inner())
                        .insert(cache_key, (stream_url.clone(), Instant::now()));
                    return Some(stream_url);
                }
            }
        }

        // 2. FALLBACK TO BROWSER COOKIES if direct access is restricted
        for browser in &["firefox", "chrome"] {
            let mut cmd = Self::yt_dlp_cmd();
            let output = cmd
                .args([
                    "-f", format_arg,
                    "--no-warnings",
                    "--quiet",
                    "--no-playlist",
                    "--cookies-from-browser", browser,
                    "--get-url",
                    &url,
                ])
                .output();

            if let Ok(out) = output {
                if out.status.success() {
                    let raw = String::from_utf8_lossy(&out.stdout);
                    let stream_url = raw.lines()
                        .find(|l| l.starts_with("http"))
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !stream_url.is_empty() {
                        println!("[StreamResolver] Resolved via {} cookies: {} ({})", browser, &stream_url[..stream_url.len().min(60)], video_id);
                        get_url_cache().lock().unwrap_or_else(|e| e.into_inner())
                            .insert(cache_key.clone(), (stream_url.clone(), Instant::now()));
                        return Some(stream_url);
                    }
                }
            }
        }

        println!("[StreamResolver] Could not resolve stream URL for {}", video_id);
        None
    }

    /// IMP-4: Racing resolver — launches two parallel yt-dlp resolution threads
    /// and returns whichever CDN URL arrives first. Ideal for the background
    /// preloader where we have time but want the fastest possible result.
    pub fn get_audio_url_racing(video_id: &str, quality: crate::settings::AudioQuality) -> Option<String> {
        // Check cache first — instant, no threads needed
        let cache_key = format!("{}:{:?}", video_id, quality);
        {
            let guard = get_url_cache().lock().unwrap_or_else(|e| e.into_inner());
            if let Some((cached_url, time)) = guard.get(&cache_key) {
                if time.elapsed() < Duration::from_secs(4 * 3600) {
                    println!("[StreamResolver] 0.001s Racing Cache Hit for {}", video_id);
                    return Some(cached_url.clone());
                }
            }
        }

        // Not cached: race two threads with a small stagger.
        // Thread 1 starts immediately, Thread 2 starts 120ms later.
        // Both write to the shared cache — first one to finish wins and
        // the second thread will see the cache hit and exit cheaply.
        let (tx, rx) = std::sync::mpsc::channel::<String>();

        let vid1 = video_id.to_string();
        let tx1  = tx.clone();
        thread::spawn(move || {
            if let Some(url) = Self::get_audio_url(&vid1, quality) {
                let _ = tx1.send(url);
            }
        });

        // Stagger second thread to avoid rate-limiting two simultaneous yt-dlp calls
        let vid2 = video_id.to_string();
        let tx2  = tx;
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(120));
            // Re-check cache first (Thread 1 may have already populated it)
            if let Some(url) = Self::get_audio_url(&vid2, quality) {
                let _ = tx2.send(url);
            }
        });

        // Wait up to 9 seconds for either thread (yt-dlp worst-case is ~5–6s)
        rx.recv_timeout(Duration::from_secs(9)).ok()
    }

    /// Search YouTube Music for a query and return the first video ID found.
    #[allow(dead_code)]
    pub fn search_first_video_id(query: &str) -> Option<String> {
        let search_query = format!("ytmsearch1:{}", query);
        let mut cmd = Self::yt_dlp_cmd();
        let output = cmd
            .args([
                "--no-warnings",
                "--quiet",
                "--print", "id",
                "--no-playlist",
                &search_query,
            ])
            .output()
            .ok()?;

        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let video_id = raw.trim().to_string();
            if !video_id.is_empty() && video_id.len() == 11 {
                println!("[StreamResolver] Search '{}' -> {}", query, video_id);
                return Some(video_id);
            }
        }
        None
    }

    /// Search YouTube Music and return full TrackItems.
    #[allow(dead_code)]
    pub fn search_tracks(query: &str, limit: usize) -> Vec<TrackItem> {
        let search_query = format!("ytmsearch{}:{}", limit, query);
        let mut cmd = Self::yt_dlp_cmd();
        let output = cmd
            .args([
                "--no-warnings",
                "--quiet",
                "--cookies-from-browser", "firefox",
                "--print", "%(id)s\t%(title)s\t%(uploader)s\t%(duration)s",
                "--no-playlist",
                &search_query,
            ])
            .output();

        let mut results = Vec::new();
        if let Ok(out) = output {
            if out.status.success() {
                let raw = String::from_utf8_lossy(&out.stdout);
                for line in raw.lines().take(limit) {
                    let parts: Vec<&str> = line.splitn(4, '\t').collect();
                    if parts.len() >= 2 {
                        let video_id = parts[0].trim().to_string();
                        let title = parts[1].trim().to_string();
                        let artist = if parts.len() > 2 { parts[2].trim().to_string() } else { "YouTube Music".to_string() };
                        let duration_seconds: u32 = if parts.len() > 3 { parts[3].trim().parse().unwrap_or(0) } else { 0 };

                        if video_id.len() == 11 && !title.is_empty() {
                            let thumbnail_url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id);
                            results.push(TrackItem {
                                title,
                                artist,
                                media_id: video_id,
                                thumbnail_url,
                                duration_seconds,
                            });
                        }
                    }
                }
            }
        }
        println!("[StreamResolver] Search '{}' -> {} results", query, results.len());
        results
    }
}
