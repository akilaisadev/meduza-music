use eframe::egui::{self, Color32, FontId, RichText, Sense, Stroke, Vec2, Rounding};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::innertube::{BrowseSection, InnerTubeClient, TrackItem};
use crate::playback_manager::{PlaybackManager, PlaybackState};

// ── Palette ──────────────────────────────────────────────────────────────────
const BG:         Color32 = Color32::from_rgb(10, 10, 10);
const BG_SIDE:    Color32 = Color32::from_rgb(16, 16, 16);
const BG_TOP:     Color32 = Color32::from_rgb(14, 14, 14);
const BG_CARD:    Color32 = Color32::from_rgb(22, 22, 22);
const BG_CARD_HV: Color32 = Color32::from_rgb(32, 32, 32);
const ACCENT:     Color32 = Color32::from_rgb(29, 185, 84);
const ACCENT_DIM: Color32 = Color32::from_rgb(20, 140, 60);

const T_PRI:      Color32 = Color32::from_rgb(255, 255, 255);
const T_SEC:      Color32 = Color32::from_rgb(170, 170, 170);
const T_DIM:      Color32 = Color32::from_rgb(90, 90, 90);
const TRACK_BG:   Color32 = Color32::from_rgb(40, 40, 40);
const PLAYER_BG:  Color32 = Color32::from_rgb(18, 18, 20);

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

    // Search
    search_query:   String,
    last_search:    String,
    search_results: Arc<Mutex<Vec<TrackItem>>>,
    is_searching:   Arc<Mutex<bool>>,
    suggestions:    Arc<Mutex<Vec<String>>>,
    show_suggest:   bool,

    // Images
    img_cache:    HashMap<String, egui::TextureHandle>,
    img_pending:  Arc<Mutex<HashMap<String, Option<Vec<u8>>>>>,
    logo_texture: Option<egui::TextureHandle>,
    show_now_playing: bool,
    disc_angle: f32,
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
        style.spacing.scroll.bar_width = 8.0;
        cc.egui_ctx.set_style(style);

        let innertube = Arc::new(InnerTubeClient::new());
        let playback  = Arc::new(PlaybackManager::new(Arc::clone(&innertube)));

        let sections = Arc::new(Mutex::new(Vec::<BrowseSection>::new()));
        let loading  = Arc::new(Mutex::new(true));

        {
            let s = Arc::clone(&sections);
            let l = Arc::clone(&loading);
            let it = Arc::clone(&innertube);
            runtime.spawn(async move {
                let feed = it.fetch_home_feed().await;
                *s.lock().unwrap() = feed;
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

        Self {
            innertube, playback, runtime,
            tab: Tab::Home,
            sections, is_loading_home: loading,
            search_query: String::new(),
            last_search:  String::new(),
            search_results: Arc::new(Mutex::new(Vec::new())),
            is_searching:   Arc::new(Mutex::new(false)),
            suggestions:    Arc::new(Mutex::new(Vec::new())),
            show_suggest:   false,
            img_cache:      HashMap::new(),
            img_pending:    Arc::new(Mutex::new(HashMap::new())),
            logo_texture,
            show_now_playing: false,
            disc_angle: 0.0,
        }
    }

    // ── Image loading ─────────────────────────────────────────────────────────

    fn queue_image(id: &str, url: &str, pending: &Arc<Mutex<HashMap<String, Option<Vec<u8>>>>>) {
        let mut p = pending.lock().unwrap();
        if p.contains_key(id) { return; }
        p.insert(id.to_string(), None);
        let id2  = id.to_string();
        let url2 = url.to_string();
        let p2   = Arc::clone(pending);
        thread::spawn(move || {
            if let Ok(resp) = ureq::get(&url2).call() {
                let mut buf = Vec::new();
                use std::io::Read;
                let _ = resp.into_reader().take(5 * 1024 * 1024).read_to_end(&mut buf);
                if !buf.is_empty() {
                    p2.lock().unwrap().insert(id2, Some(buf));
                }
            }
        });
    }

    fn get_texture<'a>(
        cache:   &'a mut HashMap<String, egui::TextureHandle>,
        pending: &Arc<Mutex<HashMap<String, Option<Vec<u8>>>>>,
        ctx:     &egui::Context,
        id:      &str,
        url:     &str,
    ) -> Option<&'a egui::TextureHandle> {
        if cache.contains_key(id) { return cache.get(id); }
        let bytes = pending.lock().unwrap().get(id).and_then(|v| v.clone());
        if let Some(data) = bytes {
            if let Ok(img) = image::load_from_memory(&data) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                let handle = ctx.load_texture(id, ci, egui::TextureOptions::LINEAR);
                cache.insert(id.to_string(), handle);
                return cache.get(id);
            }
        } else {
            Self::queue_image(id, url, pending);
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
                    let title = match self.tab {
                        Tab::Home    => "Good Evening 🎵",
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
        let it = Arc::clone(&self.innertube);
        *searching.lock().unwrap() = true;
        *results.lock().unwrap()   = Vec::new();
        self.runtime.spawn(async move {
            let res = it.search_tracks(&q).await;
            *results.lock().unwrap()   = res;
            *searching.lock().unwrap() = false;
        });
    }

    // ── Home ──────────────────────────────────────────────────────────────────

    fn show_home(&mut self, ui: &mut egui::Ui) {
        let loading  = *self.is_loading_home.lock().unwrap();
        let sections = self.sections.lock().unwrap().clone();
        let mut seen = std::collections::HashSet::<String>::new();

        let heavy_rot_raw = self.playback.recommendation_engine.lock().unwrap().get_heavy_rotation(8);
        let heavy_rot     = crate::recommendation_engine::RecommendationEngine::filter_unique(&heavy_rot_raw, &mut seen);

        let history_raw   = self.playback.history.lock().unwrap().clone();
        let history       = crate::recommendation_engine::RecommendationEngine::filter_unique(&history_raw, &mut seen);

        let liked_raw     = self.playback.liked_songs.lock().unwrap().clone();
        let liked         = crate::recommendation_engine::RecommendationEngine::filter_unique(&liked_raw, &mut seen);

        let top_artist    = self.playback.recommendation_engine.lock().unwrap().get_top_artist();

        if loading && sections.is_empty() && history.is_empty() && heavy_rot.is_empty() {
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
                    self.runtime.spawn(async move {
                        let feed = it.fetch_home_feed().await;
                        *s.lock().unwrap() = feed;
                        *l.lock().unwrap() = false;
                    });
                }
            });
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .id_source("home")
            .show(ui, |ui| {
                ui.add_space(8.0);

                // 1. Recommendation Engine: Your Heavy Rotation
                if !heavy_rot.is_empty() {
                    ui.label(
                        RichText::new("🔥 Your Heavy Rotation ⚡")
                            .color(T_PRI)
                            .font(FontId::proportional(19.0))
                            .strong(),
                    );
                    ui.add_space(10.0);
                    self.card_grid(ui, &heavy_rot);
                    ui.add_space(26.0);
                }

                // 2. Dynamic Taste Section: Recently Played (Deduplicated)
                if !history.is_empty() {
                    ui.label(
                        RichText::new("Recently Played 🎧")
                            .color(T_PRI)
                            .font(FontId::proportional(19.0))
                            .strong(),
                    );
                    ui.add_space(10.0);
                    self.card_grid(ui, &history);
                    ui.add_space(26.0);
                }

                // 3. Dynamic Taste Section: Top Artist Mix
                if let Some(ref artist) = top_artist {
                    ui.label(
                        RichText::new(format!("💡 More Like {} ✨", artist))
                            .color(T_PRI)
                            .font(FontId::proportional(19.0))
                            .strong(),
                    );
                    ui.add_space(10.0);
                }

                // 4. Dynamic Taste Section: Rediscover Favorites
                if !liked.is_empty() {
                    ui.label(
                        RichText::new("Rediscover Your Favorites ❤️")
                            .color(T_PRI)
                            .font(FontId::proportional(19.0))
                            .strong(),
                    );
                    ui.add_space(10.0);
                    self.card_grid(ui, &liked);
                    ui.add_space(26.0);
                }

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
                        self.card_grid(ui, &unique_items);
                        ui.add_space(26.0);
                    }
                }
            });
    }

    fn retry_btn(&self, ui: &mut egui::Ui) -> bool {
        ui.add(egui::Button::new(
            RichText::new("  Retry  ").color(Color32::BLACK).font(FontId::proportional(13.0))
        ).fill(ACCENT).rounding(Rounding::same(20.0))).clicked()
    }

    fn card_grid(&mut self, ui: &mut egui::Ui, tracks: &[TrackItem]) {
        // Dynamically compute how many cards fit per row with scrollbar margin
        let available_w = (ui.available_width() - 16.0).max(100.0);
        let card_w = 156.0_f32;
        let gap    = 16.0_f32;
        let cols   = ((available_w + gap) / (card_w + gap)).floor().max(1.0) as usize;

        let tracks: Vec<TrackItem> = tracks.to_vec();
        let row_count = (tracks.len() + cols - 1) / cols;

        for row in 0..row_count {
            let start = row * cols;
            let end   = (start + cols).min(tracks.len());
            let row_tracks = &tracks[start..end];

            ui.horizontal(|ui| {
                for track in row_tracks {
                    self.draw_card(ui, track, card_w);
                    ui.add_space(gap);
                }
            });
            ui.add_space(gap);
        }
    }

    fn draw_card(&mut self, ui: &mut egui::Ui, track: &TrackItem, card_w: f32) {
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
            &mut self.img_cache, &pending, ui.ctx(),
            &track.media_id, &track.thumbnail_url,
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

        let results = self.search_results.lock().unwrap().clone();
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
            .id_source("search")
            .show(ui, |ui| {
                ui.label(RichText::new(format!("{} results for \"{}\"",
                    results.len(), self.last_search))
                    .color(T_DIM).font(FontId::proportional(12.0)));
                ui.add_space(6.0);
                self.track_list(ui, &results.clone());
            });
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
            if let Some(tex) = Self::get_texture(&mut self.img_cache, &pending, ui.ctx(),
                &track.media_id, &track.thumbnail_url)
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
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
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
        ui.painter().rect_filled(full_rect, Rounding::ZERO, PLAYER_BG);

        // Read playback state
        let pos   = *self.playback.progress_secs.lock().unwrap();
        let dur_raw = *self.playback.duration_secs.lock().unwrap();
        let dur   = dur_raw.max(1.0);
        let ratio = (pos / dur).clamp(0.0, 1.0);
        let track = self.playback.current_track.lock().unwrap().clone();
        let state = self.playback.state.lock().unwrap().clone();

        // ── 1. Interactive Green Progress Bar at top edge of player ───────────
        let w = full_rect.width();
        let bar_h = 5.0_f32;
        let bar_rect = egui::Rect::from_min_size(full_rect.min, Vec2::new(w, bar_h));
        let bar_sense = ui.allocate_rect(bar_rect, Sense::click_and_drag());
        if bar_sense.clicked() || bar_sense.dragged() {
            if let Some(ptr) = bar_sense.interact_pointer_pos() {
                let frac = ((ptr.x - bar_rect.min.x) / w).clamp(0.0, 1.0);
                self.playback.seek_to(frac * dur);
            }
        }
        
        let bg_col = if bar_sense.hovered() { Color32::from_rgb(60, 60, 60) } else { TRACK_BG };
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
        let content_top = full_rect.min.y + bar_h;
        let content_h   = full_rect.height() - bar_h;
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
                    Self::get_texture(&mut self.img_cache, &p, ui.ctx(), &t.media_id, &t.thumbnail_url)
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

        // ── Center: transport controls (vertically centered) ─────────────────
        let cx = center_rect.center().x;
        let cy = center_rect.center().y;

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
        let play_r = egui::Rect::from_center_size(egui::pos2(cx, cy), Vec2::splat(40.0));
        let (icon, hint) = match state {
            PlaybackState::Playing => ("⏸", "Pause"),
            PlaybackState::Paused  => ("▶",  "Play"),
            PlaybackState::Loading => ("⏳", "Loading…"),
            _                      => ("▶",  "Play"),
        };
        let pp = egui::Button::new(
            RichText::new(icon).color(Color32::BLACK).font(FontId::proportional(18.0)))
            .fill(ACCENT).min_size(Vec2::splat(40.0)).rounding(Rounding::same(20.0));
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

        // ── Right: Volume + Time display aligned in right corner ──────────────
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

                ui.add_space(16.0);

                let time_str = format!("{} / {}", fmt_time(pos as u32), fmt_time(dur_raw as u32));
                ui.label(RichText::new(time_str).color(T_SEC).font(FontId::proportional(12.0)));
            });
        });
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.add_space(20.0);
        ui.label(RichText::new("Data Saver & Quality").color(T_PRI).font(FontId::proportional(22.0)).strong());
        ui.add_space(10.0);
        
        let mut quality = self.playback.settings.lock().unwrap().audio_quality;
        let mut changed = false;

        ui.horizontal(|ui| {
            if ui.selectable_value(&mut quality, crate::settings::AudioQuality::DataSaver, "Data Saver (Low)").clicked() { changed = true; }
            if ui.selectable_value(&mut quality, crate::settings::AudioQuality::Normal, "Normal (Medium)").clicked() { changed = true; }
            if ui.selectable_value(&mut quality, crate::settings::AudioQuality::High, "High Quality").clicked() { changed = true; }
        });

        ui.add_space(10.0);
        ui.label(RichText::new("Data Saver uses lower bitrate audio (approx 64-96kbps) to save bandwidth.")
            .color(T_SEC).font(FontId::proportional(14.0)));

        if changed {
            let mut s = self.playback.settings.lock().unwrap();
            s.audio_quality = quality;
            s.save();
        }

        ui.add_space(35.0);
        ui.separator();
        ui.add_space(25.0);

        ui.label(
            RichText::new("About & Developer")
                .color(T_PRI)
                .font(FontId::proportional(20.0))
                .strong(),
        );
        ui.add_space(12.0);

        // Premium Full-Width Developer Showcase Card
        egui::Frame::none()
            .fill(Color32::from_rgb(22, 22, 26))
            .rounding(Rounding::same(16.0))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(45, 45, 52)))
            .inner_margin(egui::Margin::same(24.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Developer Icon Badge
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
                    ui.painter().circle_filled(
                        rect.center(),
                        26.0,
                        Color32::from_rgb(30, 215, 96), // ACCENT Green badge
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "🎧",
                        FontId::proportional(26.0),
                        Color32::BLACK,
                    );

                    ui.add_space(16.0);

                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Meduza Music Player")
                                    .color(T_PRI)
                                    .font(FontId::proportional(18.0))
                                    .strong(),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("v0.2.0")
                                    .color(T_DIM)
                                    .font(FontId::proportional(13.0)),
                            );
                        });

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Crafted with")
                                    .color(T_SEC)
                                    .font(FontId::proportional(14.0)),
                            );
                            ui.label(
                                RichText::new("♥")
                                    .color(Color32::from_rgb(255, 45, 85))
                                    .font(FontId::proportional(16.0))
                                    .strong(),
                            );
                            ui.label(
                                RichText::new("by")
                                    .color(T_SEC)
                                    .font(FontId::proportional(14.0)),
                            );
                            ui.hyperlink_to(
                                RichText::new("@akilaisadev")
                                    .color(ACCENT)
                                    .font(FontId::proportional(14.0))
                                    .strong(),
                                "https://github.com/akilaisadev",
                            )
                            .on_hover_text("Open https://github.com/akilaisadev");
                        });

                        ui.add_space(14.0);

                        // Hover Action Link Cards / Buttons
                        ui.horizontal(|ui| {
                            let gh_btn = egui::Button::new(
                                RichText::new("🐙 GitHub Profile")
                                    .color(T_PRI)
                                    .font(FontId::proportional(13.0)),
                            )
                            .fill(Color32::from_rgb(34, 34, 40))
                            .rounding(Rounding::same(8.0))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(55, 55, 62)));

                            if ui.add(gh_btn).on_hover_text("Open https://github.com/akilaisadev").clicked() {
                                ui.ctx().open_url(egui::OpenUrl::same_tab("https://github.com/akilaisadev"));
                            }

                            ui.add_space(10.0);

                            let repo_btn = egui::Button::new(
                                RichText::new("⭐ Star Project")
                                    .color(T_PRI)
                                    .font(FontId::proportional(13.0)),
                            )
                            .fill(Color32::from_rgb(34, 34, 40))
                            .rounding(Rounding::same(8.0))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(55, 55, 62)));

                            if ui.add(repo_btn).on_hover_text("Open https://github.com/akilaisadev").clicked() {
                                ui.ctx().open_url(egui::OpenUrl::same_tab("https://github.com/akilaisadev"));
                            }
                        });
                    });
                });
            });
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

                // ── 1. Top Header ─────────────────────────────────────────────
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
                    Self::get_texture(&mut self.img_cache, &p, ui.ctx(), &t.media_id, &t.thumbnail_url)
                } else { None };

                draw_vinyl_record(ui, vinyl_center, radius, tex_opt, self.disc_angle);

                // ── 3. Title & Artist (Centered) ──────────────────────────────
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
    // 1. Black Outer Vinyl Disc Body
    ui.painter().circle_filled(center, radius, Color32::from_rgb(14, 14, 16));
    ui.painter().circle_stroke(center, radius, Stroke::new(2.5_f32, Color32::from_rgb(38, 38, 42)));

    // 2. Outer Vinyl Record Grooves
    let label_r = radius * 0.83;
    let groove_min = label_r + 3.0;
    let groove_max = radius * 0.95;
    let num_grooves = 3;
    let step = (groove_max - groove_min) / (num_grooves as f32);
    for i in 0..num_grooves {
        let r = groove_min + (i as f32) * step;
        ui.painter().circle_stroke(
            center,
            r,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 14)),
        );
    }

    // 3. Rotating Light Sheen Reflections
    for &a in &[angle, angle + std::f32::consts::PI] {
        let p1 = center + egui::vec2(a.cos(), a.sin()) * (label_r + 2.0);
        let p2 = center + egui::vec2(a.cos(), a.sin()) * (radius * 0.96);
        ui.painter().line_segment(
            [p1, p2],
            Stroke::new(6.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 16)),
        );
    }

    // 4. CIRCULAR ALBUM ARTWORK VINYL RECORD LABEL (WRAPPED SMOOTHLY ONTO CENTER DISC)
    if let Some(texture) = tex {
        let mut mesh = egui::Mesh::with_texture(texture.id());
        
        // Center vertex
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center,
            uv: egui::pos2(0.5, 0.5),
            color: Color32::WHITE,
        });

        // 64 segment circular fan perimeter
        let segments = 64;
        for i in 0..segments {
            let frac = (i as f32) / (segments as f32);
            let theta = frac * 2.0 * std::f32::consts::PI;
            let rot_theta = theta + angle;
            
            let pos = center + egui::vec2(rot_theta.cos(), rot_theta.sin()) * label_r;
            let uv_x = 0.5 + 0.5 * theta.cos();
            let uv_y = 0.5 + 0.5 * theta.sin();

            mesh.vertices.push(egui::epaint::Vertex {
                pos,
                uv: egui::pos2(uv_x, uv_y),
                color: Color32::WHITE,
            });
        }

        // Add fan triangles
        for i in 1..=segments {
            let next = if i == segments { 1 } else { (i + 1) as u32 };
            mesh.add_triangle(0, i as u32, next);
        }

        ui.painter().add(mesh);
        ui.painter().circle_stroke(
            center,
            label_r,
            Stroke::new(2.0_f32, Color32::from_rgb(35, 35, 40)),
        );
    } else {
        ui.painter().circle_filled(center, label_r, BG_CARD);
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            "♪",
            FontId::proportional(radius * 0.35),
            T_DIM,
        );
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────────

