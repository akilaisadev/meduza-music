use eframe::egui::{self, Color32, FontId, RichText, Sense, Stroke, Vec2, Rounding};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::innertube::{BrowseSection, InnerTubeClient, TrackItem};
use crate::playback_manager::{PlaybackManager, PlaybackState};

// ── Palette ──────────────────────────────────────────────────────────────────
const BG:         Color32 = Color32::from_rgb(10, 10, 10);
const BG_SIDE:    Color32 = Color32::from_rgb(16, 16, 16);
const BG_TOP:     Color32 = Color32::from_rgb(14, 14, 14);
const BG_CARD:    Color32 = Color32::from_rgb(22, 22, 22);
const BG_CARD_HV: Color32 = Color32::from_rgb(32, 32, 32);
const ACCENT:     Color32 = Color32::from_rgb(29, 185, 84);

const T_PRI:      Color32 = Color32::from_rgb(255, 255, 255);
const T_SEC:      Color32 = Color32::from_rgb(170, 170, 170);
const T_DIM:      Color32 = Color32::from_rgb(90, 90, 90);


#[derive(Clone, PartialEq)]
enum Tab { Home, Search, Library, Settings }

pub struct MeduzaApp {
    innertube: Arc<InnerTubeClient>,
    playback:  Arc<PlaybackManager>,
    runtime:   tokio::runtime::Handle,

    tab: Tab,

    // Home
    sections:         Arc<Mutex<Vec<BrowseSection>>>,
    is_loading_home:  Arc<Mutex<bool>>,
    // Snapshot of the home feed rebuilt only when it changes — cloning the
    // whole feed every frame was a major CPU overload source.
    home_snapshot:    Vec<BrowseSection>,
    home_dirty:       std::sync::Arc<std::sync::atomic::AtomicBool>,

    // Search
    search_query:   String,
    last_search:    String,
    search_results: Arc<Mutex<Vec<TrackItem>>>,
    // Snapshot of search results rebuilt only when a new search lands.
    search_snapshot: Vec<TrackItem>,
    search_dirty:    std::sync::Arc<std::sync::atomic::AtomicBool>,
    is_searching:   Arc<Mutex<bool>>,
    suggestions:    Arc<Mutex<Vec<String>>>,
    show_suggest:   bool,

    // Images
    img_cache:    HashMap<String, egui::TextureHandle>,
    img_pending:  Arc<Mutex<HashMap<String, Option<Vec<u8>>>>>,
    // Decoded RGBA pixels produced off the UI thread by the image workers:
    // the UI thread only memcpys these into GPU textures (no decode on the
    // frame path — big win on low-end CPUs).
    image_rgba:   Arc<Mutex<HashMap<String, (usize, usize, Vec<u8>)>>>,
    logo_texture: Option<egui::TextureHandle>,
    // System Tray
    has_tray: std::sync::Arc<std::sync::atomic::AtomicBool>,
    is_exiting: bool,
    
    show_now_playing: bool,
    disc_angle: f32,

// Dynamic background color state
    bg_color_a:   [f32; 3],   // current interpolated primary color (RGB 0-1)
    bg_color_b:   [f32; 3],   // current interpolated secondary color
    bg_target_a:  [f32; 3],   // target primary color
    bg_target_b:  [f32; 3],   // target secondary color
    bg_last_track: String,    // track id of last color submission
    // Dominant-color results computed by the decode worker pool (never on the
    // UI thread). UI just probes this map and lerps toward the result.
    bg_color_store: Arc<Mutex<HashMap<String, ([f32; 3], [f32; 3])>>>,
}

impl MeduzaApp {
    pub fn new(cc: &eframe::CreationContext<'_>, runtime: tokio::runtime::Handle) -> Self {
        // Font setup
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "AppFont".to_owned(),
            egui::FontData::from_static(include_bytes!("../assets/Inter-Regular.ttf")),
        );
        fonts.font_data.insert(
            "CJK".to_owned(),
            egui::FontData::from_static(include_bytes!("../assets/NotoSansCJK.otf")),
        );
        fonts.font_data.insert(
            "Sinhala".to_owned(),
            egui::FontData::from_static(include_bytes!("../assets/NotoSansSinhala-Regular.ttf")),
        );

        let prop = fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap();
        prop.insert(0, "AppFont".to_owned());
        prop.push("CJK".to_owned());
        prop.push("Sinhala".to_owned());

        load_system_fallback_fonts(&mut fonts);

        cc.egui_ctx.set_fonts(fonts);

        // Global styles
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.window_rounding = 12.0.into();
        style.visuals.widgets.noninteractive.rounding = 8.0.into();
        style.visuals.widgets.inactive.rounding = 8.0.into();
        style.visuals.widgets.hovered.rounding = 8.0.into();
        style.visuals.widgets.active.rounding = 8.0.into();
        style.spacing.scroll.bar_width = 12.0;
        style.spacing.scroll.handle_min_length = 40.0;
        style.visuals.widgets.inactive.bg_fill = Color32::from_gray(60);
        cc.egui_ctx.set_style(style);

        let innertube = Arc::new(InnerTubeClient::new());
        let playback  = Arc::new(PlaybackManager::new(Arc::clone(&innertube)));

