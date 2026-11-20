use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioQuality {
    DataSaver,
    Normal,
    High,
}

fn default_preload_buffer() -> usize { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub audio_quality: AudioQuality,
    pub enable_cache: bool,
    pub gapless_playback: bool,
    pub autoplay_radio: bool,
    pub preload_next_track: bool,
    /// How many upcoming queue tracks the background preloader resolves ahead.
    /// 1 = only the next track (classic behaviour).
    #[serde(default = "default_preload_buffer")]
    pub preload_buffer_tracks: usize,
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
            preload_buffer_tracks: 1,
            low_end_mode: false,
            max_cache_size_mb: 500,
        }
    }
}

// ── Shared helpers: user-private paths + file/dir permissions ───────────────
// Never fall back to world-writable /tmp. When XDG dirs are unavailable, use
// the user's home directory. Directories are forced to 0700 and data files to
// 0600 so other local users cannot read listening history / taste profiles /
// cached audio or tamper with cache files.

/// meduza-music directory under the user cache dir (or ~/.cache).
pub fn app_cache_root() -> PathBuf {
    let mut p = dirs::cache_dir()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from).map(|h| h.join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."));
    p.push("meduza-music");
    p
}

/// Return the current user's real UID (read from /proc on Linux).
pub fn current_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(vals) = line.strip_prefix("Uid:") {
            return vals.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

/// True when `path` is a real directory (not a symlink) owned by the current
/// user and not group/world-writable. Used to sanitize XDG_RUNTIME_DIR before
/// trusting it as the location for the mpv IPC socket.
fn secure_dir(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = fs::symlink_metadata(path) else { return false; };
    let me = current_uid().unwrap_or(u32::MAX);
    meta.file_type().is_dir() && meta.uid() == me && meta.mode() & 0o022 == 0
}

/// Secure per-user runtime directory used for the mpv IPC socket and transient
/// state files. Only trusts `XDG_RUNTIME_DIR` when it is a real, owned,
/// non-group/world-writable directory; otherwise falls back to
/// `~/.cache/meduza-music`. Never falls back to world-writable `/tmp`.
pub fn runtime_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            let base = PathBuf::from(&dir);
            if secure_dir(&base) {
                let p = base.join("meduza-music");
                ensure_private_dir(&p);
                return p;
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let p = PathBuf::from(home).join(".cache").join("meduza-music");
    ensure_private_dir(&p);
    p
}

/// meduza-music directory under the user config dir (or ~/.config).
pub fn app_config_dir() -> PathBuf {
    let mut p = dirs::config_dir()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from).map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    p.push("meduza-music");
    p
}

/// Create a directory and force owner-only permissions (0700).
pub fn ensure_private_dir(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::create_dir_all(path);
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

/// Atomically write a private data file (0600), creating private parents.
pub fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    write_private_bytes(path, contents.as_bytes())
}

/// Binary-safe variant of `write_private` for cached images and other raw data.
pub fn write_private_bytes(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if let Some(parent) = path.parent() {
        ensure_private_dir(parent);
    }
    // Write a sibling temp file, flush it to disk, then rename it over the
    // target. A crash or full disk mid-write leaves the previous complete file
    // in place; truncating the real file in place would leave a half-written
    // file that the loaders silently treat as empty (and the next save then
    // overwrites it for good).
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{}.{}.tmp", file_name, std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        f.write_all(contents)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, path)
}

// ── SSRF defense: host allowlists for URLs the app is willing to fetch ──────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlKind {
    Image,
    Stream,
    /// Public metadata APIs (currently the LRCLIB lyrics service).
    Api,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchMethod {
    Get,
    Head,
}

fn url_host(url: &str) -> Option<String> {
    // Use a real URL parser: naive string splitting treats the part before a
    // ':' as the host, so `https://ytimg.com:443@evil.com/` was incorrectly
    // considered allowlisted even though it connects to evil.com.
    let parsed = Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?;
    if host.is_empty() { None } else { Some(host.to_ascii_lowercase()) }
}

/// True if the URL points at a host the app is allowed to fetch from.
pub fn host_is_allowed(url: &str, kind: UrlKind) -> bool {
    let Some(host) = url_host(url) else { return false; };
    let allowed: &[&str] = match kind {
        UrlKind::Image => &[
            "ytimg.com",
            "ggpht.com",
            "youtube.com",
            "googleusercontent.com",
            "googlevideo.com",
        ],
        UrlKind::Stream => &["googlevideo.com", "youtube.com", "ytimg.com"],
        UrlKind::Api => &["lrclib.net"],
    };
    allowed.iter().any(|s| host == *s || host.ends_with(&format!(".{}", s)))
}

