use eframe::egui::{self, Color32, FontId, RichText, Sense, Stroke, Vec2, Rounding};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use meduza_core::innertube::TrackItem;
use meduza_playback::playback_manager::PlaybackState;

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
    viewmodel: Arc<meduza_viewmodel::app_viewmodel::AppViewModel>,

    tab: Tab,

    // Search
    search_query:   String,
    last_search:    String,
    suggestions:    Arc<Mutex<Vec<String>>>,
    show_suggest:   bool,

    // Images
    img_cache:    HashMap<String, egui::TextureHandle>,
    img_pending:  Arc<Mutex<HashMap<String, Option<Vec<u8>>>>>,
    image_rgba:   Arc<Mutex<HashMap<String, (usize, usize, Vec<u8>)>>>,
    logo_texture: Option<egui::TextureHandle>,

    show_now_playing: bool,

    // ── Cached snapshots (avoids cloning from RwLock every frame) ────────
    last_data_version: u64,
    cached_sections:   Vec<meduza_core::innertube::BrowseSection>,
    cached_search:     Vec<TrackItem>,
    cached_max_dim:    u32,
}

impl MeduzaApp {
    pub fn new(cc: &eframe::CreationContext<'_>, viewmodel: Arc<meduza_viewmodel::app_viewmodel::AppViewModel>) -> Self {
        // Font setup
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "AppFont".to_owned(),
            egui::FontData::from_static(include_bytes!("../../assets/Inter-Regular.ttf")),
        );
        fonts.font_data.insert(
            "CJK".to_owned(),
            egui::FontData::from_static(include_bytes!("../../assets/NotoSansCJK.otf")),
        );
        fonts.font_data.insert(
            "Sinhala".to_owned(),
            egui::FontData::from_static(include_bytes!("../../assets/NotoSansSinhala-Regular.ttf")),
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



        // Load logo texture
        let logo_bytes = include_bytes!("../../assets/logo.png");
        let logo_texture = if let Ok(img) = image::load_from_memory(logo_bytes) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            Some(cc.egui_ctx.load_texture("app_logo", ci, egui::TextureOptions::LINEAR))
        } else {
            None
        };