        // ── Visuals & Prominent Green Scrollbar Styling ──
        // Set once at startup; cloning the whole Style/Visuals every frame was
        // a significant per-frame CPU cost.
        {
            let mut vis = egui::Visuals::dark();
            vis.panel_fill                     = BG;
            vis.window_fill                    = BG;
            vis.override_text_color            = Some(T_PRI);
            vis.selection.bg_fill              = ACCENT;
            vis.widgets.noninteractive.bg_fill = Color32::from_rgb(22, 22, 26);
            vis.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(40, 40, 48));
            vis.widgets.inactive.bg_fill       = Color32::from_rgb(30, 180, 80);
            vis.widgets.inactive.fg_stroke     = Stroke::new(1.5_f32, ACCENT);
            vis.widgets.hovered.bg_fill        = ACCENT;
            vis.widgets.hovered.fg_stroke      = Stroke::new(2.0_f32, Color32::from_rgb(50, 250, 130));
            vis.widgets.active.bg_fill         = Color32::from_rgb(50, 240, 120);
            vis.widgets.active.fg_stroke       = Stroke::new(2.0_f32, Color32::WHITE);
            vis.extreme_bg_color               = Color32::from_rgb(10, 10, 12);
            cc.egui_ctx.set_visuals(vis);

            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.scroll.bar_width = 6.0;
            style.spacing.scroll.handle_min_length = 48.0;
            style.spacing.scroll.bar_inner_margin = 2.0;
            style.spacing.scroll.bar_outer_margin = 2.0;
            style.spacing.scroll.floating = false;
            cc.egui_ctx.set_style(style);
        }

        let sections = Arc::new(Mutex::new(Vec::<BrowseSection>::new()));
        let loading  = Arc::new(Mutex::new(true));
        let home_dirty = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Load home feed cache instantly
        let home_cache_path = crate::settings::app_cache_root().join("home_cache.json");
        
        if let Ok(data) = std::fs::read_to_string(&home_cache_path) {
            if let Ok(cached_feed) = serde_json::from_str::<Vec<BrowseSection>>(&data) {
                if !cached_feed.is_empty() {
                    *sections.lock().unwrap() = cached_feed;
                    *loading.lock().unwrap() = false;
                    home_dirty.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
        }

        {
            let s = Arc::clone(&sections);
            let l = Arc::clone(&loading);
            let it = Arc::clone(&innertube);
            let engine = Arc::clone(&playback.recommendation_engine);
            let dirty = Arc::clone(&home_dirty);
            let feed_parallel = playback.settings.lock().unwrap_or_else(|e| e.into_inner()).home_feed_parallel();
            runtime.spawn(async move {
                let mut feed = Vec::new();
                
                let top_artist = engine.lock().unwrap().get_top_artist();
                let top_tracks = engine.lock().unwrap().get_heavy_rotation(10);
                
                if !top_tracks.is_empty() {
                    feed.push(BrowseSection {
                        title: "Jump back in".to_string(),
                        items: top_tracks.clone(),
                    });
                }
                
                if let Some(top_track) = top_tracks.first() {
                    let radio = it.fetch_next_radio(&top_track.media_id).await;
                    if !radio.is_empty() {
                        feed.push(BrowseSection {
                            title: format!("Because you listen to {}", top_track.artist),
                            items: radio,
                        });
                    }
                }
                
                if let Some(artist) = top_artist {
                    let artist_mix = it.search_tracks(&format!("{} mix", artist)).await;
                    if !artist_mix.is_empty() {
                        feed.push(BrowseSection {
                            title: format!("{} & Similar Artists", artist),
                            items: artist_mix,
                        });
                    }
                }
                
                let std_feed = it.fetch_home_feed(feed_parallel).await;
                feed.extend(std_feed);

                if !feed.is_empty() {
                    let _ = crate::settings::write_private(&home_cache_path, &serde_json::to_string(&feed).unwrap_or_default());
                    *s.lock().unwrap() = feed;
                    dirty.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                *l.lock().unwrap() = false;
            });
        }

        // Load logo texture
        let logo_bytes = include_bytes!("../assets/logo.png");
        let logo_texture = if let Ok(img) = image::load_from_memory(logo_bytes) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            Some(cc.egui_ctx.load_texture("app_logo", ci, egui::TextureOptions::LINEAR))
        } else {
            None
        };

        // System Tray Initialization (Offloaded to fix startup transparency freeze)
        let has_tray = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ht_clone = std::sync::Arc::clone(&has_tray);
        std::thread::spawn(move || {
            #[cfg(target_os = "linux")]
            let _ = gtk::init();

            let tray_icon = std::panic::catch_unwind(|| {
                let tray_menu = tray_icon::menu::Menu::new();
                let item_show = tray_icon::menu::MenuItem::with_id("show", "Show App", true, None);
                let item_play = tray_icon::menu::MenuItem::with_id("play", "Play/Pause", true, None);
                let item_quit = tray_icon::menu::MenuItem::with_id("quit", "Quit", true, None);
                let _ = tray_menu.append_items(&[
                    &item_show,
                    &item_play,
                    &tray_icon::menu::PredefinedMenuItem::separator(),
                    &item_quit,
                ]);

                let icon_data = include_bytes!("../assets/icon.png");
                let tray_icon_img = if let Ok(img) = image::load_from_memory(icon_data) {
                    let rgba = img.into_rgba8();
                    let (w, h) = rgba.dimensions();
                    tray_icon::Icon::from_rgba(rgba.into_raw(), w, h).ok()
                } else {
                    None
                };

                if let Some(ic) = tray_icon_img {
                    tray_icon::TrayIconBuilder::new()
                        .with_menu(Box::new(tray_menu))
                        .with_tooltip("Meduza Music")
                        .with_icon(ic)
                        .build()
                        .ok()
                } else {
                    None
                }
            }).unwrap_or(None);

            if let Some(tray) = tray_icon {
                ht_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                Box::leak(Box::new(tray));
                loop { std::thread::sleep(std::time::Duration::from_secs(3600)); }
            }
        });

        Self {
            innertube, playback, runtime,
            tab: Tab::Home,
            sections, is_loading_home: loading,
            home_snapshot: Vec::new(),
            home_dirty,
            search_query: String::new(),
            last_search:  String::new(),
            search_results: Arc::new(Mutex::new(Vec::new())),
            search_snapshot: Vec::new(),
            search_dirty:    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            is_searching:   Arc::new(Mutex::new(false)),
            suggestions:    Arc::new(Mutex::new(Vec::new())),
            show_suggest:   false,
            img_cache:       HashMap::new(),
            img_pending:     Arc::new(Mutex::new(HashMap::new())),
            image_rgba:      Arc::new(Mutex::new(HashMap::new())),
            logo_texture,
            has_tray,
            is_exiting:     false,
            show_now_playing: false,
            disc_angle: 0.0,
            bg_color_a:   [0.05, 0.05, 0.08],
            bg_color_b:   [0.08, 0.03, 0.12],
            bg_target_a:  [0.05, 0.05, 0.08],
            bg_target_b:  [0.08, 0.03, 0.12],
            bg_last_track: String::new(),
            bg_color_store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // ── Image loading ─────────────────────────────────────────────────────────

    fn image_cache_dir() -> std::path::PathBuf {
        let d = crate::settings::app_cache_root().join("images");
        crate::settings::ensure_private_dir(&d);
        d
    }

    fn queue_image(
        id: &str,
        url: &str,
        pending: &Arc<Mutex<HashMap<String, Option<Vec<u8>>>>>,
        rgba: &Arc<Mutex<HashMap<String, (usize, usize, Vec<u8>)>>>,
        max_dim: u32,
        ctx: egui::Context,
    ) {
        // SSRF defense: only fetch images from known YouTube/Google hosts.
        if !crate::settings::host_is_allowed(url, crate::settings::UrlKind::Image) {
            return;
        }
        let mut p = pending.lock().unwrap();
        if p.contains_key(id) { return; }
        p.insert(id.to_string(), None);
        let id2    = id.to_string();
        let url2   = url.to_string();
        let p2     = Arc::clone(pending);
        let rgba2  = Arc::clone(rgba);
        crate::workers::decode().submit(move || {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            use std::io::Read;

            let mut hasher = DefaultHasher::new();
            id2.hash(&mut hasher);
            let cache_file = Self::image_cache_dir().join(format!("{}.img", hasher.finish()));

            let mut raw: Option<Vec<u8>> = None;

            if let Ok(buf) = std::fs::read(&cache_file) {
                // Only trust a cached file if it's a valid image within size limits.
                if Self::image_is_safe(&buf) {
                    raw = Some(buf);
                }
            } else if let Ok(resp) = crate::settings::fetch_allowed(
                &url2,
                crate::settings::UrlKind::Image,
                crate::settings::FetchMethod::Get,
                std::time::Duration::from_secs(15),
            ) {
                let mut buf = Vec::new();
                let _ = resp.into_reader().take(5 * 1024 * 1024).read_to_end(&mut buf);
                // Validate it decodes AND isn't a decompression bomb before
                // persisting it to disk (avoids poisoned/oversized cache files).
                if Self::image_is_safe(&buf) {
                    crate::settings::ensure_private_dir(&cache_file);
                    let _ = std::fs::write(&cache_file, &buf);
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&cache_file, std::fs::Permissions::from_mode(0o600));
                    raw = Some(buf);
                }
            }

            // Bound on-disk cache size: prune periodically so a long session of
            // unique thumbnails never grows the images dir without bound.
            static PRUNE_TICK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            if PRUNE_TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 64 == 0 {
                Self::prune_image_cache();
            }

            // Decode to RGBA here (worker), so the UI thread only memcpys pixels
            // into a GPU texture during the next frame.
            if let Some(bytes) = raw {
                let mut tex_px = (0usize, 0usize, Vec::<u8>::new());
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let (w, h) = (img.width(), img.height());
                    if w > 0 && h > 0 && w <= 4096 && h <= 4096 {
                        if w.max(h) > max_dim {
                            let scale = max_dim as f32 / w.max(h) as f32;
                            let nw = ((w as f32 * scale) as u32).max(1);
                            let nh = ((h as f32 * scale) as u32).max(1);
                            let small = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
                            let rgba_px = small.into_raw();
                            tex_px = (nw as usize, nh as usize, rgba_px);
                        } else {
                            let rgba_px = img.to_rgba8();
                            tex_px = (w as usize, h as usize, rgba_px.into_raw());
                        }
                    }
                }
                {
                    let mut r = rgba2.lock().unwrap();
                    r.insert(id2.clone(), tex_px);
                    if r.len() > 96 { r.clear(); } // bound decoded-RGBA memory
                }
                {
                    let mut g = p2.lock().unwrap();
                    g.insert(id2, Some(bytes));
                    if g.len() > 96 { g.clear(); } // bound raw-bytes memory
                }
                // One-shot wake so this freshly decoded image is drawn promptly
                // (thumbnails appear within ~1 frame; no sustained repaint loop).
                ctx.request_repaint();
            } else {
                // Mark as finished-but-empty so the UI never re-queues this id
                // every frame (that was a repo-wide refresh storm on failures).
                p2.lock().unwrap().insert(id2, Some(Vec::new()));
            }
        });
    }

    /// LRU-style prune of the on-disk image cache: when there are more than
    /// 300 cached thumbnails, remove the oldest by mtime until we're back at
    /// the cap. Runs rarely (once per ~64 decodes) on the decode worker.
    fn prune_image_cache() {
        use std::time::{UNIX_EPOCH, SystemTime};
        const MAX_FILES: usize = 300;
        let Ok(entries) = std::fs::read_dir(Self::image_cache_dir()) else { return; };
        let mut files: Vec<(std::path::PathBuf, SystemTime)> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let mtime = e.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
                Some((e.path(), mtime))
            })
            .collect();
        if files.len() <= MAX_FILES {
            return;
        }
        files.sort_by_key(|(_, t)| *t);
        let overflow = files.len() - MAX_FILES;
        for path in files.into_iter().take(overflow).map(|(p, _)| p) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// True if bytes decode as an image with sane dimensions (blocks decompression bombs).
    fn image_is_safe(buf: &[u8]) -> bool {
        use std::io::Cursor;

        // Cheap header-only dimension probe BEFORE decoding any pixels. A small
        // crafted file declaring huge dimensions never reaches the decoder.
        let header_dims = (|| -> Option<(u32, u32)> {
            let reader = image::io::Reader::new(Cursor::new(buf));
            reader.with_guessed_format().ok()?.into_dimensions().ok()
        })();
        let Some((w, h)) = header_dims else { return false; };
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return false;
        }

        match image::load_from_memory(buf) {
            Ok(img) => {
                let (w, h) = (img.width(), img.height());
                w > 0 && h > 0 && w <= 4096 && h <= 4096
            }
            Err(_) => false,
        }
    }

    fn get_texture<'a>(
        cache:   &'a mut HashMap<String, egui::TextureHandle>,
        pending: &Arc<Mutex<HashMap<String, Option<Vec<u8>>>>>,
        rgba:    &Arc<Mutex<HashMap<String, (usize, usize, Vec<u8>)>>>,
        ctx:     &egui::Context,
        id:      &str,
        url:     &str,
        max_dim: u32,
    ) -> Option<&'a egui::TextureHandle> {
        if cache.contains_key(id) { return cache.get(id); }

        // Designed: image workers wrote decoded RGBA; just upload it.
        if let Some((w, h, px)) = rgba.lock().unwrap().get(id).cloned() {
            if w > 0 && h > 0 {
                let ci = egui::ColorImage::from_rgba_unmultiplied([w, h], &px);
                let handle = ctx.load_texture(id, ci, egui::TextureOptions::LINEAR);
                cache.insert(id.to_string(), handle);
                rgba.lock().unwrap().remove(id);
                return cache.get(id);
            }
        }

        // Not decoded yet — make sure a fetch/decode worker is queued.
        {
            let ready = pending.lock().unwrap().get(id).is_some();
            if !ready {
                Self::queue_image(id, url, pending, rgba, max_dim, ctx.clone());
            }
        }
        None
    }

    // ── Panels ────────────────────────────────────────────────────────────────

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let h = ui.available_height();
        egui::Frame::none().fill(BG_SIDE).show(ui, |ui| {
            ui.set_min_size(Vec2::new(200.0, h));
            ui.add_space(20.0);

            // Logo
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                if let Some(ref tex) = self.logo_texture {
                    ui.add(egui::Image::new(tex)
                        .fit_to_exact_size(Vec2::splat(28.0))
                        .rounding(Rounding::same(6.0)));
                } else {
                    ui.label(RichText::new("♪").color(ACCENT).font(FontId::proportional(26.0)));
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Meduza").color(T_PRI)
                    .font(FontId::proportional(20.0)).strong());
            });
            ui.add_space(28.0);

            // Nav items
            for (icon, label, tgt) in [
                ("🏠", "Home",         Tab::Home),
                ("🔍", "Search",       Tab::Search),
                ("📚", "Your Library", Tab::Library),
                ("⚙", "Settings",     Tab::Settings),
            ] {
                let active = self.tab == tgt;
                let fg = if active { T_PRI } else { T_SEC };
                let frame = egui::Frame::none()
                    .fill(if active { Color32::from_rgb(28, 28, 28) } else { Color32::TRANSPARENT })
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0));
                let resp = frame.show(ui, |ui| {
                    ui.set_min_width(172.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon)
                            .font(FontId::proportional(15.0))
                            .color(if active { ACCENT } else { T_DIM }));
                        ui.add_space(8.0);
                        ui.label(RichText::new(label)
                            .font(FontId::proportional(14.0))
                            .color(fg));
                    });
                }).response;
                if resp.interact(Sense::click()).clicked() { self.tab = tgt; }
                ui.add_space(4.0);
            }

            ui.add_space(20.0);
            ui.add(egui::Separator::default().shrink(16.0));
            ui.add_space(12.0);

            // Queue preview wrapped in dedicated ScrollArea
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_source("sidebar_queue")
                .show(ui, |ui| {
                    let queue = self.playback.queue.lock().unwrap().clone();
                    let idx   = *self.playback.queue_index.lock().unwrap();
                    if !queue.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            ui.label(RichText::new("UP NEXT").color(T_DIM)
                                .font(FontId::proportional(10.0)));
                        });
                        ui.add_space(6.0);
                        for (i, item) in queue.iter().skip(idx).take(15).enumerate() {
                            let col = if i == 0 { ACCENT } else { T_SEC };
                            let nm  = if item.track.title.is_empty() { "—" } else { &item.track.title };
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(172.0, 24.0), Sense::click());
                            let clip_rect = egui::Rect::from_min_max(
                                rect.min + egui::vec2(16.0, 0.0),
                                rect.max - egui::vec2(8.0, 0.0),
                            );
                            let prefix = if i == 0 { "▶ " } else { "" };
                            let display_txt = format!("{}{}", prefix, trunc(nm, 18));
                            ui.painter().with_clip_rect(clip_rect).text(
                                rect.min + egui::vec2(16.0, 4.0),
                                egui::Align2::LEFT_TOP,
                                display_txt,
                                FontId::proportional(11.5),
                                col,
                            );
                            if resp.clicked() {
                                self.playback.play_now(item.track.clone());
                            }
                        }
                    }
                });
        });
    }

    fn topbar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(BG_TOP)
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(28, 28, 28)))
            .inner_margin(egui::Margin::symmetric(20.0, 10.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    use chrono::Timelike;
                    let hour = chrono::Local::now().hour();
                    let greeting = match hour {
                        5..=11 => "Good Morning 🎵",
                        12..=17 => "Good Afternoon 🎵",
                        18..=21 => "Good Evening 🎵",
                        _ => "Good Night 🎵",
                    };
                    let title = match self.tab {
                        Tab::Home    => greeting,
                        Tab::Search  => "Search",
                        Tab::Library => "Your Library",
                        Tab::Settings => "Settings",
                    };
                    ui.label(RichText::new(title).color(T_PRI)
                        .font(FontId::proportional(22.0)).strong());

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.tab != Tab::Search {
                            egui::Frame::none()
                                .fill(Color32::from_rgb(30, 30, 30))
                                .rounding(Rounding::same(16.0))
                                .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("🔍").color(T_DIM)
                                            .font(FontId::proportional(12.0)));
                                        ui.add_space(4.0);
                                        let resp = ui.add(
                                            egui::TextEdit::singleline(&mut self.search_query)
                                                .hint_text("Quick search…")
                                                .font(FontId::proportional(13.0))
                                                .frame(false)
                                                .desired_width(160.0),
                                        );
                                        if resp.lost_focus()
                                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                        {
                                            self.tab = Tab::Search;
                                            self.do_search();
                                        }
                                    });
                                });
                        }
                    });
                });
            });
    }

    fn do_search(&mut self) {
        let q = self.search_query.trim().to_string();
        if q.is_empty() || q == self.last_search { return; }
        self.last_search = q.clone();
        let results  = Arc::clone(&self.search_results);
        let searching = Arc::clone(&self.is_searching);
        let dirty = Arc::clone(&self.search_dirty);
        let it = Arc::clone(&self.innertube);
        *searching.lock().unwrap() = true;
        *results.lock().unwrap()   = Vec::new();
        dirty.store(true, std::sync::atomic::Ordering::SeqCst);
        self.runtime.spawn(async move {
            let res = it.search_tracks(&q).await;
            *results.lock().unwrap()   = res;
            dirty.store(true, std::sync::atomic::Ordering::SeqCst);
            *searching.lock().unwrap() = false;
        });
    }

    // ── Home ──────────────────────────────────────────────────────────────────

    fn show_home(&mut self, ui: &mut egui::Ui) {
        let loading  = *self.is_loading_home.lock().unwrap();

        // Rebuild the feed snapshot only when the underlying feed changed —
        // avoids cloning the whole feed every frame.
        if self.home_dirty.swap(false, std::sync::atomic::Ordering::SeqCst) || self.home_snapshot.is_empty() {
            self.home_snapshot = self.sections.lock().unwrap().clone();
        }
        // NOTE: Do NOT mem::take here — that empties home_snapshot every frame,
        // forcing a full re-clone from the mutex on the very next frame (was causing
        // the settings/sections mutex lock storm that froze the UI thread).
        let sections = self.home_snapshot.clone();
        let mut seen = std::collections::HashSet::<String>::new();

        let heavy_rot_raw = self.playback.recommendation_engine.lock().unwrap().get_heavy_rotation(8);
        let heavy_rot     = crate::recommendation_engine::RecommendationEngine::filter_unique(&heavy_rot_raw, &mut seen);

        let history_raw   = self.playback.history.lock().unwrap().clone();
        let history       = crate::recommendation_engine::RecommendationEngine::filter_unique(&history_raw, &mut seen);

        let liked_raw     = self.playback.liked_songs.lock().unwrap().clone();
        let liked         = crate::recommendation_engine::RecommendationEngine::filter_unique(&liked_raw, &mut seen);

        if loading && sections.is_empty() && history.is_empty() && heavy_rot.is_empty() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(44.0).color(ACCENT));
                ui.add_space(12.0);
                ui.label(RichText::new("Loading your personalized feed…").color(T_SEC)
                    .font(FontId::proportional(15.0)));
            });
            return;
        }

        if sections.is_empty() && history.is_empty() && liked.is_empty() && heavy_rot.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("⚠  Could not load feed").color(T_SEC)
                    .font(FontId::proportional(15.0)));
                ui.add_space(10.0);
                if self.retry_btn(ui) {
                    *self.is_loading_home.lock().unwrap() = true;
                    let s  = Arc::clone(&self.sections);
                    let l  = Arc::clone(&self.is_loading_home);
                    let it = Arc::clone(&self.innertube);
                    let engine = Arc::clone(&self.playback.recommendation_engine);
                    let dirty = Arc::clone(&self.home_dirty);
                    let feed_parallel = self.playback.settings.lock().unwrap_or_else(|e| e.into_inner()).home_feed_parallel();
                    self.runtime.spawn(async move {
                        let mut feed = Vec::new();
                        
                        let top_artist = engine.lock().unwrap().get_top_artist();
                        let top_tracks = engine.lock().unwrap().get_heavy_rotation(10);
                        
                        if !top_tracks.is_empty() {
                            feed.push(BrowseSection {
                                title: "Jump back in".to_string(),
                                items: top_tracks.clone(),
                            });
                        }
                        
                        if let Some(top_track) = top_tracks.first() {
                            let radio = it.fetch_next_radio(&top_track.media_id).await;
                            if !radio.is_empty() {
                                feed.push(BrowseSection {
                                    title: format!("Because you listen to {}", top_track.artist),
                                    items: radio,
                                });
                            }
                        }
                        
                        if let Some(artist) = top_artist {
                            let artist_mix = it.search_tracks(&format!("{} mix", artist)).await;
                            if !artist_mix.is_empty() {
                                feed.push(BrowseSection {
                                    title: format!("{} & Similar Artists", artist),
                                    items: artist_mix,
                                });
                            }
                        }
                        
                        let std_feed = it.fetch_home_feed(feed_parallel).await;
                        feed.extend(std_feed);

                        *s.lock().unwrap() = feed;
                        dirty.store(true, std::sync::atomic::Ordering::SeqCst);
                        *l.lock().unwrap() = false;
                    });
                }
            });
            return;
        }

        let max_dim = self.playback.settings.lock().unwrap_or_else(|e| e.into_inner()).image_max_dim();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .id_source("home")
            .show(ui, |ui| {
                ui.add_space(8.0);

                // 5. InnerTube Discovery Feed Sections (Deduplicated)
                for section in &sections {
                    let unique_items = crate::recommendation_engine::RecommendationEngine::filter_unique(&section.items, &mut seen);
                    if !unique_items.is_empty() {
                        ui.label(
                            RichText::new(&section.title)
                                .color(T_PRI)
                                .font(FontId::proportional(19.0))
                                .strong(),
                        );
                        ui.add_space(10.0);
                        self.card_grid(ui, &unique_items, max_dim);
                        ui.add_space(26.0);
                    }
                }
            });
        self.home_snapshot = sections;
    }

    fn retry_btn(&self, ui: &mut egui::Ui) -> bool {
        ui.add(egui::Button::new(
            RichText::new("  Retry  ").color(Color32::BLACK).font(FontId::proportional(13.0))
        ).fill(ACCENT).rounding(Rounding::same(20.0))).clicked()
    }

    fn card_grid(&mut self, ui: &mut egui::Ui, tracks: &[TrackItem], max_dim: u32) {
        // Dynamically compute how many cards fit per row with scrollbar margin
        let available_w = (ui.available_width() - 16.0).max(100.0);
        let card_w = 156.0_f32;
        let gap    = 16.0_f32;
        let cols   = ((available_w + gap) / (card_w + gap)).floor().max(1.0) as usize;

        let row_count = (tracks.len() + cols - 1) / cols;

        for row in 0..row_count {
            let start = row * cols;
            let end   = (start + cols).min(tracks.len());
            let row_tracks = &tracks[start..end];

            ui.horizontal(|ui| {
                for track in row_tracks {
                    self.draw_card(ui, track, card_w, max_dim);
                    ui.add_space(gap);
                }
            });
            ui.add_space(gap);
        }
    }

    fn draw_card(&mut self, ui: &mut egui::Ui, track: &TrackItem, card_w: f32, max_dim: u32) {
        let pad       = 10.0_f32;
        let img_size  = card_w - pad * 2.0;
        let card_h    = img_size + 64.0;
        let desired   = Vec2::new(card_w, card_h);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click());

        // Hover / Active Background Frame (Elevated Glassmorphic Fill & Border)
        let (bg_color, stroke_col) = if response.is_pointer_button_down_on() {
            (Color32::from_rgb(38, 38, 46), Color32::from_rgb(60, 60, 70))
        } else if response.hovered() {
            (Color32::from_rgb(28, 28, 34), Color32::from_rgb(50, 50, 58))
        } else {
            (Color32::from_rgb(20, 20, 24), Color32::from_rgb(32, 32, 36))
        };

        ui.painter().rect_filled(rect, Rounding::same(12.0), bg_color);
        ui.painter().rect_stroke(rect, Rounding::same(12.0), Stroke::new(1.0_f32, stroke_col));

        let img_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(pad, pad),
            Vec2::splat(img_size),
        );

        // Album art
        let pending = Arc::clone(&self.img_pending);
        if let Some(tex) = Self::get_texture(
            &mut self.img_cache, &pending, &self.image_rgba, ui.ctx(),
            &track.media_id, &track.thumbnail_url, max_dim,
        ) {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().with_clip_rect(img_rect).image(tex.id(), img_rect, uv, Color32::WHITE);
            ui.painter().rect_stroke(img_rect, Rounding::same(8.0), Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255,255,255,15)));
        } else {
            ui.painter().rect_filled(img_rect, Rounding::same(8.0), Color32::from_rgb(28, 28, 36));
            ui.painter().text(
                img_rect.center(), egui::Align2::CENTER_CENTER,
                "♪", FontId::proportional(34.0), T_DIM,
            );
        }

        // Text Clip
        let card_clip = egui::Rect::from_min_max(
            rect.min + egui::vec2(pad, 0.0),
            rect.max - egui::vec2(pad, 0.0),
        );

        // Title
        let text_x  = rect.min.x + pad;
        let title_y = img_rect.max.y + 10.0;
        ui.painter().with_clip_rect(card_clip).text(
            egui::pos2(text_x, title_y),
            egui::Align2::LEFT_TOP,
            trunc(&track.title, 16),
            FontId::proportional(13.5),
            T_PRI,
        );

        // Artist
        let artist_y = title_y + 18.0;
        ui.painter().with_clip_rect(card_clip).text(
            egui::pos2(text_x, artist_y),
            egui::Align2::LEFT_TOP,
            trunc(&track.artist, 16),
            FontId::proportional(12.0),
            T_SEC,
        );

        // Animated Hover Green Play Button Overlay
        if response.hovered() {
            let c = egui::pos2(img_rect.max.x - 22.0, img_rect.max.y - 22.0);
            ui.painter().circle_filled(c, 18.0, ACCENT);
            ui.painter().text(c, egui::Align2::CENTER_CENTER, "▶", FontId::proportional(14.0), Color32::BLACK);
        }

        if response.clicked() {
            self.playback.play_now(track.clone());
        }
    }

    // ── Search ────────────────────────────────────────────────────────────────

    fn show_search(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);

        let mut changed       = false;
        let mut enter_pressed = false;

        egui::Frame::none()
            .fill(Color32::from_rgb(30, 30, 30))
            .rounding(Rounding::same(24.0))
            .inner_margin(egui::Margin::symmetric(18.0, 12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🔍").color(T_DIM).font(FontId::proportional(18.0)));
                    ui.add_space(10.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Artists, songs, albums…")
                            .font(FontId::proportional(16.0))
                            .frame(false)
                            .desired_width(f32::INFINITY),
                    );
                    if resp.changed() { changed = true; }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        enter_pressed = true;
                    }
                });
            });

        if changed && !self.search_query.is_empty() {
            let q   = self.search_query.clone();
            let sug = Arc::clone(&self.suggestions);
            let it  = Arc::clone(&self.innertube);
            self.show_suggest = true;
            self.runtime.spawn(async move {
                let s = it.fetch_search_suggestions(&q).await;
                *sug.lock().unwrap() = s;
            });
        }

        if enter_pressed {
            self.show_suggest = false;
            self.do_search();
        }

        // Suggestions dropdown
        let sugs = self.suggestions.lock().unwrap().clone();
        if self.show_suggest && !sugs.is_empty() {
            for sug in sugs.iter().take(5) {
                let sr = ui.add(
                    egui::Button::new(
                        RichText::new(format!("🔍  {}", sug)).color(T_SEC)
                            .font(FontId::proportional(13.0))
                    ).fill(Color32::TRANSPARENT).frame(false)
                     .min_size(Vec2::new(400.0, 28.0))
                );
                if sr.clicked() {
                    self.search_query = sug.clone();
                    self.show_suggest = false;
                    self.do_search();
                }
            }
            ui.add_space(4.0);
            ui.separator();
        }

        ui.add_space(8.0);

        let is_searching = *self.is_searching.lock().unwrap();
        if is_searching {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(36.0).color(ACCENT));
                ui.add_space(8.0);
                ui.label(RichText::new("Searching YouTube Music…").color(T_SEC)
                    .font(FontId::proportional(14.0)));
            });
            return;
        }

        // Rebuild the results snapshot only when a new search lands — avoids
        // cloning the full result list every frame while idle.
        if self.search_dirty.swap(false, std::sync::atomic::Ordering::SeqCst) || self.search_snapshot.is_empty() {
            self.search_snapshot = self.search_results.lock().unwrap().clone();
        }
        let results = std::mem::take(&mut self.search_snapshot);
        if results.is_empty() && !self.last_search.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No results found").color(T_SEC)
                    .font(FontId::proportional(14.0)));
            });
            return;
        }

        if results.is_empty() {
            ui.add_space(12.0);
            ui.label(RichText::new("Browse categories").color(T_PRI)
                .font(FontId::proportional(16.0)).strong());
            ui.add_space(10.0);
            self.genre_grid(ui);
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .id_source("search")
            .show(ui, |ui| {
                ui.label(RichText::new(format!("{} results for \"{}\"",
                    results.len(), self.last_search))
                    .color(T_DIM).font(FontId::proportional(12.0)));
                ui.add_space(6.0);
                self.track_list(ui, &results);
            });
        self.search_snapshot = results;
    }

    fn genre_grid(&mut self, ui: &mut egui::Ui) {
        let genres = [
            ("Pop",        Color32::from_rgb(148, 41, 189)),
            ("Hip-Hop",    Color32::from_rgb(233, 84,  32)),
            ("Rock",       Color32::from_rgb(186, 30,  30)),
            ("Electronic", Color32::from_rgb(30,  112, 186)),
            ("R&B",        Color32::from_rgb(141, 103, 171)),
            ("Latin",      Color32::from_rgb(229, 30,  75)),
            ("Podcasts",   Color32::from_rgb(39,  133, 106)),
            ("Indie",      Color32::from_rgb(13,  115, 236)),
        ];
        let cols = 4usize;
        egui::Grid::new("genres").spacing([10.0, 10.0]).show(ui, |ui| {
            for (i, (name, color)) in genres.iter().enumerate() {
                if i > 0 && i % cols == 0 { ui.end_row(); }
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(140.0, 72.0), Sense::click());
                let bg = if resp.hovered() {
                    Color32::from_rgb(
                        color.r().saturating_add(20),
                        color.g().saturating_add(20),
                        color.b().saturating_add(20),
                    )
                } else { *color };
                ui.painter().rect_filled(rect, Rounding::same(8.0), bg);
                ui.painter().text(
                    rect.left_bottom() + egui::vec2(12.0, -12.0),
                    egui::Align2::LEFT_BOTTOM,
                    name, FontId::proportional(14.0), T_PRI,
                );
                if resp.clicked() {
                    self.search_query = name.to_string();
                    self.do_search();
                    self.tab = Tab::Search;
                }
            }
        });
    }

    fn track_list(&mut self, ui: &mut egui::Ui, tracks: &[TrackItem]) {
        let cur = self.playback.current_track.lock().unwrap().clone();
        // Read max_dim ONCE per frame, not once per track — avoids locking settings mutex N times per frame
        let max_dim = self.playback.settings.lock().unwrap_or_else(|e| e.into_inner()).image_max_dim();
        for (i, track) in tracks.iter().enumerate() {
            let is_cur = cur.as_ref().map(|c| c.media_id == track.media_id).unwrap_or(false);
            let (rect, resp) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), 58.0), Sense::click());
            let bg = if resp.hovered()  { BG_CARD_HV }
                     else if is_cur    { Color32::from_rgb(20, 35, 20) }
                     else              { Color32::TRANSPARENT };
            ui.painter().rect_filled(rect, Rounding::same(6.0), bg);

            let p = 12.0;
            // Index / play indicator
            if is_cur {
                ui.painter().text(egui::pos2(rect.min.x + p, rect.center().y),
                    egui::Align2::LEFT_CENTER, "▶", FontId::proportional(12.0), ACCENT);
            } else {
                ui.painter().text(egui::pos2(rect.min.x + p, rect.center().y),
                    egui::Align2::LEFT_CENTER, format!("{}", i + 1),
                    FontId::proportional(12.0), T_DIM);
            }

            // Thumbnail
            let tr = egui::Rect::from_center_size(
                egui::pos2(rect.min.x + p + 34.0, rect.center().y), Vec2::splat(42.0));
            let pending = Arc::clone(&self.img_pending);
            if let Some(tex) = Self::get_texture(&mut self.img_cache, &pending, &self.image_rgba, ui.ctx(),
                &track.media_id, &track.thumbnail_url, max_dim)
            {
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().with_clip_rect(tr).image(tex.id(), tr, uv, Color32::WHITE);
            } else {
                ui.painter().rect_filled(tr, Rounding::same(4.0), BG_CARD);
                ui.painter().text(tr.center(), egui::Align2::CENTER_CENTER,
                    "♪", FontId::proportional(16.0), T_DIM);
            }

            // Title + artist
            let tx = rect.min.x + p + 64.0;
            let title_col = if is_cur { ACCENT } else { T_PRI };
            ui.painter().text(egui::pos2(tx, rect.center().y - 9.0),
                egui::Align2::LEFT_CENTER, trunc(&track.title, 45),
                FontId::proportional(13.5), title_col);
            ui.painter().text(egui::pos2(tx, rect.center().y + 10.0),
                egui::Align2::LEFT_CENTER, trunc(&track.artist, 35),
                FontId::proportional(12.0), T_SEC);

            // Duration
            if track.duration_seconds > 0 {
                ui.painter().text(egui::pos2(rect.max.x - p, rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    track.duration_str(), FontId::proportional(12.0), T_DIM);
            }

            if resp.clicked() { self.playback.play_now(track.clone()); }
        }
    }

    // ── Library ───────────────────────────────────────────────────────────────

    fn show_library(&mut self, ui: &mut egui::Ui) {
        let liked = self.playback.liked_songs.lock().unwrap().clone();
        if liked.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🎵").font(FontId::proportional(64.0)));
                ui.add_space(16.0);
                ui.label(RichText::new("Your library is empty").color(T_PRI)
                    .font(FontId::proportional(18.0)).strong());
                ui.add_space(6.0);
                ui.label(RichText::new("Click the 💚 icon on any song to save it.")
                    .color(T_SEC).font(FontId::proportional(13.0)));
            });
        } else {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .id_source("library")
                .show(ui, |ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("Liked Songs 💚")
                    .font(FontId::proportional(22.0)).strong());
                ui.add_space(14.0);
                self.track_list(ui, &liked);
            });
        }
    }

    // ── Bottom Player ─────────────────────────────────────────────────────────

    fn bottom_player(&mut self, ui: &mut egui::Ui) {
        // Fill player background
        let full_rect = ui.max_rect();
        ui.painter().rect_filled(full_rect, Rounding::ZERO, Color32::from_rgb(18, 18, 20));

        // Read playback state
        let pos   = *self.playback.progress_secs.lock().unwrap();
        let dur_raw = *self.playback.duration_secs.lock().unwrap();
        let dur   = dur_raw.max(1.0);
        let ratio = (pos / dur).clamp(0.0, 1.0);
        let track = self.playback.current_track.lock().unwrap().clone();
        let state = self.playback.state.lock().unwrap().clone();
        // Read max_dim once here (not inside the art rendering block) to avoid locking settings inside a closure
        let max_dim = self.playback.settings.lock().unwrap_or_else(|e| e.into_inner()).image_max_dim();

        // ── 1. Interactive Green Progress Bar at top edge of player ───────────
        let w = full_rect.width();
        let bar_h = 3.0_f32;
        let bar_y_offset = 3.0_f32; // prevent handle clipping
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(full_rect.min.x, full_rect.min.y + bar_y_offset),
            Vec2::new(w, bar_h)
        );
        let bar_sense = ui.allocate_rect(bar_rect, Sense::click_and_drag());
        if bar_sense.clicked() || bar_sense.dragged() {
            if let Some(ptr) = bar_sense.interact_pointer_pos() {
                let frac = ((ptr.x - bar_rect.min.x) / w).clamp(0.0, 1.0);
                self.playback.seek_to(frac * dur);
            }
        }
        
        let bg_col = if bar_sense.hovered() { Color32::from_rgb(60, 60, 60) } else { Color32::from_rgb(30, 30, 30) };
        ui.painter().rect_filled(bar_rect, Rounding::ZERO, bg_col);
        
        if ratio > 0.0 {
            let fill = egui::Rect::from_min_size(bar_rect.min, Vec2::new(w * ratio, bar_h));
            ui.painter().rect_filled(fill, Rounding::ZERO, ACCENT);

            if bar_sense.hovered() || bar_sense.dragged() {
                let handle_pos = egui::pos2(bar_rect.min.x + w * ratio, bar_rect.center().y);
                ui.painter().circle_filled(handle_pos, 5.0, Color32::WHITE);
            }
        }

        // ── 2. Three columns below bar ────────────────────────────────────────
        let content_top = full_rect.min.y + bar_h + bar_y_offset;
        let content_h   = full_rect.height() - (bar_h + bar_y_offset);
        let left_rect   = egui::Rect::from_min_size(
            egui::pos2(full_rect.min.x, content_top), Vec2::new(w * 0.30, content_h));
        let center_rect = egui::Rect::from_min_size(
            egui::pos2(full_rect.min.x + w * 0.30, content_top), Vec2::new(w * 0.40, content_h));
        let right_rect  = egui::Rect::from_min_size(
            egui::pos2(full_rect.min.x + w * 0.70, content_top), Vec2::new(w * 0.30, content_h));

        // ── Left: track art + info + heart ───────────────────────────────────
        ui.allocate_ui_at_rect(left_rect, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                let art_sz = Vec2::splat(48.0);
                let (art_rect, art_resp) = ui.allocate_exact_size(art_sz, Sense::click());
                if art_resp.clicked() {
                    self.show_now_playing = true;
                }

                let tex_opt = if let Some(ref t) = track {
                    let p = Arc::clone(&self.img_pending);
                    Self::get_texture(&mut self.img_cache, &p, &self.image_rgba, ui.ctx(), &t.media_id, &t.thumbnail_url, max_dim)
                } else { None };

                if let Some(texture) = tex_opt {
                    ui.painter().image(
                        texture.id(),
                        art_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    ui.painter().rect_stroke(art_rect, Rounding::same(6.0), Stroke::new(1.0_f32, Color32::from_rgb(45, 45, 50)));
                } else {
                    ui.painter().rect_filled(art_rect, Rounding::same(6.0), BG_CARD);
                    ui.painter().text(
                        art_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "♪",
                        FontId::proportional(20.0),
                        T_DIM,
                    );
                }

                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    if let Some(ref t) = track {
                        let resp = ui.add(egui::Label::new(
                            RichText::new(trunc(&t.title, 24)).color(T_PRI).font(FontId::proportional(13.5)).strong()
                        ).sense(Sense::click()));
                        if resp.clicked() { self.show_now_playing = true; }

                        ui.add_space(2.0);
                        ui.label(RichText::new(trunc(&t.artist, 24))
                            .color(T_SEC).font(FontId::proportional(12.0)));
                    } else {
                        ui.label(RichText::new("Nothing playing")
                            .color(T_DIM).font(FontId::proportional(13.0)));
                    }
                    if let PlaybackState::Error(ref e) = state {
                        ui.label(RichText::new(trunc(e, 22))
                            .color(Color32::from_rgb(255, 70, 70))
                            .font(FontId::proportional(11.0)));
                    }
                });

                if let Some(ref t) = track {
                    ui.add_space(8.0);
                    let is_liked  = self.playback.is_liked(&t.media_id);
                    let heart_ico = if is_liked { "♥" } else { "♡" };
                    let heart_col = if is_liked { Color32::from_rgb(255, 45, 85) } else { T_SEC };
                    if ui.add(egui::Button::new(
                        RichText::new(heart_ico).color(heart_col).font(FontId::proportional(16.0)))
                        .frame(false))
                        .on_hover_text("Save to Library")
                        .clicked()
                    {
                        self.playback.toggle_like(t.clone());
                    }

                    ui.add_space(6.0);
                    if ui.add(egui::Button::new(
                        RichText::new("⛶").font(FontId::proportional(14.0)).color(T_SEC))
                        .frame(false))
                        .on_hover_text("Full Screen Player")
                        .clicked()
                    {
                        self.show_now_playing = true;
                    }
                }
            });
        });

        // ── Center: transport controls + Progress Seek Bar ─────────────────
        let cx = center_rect.center().x;
        let cy = center_rect.center().y - 12.0;

        // Shuffle
        let is_shuf = *self.playback.is_shuffle.lock().unwrap();
        let shuf_r  = egui::Rect::from_center_size(egui::pos2(cx - 92.0, cy), Vec2::splat(32.0));
        let shuf_btn = egui::Button::new(
            RichText::new("🔀").color(if is_shuf { ACCENT } else { T_DIM })
                .font(FontId::proportional(15.0))).frame(false);
        if ui.put(shuf_r, shuf_btn).on_hover_text("Shuffle").clicked() {
            self.playback.toggle_shuffle();
        }

        // Prev
        let prev_r = egui::Rect::from_center_size(egui::pos2(cx - 46.0, cy), Vec2::splat(32.0));
        if ui.put(prev_r, ctrl_btn("⏮")).on_hover_text("Previous").clicked() {
            self.playback.skip_prev();
        }

        // Play/Pause
        let play_r = egui::Rect::from_center_size(egui::pos2(cx, cy), Vec2::splat(36.0));
        let (icon, hint) = match state {
            PlaybackState::Playing => ("⏸", "Pause"),
            PlaybackState::Paused  => ("▶",  "Play"),
            PlaybackState::Loading => ("⏳", "Loading…"),
            _                      => ("▶",  "Play"),
        };
        let pp = egui::Button::new(
            RichText::new(icon).color(Color32::BLACK).font(FontId::proportional(17.0)))
            .fill(ACCENT).min_size(Vec2::splat(36.0)).rounding(Rounding::same(18.0));
        if ui.put(play_r, pp).on_hover_text(hint).clicked() {
            self.playback.toggle_pause();
        }

        // Next
        let next_r = egui::Rect::from_center_size(egui::pos2(cx + 46.0, cy), Vec2::splat(32.0));
        if ui.put(next_r, ctrl_btn("⏭")).on_hover_text("Next").clicked() {
            self.playback.skip_next();
        }

        // Repeat
        let rep_mode = *self.playback.repeat_mode.lock().unwrap();
        let (rep_icon, rep_color) = match rep_mode {
            crate::playback_manager::RepeatMode::Off => ("🔁", T_DIM),
            crate::playback_manager::RepeatMode::All => ("🔁", ACCENT),
            crate::playback_manager::RepeatMode::One => ("🔂", ACCENT),
        };
        let rep_r = egui::Rect::from_center_size(egui::pos2(cx + 92.0, cy), Vec2::splat(32.0));
        let rep_btn = egui::Button::new(
            RichText::new(rep_icon).color(rep_color).font(FontId::proportional(15.0))).frame(false);
        if ui.put(rep_r, rep_btn).on_hover_text("Repeat").clicked() {
            self.playback.toggle_repeat();
        }

        // ── Center Time Display ──
        let time_curr_str = fmt_time(pos as u32);
        let time_total_str = fmt_time(dur_raw as u32);
        
        ui.painter().text(
            egui::pos2(cx + 130.0, cy),
            egui::Align2::LEFT_CENTER,
            format!("{} / {}", time_curr_str, time_total_str),
            FontId::proportional(11.5),
            T_DIM,
        );

        // ── Right: Volume control ─────────────────────────────────────────────
        ui.allocate_ui_at_rect(right_rect, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                
                // Custom Sleek Volume Slider (90px wide, 4px tall)
                let vol_w = 90.0_f32;
                let (vol_rect, vol_sense) = ui.allocate_exact_size(
                    Vec2::new(vol_w, 16.0),
                    Sense::click_and_drag(),
                );
                let line_y = vol_rect.center().y;
                let vol_line = egui::Rect::from_center_size(
                    vol_rect.center(),
                    Vec2::new(vol_w, 4.0),
                );

                let mut vol = *self.playback.volume.lock().unwrap();

                if vol_sense.clicked() || vol_sense.dragged() {
                    if let Some(ptr) = vol_sense.interact_pointer_pos() {
                        let frac = ((ptr.x - vol_line.min.x) / vol_w).clamp(0.0, 1.0);
                        vol = frac * 100.0;
                        self.playback.set_volume(vol);
                    }
                }

                // Draw background track
                let track_col = if vol_sense.hovered() { Color32::from_rgb(70, 70, 75) } else { Color32::from_rgb(45, 45, 50) };
                ui.painter().rect_filled(vol_line, Rounding::same(2.0), track_col);

                // Draw fill
                let fill_w = vol_w * (vol / 100.0).clamp(0.0, 1.0);
                if fill_w > 0.0 {
                    let fill_rect = egui::Rect::from_min_size(vol_line.min, Vec2::new(fill_w, 4.0));
                    let fill_col = if vol_sense.hovered() || vol_sense.dragged() { ACCENT } else { Color32::from_rgb(220, 220, 220) };
                    ui.painter().rect_filled(fill_rect, Rounding::same(2.0), fill_col);

                    if vol_sense.hovered() || vol_sense.dragged() {
                        let handle_pos = egui::pos2(vol_line.min.x + fill_w, line_y);
                        ui.painter().circle_filled(handle_pos, 5.0, Color32::WHITE);
                    }
                }

                ui.add_space(6.0);

                // Mute / Unmute Toggle Button
                let vol_icon = if vol == 0.0 { "🔇" }
                    else if vol < 40.0 { "🔈" }
                    else if vol < 70.0 { "🔉" }
                    else               { "🔊" };

                let vol_btn = egui::Button::new(
                    RichText::new(vol_icon).color(T_SEC).font(FontId::proportional(15.0))
                ).frame(false);
                
                if ui.add(vol_btn).on_hover_text(if vol == 0.0 { "Unmute" } else { "Mute" }).clicked() {
                    if vol > 0.0 {
                        self.playback.set_volume(0.0);
                    } else {
                        self.playback.set_volume(80.0);
                    }
                }
            });
        });
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .id_source("settings_scroll")
            .show(ui, |ui| {
                ui.add_space(16.0);
                ui.label(
                    RichText::new("Settings Control Center")
                        .color(T_PRI)
                        .font(FontId::proportional(24.0))
                        .strong(),
                );
                ui.add_space(16.0);

                let mut settings = self.playback.settings.lock().unwrap().clone();
                let mut changed  = false;

                // ── 1. Audio Quality & Bitrate ──────────────────────────────
                ui.label(RichText::new("Audio Streaming Quality").color(T_PRI).font(FontId::proportional(16.0)).strong());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.selectable_value(&mut settings.audio_quality, crate::settings::AudioQuality::DataSaver, "Data Saver (~1 MB/song)").clicked() { changed = true; }
                    if ui.selectable_value(&mut settings.audio_quality, crate::settings::AudioQuality::Normal, "Normal (~3 MB/song)").clicked() { changed = true; }
                    if ui.selectable_value(&mut settings.audio_quality, crate::settings::AudioQuality::High, "High Quality (~5 MB/song)").clicked() { changed = true; }
                });
                ui.add_space(18.0);
                ui.separator();
                ui.add_space(18.0);

                // ── 2. Playback Engine & Autoplay Toggles ───────────────────
                ui.label(RichText::new("Playback Engine & Performance").color(T_PRI).font(FontId::proportional(16.0)).strong());
                ui.add_space(10.0);

                if Self::render_setting_toggle_card(
                    ui,
                    "Seamless 0ms Gapless Advance",
                    "Pre-buffers next track for instant zero-gap transitions.",
                    &mut settings.gapless_playback,
                ) { changed = true; }
                ui.add_space(8.0);

                if Self::render_setting_toggle_card(
                    ui,
                    "Infinite Autoplay Radio",
                    "Automatically keeps playing related music when queue finishes.",
                    &mut settings.autoplay_radio,
                ) { changed = true; }
                ui.add_space(8.0);

                if Self::render_setting_toggle_card(
                    ui,
                    "RAM Stream Preloader",
                    "Pre-resolves upcoming track stream URLs in background.",
                    &mut settings.preload_next_track,
                ) { changed = true; }
                ui.add_space(8.0);

                if Self::render_setting_toggle_card(
                    ui,
                    "Low-End Device Performance Mode",
                    "Optimizes CPU & GPU memory usage for older PCs and low-spec hardware.",
                    &mut settings.low_end_mode,
                ) { changed = true; }
                ui.add_space(18.0);
                ui.separator();
                ui.add_space(18.0);

                // ── 3. Data Saver & Storage Cache Control ─────────────────
                ui.label(RichText::new("Offline Storage & Data Saver").color(T_PRI).font(FontId::proportional(16.0)).strong());
                ui.add_space(10.0);

                if Self::render_setting_toggle_card(
                    ui,
                    "Enable Offline Audio Caching",
                    "Saves streamed tracks locally so replays use 0 KB network data.",
                    &mut settings.enable_cache,
                ) { changed = true; }
                ui.add_space(12.0);

                let cache_size_mb = self.playback.data_saver.lock().unwrap().get_cache_size_mb();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Current Audio Cache: {:.1} MB", cache_size_mb)).color(T_SEC).font(FontId::proportional(14.0)));
                    ui.add_space(16.0);
                    if ui.add(egui::Button::new(
                        RichText::new("Clear Audio Cache").color(Color32::from_rgb(255, 100, 100)).font(FontId::proportional(13.0))
                    ).fill(Color32::from_rgb(45, 20, 20)).rounding(Rounding::same(8.0))).clicked() {
                        self.playback.data_saver.lock().unwrap().clear_cache();
                    }
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Max Storage Cap Limit:").color(T_SEC).font(FontId::proportional(14.0)));
                    let mut cap_float = settings.max_cache_size_mb as f32;
                    if ui.add(egui::Slider::new(&mut cap_float, 100.0..=5000.0).text("MB").step_by(100.0)).changed() {
                        settings.max_cache_size_mb = cap_float as u64;
                        changed = true;
                    }
                });
                ui.label(RichText::new("Automatically purges oldest cached tracks when storage limit is reached.").color(T_DIM).font(FontId::proportional(12.0)));

                ui.add_space(24.0);
                ui.separator();
                ui.add_space(24.0);

                // ── 4. Developer Showcase Card ──────────────────────────────
                ui.label(RichText::new("About & Developer").color(T_PRI).font(FontId::proportional(18.0)).strong());
                ui.add_space(12.0);

                egui::Frame::none()
                    .fill(Color32::from_rgb(22, 22, 26))
                    .rounding(Rounding::same(16.0))
                    .stroke(Stroke::new(1.2_f32, Color32::from_rgb(45, 45, 52)))
                    .inner_margin(egui::Margin::same(24.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
                            ui.painter().circle_filled(rect.center(), 26.0, ACCENT);
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "♪", FontId::proportional(26.0), Color32::BLACK);

                            ui.add_space(16.0);

                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Meduza Music Player").color(T_PRI).font(FontId::proportional(18.0)).strong());
                                    ui.add_space(8.0);
                                    ui.label(RichText::new("v1.2.0 (Release)").color(T_DIM).font(FontId::proportional(13.0)));
                                });

                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Crafted with").color(T_SEC).font(FontId::proportional(14.0)));
                                    ui.label(RichText::new("♥").color(Color32::from_rgb(255, 45, 85)).font(FontId::proportional(16.0)).strong());
                                    ui.label(RichText::new("by").color(T_SEC).font(FontId::proportional(14.0)));
                                    ui.hyperlink_to(
                                        RichText::new("@akilaisadev").color(ACCENT).font(FontId::proportional(14.0)).strong(),
                                        "https://github.com/akilaisadev",
                                    );
                                });

                                ui.add_space(14.0);

                                ui.horizontal(|ui| {
                                    let gh_btn = egui::Button::new(
                                        RichText::new("GitHub Profile").color(T_PRI).font(FontId::proportional(13.0)),
                                    )
                                    .fill(Color32::from_rgb(34, 34, 40))
                                    .rounding(Rounding::same(8.0))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(55, 55, 62)));

                                    if ui.add(gh_btn).on_hover_text("Open https://github.com/akilaisadev").clicked() {
                                        ui.ctx().open_url(egui::OpenUrl::same_tab("https://github.com/akilaisadev"));
                                    }

                                    ui.add_space(10.0);

                                    let repo_btn = egui::Button::new(
                                        RichText::new("⭐ Star Project").color(T_PRI).font(FontId::proportional(13.0)),
                                    )
                                    .fill(Color32::from_rgb(34, 34, 40))
                                    .rounding(Rounding::same(8.0))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(55, 55, 62)));

                                    if ui.add(repo_btn).on_hover_text("Open https://github.com/akilaisadev/meduza-music").clicked() {
                                        ui.ctx().open_url(egui::OpenUrl::same_tab("https://github.com/akilaisadev/meduza-music"));
                                    }
                                });
                            });
                        });
                    });

                if changed {
                    settings.save();
                    *self.playback.settings.lock().unwrap() = settings;
                }
            });
    }

    fn render_setting_toggle_card(
        ui: &mut egui::Ui,
        title: &str,
        description: &str,
        value: &mut bool,
    ) -> bool {
        let mut changed = false;
        let card_h = 56.0_f32;
        let card_w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());

        let is_hover = resp.hovered();
        let bg_col = if is_hover { Color32::from_rgb(32, 32, 38) } else { Color32::from_rgb(22, 22, 26) };
        let stroke_col = if is_hover { Color32::from_rgb(60, 60, 70) } else { Color32::from_rgb(40, 40, 46) };

        ui.painter().rect(rect, Rounding::same(12.0), bg_col, Stroke::new(1.0_f32, stroke_col));

        if resp.clicked() {
            *value = !*value;
            changed = true;
        }

        // Title & Description on Left
        let text_rect = egui::Rect::from_min_max(
            rect.min + Vec2::new(16.0, 8.0),
            rect.max - Vec2::new(60.0, 8.0),
        );
        ui.allocate_ui_at_rect(text_rect, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(title).color(T_PRI).font(FontId::proportional(15.0)).strong());
                ui.add_space(2.0);
                ui.label(RichText::new(description).color(T_SEC).font(FontId::proportional(12.0)));
            });
        });

        // Pill Toggle Switch on Right
        let pill_w = 40.0_f32;
        let pill_h = 22.0_f32;
        let pill_pos = egui::pos2(rect.max.x - 52.0, rect.center().y - (pill_h / 2.0));
        let pill_rect = egui::Rect::from_min_size(pill_pos, Vec2::new(pill_w, pill_h));

        let active = *value;
        let pill_bg = if active { ACCENT } else { Color32::from_rgb(50, 50, 56) };
        ui.painter().rect_filled(pill_rect, Rounding::same(11.0), pill_bg);

        // Knob
        let knob_r = 8.0_f32;
        let knob_x = if active { pill_rect.max.x - 11.0 } else { pill_rect.min.x + 11.0 };
        let knob_pos = egui::pos2(knob_x, pill_rect.center().y);
        let knob_col = if active { Color32::BLACK } else { Color32::from_rgb(180, 180, 180) };
        ui.painter().circle_filled(knob_pos, knob_r, knob_col);

        changed
    }

    fn show_now_playing_screen(&mut self, ctx: &egui::Context) {
        let track   = self.playback.current_track.lock().unwrap().clone();
        let state   = self.playback.state.lock().unwrap().clone();
        let pos     = *self.playback.progress_secs.lock().unwrap();
        let dur_raw = *self.playback.duration_secs.lock().unwrap();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(14, 14, 16)))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let cx   = rect.center().x;
                let cy   = rect.center().y;

                // ── 1. Compute background colors (lerp) and draw background FIRST ──
                // Must be drawn before any widgets so it doesn't paint over them
                if let Some(ref t) = track {
                    if t.media_id != self.bg_last_track {
                        // Deterministic cheap target first…
                        let (ta, tb) = generate_track_colors(&t.title, &t.artist);
                        self.bg_target_a = ta;
                        self.bg_target_b = tb;
                        self.bg_last_track = t.media_id.clone();

                        // …then enrich with worker-extracted dominant colors so the
                        // frame never blocks on image decode (esp. low-end CPUs).
                        let heavy = self.playback.settings.lock().unwrap_or_else(|e| e.into_inner()).heavy_background();
                        if heavy {
                            let store = Arc::clone(&self.bg_color_store);
                            let id = t.media_id.clone();
                            let bytes_opt = self.img_pending.lock().unwrap()
                                .get(&t.media_id).and_then(|v| v.clone());
                            let title = t.title.clone();
                            let artist = t.artist.clone();
                            crate::workers::decode().submit(move || {
                                let (da, db) = if let Some(bytes) = bytes_opt {
                                    extract_dominant_colors(&bytes)
                                } else {
                                    generate_track_colors(&title, &artist)
                                };
                                store.lock().unwrap_or_else(|e| e.into_inner()).insert(id, (da, db));
                            });
                        }
                    }
                    // Promote worker result if it has landed for this track.
                    if let Some(&(a, b)) = self.bg_color_store.lock().unwrap_or_else(|e| e.into_inner()).get(&t.media_id) {
                        self.bg_target_a = a;
                        self.bg_target_b = b;
                    }
                }
                let is_low_end = self.playback.settings.lock().unwrap().low_end_mode;
                let lerp_speed = if is_low_end { 1.0_f32 } else { 0.04_f32 };
                for i in 0..3 {
                    self.bg_color_a[i] += (self.bg_target_a[i] - self.bg_color_a[i]) * lerp_speed;
                    self.bg_color_b[i] += (self.bg_target_b[i] - self.bg_color_b[i]) * lerp_speed;
                }
                draw_ambient_blur_background(ui, rect, self.bg_color_a, self.bg_color_b, !is_low_end);

                // ── 2. Top Header (back button + label) ───────────────────────
                let collapse_r = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + 20.0, rect.min.y + 16.0),
                    Vec2::splat(36.0),
                );
                if ui.put(collapse_r, egui::Button::new(
                    RichText::new("▼").font(FontId::proportional(22.0)).color(T_PRI)
                ).frame(false)).on_hover_text("Collapse Player").clicked() {
                    self.show_now_playing = false;
                }

                ui.painter().text(
                    egui::pos2(rect.max.x - 24.0, rect.min.y + 34.0),
                    egui::Align2::RIGHT_CENTER,
                    "PLAYING FROM QUEUE",
                    FontId::proportional(11.0),
                    T_DIM,
                );

                // ── 2. Gramophone Vinyl Record Disk (Enlarged) ────────────────
                let vinyl_center = egui::pos2(cx, cy - 130.0);
                let radius = 165.0_f32;

                let tex_opt = if let Some(ref t) = track {
                    let p = Arc::clone(&self.img_pending);
                    let max_dim = self.playback.settings.lock().unwrap_or_else(|e| e.into_inner()).image_max_dim();
                    Self::get_texture(&mut self.img_cache, &p, &self.image_rgba, ui.ctx(), &t.media_id, &t.thumbnail_url, max_dim)
                } else { None };

                // ── 3. Vinyl Record Disk ───────────────────────────────────────
                draw_vinyl_record(ui, vinyl_center, radius, tex_opt, self.disc_angle);

                // ── 4. Title & Artist (Centered) ──────────────────────────────
                if let Some(ref t) = track {
                    ui.painter().text(
                        egui::pos2(cx, cy + 60.0),
                        egui::Align2::CENTER_CENTER,
                        trunc(&t.title, 32),
                        FontId::proportional(26.0),
                        T_PRI,
                    );
                    ui.painter().text(
                        egui::pos2(cx, cy + 92.0),
                        egui::Align2::CENTER_CENTER,
                        trunc(&t.artist, 32),
                        FontId::proportional(17.0),
                        T_SEC,
                    );
                } else {
                    ui.painter().text(
                        egui::pos2(cx, cy + 75.0),
                        egui::Align2::CENTER_CENTER,
                        "Nothing playing",
                        FontId::proportional(22.0),
                        T_DIM,
                    );
                }

                // ── 4. Interactive Green Progress Slider (Wider 480px) ────────
                let progress_w = 480.0_f32;
                let slider_y   = cy + 145.0;
                let line_rect  = egui::Rect::from_center_size(
                    egui::pos2(cx, slider_y),
                    Vec2::new(progress_w, 6.0),
                );

                let slider_sense = ui.allocate_rect(
                    egui::Rect::from_center_size(egui::pos2(cx, slider_y), Vec2::new(progress_w, 20.0)),
                    Sense::click_and_drag(),
                );

                if slider_sense.clicked() || slider_sense.dragged() {
                    if let Some(ptr) = slider_sense.interact_pointer_pos() {
                        let frac = ((ptr.x - line_rect.min.x) / progress_w).clamp(0.0, 1.0);
                        self.playback.seek_to(frac * dur_raw);
                    }
                }

                let bg_col = if slider_sense.hovered() { Color32::from_rgb(70, 70, 70) } else { Color32::from_rgb(45, 45, 50) };
                ui.painter().rect_filled(line_rect, Rounding::same(3.0), bg_col);

                if dur_raw > 0.0 {
                    let pct = (pos / dur_raw).clamp(0.0, 1.0);
                    let fill_w = progress_w * pct;
                    let fill_rect = egui::Rect::from_min_size(
                        line_rect.min,
                        Vec2::new(fill_w, 6.0),
                    );
                    ui.painter().rect_filled(fill_rect, Rounding::same(3.0), ACCENT);

                    if slider_sense.hovered() || slider_sense.dragged() {
                        let handle_pos = egui::pos2(line_rect.min.x + fill_w, slider_y);
                        ui.painter().circle_filled(handle_pos, 6.0, Color32::WHITE);
                    }
                }

                // Time labels
                ui.painter().text(
                    egui::pos2(cx - progress_w / 2.0, slider_y + 16.0),
                    egui::Align2::LEFT_TOP,
                    fmt_time(pos as u32),
                    FontId::proportional(13.0),
                    T_DIM,
                );
                ui.painter().text(
                    egui::pos2(cx + progress_w / 2.0, slider_y + 16.0),
                    egui::Align2::RIGHT_TOP,
                    fmt_time(dur_raw as u32),
                    FontId::proportional(13.0),
                    T_DIM,
                );

                // ── 5. Transport Controls (Enlarged & Centered) ──────────────
                let ctrl_y = cy + 225.0;

                // Shuffle
                let is_shuf = *self.playback.is_shuffle.lock().unwrap();
                let shuf_r  = egui::Rect::from_center_size(egui::pos2(cx - 140.0, ctrl_y), Vec2::splat(40.0));
                let shuf_btn = egui::Button::new(
                    RichText::new("🔀").color(if is_shuf { ACCENT } else { T_DIM })
                        .font(FontId::proportional(22.0))).frame(false);
                if ui.put(shuf_r, shuf_btn).on_hover_text("Shuffle").clicked() {
                    self.playback.toggle_shuffle();
                }

                // Prev
                let prev_r = egui::Rect::from_center_size(egui::pos2(cx - 70.0, ctrl_y), Vec2::splat(40.0));
                let prev_btn = egui::Button::new(
                    RichText::new("⏮").color(T_PRI).font(FontId::proportional(30.0))).frame(false);
                if ui.put(prev_r, prev_btn).on_hover_text("Previous").clicked() {
                    self.playback.skip_prev();
                }

                // Play/Pause Big 64px Button
                let play_r = egui::Rect::from_center_size(egui::pos2(cx, ctrl_y), Vec2::splat(64.0));
                let (icon, hint) = match state {
                    PlaybackState::Playing => ("⏸", "Pause"),
                    PlaybackState::Paused  => ("▶",  "Play"),
                    PlaybackState::Loading => ("⏳", "Loading…"),
                    _                      => ("▶",  "Play"),
                };
                let pp = egui::Button::new(
                    RichText::new(icon).color(Color32::BLACK).font(FontId::proportional(28.0))
                ).fill(ACCENT).min_size(Vec2::splat(64.0)).rounding(Rounding::same(32.0));
                if ui.put(play_r, pp).on_hover_text(hint).clicked() {
                    self.playback.toggle_pause();
                }

                // Next
                let next_r = egui::Rect::from_center_size(egui::pos2(cx + 70.0, ctrl_y), Vec2::splat(40.0));
                let next_btn = egui::Button::new(
                    RichText::new("⏭").color(T_PRI).font(FontId::proportional(30.0))).frame(false);
                if ui.put(next_r, next_btn).on_hover_text("Next").clicked() {
                    self.playback.skip_next();
                }

                // Repeat
                let rep_mode = *self.playback.repeat_mode.lock().unwrap();
                let (rep_icon, rep_color) = match rep_mode {
                    crate::playback_manager::RepeatMode::Off => ("🔁", T_DIM),
                    crate::playback_manager::RepeatMode::All => ("🔁", ACCENT),
                    crate::playback_manager::RepeatMode::One => ("🔂", ACCENT),
                };
                let rep_r = egui::Rect::from_center_size(egui::pos2(cx + 140.0, ctrl_y), Vec2::splat(40.0));
                let rep_btn = egui::Button::new(
                    RichText::new(rep_icon).color(rep_color).font(FontId::proportional(22.0))).frame(false);
                if ui.put(rep_r, rep_btn).on_hover_text("Repeat").clicked() {
                    self.playback.toggle_repeat();
                }

                // Heart Like Button (Moved directly to transport row beside Repeat)
                if let Some(ref t) = track {
                    let heart_r = egui::Rect::from_center_size(
                        egui::pos2(cx + 210.0, ctrl_y),
                        Vec2::splat(40.0),
                    );
                    let is_liked  = self.playback.is_liked(&t.media_id);
                    let heart_ico = if is_liked { "♥" } else { "♡" };
                    let heart_col = if is_liked { Color32::from_rgb(255, 45, 85) } else { T_SEC };
                    if ui.put(heart_r, egui::Button::new(
                        RichText::new(heart_ico).color(heart_col).font(FontId::proportional(22.0))
                    ).frame(false)).on_hover_text("Save to Library").clicked() {
                        self.playback.toggle_like(t.clone());
                    }
                }

                // ── 6. Full Screen Volume Control Bar ─────────────────────────
                let vol_y = cy + 295.0;
                let vol_w = 140.0_f32;
                
                let mut vol = *self.playback.volume.lock().unwrap();

                // Mute / Unmute Button
                let vol_icon = if vol == 0.0 { "🔇" }
                    else if vol < 40.0 { "🔈" }
                    else if vol < 70.0 { "🔉" }
                    else               { "🔊" };

                let mute_r = egui::Rect::from_center_size(
                    egui::pos2(cx - (vol_w / 2.0 + 20.0), vol_y),
                    Vec2::splat(32.0),
                );
                let mute_btn = egui::Button::new(
                    RichText::new(vol_icon).color(T_SEC).font(FontId::proportional(18.0))
                ).frame(false);
                if ui.put(mute_r, mute_btn).on_hover_text(if vol == 0.0 { "Unmute" } else { "Mute" }).clicked() {
                    if vol > 0.0 {
                        self.playback.set_volume(0.0);
                    } else {
                        self.playback.set_volume(80.0);
                    }
                }

                // Volume Bar
                let vol_line = egui::Rect::from_center_size(
                    egui::pos2(cx + 10.0, vol_y),
                    Vec2::new(vol_w, 5.0),
                );

                let vol_sense = ui.allocate_rect(
                    egui::Rect::from_center_size(egui::pos2(cx + 10.0, vol_y), Vec2::new(vol_w, 20.0)),
                    Sense::click_and_drag(),
                );

                if vol_sense.clicked() || vol_sense.dragged() {
                    if let Some(ptr) = vol_sense.interact_pointer_pos() {
                        let frac = ((ptr.x - vol_line.min.x) / vol_w).clamp(0.0, 1.0);
                        vol = frac * 100.0;
                        self.playback.set_volume(vol);
                    }
                }

                let track_col = if vol_sense.hovered() { Color32::from_rgb(75, 75, 80) } else { Color32::from_rgb(45, 45, 50) };
                ui.painter().rect_filled(vol_line, Rounding::same(2.5), track_col);

                let fill_w = vol_w * (vol / 100.0).clamp(0.0, 1.0);
                if fill_w > 0.0 {
                    let fill_rect = egui::Rect::from_min_size(vol_line.min, Vec2::new(fill_w, 5.0));
                    let fill_col = if vol_sense.hovered() || vol_sense.dragged() { ACCENT } else { Color32::from_rgb(220, 220, 220) };
                    ui.painter().rect_filled(fill_rect, Rounding::same(2.5), fill_col);

                    if vol_sense.hovered() || vol_sense.dragged() {
                        let handle_pos = egui::pos2(vol_line.min.x + fill_w, vol_y);
                        ui.painter().circle_filled(handle_pos, 5.0, Color32::WHITE);
                    }
                }
            });
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract two dominant colors from raw image bytes (JPEG/PNG/WebP).
/// Returns (primary, secondary) as [f32;3] RGB normalized 0-1.
/// Uses a fast 8×8 downsampled grid to find vibrant colors.
fn extract_dominant_colors(bytes: &[u8]) -> ([f32; 3], [f32; 3]) {
    let img = match image::load_from_memory(bytes) {
        Ok(i) => i,
        Err(_) => return default_bg_colors(),
    };

    let small = img.thumbnail(32, 32).to_rgba8();
    let pixels: Vec<(f32, f32, f32)> = small
        .chunks_exact(4)
        .filter(|p| p[3] > 128)
        .map(|p| (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0))
        .collect();

    if pixels.is_empty() {
        return default_bg_colors();
    }

    // Sort by "vibrancy" (saturation proxy = max-min channel diff)
    let mut vibrant: Vec<(f32, f32, f32, f32)> = pixels.iter()
        .map(|&(r, g, b)| {
            let mx = r.max(g).max(b);
            let mn = r.min(g).min(b);
            (r, g, b, mx - mn) // (r, g, b, vibrancy)
        })
        .collect();
    vibrant.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));

    // Primary: top vibrant pixel, dimmed to stay dark
    let (r1, g1, b1, _) = vibrant[0];
    let primary = [r1 * 0.35, g1 * 0.35, b1 * 0.35];

    // Secondary: pick from bottom-quarter of sorted list for contrast
    let sec_idx = (vibrant.len() * 3 / 4).max(1).min(vibrant.len() - 1);
    let (r2, g2, b2, _) = vibrant[sec_idx];
    let secondary = [r2 * 0.25, g2 * 0.25, b2 * 0.25];

    (primary, secondary)
}