impl eframe::App for MeduzaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle auto advance
        self.playback.handle_auto_advance();

        // Rotate vinyl record disc animation when playing
        let st = self.playback.state.lock().unwrap().clone();
        if matches!(st, PlaybackState::Playing) {
            self.disc_angle += 0.015;
            if self.disc_angle > std::f32::consts::TAU * 100.0 {
                self.disc_angle = 0.0;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        } else if matches!(st, PlaybackState::Loading) {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        if self.show_now_playing {
            self.show_now_playing_screen(ctx);
            return;
        }

        // Request repaint when new images arrive
        if self.img_pending.lock().unwrap().values().any(|v| v.is_some()) {
            ctx.request_repaint();
        }

        // ── Visuals ──────────────────────────────────────────────────────────
        let mut vis = egui::Visuals::dark();
        vis.panel_fill                     = BG;
        vis.window_fill                    = BG;
        vis.override_text_color            = Some(T_PRI);
        vis.selection.bg_fill              = ACCENT;
        vis.widgets.noninteractive.bg_fill = BG_CARD;
        vis.widgets.inactive.bg_fill       = BG_CARD;
        vis.widgets.hovered.bg_fill        = BG_CARD_HV;
        vis.widgets.active.bg_fill         = ACCENT_DIM;
        vis.extreme_bg_color               = Color32::from_rgb(6, 6, 6);
        vis.widgets.inactive.fg_stroke     = Stroke::NONE;
        ctx.set_visuals(vis);

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
                    .inner_margin(egui::Margin::symmetric(20.0, 12.0))
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