        Self {
            viewmodel,
            tab: Tab::Home,
            search_query: String::new(),
            last_search:  String::new(),
            suggestions:    Arc::new(Mutex::new(Vec::new())),
            show_suggest:   false,
            img_cache:       HashMap::new(),
            img_pending:     Arc::new(Mutex::new(HashMap::new())),
            image_rgba:      Arc::new(Mutex::new(HashMap::new())),
            logo_texture,
            show_now_playing: false,
            last_data_version: u64::MAX, // force first-frame refresh
            cached_sections:   Vec::new(),
            cached_search:     Vec::new(),
            cached_max_dim:    200,
        }
    }

    // ── Image loading ─────────────────────────────────────────────────────────

    fn image_cache_dir() -> std::path::PathBuf {
        let d = meduza_core::settings::app_cache_root().join("images");
        meduza_core::settings::ensure_private_dir(&d);
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
        let mut p = pending.lock().unwrap();
        if p.contains_key(id) { return; }

        let mut fixed_url = url.trim().to_string();
        if fixed_url.starts_with("//") {
            fixed_url = format!("https:{}", fixed_url);
        }

        // SSRF defense: only fetch images from known YouTube/Google hosts.
        if fixed_url.is_empty() || !meduza_core::settings::host_is_allowed(&fixed_url, meduza_core::settings::UrlKind::Image) {
            // Mark as empty immediately so we don't re-queue/re-check every frame
            p.insert(id.to_string(), Some(Vec::new()));
            return;
        }

        p.insert(id.to_string(), None);
        let id2    = id.to_string();
        let url2   = fixed_url;
        let p2     = Arc::clone(pending);
        let rgba2  = Arc::clone(rgba);
        meduza_core::workers::image_fetch().submit(move || {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            use std::io::Read;

            let mut hasher = DefaultHasher::new();
            id2.hash(&mut hasher);
            let cache_file = Self::image_cache_dir().join(format!("{}.img", hasher.finish()));

            let mut raw: Option<Vec<u8>> = None;

            if let Ok(buf) = std::fs::read(&cache_file) {
                if !buf.is_empty() {
                    raw = Some(buf);
                }
            } else if let Ok(resp) = meduza_core::settings::fetch_allowed(
                &url2,
                meduza_core::settings::UrlKind::Image,
                meduza_core::settings::FetchMethod::Get,
                std::time::Duration::from_secs(10),
            ) {
                let mut buf = Vec::new();
                if resp.into_reader().take(5 * 1024 * 1024).read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                    meduza_core::settings::ensure_private_dir(&cache_file);
                    let _ = std::fs::write(&cache_file, &buf);
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&cache_file, std::fs::Permissions::from_mode(0o600));
                    raw = Some(buf);
                }
            }

            // Bound on-disk cache size: prune periodically
            static PRUNE_TICK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            if PRUNE_TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 64 == 0 {
                Self::prune_image_cache();
            }

            if let Some(bytes) = raw {
                meduza_core::workers::decode().submit(move || {
                    let mut tex_px = (0usize, 0usize, Vec::<u8>::new());
                    if let Ok(img) = image::load_from_memory(&bytes) {
                        let (w, h) = (img.width(), img.height());
                        if w > 0 && h > 0 && w <= 4096 && h <= 4096 {
                            if w.max(h) > max_dim {
                                let scale = max_dim as f32 / w.max(h) as f32;
                                let nw = ((w as f32 * scale) as u32).max(1);
                                let nh = ((h as f32 * scale) as u32).max(1);
                                let small = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
                                tex_px = (nw as usize, nh as usize, small.into_raw());
                            } else {
                                tex_px = (w as usize, h as usize, img.to_rgba8().into_raw());
                            }
                        }
                    }

                    if tex_px.0 > 0 {
                        {
                            let mut r = rgba2.lock().unwrap();
                            r.insert(id2.clone(), tex_px);
                            if r.len() > 96 { r.clear(); }
                        }
                        {
                            let mut g = p2.lock().unwrap();
                            g.insert(id2, Some(bytes));
                            if g.len() > 96 { g.clear(); }
                        }
                        ctx.request_repaint();
                    } else {
                        let mut g = p2.lock().unwrap();
                        g.insert(id2, Some(Vec::new()));
                    }
                });
            } else {
                let mut g = p2.lock().unwrap();
                g.insert(id2, Some(Vec::new()));
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

        // try_lock: if the decode worker is writing, skip this frame (never block UI)
        if let Ok(mut rgba_map) = rgba.try_lock() {
            if let Some((w, h, px)) = rgba_map.get(id).cloned() {
                if w > 0 && h > 0 {
                    let ci = egui::ColorImage::from_rgba_unmultiplied([w, h], &px);
                    let handle = ctx.load_texture(id, ci, egui::TextureOptions::LINEAR);
                    cache.insert(id.to_string(), handle);
                    rgba_map.remove(id);
                    return cache.get(id);
                }
            }
        }

        // Not decoded yet — make sure a fetch/decode worker is queued (try_lock to avoid blocking)
        if let Ok(p) = pending.try_lock() {
            if !p.contains_key(id) {
                drop(p);
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
                    let queue_opt = self.viewmodel.queue_snapshot.try_read();
                    let idx_opt   = self.viewmodel.queue_index_snapshot.try_read();
                    if let (Ok(queue), Ok(idx_g)) = (queue_opt, idx_opt) {
                        let idx = *idx_g;
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
                                self.viewmodel.playback.play_now(item.track.clone());
                            }
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
        self.viewmodel.search(q);
    }

    // ── Home ──────────────────────────────────────────────────────────────────

    fn show_home(&mut self, ui: &mut egui::Ui) {
        let loading = self.viewmodel.is_loading_home.load(std::sync::atomic::Ordering::SeqCst);

        if loading && self.cached_sections.is_empty() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(44.0).color(ACCENT));
                ui.add_space(12.0);
                ui.label(RichText::new("Loading your personalized feed…").color(T_SEC)
                    .font(FontId::proportional(15.0)));
            });
            return;
        }

        if self.cached_sections.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("⚠  Could not load feed").color(T_SEC)
                    .font(FontId::proportional(15.0)));
                ui.add_space(10.0);
                if self.retry_btn(ui) {
                    self.viewmodel.load_home_feed();
                }
            });
            return;
        }

        let max_dim = self.cached_max_dim;
        let mut seen = std::collections::HashSet::<String>::new();
        let sections = std::mem::take(&mut self.cached_sections);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .id_source("home")
            .show(ui, |ui| {
                ui.add_space(8.0);

                for section in &sections {
                    let unique_items = meduza_core::recommendation_engine::RecommendationEngine::filter_unique(&section.items, &mut seen);
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
        
        self.cached_sections = sections;
    }

    fn retry_btn(&self, ui: &mut egui::Ui) -> bool {
        ui.add(egui::Button::new(
            RichText::new("  Retry  ").color(Color32::BLACK).font(FontId::proportional(13.0))
        ).fill(ACCENT).rounding(Rounding::same(20.0))).clicked()
    }

    fn card_grid(&mut self, ui: &mut egui::Ui, tracks: &[TrackItem], max_dim: u32) {
        // Filter out tracks that have permanently failed to load thumbnails
        let mut valid_tracks = Vec::new();
        if let Ok(pending) = self.img_pending.try_lock() {
            for track in tracks {
                if let Some(Some(bytes)) = pending.get(&track.media_id) {
                    if bytes.is_empty() {
                        continue;
                    }
                }
                valid_tracks.push(track.clone());
            }
        } else {
            valid_tracks = tracks.to_vec();
        }

        if valid_tracks.is_empty() { return; }

        // Dynamically compute how many cards fit per row with scrollbar margin
        let available_w = (ui.available_width() - 16.0).max(100.0);
        let card_w = 156.0_f32;
        let gap    = 16.0_f32;
        let cols   = ((available_w + gap) / (card_w + gap)).floor().max(1.0) as usize;

        let row_count = (valid_tracks.len() + cols - 1) / cols;
        let card_h = (card_w - 20.0) + 64.0;

        for row in 0..row_count {
            let start = row * cols;
            let end   = (start + cols).min(valid_tracks.len());
            let row_tracks = &valid_tracks[start..end];

            // Allocate space for the row
            let (rect, _) = ui.allocate_exact_size(Vec2::new(available_w, card_h), Sense::hover());
            
            // Only draw if the row is actually visible in the scroll area
            if ui.is_rect_visible(rect) {
                ui.allocate_ui_at_rect(rect, |ui| {
                    ui.horizontal(|ui| {
                        for track in row_tracks {
                            self.draw_card(ui, track, card_w, max_dim);
                            ui.add_space(gap);
                        }
                    });
                });
            }
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

        // Removed Animated Hover Green Play Button Overlay

        if response.clicked() {
            self.viewmodel.playback.play_now(track.clone());
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
            let it  = Arc::clone(&self.viewmodel.innertube);
            self.show_suggest = true;
            self.viewmodel.runtime.spawn(async move {
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

        let is_searching = self.viewmodel.is_searching.load(std::sync::atomic::Ordering::SeqCst);
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

        if self.cached_search.is_empty() && !self.last_search.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No results found").color(T_SEC)
                    .font(FontId::proportional(14.0)));
            });
            return;
        }

        if self.cached_search.is_empty() {
            ui.add_space(12.0);
            ui.label(RichText::new("Browse categories").color(T_PRI)
                .font(FontId::proportional(16.0)).strong());
            ui.add_space(10.0);
            self.genre_grid(ui);
            return;
        }

        let results = std::mem::take(&mut self.cached_search);
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
        
        self.cached_search = results;
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
        let cur = self.viewmodel.playback.current_track.try_lock().ok().and_then(|g| g.clone());
        // Read max_dim ONCE per frame, not once per track — avoids locking settings mutex N times per frame
        let max_dim = self.cached_max_dim;
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

            if resp.clicked() { self.viewmodel.playback.play_now(track.clone()); }
        }
    }

    // ── Library ───────────────────────────────────────────────────────────────

    fn show_library(&mut self, ui: &mut egui::Ui) {
        let liked = self.viewmodel.liked_songs_snapshot.try_read()
            .map(|g| g.clone()).unwrap_or_default();
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

        // Read playback state (try_lock: never block the UI thread)
        let pos   = self.viewmodel.playback.progress_secs.try_lock().map(|g| *g).unwrap_or(0.0);
        let dur_raw = self.viewmodel.playback.duration_secs.try_lock().map(|g| *g).unwrap_or(1.0);
        let dur   = dur_raw.max(1.0);
        let ratio = (pos / dur).clamp(0.0, 1.0);
        let track = self.viewmodel.playback.current_track.try_lock().ok().and_then(|g| g.clone());
        let state = self.viewmodel.playback.state.try_lock().map(|g| g.clone()).unwrap_or(PlaybackState::Idle);
        // Read max_dim once here (not inside the art rendering block) to avoid locking settings inside a closure
        let max_dim = self.cached_max_dim;

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
                self.viewmodel.playback.seek_to(frac * dur);
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
                    let is_liked  = self.viewmodel.playback.is_liked(&t.media_id);
                    let heart_ico = if is_liked { "♥" } else { "♡" };
                    let heart_col = if is_liked { Color32::from_rgb(255, 45, 85) } else { T_SEC };
                    if ui.add(egui::Button::new(
                        RichText::new(heart_ico).color(heart_col).font(FontId::proportional(16.0)))
                        .frame(false))
                        .on_hover_text("Save to Library")
                        .clicked()
                    {
                        self.viewmodel.playback.toggle_like(t.clone());
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
        let is_shuf = self.viewmodel.playback.is_shuffle.try_lock().map(|g| *g).unwrap_or(false);
        let shuf_r  = egui::Rect::from_center_size(egui::pos2(cx - 92.0, cy), Vec2::splat(32.0));
        let shuf_btn = egui::Button::new(
            RichText::new("🔀").color(if is_shuf { ACCENT } else { T_DIM })
                .font(FontId::proportional(15.0))).frame(false);
        if ui.put(shuf_r, shuf_btn).on_hover_text("Shuffle").clicked() {
            self.viewmodel.playback.toggle_shuffle();
        }

        // Prev
        let prev_r = egui::Rect::from_center_size(egui::pos2(cx - 46.0, cy), Vec2::splat(32.0));
        if ui.put(prev_r, ctrl_btn("⏮")).on_hover_text("Previous").clicked() {
            self.viewmodel.playback.skip_prev();
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
            self.viewmodel.playback.toggle_pause();
        }

        // Next
        let next_r = egui::Rect::from_center_size(egui::pos2(cx + 46.0, cy), Vec2::splat(32.0));
        if ui.put(next_r, ctrl_btn("⏭")).on_hover_text("Next").clicked() {
            self.viewmodel.playback.skip_next();
        }

        // Repeat
        let rep_mode = self.viewmodel.playback.repeat_mode.try_lock().map(|g| *g).unwrap_or(meduza_playback::playback_manager::RepeatMode::Off);
        let (rep_icon, rep_color) = match rep_mode {
            meduza_playback::playback_manager::RepeatMode::Off => ("🔁", T_DIM),
            meduza_playback::playback_manager::RepeatMode::All => ("🔁", ACCENT),
            meduza_playback::playback_manager::RepeatMode::One => ("🔂", ACCENT),
        };
        let rep_r = egui::Rect::from_center_size(egui::pos2(cx + 92.0, cy), Vec2::splat(32.0));
        let rep_btn = egui::Button::new(
            RichText::new(rep_icon).color(rep_color).font(FontId::proportional(15.0))).frame(false);
        if ui.put(rep_r, rep_btn).on_hover_text("Repeat").clicked() {
            self.viewmodel.playback.toggle_repeat();
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

                let mut vol = self.viewmodel.playback.volume.try_lock().map(|g| *g).unwrap_or(80.0);

                if vol_sense.clicked() || vol_sense.dragged() {
                    if let Some(ptr) = vol_sense.interact_pointer_pos() {
                        let frac = ((ptr.x - vol_line.min.x) / vol_w).clamp(0.0, 1.0);
                        vol = frac * 100.0;
                        self.viewmodel.playback.set_volume(vol);
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
                        self.viewmodel.playback.set_volume(0.0);
                    } else {
                        self.viewmodel.playback.set_volume(80.0);
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

                let mut settings = self.viewmodel.playback.settings.lock().unwrap().clone();
                let mut changed  = false;

                // ── 1. Audio Quality & Bitrate ──────────────────────────────
                ui.label(RichText::new("Audio Streaming Quality").color(T_PRI).font(FontId::proportional(16.0)).strong());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.selectable_value(&mut settings.audio_quality, meduza_core::settings::AudioQuality::DataSaver, "Data Saver (~1 MB/song)").clicked() { changed = true; }
                    if ui.selectable_value(&mut settings.audio_quality, meduza_core::settings::AudioQuality::Normal, "Normal (~3 MB/song)").clicked() { changed = true; }
                    if ui.selectable_value(&mut settings.audio_quality, meduza_core::settings::AudioQuality::High, "High Quality (~5 MB/song)").clicked() { changed = true; }
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

                let cache_size_mb = self.viewmodel.playback.data_saver.lock().unwrap().get_cache_size_mb();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Current Audio Cache: {:.1} MB", cache_size_mb)).color(T_SEC).font(FontId::proportional(14.0)));
                    ui.add_space(16.0);
                    if ui.add(egui::Button::new(
                        RichText::new("Clear Audio Cache").color(Color32::from_rgb(255, 100, 100)).font(FontId::proportional(13.0))
                    ).fill(Color32::from_rgb(45, 20, 20)).rounding(Rounding::same(8.0))).clicked() {
                        self.viewmodel.playback.data_saver.lock().unwrap().clear_cache();
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
                    *self.viewmodel.playback.settings.lock().unwrap() = settings;
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
        let track   = self.viewmodel.playback.current_track.try_lock().ok().and_then(|g| g.clone());
        let state   = self.viewmodel.playback.state.try_lock().map(|g| g.clone()).unwrap_or(PlaybackState::Idle);
        let pos     = self.viewmodel.playback.progress_secs.try_lock().map(|g| *g).unwrap_or(0.0);
        let dur_raw = self.viewmodel.playback.duration_secs.try_lock().map(|g| *g).unwrap_or(1.0);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(18, 18, 20)))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let cx   = rect.center().x;
                let w    = rect.width();
                let h    = rect.height();

                // ── 1. Static Background ────────────────────────────────────────────────
                ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(18, 18, 20));

                let tex_opt = if let Some(ref t) = track {
                    let p = Arc::clone(&self.img_pending);
                    let max_dim = self.cached_max_dim;
                    Self::get_texture(&mut self.img_cache, &p, &self.image_rgba, ui.ctx(), &t.media_id, &t.thumbnail_url, max_dim)
                } else { None };

                // ── 2. Ambient Background (Zero CPU Cost) ──────────────────────────────
                if let Some(tex) = tex_opt {
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 30),
                    );
                    ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(18, 18, 20, 210));
                }

                // ── 3. Top Header (Collapse button + label) ───────────────────────
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
                    egui::pos2(cx, rect.min.y + 34.0),
                    egui::Align2::CENTER_CENTER,
                    "PLAYING FROM QUEUE",
                    FontId::proportional(11.0),
                    T_DIM,
                );

                // ── 4. Main Stage (Massive Dynamic Album Art) ──────────────────────
                let transport_y = rect.max.y - (h * 0.22).clamp(160.0, 300.0);
                let img_center_y = rect.min.y + 50.0 + (transport_y - (rect.min.y + 50.0)) / 2.0;
                
                let max_img_h = transport_y - rect.min.y - 120.0;
                let max_img_w = w * 0.6;
                let img_size = max_img_h.min(max_img_w).min(700.0).max(150.0);

                let img_rect = egui::Rect::from_center_size(egui::pos2(cx, img_center_y), Vec2::splat(img_size));
                
                // Drop shadow
                ui.painter().rect_filled(
                    img_rect.translate(egui::vec2(0.0, 10.0)),
                    Rounding::same(8.0),
                    Color32::from_rgba_unmultiplied(0, 0, 0, 80),
                );

                if let Some(tex) = tex_opt {
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    // Add a subtle border
                    ui.painter().rect_stroke(img_rect, Rounding::ZERO, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255,255,255,15)));
                } else {
                    ui.painter().rect_filled(img_rect, 12.0, Color32::from_rgb(30, 30, 34));
                    ui.painter().text(
                        img_rect.center(), egui::Align2::CENTER_CENTER, "♪", FontId::proportional(img_size * 0.2), T_DIM
                    );
                }

                // ── 4. Bottom Transport Hub (Spotify Style) ────────────────────────
                
                // --- Progress Bar (Spans 60% of screen to add padding) ---
                let progress_w = (w * 0.60).clamp(400.0, 1000.0);
                let slider_y   = transport_y;
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
                        self.viewmodel.playback.seek_to(frac * dur_raw);
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
                    ui.painter().rect_filled(fill_rect, Rounding::same(3.0), Color32::WHITE); // Spotify uses white/green

                    if slider_sense.hovered() || slider_sense.dragged() {
                        let handle_pos = egui::pos2(line_rect.min.x + fill_w, slider_y);
                        ui.painter().circle_filled(handle_pos, 6.0, Color32::WHITE);
                        // Make progress bar green on hover (Spotify behavior)
                        ui.painter().rect_filled(fill_rect, Rounding::same(3.0), ACCENT);
                    }
                }

                // Time labels (Left and Right aligned to progress bar edges)
                ui.painter().text(
                    egui::pos2(cx - progress_w / 2.0, slider_y + 16.0),
                    egui::Align2::LEFT_TOP,
                    fmt_time(pos as u32),
                    FontId::proportional(12.0),
                    T_DIM,
                );
                ui.painter().text(
                    egui::pos2(cx + progress_w / 2.0, slider_y + 16.0),
                    egui::Align2::RIGHT_TOP,
                    fmt_time(dur_raw as u32),
                    FontId::proportional(12.0),
                    T_DIM,
                );

                // --- Transport Row (Under progress bar) ---
                let ctrl_y = transport_y + 50.0;
                let meta_start_x = cx - progress_w / 2.0;
                let right_end_x = cx + progress_w / 2.0;

                // [Left] Metadata: Title, Artist, Heart
                let meta_rect = egui::Rect::from_min_max(
                    egui::pos2(meta_start_x, ctrl_y - 25.0),
                    egui::pos2(cx - 180.0, ctrl_y + 25.0) 
                );
                ui.allocate_ui_at_rect(meta_rect, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            if let Some(ref t) = track {
                                ui.label(RichText::new(trunc(&t.title, 45)).font(FontId::proportional(22.0)).color(T_PRI).strong());
                                ui.add_space(-2.0);
                                ui.label(RichText::new(trunc(&t.artist, 45)).font(FontId::proportional(14.0)).color(T_SEC));
                            } else {
                                ui.label(RichText::new("Nothing playing").font(FontId::proportional(22.0)).color(T_DIM));
                            }
                        });
                        ui.add_space(16.0);
                        if let Some(ref t) = track {
                            let is_liked = self.viewmodel.playback.is_liked(&t.media_id);
                            let heart_ico = if is_liked { "♥" } else { "♡" };
                            let heart_col = if is_liked { Color32::from_rgb(255, 45, 85) } else { T_SEC };
                            if ui.add(egui::Button::new(RichText::new(heart_ico).color(heart_col).font(FontId::proportional(22.0))).frame(false)).clicked() {
                                self.viewmodel.playback.toggle_like(t.clone());
                            }
                        }
                    });
                });

                // [Center] Playback Controls
                let spacing = 65.0;

                // Shuffle
                let is_shuf = self.viewmodel.playback.is_shuffle.try_lock().map(|g| *g).unwrap_or(false);
                let shuf_r  = egui::Rect::from_center_size(egui::pos2(cx - spacing*2.0, ctrl_y), Vec2::splat(40.0));
                let shuf_btn = egui::Button::new(
                    RichText::new("🔀").color(if is_shuf { ACCENT } else { T_DIM })
                        .font(FontId::proportional(20.0))).frame(false);
                if ui.put(shuf_r, shuf_btn).on_hover_text("Shuffle").clicked() {
                    self.viewmodel.playback.toggle_shuffle();
                }

                // Prev
                let prev_r = egui::Rect::from_center_size(egui::pos2(cx - spacing, ctrl_y), Vec2::splat(40.0));
                let prev_btn = egui::Button::new(
                    RichText::new("⏮").color(T_PRI).font(FontId::proportional(28.0))).frame(false);
                if ui.put(prev_r, prev_btn).on_hover_text("Previous").clicked() {
                    self.viewmodel.playback.skip_prev();
                }

                // Play/Pause Big 64px Button
                let play_r = egui::Rect::from_center_size(egui::pos2(cx, ctrl_y), Vec2::splat(60.0));
                let (icon, hint) = match state {
                    PlaybackState::Playing => ("⏸", "Pause"),
                    PlaybackState::Paused  => ("▶",  "Play"),
                    PlaybackState::Loading => ("⏳", "Loading…"),
                    _                      => ("▶",  "Play"),
                };
                let pp = egui::Button::new(
                    RichText::new(icon).color(Color32::BLACK).font(FontId::proportional(26.0))
                ).fill(Color32::WHITE).min_size(Vec2::splat(60.0)).rounding(Rounding::same(30.0));
                if ui.put(play_r, pp).on_hover_text(hint).clicked() {
                    self.viewmodel.playback.toggle_pause();
                }

                // Next
                let next_r = egui::Rect::from_center_size(egui::pos2(cx + spacing, ctrl_y), Vec2::splat(40.0));
                let next_btn = egui::Button::new(
                    RichText::new("⏭").color(T_PRI).font(FontId::proportional(28.0))).frame(false);
                if ui.put(next_r, next_btn).on_hover_text("Next").clicked() {
                    self.viewmodel.playback.skip_next();
                }

                // Repeat
                let rep_mode = self.viewmodel.playback.repeat_mode.try_lock().map(|g| *g).unwrap_or(meduza_playback::playback_manager::RepeatMode::Off);
                let (rep_icon, rep_color) = match rep_mode {
                    meduza_playback::playback_manager::RepeatMode::Off => ("🔁", T_DIM),
                    meduza_playback::playback_manager::RepeatMode::All => ("🔁", ACCENT),
                    meduza_playback::playback_manager::RepeatMode::One => ("🔂", ACCENT),
                };
                let rep_r = egui::Rect::from_center_size(egui::pos2(cx + spacing*2.0, ctrl_y), Vec2::splat(40.0));
                let rep_btn = egui::Button::new(
                    RichText::new(rep_icon).color(rep_color).font(FontId::proportional(20.0))).frame(false);
                if ui.put(rep_r, rep_btn).on_hover_text("Repeat").clicked() {
                    self.viewmodel.playback.toggle_repeat();
                }

                // [Right] Volume Control
                let vol_w = 120.0_f32;
                let vol_rect = egui::Rect::from_min_max(
                    egui::pos2(right_end_x - vol_w - 40.0, ctrl_y - 20.0),
                    egui::pos2(right_end_x, ctrl_y + 20.0) 
                );
                
                ui.allocate_ui_at_rect(vol_rect, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        
                        let mut vol = self.viewmodel.playback.volume.try_lock().map(|g| *g).unwrap_or(80.0);
                        
                        // Volume Bar
                        let (vol_rect, vol_sense) = ui.allocate_exact_size(Vec2::new(vol_w, 20.0), Sense::click_and_drag());
                        let vol_line = egui::Rect::from_center_size(vol_rect.center(), Vec2::new(vol_w, 5.0));

                        if vol_sense.clicked() || vol_sense.dragged() {
                            if let Some(ptr) = vol_sense.interact_pointer_pos() {
                                let frac = ((ptr.x - vol_line.min.x) / vol_w).clamp(0.0, 1.0);
                                vol = frac * 100.0;
                                self.viewmodel.playback.set_volume(vol);
                            }
                        }

                        let track_col = if vol_sense.hovered() { Color32::from_rgb(75, 75, 80) } else { Color32::from_rgb(45, 45, 50) };
                        ui.painter().rect_filled(vol_line, Rounding::same(2.5), track_col);

                        let fill_w = vol_w * (vol / 100.0).clamp(0.0, 1.0);
                        if fill_w > 0.0 {
                            let fill_rect = egui::Rect::from_min_size(vol_line.min, Vec2::new(fill_w, 5.0));
                            let fill_col = if vol_sense.hovered() || vol_sense.dragged() { ACCENT } else { Color32::WHITE };
                            ui.painter().rect_filled(fill_rect, Rounding::same(2.5), fill_col);

                            if vol_sense.hovered() || vol_sense.dragged() {
                                let handle_pos = egui::pos2(vol_line.min.x + fill_w, vol_rect.center().y);
                                ui.painter().circle_filled(handle_pos, 5.0, Color32::WHITE);
                            }
                        }

                        // Mute button
                        let vol_icon = if vol == 0.0 { "🔇" }
                            else if vol < 40.0 { "🔈" }
                            else if vol < 70.0 { "🔉" }
                            else               { "🔊" };
                        
                        ui.add_space(8.0);
                        if ui.add(egui::Button::new(RichText::new(vol_icon).color(T_SEC).font(FontId::proportional(18.0))).frame(false)).clicked() {
                            if vol > 0.0 {
                                self.viewmodel.playback.set_volume(0.0);
                            } else {
                                self.viewmodel.playback.set_volume(80.0);
                            }
                        }
                    });
                });
            });
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────


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



