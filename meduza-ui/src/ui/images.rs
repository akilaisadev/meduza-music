//! Thumbnail pipeline.
//!
//! fetch (disk cache first, allow-listed hosts only) → decode and resize on a
//! worker → upload to a GPU texture on the UI thread. While decoding, the worker
//! also measures the artwork's dominant colour so the player can tint itself to
//! the current track.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use eframe::egui::{self, Color32, TextureHandle};
use meduza_core::settings::{self, FetchMethod, UrlKind};

use crate::theme::ACCENT;

/// Upper bound on uploaded GPU textures. Visible cards are always kept.
const GPU_TEXTURE_CAP: usize = 256;
/// Decoded pixels waiting for upload.
const DECODED_CAP: usize = 256;
const ACCENT_CAP: usize = 2048;
const DISK_CACHE_CAP: usize = 400;
const MAX_DOWNLOAD_BYTES: u64 = 5 * 1024 * 1024;

/// Lifecycle of one image, keyed by track id.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Loading,
    Ready,
    Failed,
}

struct Decoded {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

pub struct ImageStore {
    textures: HashMap<String, TextureHandle>,
    last_used: HashMap<String, u64>,
    frame: u64,
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    decoded: Arc<Mutex<HashMap<String, Decoded>>>,
    accents: Arc<Mutex<HashMap<String, Color32>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ImageStore {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            last_used: HashMap::new(),
            frame: 0,
            slots: Arc::new(Mutex::new(HashMap::new())),
            decoded: Arc::new(Mutex::new(HashMap::new())),
            accents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Call once per repaint; drives the LRU clock.
    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Returns the texture for `id` if it is ready, queueing a fetch otherwise.
    pub fn texture(&mut self, ctx: &egui::Context, id: &str, url: &str, max_dim: u32) -> Option<&TextureHandle> {
        if self.textures.contains_key(id) {
            self.last_used.insert(id.to_string(), self.frame);
            return self.textures.get(id);
        }

        // Upload a decoded image the worker has finished with.
        let ready = match self.decoded.try_lock() {
            Ok(mut map) => map.remove(id),
            Err(_) => None,
        };
        if let Some(d) = ready {
            let image = egui::ColorImage::from_rgba_unmultiplied([d.w, d.h], &d.px);
            let handle = ctx.load_texture(id, image, egui::TextureOptions::LINEAR);
            self.textures.insert(id.to_string(), handle);
            self.last_used.insert(id.to_string(), self.frame);
            self.prune();
            return self.textures.get(id);
        }

        // Only (re)queue when nothing is in flight and nothing has failed.
        let slot = self.slots.try_lock().ok().and_then(|s| s.get(id).copied());
        if matches!(slot, None | Some(Slot::Ready)) {
            self.queue(ctx, id, url, max_dim);
        }
        None
    }

    /// Dominant colour of the artwork, once it has been decoded.
    pub fn accent(&self, id: &str) -> Option<Color32> {
        self.accents.try_lock().ok().and_then(|m| m.get(id).copied())
    }

    fn queue(&self, ctx: &egui::Context, id: &str, url: &str, max_dim: u32) {
        let url = normalize_url(url);
        if url.is_empty() || !settings::host_is_allowed(&url, UrlKind::Image) {
            lock(&self.slots).insert(id.to_string(), Slot::Failed);
            return;
        }
        lock(&self.slots).insert(id.to_string(), Slot::Loading);

        let id = id.to_string();
        let ctx = ctx.clone();
        let slots = Arc::clone(&self.slots);
        let decoded = Arc::clone(&self.decoded);
        let accents = Arc::clone(&self.accents);

        meduza_core::workers::image_fetch().submit(move || {
            let Some(bytes) = load_raw(&url) else {
                lock(&slots).insert(id, Slot::Failed);
                return;
            };
            meduza_core::workers::decode().submit(move || {
                match decode_thumbnail(&bytes, max_dim) {
                    Some((image, accent)) => {
                        {
                            let mut acc = lock(&accents);
                            if acc.len() >= ACCENT_CAP {
                                acc.clear();
                            }
                            acc.insert(id.clone(), accent);
                        }
                        {
                            let mut map = lock(&decoded);
                            map.insert(id.clone(), image);
                            while map.len() > DECODED_CAP {
                                let Some(k) = map.keys().find(|k| **k != id).cloned() else { break; };
                                map.remove(&k);
                            }
                        }
                        lock(&slots).insert(id, Slot::Ready);
                        ctx.request_repaint();
                    }
                    None => {
                        lock(&slots).insert(id, Slot::Failed);
                    }
                }
            });
        });
    }

    /// Keeps GPU memory bounded. Only textures not drawn this frame are evicted,
    /// so anything on screen can never disappear mid-scroll.
    fn prune(&mut self) {
        if self.textures.len() <= GPU_TEXTURE_CAP {
            return;
        }
        let frame = self.frame;
        let mut stale: Vec<(u64, String)> = self
            .last_used
            .iter()
            .filter(|(k, &t)| t < frame && self.textures.contains_key(*k))
            .map(|(k, &t)| (t, k.clone()))
            .collect();
        stale.sort_unstable();
        let overflow = self.textures.len() - GPU_TEXTURE_CAP;
        for (_, key) in stale.into_iter().take(overflow) {
            self.textures.remove(&key);
            self.last_used.remove(&key);
        }
    }
}

fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("//") {
        format!("https:{}", url)
    } else {
        url.to_string()
    }
}

fn cache_dir() -> std::path::PathBuf {
    let dir = settings::app_cache_root().join("images");
    settings::ensure_private_dir(&dir);
    dir
}

/// Bytes of the image: from the disk cache when present, otherwise downloaded
/// through the allow-listed fetcher and stored for next time.
fn load_raw(url: &str) -> Option<Vec<u8>> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let path = cache_dir().join(format!("{}.img", hasher.finish()));