/// Generate deterministic ambient colors from track metadata when image is unavailable.
fn generate_track_colors(title: &str, artist: &str) -> ([f32; 3], [f32; 3]) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut h1 = DefaultHasher::new();
    title.hash(&mut h1);
    let hue1 = (h1.finish() % 360) as f32;

    let mut h2 = DefaultHasher::new();
    artist.hash(&mut h2);
    let hue2 = (h2.finish() % 360) as f32;

    (hsl_to_rgb_dark(hue1, 0.7, 0.18), hsl_to_rgb_dark(hue2, 0.6, 0.14))
}

fn default_bg_colors() -> ([f32; 3], [f32; 3]) {
    ([0.05, 0.04, 0.10], [0.10, 0.03, 0.08])
}

/// Convert HSL to dark-mode RGB (0-1).
fn hsl_to_rgb_dark(h: f32, s: f32, l: f32) -> [f32; 3] {
    let h = h / 360.0;
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue2rgb = |p: f32, q: f32, mut t: f32| -> f32 {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
        if t < 1.0 / 2.0 { return q; }
        if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
        p
    };
    [hue2rgb(p, q, h + 1.0 / 3.0), hue2rgb(p, q, h), hue2rgb(p, q, h - 1.0 / 3.0)]
}

/// Paint a multi-layer soft ambient blur background for the Now Playing screen.
/// Uses layered radial mesh gradients to create a blurred glow effect.
/// When `glow` is false (low-end profile) we skip the 20-layer radial loop and
/// draw a single flat tint instead — same look family, ~zero fill cost.
fn draw_ambient_blur_background(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    color_a: [f32; 3],
    color_b: [f32; 3],
    glow: bool,
) {
    let painter = ui.painter();

    // Clean near-black base
    painter.rect_filled(rect, Rounding::ZERO, Color32::from_rgb(10, 10, 12));

    let cx = rect.center().x;
    let cy = rect.center().y;
    let w  = rect.width();
    let h  = rect.height();

    // Single very subtle glow strictly behind the vinyl disc (upper half only)
    // Blend colors for a unified, harmonious tint
    let r_ch = ((color_a[0] + color_b[0]) * 0.5 * 255.0) as u8;
    let g_ch = ((color_a[1] + color_b[1]) * 0.5 * 255.0) as u8;
    let b_ch = ((color_a[2] + color_b[2]) * 0.5 * 255.0) as u8;

    let glow_center = egui::pos2(cx, cy - h * 0.12);
    let glow_radius = w.min(h) * 0.32; // tight radius — stays inside vinyl area

    if !glow {
        // One soft radial fill centered behind the vinyl — no radial loop.
        painter.circle_filled(
            glow_center,
            glow_radius,
            Color32::from_rgba_unmultiplied(r_ch, g_ch, b_ch, 28),
        );
        return;
    }

    let layers = 20u32;
    for i in 0..layers {
        let t    = i as f32 / layers as f32;
        let r    = glow_radius * t;
        let fade = 1.0 - t;
        // max alpha 28 — just a whisper of color, not visible as a shape
        let a = (28.0_f32 * fade * fade) as u8;
        painter.circle_filled(
            glow_center,
            r,
            Color32::from_rgba_unmultiplied(r_ch, g_ch, b_ch, a),
        );
    }
}