// ── eframe::App ───────────────────────────────────────────────────────────────

impl eframe::App for MeduzaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // Handle auto advance
        self.viewmodel.playback.handle_auto_advance();

        // ── Refresh cached snapshots only when background data actually changed ──
        let cur_ver = self.viewmodel.data_version.load(std::sync::atomic::Ordering::SeqCst);
        if cur_ver != self.last_data_version {
            self.last_data_version = cur_ver;

            // Clone feed data — this only happens when data_version bumps (rare),
            // NOT every frame (which was the root cause of the freeze).
            if let Ok(feed) = self.viewmodel.home_feed.try_read() {
                self.cached_sections = feed.clone();
            }
            if let Ok(sr) = self.viewmodel.search_results.try_read() {
                self.cached_search = sr.clone();
            }
            if let Ok(s) = self.viewmodel.playback.settings.try_lock() {
                self.cached_max_dim = s.image_max_dim();
            }
        }

        // ── Repaint throttling (never pegs the CPU) ─────────────────────────
        let st = self.viewmodel.playback.state.try_lock()
            .map(|g| g.clone())
            .unwrap_or(PlaybackState::Idle);
        if matches!(st, PlaybackState::Playing) {
            ctx.request_repaint_after(std::time::Duration::from_millis(1000));
        } else if matches!(st, PlaybackState::Loading)
            || self.viewmodel.is_loading_home.load(std::sync::atomic::Ordering::SeqCst)
            || self.viewmodel.is_searching.load(std::sync::atomic::Ordering::SeqCst)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        if self.show_now_playing {
            self.show_now_playing_screen(ctx);
            return;
        }

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
