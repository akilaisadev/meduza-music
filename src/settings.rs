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
pub(crate) fn current_uid() -> Option<u32> {
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
    use std::io::Write as _;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if let Some(parent) = path.parent() {
        ensure_private_dir(parent);
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(contents.as_bytes())?;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    Ok(())
}

// ── SSRF defense: host allowlists for URLs the app is willing to fetch ──────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlKind {
    Image,
    Stream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchMethod {
    Get,
    Head,
}

fn url_host(url: &str) -> Option<&str> {
    let after_scheme = url.split("://").nth(1)?;
    let host = after_scheme.split(['/', '?']).next()?;
    let host = host.split(':').next()?;
    if host.is_empty() { None } else { Some(host) }
}

/// True if the URL points at a host the app is allowed to fetch from.
pub fn host_is_allowed(url: &str, kind: UrlKind) -> bool {
    let Some(host) = url_host(url) else { return false; };
    let host = host.to_ascii_lowercase();
    let allowed: &[&str] = match kind {
        UrlKind::Image => &[
            "ytimg.com",
            "ggpht.com",
            "youtube.com",
            "googleusercontent.com",
            "googlevideo.com",
        ],
        UrlKind::Stream => &["googlevideo.com", "youtube.com", "ytimg.com"],
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
            .https_only(true)
            .build()
    });

    let mut current = url.to_string();
    for _ in 0..6 {
        if !host_is_allowed(&current, kind) {
            return Err(format!("host not allowlisted: {}", url_host(&current).unwrap_or("?")));
        }

        let req = match method {
            FetchMethod::Get => agent.get(&current),
            FetchMethod::Head => agent.head(&current),
        };
        let resp = req.timeout(timeout).call().map_err(|e| e.to_string())?;

        let code = resp.status();
        if !(300..400).contains(&code) {
            return Ok(resp);
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
    /// Now-playing repaint cadence (ms/frame): 30 fps on low-end, 60 fps
    /// otherwise. The 10 fps navigation cadence is unaffected.
    pub fn now_playing_frame_ms(&self) -> u64 {
        if self.low_end_mode { 33 } else { 16 }
    }

    /// Layered glowing ambient-ink background behind the now-playing vinyl.
    pub fn ambient_glow(&self) -> bool {
        !self.low_end_mode
    }

    /// Competing stream resolutions to race (single resolve on low-end).
    pub fn resolve_racing(&self) -> bool {
        !self.low_end_mode
    }

    /// Concurrent browse calls when assembling the home feed / radio queue.
    pub fn home_feed_parallel(&self) -> usize {
        if self.low_end_mode { 1 } else { 5 }
    }

    /// How many upcoming tracks get their stream URLs pre-resolved.
    pub fn prefetch_depth(&self) -> usize {
        if self.low_end_mode { 1 } else { 2 }
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

    /// Decode (to RGBA) can be heavy; low-end profiles skip dominant-color
    /// extraction for the ambient background (single deterministic hue instead).
    pub fn heavy_background(&self) -> bool {
        !self.low_end_mode
    }
}
