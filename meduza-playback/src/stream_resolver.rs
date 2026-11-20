use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::thread;
use meduza_core::settings::{self, Semaphore};

/// In-memory CDN URL cache. `order` is insertion order; since every entry has
/// the same TTL, FIFO eviction is also expiry-order eviction — no O(n) scan.
struct UrlCache {
    entries: HashMap<String, (String, Instant)>,
    order: VecDeque<String>,
}

static URL_CACHE: OnceLock<Mutex<UrlCache>> = OnceLock::new();
// IMP-6: Track whether disk cache has been loaded this session
static DISK_CACHE_LOADED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// Bound concurrent yt-dlp (python) resolutions. Each python process is heavy —
// an unbounded number of simultaneous preloader/racer/warmer calls was a major
// CPU overload source.
static RESOLVE_SEM: Semaphore = Semaphore::new(3);

const URL_CACHE_MAX: usize = 1024;
const URL_CACHE_TTL: Duration = Duration::from_secs(4 * 3600);

fn get_url_cache() -> &'static Mutex<UrlCache> {
    URL_CACHE.get_or_init(|| Mutex::new(UrlCache {
        entries: HashMap::new(),
        order: VecDeque::new(),
    }))
}

/// Drop expired entries from the front of the FIFO queue.
fn prune_locked(cache: &mut UrlCache) {
    loop {
        let Some(oldest) = cache.order.front().cloned() else { break; };
        let expired = cache.entries.get(&oldest)
            .map(|(_, t)| t.elapsed() >= URL_CACHE_TTL)
            .unwrap_or(true);
        if !expired { break; }
        cache.order.pop_front();
        cache.entries.remove(&oldest);
    }
}

/// YouTube CDN URLs carry an absolute `expire=<unix secs>` query parameter.
/// Reject a URL shortly before that deadline so mpv never receives a URL that
/// is about to start returning 403. URLs without the parameter fall back to
/// the local TTL check only.
fn url_is_fresh(url: &str) -> bool {
    let Some(query) = url.split_once('?').map(|(_, q)| q) else { return true; };
    let Some(expire) = query
        .split('&')
        .find_map(|p| p.strip_prefix("expire="))
        .and_then(|v| v.parse::<u64>().ok())
    else { return true; };

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    expire > now + 60
}

/// Look up a cached URL, validating both the local TTL and the CDN's absolute
/// expiry. Stale entries are evicted so the caller re-resolves them.
fn cached_url(key: &str) -> Option<String> {
    let mut cache = get_url_cache().lock().unwrap_or_else(|e| e.into_inner());
    prune_locked(&mut cache);
    let fresh = cache.entries.get(key).map(|(url, t)| {
        t.elapsed() < URL_CACHE_TTL && url_is_fresh(url)
    }).unwrap_or(false);
    if fresh {
        cache.entries.get(key).map(|(url, _)| url.clone())
    } else {
        if cache.entries.remove(key).is_some() {
            // The FIFO entry will be skipped by prune_locked next time.
        }
        None
    }
}

/// Insert into the in-memory URL cache with bounded, O(1) amortised eviction.
fn url_cache_insert(key: String, value: (String, Instant)) {
    let mut cache = get_url_cache().lock().unwrap_or_else(|e| e.into_inner());
    prune_locked(&mut cache);
    if !cache.entries.contains_key(&key) {
        while cache.entries.len() >= URL_CACHE_MAX {
            let Some(oldest) = cache.order.pop_front() else { break; };
            cache.entries.remove(&oldest);
        }
        cache.order.push_back(key.clone());
    }
    cache.entries.insert(key, value);
}

/// IMP-6: Path to the on-disk URL cache file.
fn cache_file_path() -> PathBuf {
    settings::app_cache_root().join("url_cache.json")
}

pub struct StreamResolver;

impl StreamResolver {
    /// Prefer a standalone `yt-dlp` executable (used by the Flatpak build) and
    /// fall back to the Python module for source/AppImage installs. Detection
    /// is cached so we do not spawn a `--version` process on every resolve.
    fn yt_dlp_cmd() -> Command {
        // The .deb bundles a current yt-dlp here so it works even when the
        // desktop session PATH does not include /usr/lib/meduza-music.
        const BUNDLED: &str = "/usr/lib/meduza-music/yt-dlp";

        static USE_BINARY: OnceLock<bool> = OnceLock::new();
        let use_binary = *USE_BINARY.get_or_init(|| {
            if std::path::Path::new(BUNDLED).exists() {
                return true;
            }
            Command::new("yt-dlp")
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        });

        if use_binary {
            if std::path::Path::new(BUNDLED).exists() {
                Command::new(BUNDLED)
            } else {
                Command::new("yt-dlp")
            }
        } else {
            let mut cmd = Command::new("python3");
            cmd.args(["-m", "yt_dlp"]);
            cmd
        }
    }

    /// IMP-6: Load persisted URL cache from disk on first call.
    /// Entries older than the TTL are discarded (same TTL as in-memory cache).
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

        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut cache = get_url_cache().lock().unwrap_or_else(|e| e.into_inner());
        let mut loaded = 0usize;