fn load_system_fallback_fonts(fonts: &mut egui::FontDefinitions) {
    let font_dirs = [
        "/usr/share/fonts/truetype/noto",
        "/usr/share/fonts/opentype/noto",
        "/usr/share/fonts/noto",
        "/usr/share/fonts/TTF",
        "/usr/share/fonts",
        "/app/share/fonts",
    ];

    let font_files = [
        "NotoSansSinhala-Regular.ttf",
        "NotoSansSinhala-Bold.ttf",
        "NotoSerifSinhala-Regular.ttf",
        "NotoSansDevanagari-Regular.ttf",
        "NotoSansTamil-Regular.ttf",
        "NotoSansTelugu-Regular.ttf",
        "NotoSansBengali-Regular.ttf",
        "NotoSansMalayalam-Regular.ttf",
        "NotoSansKannada-Regular.ttf",
        "NotoSansGujarati-Regular.ttf",
        "NotoSansGurmukhi-Regular.ttf",
        "NotoSansArabic-Regular.ttf",
        "NotoSansThai-Regular.ttf",
        "NotoSansHebrew-Regular.ttf",
        "NotoSansMyanmar-Regular.ttf",
        "NotoSansKhmer-Regular.ttf",
    ];

    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        for file_name in &font_files {
            for dir in &font_dirs {
                let p = std::path::Path::new(dir).join(file_name);
                if p.exists() {
                    if let Ok(bytes) = std::fs::read(&p) {
                        let name = file_name.to_string();
                        if !fonts.font_data.contains_key(&name) {
                            fonts.font_data.insert(name.clone(), egui::FontData::from_owned(bytes));
                            prop.push(name);
                        }
                    }
                    break;
                }
            }
        }
    }
}