/// HTTP(S) GET/HEAD that follows redirects only while *every* hop stays on an
/// allowlisted host (SSRF defense). Non-HTTP(S), non-allowlisted, or looping
/// redirects are refused. Returns the final non-redirect response.
///
/// Redirect following is controlled per-hop via an agent built with
/// `redirects(0)`, so we can validate the target host before every request.
pub fn fetch_allowed(
    url: &str,
    kind: UrlKind,
    method: FetchMethod,
    timeout: Duration,
) -> Result<ureq::Response, String> {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    let agent = AGENT.get_or_init(|| {
        ureq::builder()
            .redirects(0)
            .build()
    });

    let mut current = url.to_string();
    for _ in 0..6 {
        if !host_is_allowed(&current, kind) {
            return Err(format!("host not allowlisted: {}", url_host(&current).unwrap_or_else(|| "?".to_string())));
        }

        let req = match method {
            FetchMethod::Get => agent.get(&current),
            FetchMethod::Head => agent.head(&current),
        };
        let resp = req.timeout(timeout).call().map_err(|e| e.to_string())?;

        let code = resp.status();
        // Only successful responses are usable. Treating 404/500 as success
        // caused error pages to be cached as album art and permanently hid
        // affected tracks from the UI.
        if (200..300).contains(&code) {
            return Ok(resp);
        }
        if !(300..400).contains(&code) {
            return Err(format!("HTTP {}", code));
        }
        let Some(loc) = resp.header("Location") else {
            return Err("redirect without Location header".to_string());
        };
        match Url::parse(&current).ok().and_then(|u| u.join(loc).ok()) {
            Some(next) => current = next.to_string(),
            None => return Err("malformed redirect Location".to_string()),
        }
    }
    Err("too many redirects".to_string())
}

/// Redact query strings (which carry signed URL tokens) before logging.
pub fn redact_url(url: &str) -> String {
    match url.split_once('?') {
        Some((base, _)) => format!("{}?[redacted]", base),
        None => url.to_string(),
    }
}

// ── Simple counting semaphore to bound concurrent heavy subprocess /
//    network work (yt-dlp processes, cache downloads). Prevents CPU overload
//    and thread explosion. ────────────────────────────────────────────────────

pub struct Semaphore {
    max: u32,
    count: Mutex<u32>,
    cv: Condvar,
}

impl Semaphore {
    pub const fn new(max: u32) -> Self {
        Self { max, count: Mutex::new(0), cv: Condvar::new() }
    }

    pub fn acquire(&self) -> SemGuard<'_> {
        let mut c = self.count.lock().unwrap_or_else(|e| e.into_inner());
        while *c >= self.max {
            c = self.cv.wait(c).unwrap_or_else(|e| e.into_inner());
        }
        *c += 1;
        SemGuard { sem: self }
    }

    fn release(&self) {
        let mut c = self.count.lock().unwrap_or_else(|e| e.into_inner());
        *c = c.saturating_sub(1);
        self.cv.notify_one();
    }
}

pub struct SemGuard<'a> {
    sem: &'a Semaphore,
}

impl Drop for SemGuard<'_> {
    fn drop(&mut self) {
        self.sem.release();
    }
}

// ── AppSettings ──────────────────────────────────────────────────────────────

impl AppSettings {
    fn config_path() -> PathBuf {
        app_config_dir().join("settings.json")
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
            let _ = write_private(&path, &data);
        }
    }
}

// ── Resource governor ────────────────────────────────────────────────────────
// Centralizes every per-hardware cost knob behind `low_end_mode` so the rest of
// the app reads one profile instead of scattering `if low_end_mode` checks
// (and forgetting some, like the RGBA background). Each knob has a sensible
// default the user can flip with a single checkbox.

impl AppSettings {
    /// Repaint cadence for progress/auto-advance polling, in milliseconds.
    /// 10 fps normally, 5 fps on low-end hardware; enough for a smooth
    /// progress bar without the 60 fps repaint storm of earlier builds.
    pub fn now_playing_frame_ms(&self) -> u64 {
        if self.low_end_mode { 200 } else { 100 }
    }

    /// Competing stream resolutions to race (single resolve on low-end).
    pub fn resolve_racing(&self) -> bool {
        !self.low_end_mode
    }

    /// Concurrent browse calls when assembling the home feed / radio queue.
    pub fn home_feed_parallel(&self) -> usize {
        if self.low_end_mode { 1 } else { 5 }
    }

    /// Max pixel dimension for decoded images (we downscale through the fetch
    /// pipeline, never decode full-res on low-end).
    pub fn image_max_dim(&self) -> u32 {
        if self.low_end_mode { 480 } else { 512 }
    }

    /// True: tell mpv to keep ~1s of buffered audio instead of a larger
    /// look-ahead; helps RAM on low-end machines while cargo-free via `--no-osc`.
    pub fn mpv_small_buffer(&self) -> bool {
        self.low_end_mode
    }

    /// Interleave cache-download writes so a big download shares the disk
    /// reasonably with the audio already playing.
    pub fn pace_downloads(&self) -> bool {
        self.low_end_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_private_replaces_contents_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("meduza-settings-test-{}", std::process::id()));
        let path = dir.join("data.json");
        write_private(&path, "first").unwrap();
        write_private(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        let temp_files = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(temp_files, 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