        for (key, (url, timestamp)) in map {
            if loaded >= URL_CACHE_MAX { break; }
            let age_secs = now_unix.saturating_sub(timestamp);
            if age_secs < URL_CACHE_TTL.as_secs() && url_is_fresh(&url) {
                // Reconstruct an Instant that's `age_secs` in the past
                let instant = Instant::now()
                    .checked_sub(Duration::from_secs(age_secs))
                    .unwrap_or_else(Instant::now);
                if cache.entries.insert(key.clone(), (url, instant)).is_none() {
                    cache.order.push_back(key);
                }
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
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut map: HashMap<String, (String, u64)> = HashMap::new();
        for (key, (url, instant)) in cache.entries.iter() {
            let age_secs = instant.elapsed().as_secs();
            if age_secs < URL_CACHE_TTL.as_secs() && url_is_fresh(url) {
                let timestamp = now_unix.saturating_sub(age_secs);
                map.insert(key.clone(), (url.clone(), timestamp));
            }
        }

        if map.is_empty() { return; }

        let path = cache_file_path();
        if let Ok(data) = serde_json::to_string(&map) {
            let _ = settings::write_private(&path, &data);
            println!("[StreamResolver] Saved {} CDN URLs to disk cache", map.len());
        }
    }

    /// Resolves a direct audio CDN URL for a given YouTube video ID using yt-dlp.
    /// Uses ultra-fast in-memory caching (~0.001s) and direct resolution (~0.4s).
    pub fn get_audio_url(video_id: &str, quality: meduza_core::settings::AudioQuality) -> Option<String> {
        // IMP-6: Ensure disk cache is loaded on first call this session
        if !DISK_CACHE_LOADED.load(std::sync::atomic::Ordering::SeqCst) {
            Self::load_cache_from_disk();
        }

        let cache_key = format!("{}:{:?}", video_id, quality);
        if let Some(url) = cached_url(&cache_key) {
            println!("[StreamResolver] 0.001s Cache Hit for {}", video_id);
            return Some(url);
        }

        let url = format!("https://www.youtube.com/watch?v={}", video_id);

        let format_arg = match quality {
            meduza_core::settings::AudioQuality::DataSaver => "bestaudio[acodec=opus][abr<=70]/bestaudio[abr<=96]/bestaudio[ext=webm]",
            meduza_core::settings::AudioQuality::Normal    => "bestaudio[acodec=opus][abr<=128]/bestaudio[abr<=128]/bestaudio[ext=webm]",
            meduza_core::settings::AudioQuality::High      => "bestaudio[acodec=opus]/bestaudio[ext=webm]/bestaudio/best",
        };

        // Bound concurrent yt-dlp processes (CPU overload guard).
        let _resolve_guard = RESOLVE_SEM.acquire();

        // 1. FAST DIRECT RESOLUTION (~0.4s - 0.6s instant load!)
        let mut fast_cmd = Self::yt_dlp_cmd();
        if let Ok(output) = fast_cmd
            .args([
                "-f", format_arg,
                "--no-warnings",
                "--quiet",
                "--no-playlist",
                // YouTube now requires a JS runtime for some formats.
                "--js-runtimes", "node",
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
                    println!("[StreamResolver] Fast Resolved (0.5s): {} ({})", settings::redact_url(&stream_url), video_id);
                    url_cache_insert(cache_key, (stream_url.clone(), Instant::now()));
                    return Some(stream_url);
                }
            }
        }

        // NOTE: The `--cookies-from-browser` fallback was removed: it reads the
        // user's browser session cookies, exposing them to a subprocess and
        // (in a sandbox) bypassing sandbox boundaries. Direct resolution above
        // covers all public tracks.

        println!("[StreamResolver] Could not resolve stream URL for {}", video_id);
        None
    }

    /// IMP-4: Racing resolver — launches two parallel yt-dlp resolution threads
    /// and returns whichever CDN URL arrives first. Ideal for the background
    /// preloader where we have time but want the fastest possible result.
    pub fn get_audio_url_racing(video_id: &str, quality: meduza_core::settings::AudioQuality) -> Option<String> {
        // Check cache first — instant, no threads needed
        let cache_key = format!("{}:{:?}", video_id, quality);
        if let Some(url) = cached_url(&cache_key) {
            println!("[StreamResolver] 0.001s Racing Cache Hit for {}", video_id);
            return Some(url);
        }

        // Not cached: race two threads with a small stagger.
        // Thread 1 starts immediately, Thread 2 starts 120ms later.
        // Both write to the shared cache — first one to finish wins and
        // the second thread will see the cache hit and exit cheaply.
        let (tx, rx) = std::sync::mpsc::channel::<String>();

        let vid1 = video_id.to_string();
        let tx1  = tx.clone();
        meduza_core::workers::io().submit(move || {
            if let Some(url) = Self::get_audio_url(&vid1, quality) {
                let _ = tx1.send(url);
            }
        });

        // Stagger second thread to avoid rate-limiting two simultaneous yt-dlp calls
        let vid2 = video_id.to_string();
        let tx2  = tx;
        meduza_core::workers::io().submit(move || {
            thread::sleep(Duration::from_millis(120));
            // Re-check cache first (Thread 1 may have already populated it)
            if let Some(url) = Self::get_audio_url(&vid2, quality) {
                let _ = tx2.send(url);
            }
        });

        // Wait up to 9 seconds for either thread (yt-dlp worst-case is ~5–6s)
        rx.recv_timeout(Duration::from_secs(9)).ok()
    }
}