fn ctrl_btn(label: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label).color(T_SEC).font(FontId::proportional(20.0)))
        .fill(Color32::TRANSPARENT)
        .frame(false)
        .min_size(Vec2::splat(32.0))
}

fn trunc(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max { s.to_string() }
    else { chars[..max].iter().collect::<String>() + "…" }
}

fn fmt_time(secs: u32) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

fn draw_vinyl_record(
    ui: &mut egui::Ui,
    center: egui::Pos2,
    radius: f32,
    tex: Option<&egui::TextureHandle>,
    angle: f32,
) {
    let center_hole_r = radius * 0.18; // Sleek central black hub radius (~30px)

    // 1. Base Vinyl Outer Body & Outline
    ui.painter().circle_filled(center, radius, Color32::from_rgb(18, 18, 20));
    ui.painter().circle_stroke(center, radius, Stroke::new(1.5_f32, Color32::from_rgb(45, 45, 50)));

    // 2. Full-Cover Rotating Album Artwork Texture (Spans full disk radius)
    if let Some(texture) = tex {
        let mut mesh = egui::Mesh::with_texture(texture.id());
        
        // Center vertex
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center,
            uv: egui::pos2(0.5, 0.5),
            color: Color32::from_rgb(235, 235, 235),
        });

        // 64 segment circular fan perimeter spanning full disk radius
        let segments = 64;
        let art_radius = radius - 1.5;
        for i in 0..segments {
            let frac = (i as f32) / (segments as f32);
            let theta = frac * 2.0 * std::f32::consts::PI;
            let rot_theta = theta + angle;
            
            let pos = center + egui::vec2(rot_theta.cos(), rot_theta.sin()) * art_radius;
            let uv_x = 0.5 + 0.5 * theta.cos();
            let uv_y = 0.5 + 0.5 * theta.sin();

            mesh.vertices.push(egui::epaint::Vertex {
                pos,
                uv: egui::pos2(uv_x, uv_y),
                color: Color32::from_rgb(235, 235, 235),
            });
        }

        // Add fan triangles
        for i in 1..=segments {
            let next = if i == segments { 1 } else { (i + 1) as u32 };
            mesh.add_triangle(0, i as u32, next);
        }

        ui.painter().add(mesh);
    } else {
        // Fallback default vinyl record body gradient
        ui.painter().circle_filled(center, radius - 2.0, Color32::from_rgb(28, 28, 32));
    }

    // 3. Realistic Concentric Vinyl Record Grooves overlayed on full disk
    let groove_min = center_hole_r + 4.0;
    let groove_max = radius - 4.0;
    let num_grooves = (radius * 0.35) as usize; 
    let step = (groove_max - groove_min) / (num_grooves as f32).max(1.0);
    
    for i in 0..num_grooves {
        let r = groove_min + (i as f32) * step;
        let alpha = if i % 7 == 0 { 42 } else if i % 3 == 0 { 28 } else { 18 };
        ui.painter().circle_stroke(
            center,
            r,
            Stroke::new(0.8_f32, Color32::from_rgba_unmultiplied(10, 10, 15, alpha)),
        );
    }

    // 4. Light Sheen Reflections (Hourglass shape)
    let sheen_color = Color32::from_rgba_unmultiplied(255, 255, 255, 14);
    for &base_angle in &[-std::f32::consts::FRAC_PI_4, std::f32::consts::PI - std::f32::consts::FRAC_PI_4] {
        let mut mesh = egui::Mesh::default();
        let spread = 0.35_f32;
        
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center,
            uv: egui::pos2(0.5, 0.5),
            color: sheen_color,
        });
        
        let segments = 12;
        for i in 0..=segments {
            let frac = (i as f32) / (segments as f32);
            let theta = base_angle - spread + (spread * 2.0 * frac);
            let pos = center + egui::vec2(theta.cos(), theta.sin()) * radius;
            let alpha = (1.0 - (frac - 0.5).abs() * 2.0) * 18.0; 
            let edge_color = Color32::from_rgba_unmultiplied(255, 255, 255, alpha as u8);
            
            mesh.vertices.push(egui::epaint::Vertex {
                pos,
                uv: egui::pos2(0.5, 0.5),
                color: edge_color,
            });
        }
        
        for i in 1..=segments {
            mesh.add_triangle(0, i as u32, (i + 1) as u32);
        }
        ui.painter().add(mesh);
    }

    // 5. Rotating Specular Highlight Lines
    for &a in &[angle, angle + std::f32::consts::PI] {
        let p1 = center + egui::vec2(a.cos(), a.sin()) * (center_hole_r + 2.0);
        let p2 = center + egui::vec2(a.cos(), a.sin()) * (radius - 2.0);
        ui.painter().line_segment(
            [p1, p2],
            Stroke::new(1.8_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 16)),
        );
    }

    // 6. Sleek Center Black Hub & Metallic Spindle Hole
    ui.painter().circle_filled(center, center_hole_r, Color32::from_rgb(10, 10, 12));
    ui.painter().circle_stroke(
        center,
        center_hole_r,
        Stroke::new(2.0_f32, Color32::from_rgb(35, 35, 40)),
    );
    let spindle_r = 5.5_f32;
    ui.painter().circle_filled(center, spindle_r, Color32::from_rgb(4, 4, 6));
    ui.painter().circle_stroke(
        center,
        spindle_r,
        Stroke::new(1.2_f32, Color32::from_rgb(75, 75, 80)),
    );
}