    if let Ok(buf) = std::fs::read(&path) {
        if !buf.is_empty() {
            return Some(buf);
        }
    }

    let resp = settings::fetch_allowed(url, UrlKind::Image, FetchMethod::Get, Duration::from_secs(10)).ok()?;
    let mut buf = Vec::new();
    resp.into_reader().take(MAX_DOWNLOAD_BYTES).read_to_end(&mut buf).ok()?;
    if buf.is_empty() {
        return None;
    }
    let _ = settings::write_private_bytes(&path, &buf);
    prune_disk_cache();
    Some(buf)
}

/// Removes the oldest cached thumbnails once the disk cache grows past its cap.
fn prune_disk_cache() {
    use std::time::SystemTime;
    let Ok(entries) = std::fs::read_dir(cache_dir()) else { return; };
    let mut files: Vec<(std::path::PathBuf, SystemTime)> = entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let mtime = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            (e.path(), mtime)
        })
        .collect();
    if files.len() <= DISK_CACHE_CAP {
        return;
    }
    files.sort_by_key(|(_, t)| *t);
    let overflow = files.len() - DISK_CACHE_CAP;
    for (path, _) in files.into_iter().take(overflow) {
        let _ = std::fs::remove_file(path);
    }
}

fn decode_thumbnail(bytes: &[u8], max_dim: u32) -> Option<(Decoded, Color32)> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let rgba = if w.max(h) > max_dim {
        let scale = max_dim as f32 / w.max(h) as f32;
        let nw = ((w as f32 * scale) as u32).max(1);
        let nh = ((h as f32 * scale) as u32).max(1);
        image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        img.to_rgba8()
    };
    let (w, h) = rgba.dimensions();
    let accent = average_accent(rgba.as_raw());
    Some((
        Decoded { w: w as usize, h: h as usize, px: rgba.into_raw() },
        accent,
    ))
}

/// Saturation-weighted average colour, lifted so it reads on a dark UI.
/// Near-black and grey pixels contribute little, so a dark cover still yields
/// its coloured details rather than mud.
fn average_accent(px: &[u8]) -> Color32 {
    let (mut r, mut g, mut b, mut total) = (0f32, 0f32, 0f32, 0f32);
    for p in px.chunks_exact(4).step_by(7) {
        if p[3] < 128 {
            continue;
        }
        let (pr, pg, pb) = (p[0] as f32, p[1] as f32, p[2] as f32);
        let max = pr.max(pg).max(pb);
        if max < 40.0 {
            continue;
        }
        let min = pr.min(pg).min(pb);
        let weight = 0.2 + (max - min) / max;
        r += pr * weight;
        g += pg * weight;
        b += pb * weight;
        total += weight;
    }
    if total == 0.0 {
        return ACCENT;
    }
    let (mut r, mut g, mut b) = (r / total, g / total, b / total);
    let max = r.max(g).max(b);
    if max > 0.0 && max < 170.0 {
        let k = 170.0 / max;
        r = (r * k).min(255.0);
        g = (g * k).min(255.0);
        b = (b * k).min(255.0);
    }
    Color32::from_rgb(r as u8, g as u8, b as u8)
}