// ── eframe::App ───────────────────────────────────────────────────────────────

impl eframe::App for MeduzaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Tray Event Handling
        if let Ok(_event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            match event.id.as_ref() {
                "show" => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                },
                "play" => self.playback.toggle_pause(),
                "quit" => {
                    self.is_exiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                },
                _ => {}
            }
        }

        // Intercept window close (X button) to hide to tray instead (only if system tray icon exists)
        if self.has_tray.load(std::sync::atomic::Ordering::SeqCst) && ctx.input(|i| i.viewport().close_requested()) && !self.is_exiting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // Handle auto advance
        self.playback.handle_auto_advance();

        // Repaint/cpu-load throttling:
        // - 60fps (16ms) on the now-playing vinyl; 30fps (33ms) on low-end.
        //   This was the main cause of the "music player lag" / CPU overload:
        //   the app was forcing a full 60fps repaint of the whole UI (feed
        //   panels, images, sidebar) at all times while playing.
        // - ~10fps is plenty for the progress bar & navigation elsewhere.
        let st = self.playback.state.lock().unwrap().clone();
        if matches!(st, PlaybackState::Playing) {
            if self.show_now_playing {
                let frame_ms = self.playback.settings.lock().unwrap_or_else(|e| e.into_inner()).now_playing_frame_ms();
                self.disc_angle += 0.015;
                if self.disc_angle > std::f32::consts::TAU * 100.0 {
                    self.disc_angle = 0.0;
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(frame_ms));
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        } else if matches!(st, PlaybackState::Loading) || *self.is_loading_home.lock().unwrap() || *self.is_searching.lock().unwrap() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        if self.show_now_playing {
            self.show_now_playing_screen(ctx);
            return;
        }

        // NOTE: freshly decoded images wake the frame loop themselves via
        // `ctx.request_repaint()` from the (single) decode worker — one wake
        // per image, never a sustained repaint loop.

        // ── Layout ───────────────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("player")
            .exact_height(90.0)
            .frame(egui::Frame::none())
            .show(ctx, |ui| { self.bottom_player(ui); });

        egui::TopBottomPanel::top("topbar")
            .exact_height(50.0)
            .frame(egui::Frame::none())
            .show(ctx, |ui| { self.topbar(ui); });

        egui::SidePanel::left("sidebar")
            .exact_width(200.0)
            .resizable(false)
            .frame(egui::Frame::none())
            .show(ctx, |ui| { self.sidebar(ui); });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(egui::Margin { left: 20.0, right: 6.0, top: 12.0, bottom: 12.0 })
            )
            .show(ctx, |ui| {
                match self.tab {
                    Tab::Home     => self.show_home(ui),
                    Tab::Search   => self.show_search(ui),
                    Tab::Library  => self.show_library(ui),
                    Tab::Settings => self.show_settings(ui),
                }
            });
    }
}
