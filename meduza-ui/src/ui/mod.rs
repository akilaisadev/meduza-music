//! Meduza UI: application shell, shared state and per-frame wiring.
//!
//! Each frame the shell builds a [`Cx`] (read-only snapshot, image store, action
//! queue) and hands it to the panels and pages. Views never mutate the backend
//! directly: they push an [`Action`], and the shell applies the queue after the
//! frame is drawn. That keeps borrows simple and makes every state change visible
//! in one place ([`MeduzaApp::apply`]).

mod home;
mod icons;
mod images;
mod library;
mod now_playing;
mod player;
mod search;
mod settings;
mod sidebar;
mod widgets;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Frame, Key, Margin, Rounding, RichText, Sense};

use meduza_core::innertube::{BrowseSection, TrackItem};
use meduza_core::playlist::Playlist;
use meduza_playback::playback_manager::PlaybackState;
use meduza_viewmodel::app_viewmodel::AppViewModel;

use crate::theme::*;
use images::ImageStore;

const TOAST_TTL: Duration = Duration::from_secs(4);
const REFRESH_EVERY: Duration = Duration::from_millis(500);
const SEEK_STEP_SECS: f32 = 5.0;
/// Id of the search field on the Search page (Ctrl+K focuses it).
pub const SEARCH_FIELD_ID: &str = "search_page_field";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Search,
    Library,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LibraryTab {
    Playlists,
    Liked,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// Something a view wants the shell to do after the frame.
pub enum Action {
    /// Play one track; the queue is rebuilt from its radio.
    Play(TrackItem),
    /// Play `tracks` as the queue, starting at `start`.
    PlayFrom { tracks: Vec<TrackItem>, start: usize, shuffle: bool },
    PlayQueueAt(usize),
    PlayPlaylist { id: String, shuffle: bool },
    QueueNext(TrackItem),
    QueueLast(TrackItem),
    ToggleLike(TrackItem),
    AddToPlaylist { playlist: String, track: TrackItem },
    RemoveFromPlaylist { playlist: String, media_id: String },
    CreatePlaylist(String),
    RenamePlaylist { id: String, name: String },
    DeletePlaylist(String),
    OpenPlaylist(Option<String>),
    Search(String),
    Export { playlist: String, path: PathBuf },
    Import(PathBuf),
    ReloadHome,
    Toast(ToastKind, String),
    Seek(f32),
    TogglePause,
    SkipNext,
    SkipPrev,
    ToggleShuffle,
    ToggleRepeat,
    SetVolume(f32),
    ShowNowPlaying(bool),
    OpenUrl(String),
    ClearAudioCache,
}

/// Cached copies of backend data that views read. Refreshed on a timer or when
/// the home/search data version changes; never per frame and never blocking.
#[derive(Default)]
pub struct Snapshot {
    pub sections: Vec<BrowseSection>,
    pub search: Vec<TrackItem>,
    pub history: Vec<TrackItem>,
    pub liked: Vec<TrackItem>,
    pub liked_ids: HashSet<String>,
    pub playlists: Vec<Playlist>,
    pub queue: Vec<TrackItem>,
    pub queue_index: usize,
}

/// Per-frame context: read-only data, the image store and the action queue.
pub struct Cx<'a> {
    pub vm: &'a AppViewModel,
    pub snap: &'a Snapshot,
    pub images: &'a mut ImageStore,
    pub actions: &'a mut Vec<Action>,
    pub now_id: Option<String>,
    pub playing: bool,
    /// Colour of the current track's artwork (brand green when unknown).
    pub accent: Color32,
    pub zoom: f32,
    /// Seconds since start; drives animations.
    pub time: f64,
    pub max_dim: u32,
    pub logo: Option<&'a egui::TextureHandle>,
}

impl Cx<'_> {
    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn is_liked(&self, media_id: &str) -> bool {
        self.snap.liked_ids.contains(media_id)
    }

    pub fn is_now(&self, media_id: &str) -> bool {
        self.now_id.as_deref() == Some(media_id)
    }
}

/// Navigation and transient view state owned by the shell.
pub struct UiState {
    pub tab: Tab,
    pub sidebar_expanded: bool,
    pub show_now_playing: bool,
    pub show_queue: bool,
    pub show_lyrics: bool,

    pub search_query: String,
    pub last_search: String,
    pub suggestions: Arc<Mutex<Vec<String>>>,
    pub suggest_visible: bool,
    pub pending_suggest: Option<String>,
    pub suggest_changed_at: Instant,
    pub suggest_gen: Arc<AtomicU64>,

    pub library_tab: LibraryTab,
    pub open_playlist: Option<String>,
    pub new_playlist_name: String,
    /// Playlist being renamed: (id, draft name).
    pub rename: Option<(String, String)>,
    pub io_path: String,

    pub last_lyric: Option<usize>,
    pub delete_armed: bool,
    pub volume_before_mute: f32,
    pub cache_size_mb: f32,
    pub cache_checked_at: Instant,
}

impl UiState {
    fn new() -> Self {
        Self {
            tab: Tab::Home,
            sidebar_expanded: true,
            show_now_playing: false,
            show_queue: false,
            show_lyrics: true,
            search_query: String::new(),
            last_search: String::new(),
            suggestions: Arc::new(Mutex::new(Vec::new())),
            suggest_visible: false,
            pending_suggest: None,
            suggest_changed_at: Instant::now(),
            suggest_gen: Arc::new(AtomicU64::new(0)),
            library_tab: LibraryTab::Playlists,
            open_playlist: None,
            new_playlist_name: String::new(),
            rename: None,
            io_path: String::new(),
            last_lyric: None,
            delete_armed: false,
            volume_before_mute: 80.0,
            cache_size_mb: 0.0,
            cache_checked_at: Instant::now() - Duration::from_secs(60),
        }
    }
}

struct Toast {
    kind: ToastKind,
    text: String,
    born: Instant,
}

pub struct MeduzaApp {
    viewmodel: Arc<AppViewModel>,
    ui: UiState,
    snap: Snapshot,
    images: ImageStore,
    toasts: Vec<Toast>,
    logo: Option<egui::TextureHandle>,
    last_data_version: u64,
    last_refresh: Instant,
    last_error: Option<String>,
}

impl MeduzaApp {
    pub fn new(cc: &eframe::CreationContext<'_>, viewmodel: Arc<AppViewModel>) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);
        // Open at the compact scale; Ctrl+0 / Ctrl+± are handled by the shell
        // (egui's built-in handler would snap back to 1.0).
        cc.egui_ctx.set_zoom_factor(DEFAULT_ZOOM);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);

        let logo = image::load_from_memory(include_bytes!("../../../assets/logo.png"))
            .ok()
            .map(|img| {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                cc.egui_ctx.load_texture("app_logo", image, egui::TextureOptions::LINEAR)
            });

        Self {
            viewmodel,
            ui: UiState::new(),
            snap: Snapshot::default(),
            images: ImageStore::new(),
            toasts: Vec::new(),
            logo,
            last_data_version: u64::MAX,
            last_refresh: Instant::now() - REFRESH_EVERY,
            last_error: None,
        }
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.toasts.push(Toast { kind, text: text.into(), born: Instant::now() });
        if self.toasts.len() > 8 {
            self.toasts.remove(0);
        }
    }

    /// Copies backend state the views need. Home/search change with the data
    /// version; the rest is polled at most every `REFRESH_EVERY`.
    fn refresh_snapshot(&mut self, force: bool) {
        let vm = Arc::clone(&self.viewmodel);
        let version = vm.data_version.load(Ordering::SeqCst);
        if version != self.last_data_version {
            self.last_data_version = version;
            if let Ok(feed) = vm.home_feed.try_read() {
                self.snap.sections = feed.clone();
            }
            if let Ok(results) = vm.search_results.try_read() {
                self.snap.search = results.clone();
            }
        }

        if !force && self.last_refresh.elapsed() < REFRESH_EVERY {
            return;
        }
        self.last_refresh = Instant::now();
        let pb = &vm.playback;
        if let Ok(history) = pb.history.try_lock() {
            self.snap.history = history.iter().take(30).cloned().collect();
        }
        if let Ok(liked) = pb.liked_songs.try_lock() {
            self.snap.liked_ids = liked.iter().map(|t| t.media_id.clone()).collect();
            self.snap.liked = liked.clone();
        }
        if let Ok(playlists) = pb.playlists.try_lock() {
            self.snap.playlists = playlists.clone();
        }
        if let Ok(queue) = pb.queue.try_lock() {
            self.snap.queue = queue.iter().map(|item| item.track.clone()).collect();
        }
        let index = pb.queue_index.try_lock().map(|g| *g).ok();
        if let Some(index) = index {
            self.snap.queue_index = index;
        }
    }

    /// Debounced search suggestions. Responses for superseded queries are dropped.
    fn drive_suggestions(&mut self, ctx: &egui::Context) {
        let Some(query) = self.ui.pending_suggest.take() else { return; };
        if self.ui.suggest_changed_at.elapsed() < Duration::from_millis(180) {
            self.ui.pending_suggest = Some(query);
            // egui only repaints on input; without this the request would wait
            // for the next mouse move.
            ctx.request_repaint_after(Duration::from_millis(180));
            return;
        }
        let out = Arc::clone(&self.ui.suggestions);
        let client = Arc::clone(&self.viewmodel.innertube);
        let gen = self.ui.suggest_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let gen_now = Arc::clone(&self.ui.suggest_gen);
        self.viewmodel.runtime.spawn(async move {
            let suggestions = client.fetch_search_suggestions(&query).await;
            if gen_now.load(Ordering::SeqCst) != gen {
                return;
            }
            if let Ok(mut guard) = out.try_lock() {
                *guard = suggestions;
            }
        });
    }

    fn handle_zoom(&self, ctx: &egui::Context) {
        let zoom_delta = ctx.input(|i| i.zoom_delta());
        if (zoom_delta - 1.0).abs() > 0.001 {
            ctx.set_zoom_factor((ctx.zoom_factor() * zoom_delta).clamp(MIN_ZOOM, MAX_ZOOM));
        }
        let current = ctx.zoom_factor();
        let mut next: Option<f32> = None;
        ctx.input_mut(|i| {
            let cmd = egui::Modifiers::COMMAND;
            if i.consume_shortcut(&egui::KeyboardShortcut::new(cmd, Key::Num0)) {
                next = Some(DEFAULT_ZOOM);
            } else if i.consume_shortcut(&egui::KeyboardShortcut::new(cmd, Key::Equals))
                || i.consume_shortcut(&egui::KeyboardShortcut::new(cmd, Key::Plus))
            {
                next = Some(((current + 0.1) * 10.0).round() / 10.0);
            } else if i.consume_shortcut(&egui::KeyboardShortcut::new(cmd, Key::Minus)) {
                next = Some(((current - 0.1) * 10.0).round() / 10.0);
            }
        });
        if let Some(z) = next {
            ctx.set_zoom_factor(z.clamp(MIN_ZOOM, MAX_ZOOM));
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let typing = ctx.wants_keyboard_input();
        let (space, left, right, escape, ctrl, ctrl_k) = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::Escape),
                i.modifiers.command,
                i.modifiers.command && i.key_pressed(Key::K),
            )
        });

        if ctrl_k {
            self.ui.tab = Tab::Search;
            ctx.memory_mut(|m| m.request_focus(egui::Id::new(SEARCH_FIELD_ID)));
        }
        if escape {
            if self.ui.show_now_playing {
                self.ui.show_now_playing = false;
            } else if self.ui.show_queue {
                self.ui.show_queue = false;
            } else {
                self.ui.suggest_visible = false;
            }
        }
        if typing {
            return;
        }
        if space {
            actions.push(Action::TogglePause);
        }
        if ctrl && right {
            actions.push(Action::SkipNext);
        } else if ctrl && left {
            actions.push(Action::SkipPrev);
        } else if left || right {
            let pos = self.viewmodel.playback.progress_secs.try_lock().map(|g| *g).unwrap_or(0.0);
            let dur = self.viewmodel.playback.duration_secs.try_lock().map(|g| *g).unwrap_or(0.0);
            let delta = if right { SEEK_STEP_SECS } else { -SEEK_STEP_SECS };
            let target = (pos + delta).max(0.0);
            let target = if dur > 0.0 { target.min(dur) } else { target };
            actions.push(Action::Seek(target));
        }
    }

    /// Surfaces new playback errors as toasts (once per distinct message).
    fn watch_errors(&mut self, state: &PlaybackState) {
        let error = match state {
            PlaybackState::Error(e) => Some(e.clone()),
            _ => None,
        };
        if error != self.last_error {
            if let Some(e) = &error {
                self.toast(ToastKind::Error, e.clone());
            }
            self.last_error = error;
        }
    }

    fn schedule_repaint(&self, ctx: &egui::Context, state: &PlaybackState) {
        let ms = match state {
            PlaybackState::Playing => self
                .viewmodel
                .playback
                .settings
                .try_lock()
                .map(|s| s.now_playing_frame_ms())
                .unwrap_or(100),
            PlaybackState::Loading => 100,
            _ if self.viewmodel.is_loading_home.load(Ordering::SeqCst)
                || self.viewmodel.is_searching.load(Ordering::SeqCst) => 100,
            _ => 0,
        };
        if ms > 0 {
            ctx.request_repaint_after(Duration::from_millis(ms));
        }
    }

    /// Applies the actions queued during the frame.
    fn apply(&mut self, ctx: &egui::Context, actions: Vec<Action>) {
        if actions.is_empty() {
            return;
        }
        let vm = Arc::clone(&self.viewmodel);
        let pb = &vm.playback;
        let mut mutated = false;

        for action in actions {
            match action {
                Action::Play(track) => pb.play_now(track),
                Action::PlayFrom { tracks, start, shuffle } => {
                    set_flag(&pb.is_shuffle, shuffle);
                    pb.play_tracks_at(tracks, start);
                }
                Action::PlayQueueAt(index) => pb.play_queue_at(index),
                Action::PlayPlaylist { id, shuffle } => {
                    set_flag(&pb.is_shuffle, shuffle);
                    pb.play_playlist(&id);
                }
                Action::QueueNext(track) => {
                    pb.queue_next(track);
                    self.toast(ToastKind::Info, "Plays next");
                    mutated = true;
                }
                Action::QueueLast(track) => {
                    pb.queue_last(track);
                    self.toast(ToastKind::Info, "Added to queue");
                    mutated = true;
                }
                Action::ToggleLike(track) => {
                    pb.toggle_like(track);
                    mutated = true;
                }
                Action::AddToPlaylist { playlist, track } => {
                    let name = playlist_name(&self.snap, &playlist);
                    if pb.add_to_playlist(&playlist, track) {
                        self.toast(ToastKind::Success, format!("Added to {}", name));
                    } else {
                        self.toast(ToastKind::Info, format!("Already in {}", name));
                    }
                    mutated = true;
                }
                Action::RemoveFromPlaylist { playlist, media_id } => {
                    pb.remove_from_playlist(&playlist, &media_id);
                    mutated = true;
                }
                Action::CreatePlaylist(name) => {
                    match pb.create_playlist(name) {
                        Some(_) => self.toast(ToastKind::Success, "Playlist created"),
                        None => self.toast(ToastKind::Error, "Give the playlist a name first"),
                    }
                    mutated = true;
                }
                Action::RenamePlaylist { id, name } => {
                    if pb.rename_playlist(&id, name) {
                        self.toast(ToastKind::Success, "Playlist renamed");
                    } else {
                        self.toast(ToastKind::Error, "Name cannot be empty");
                    }
                    mutated = true;
                }
                Action::DeletePlaylist(id) => {
                    pb.delete_playlist(&id);
                    if self.ui.open_playlist.as_deref() == Some(id.as_str()) {
                        self.ui.open_playlist = None;
                    }
                    self.toast(ToastKind::Info, "Playlist deleted");
                    mutated = true;
                }
                Action::OpenPlaylist(id) => {
                    self.ui.open_playlist = id;
                    self.ui.tab = Tab::Library;
                    self.ui.library_tab = LibraryTab::Playlists;
                }
                Action::Search(query) => {
                    let query = query.trim().to_string();
                    if !query.is_empty() {
                        self.ui.search_query = query.clone();
                        self.ui.last_search = query.clone();
                        self.ui.suggest_visible = false;
                        self.ui.pending_suggest = None;
                        self.ui.tab = Tab::Search;
                        vm.search(query);
                    }
                }
                Action::Export { playlist, path } => match pb.export_playlist(&playlist, &path) {
                    Ok(()) => self.toast(ToastKind::Success, format!("Exported to {}", path.display())),
                    Err(e) => self.toast(ToastKind::Error, format!("Export failed: {}", e)),
                },
                Action::Import(path) => match pb.import_playlists(&path) {
                    Ok(n) => {
                        self.toast(ToastKind::Success, format!("Imported {} playlist(s)", n));
                        mutated = true;
                    }
                    Err(e) => self.toast(ToastKind::Error, format!("Import failed: {}", e)),
                },
                Action::ReloadHome => vm.load_home_feed(),
                Action::Toast(kind, text) => self.toast(kind, text),
                Action::Seek(secs) => pb.seek_to(secs),
                Action::TogglePause => pb.toggle_pause(),
                Action::SkipNext => pb.skip_next(),
                Action::SkipPrev => pb.skip_prev(),
                Action::ToggleShuffle => pb.toggle_shuffle(),
                Action::ToggleRepeat => pb.toggle_repeat(),
                Action::SetVolume(v) => {
                    if v > 0.0 {
                        self.ui.volume_before_mute = v;
                    }
                    pb.set_volume(v);
                }
                Action::ShowNowPlaying(show) => self.ui.show_now_playing = show,
                Action::OpenUrl(url) => ctx.open_url(egui::OpenUrl::same_tab(url)),
                Action::ClearAudioCache => {
                    if let Ok(ds) = pb.data_saver.try_lock() {
                        ds.clear_cache();
                    }
                    self.ui.cache_size_mb = 0.0;
                    self.ui.cache_checked_at = Instant::now();
                    self.toast(ToastKind::Success, "Audio cache cleared");
                }
            }
        }

        if mutated {
            self.refresh_snapshot(true);
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|t| t.born.elapsed() < TOAST_TTL);
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        let items: Vec<(ToastKind, String)> = self
            .toasts
            .iter()
            .rev()
            .take(4)
            .map(|t| (t.kind, t.text.clone()))
            .collect();

        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-SPACE_LG, -(PLAYER_H + SPACE_LG)))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for (kind, text) in items {
                    let color = match kind {
                        ToastKind::Info => T_SEC,
                        ToastKind::Success => ACCENT,
                        ToastKind::Error => DANGER,
                    };
                    Frame::none()
                        .fill(SURFACE_PRESS)
                        .rounding(Rounding::same(RADIUS_MD))
                        .inner_margin(Margin::symmetric(14.0, 10.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                                ui.painter().circle_filled(dot.center(), 4.0, color);
                                ui.label(RichText::new(text).color(T_PRI).font(font(TEXT_SM)));
                            });
                        });
                    ui.add_space(SPACE_SM);
                }
            });
    }
}

impl eframe::App for MeduzaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.images.begin_frame();
        self.handle_zoom(ctx);

        let mut actions = Vec::new();
        self.handle_shortcuts(ctx, &mut actions);
        self.refresh_snapshot(false);
        self.drive_suggestions(ctx);
        self.viewmodel.playback.handle_auto_advance();

        let now = self.viewmodel.playback.current_track.try_lock().ok().and_then(|g| g.clone());
        if let Some(track) = &now {
            self.viewmodel.ensure_lyrics(track.clone());
        }
        let state = self.viewmodel.playback.state.try_lock().map(|g| g.clone()).unwrap_or(PlaybackState::Idle);
        self.watch_errors(&state);
        self.schedule_repaint(ctx, &state);

        let accent = now
            .as_ref()
            .and_then(|t| self.images.accent(&t.media_id))
            .map(readable_accent)
            .unwrap_or(ACCENT);
        let max_dim = self
            .viewmodel
            .playback
            .settings
            .try_lock()
            .map(|s| s.image_max_dim())
            .unwrap_or(512);

        {
            let mut cx = Cx {
                vm: &self.viewmodel,
                snap: &self.snap,
                images: &mut self.images,
                actions: &mut actions,
                now_id: now.as_ref().map(|t| t.media_id.clone()),
                playing: matches!(state, PlaybackState::Playing),
                accent,
                zoom: ctx.zoom_factor(),
                time: ctx.input(|i| i.time),
                max_dim,
                logo: self.logo.as_ref(),
            };
            layout(ctx, &mut cx, &mut self.ui, now.as_ref());
        }

        self.apply(ctx, actions);
        self.toasts_ui(ctx);
    }
}

/// Lays out the window: player, sidebar, top bar, optional queue, and the page.
/// The full-screen player replaces everything except the dialogs.
fn layout(ctx: &egui::Context, cx: &mut Cx, st: &mut UiState, now: Option<&TrackItem>) {
    if st.show_now_playing {
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG))
            .show(ctx, |ui| now_playing::show(ui, cx, st, now));
        rename_dialog(ctx, cx, st);
        return;
    }

    egui::TopBottomPanel::bottom("player")
        .exact_height(PLAYER_H)
        .frame(Frame::none().fill(BG_PLAYER))
        .show(ctx, |ui| player::show(ui, cx, st, now));

    let sidebar_w = if st.sidebar_expanded { SIDEBAR_FULL } else { SIDEBAR_RAIL };
    egui::SidePanel::left("sidebar")
        .exact_width(sidebar_w)
        .resizable(false)
        .frame(Frame::none().fill(BG_SIDE))
        .show(ctx, |ui| sidebar::show(ui, cx, st));

    egui::TopBottomPanel::top("topbar")
        .exact_height(TOPBAR_H)
        .frame(Frame::none().fill(BG))
        .show(ctx, |ui| sidebar::topbar(ui, cx, st));

    if st.show_queue {
        egui::SidePanel::right("queue")
            .exact_width(QUEUE_W)
            .resizable(false)
            .frame(Frame::none().fill(BG_SIDE))
            .show(ctx, |ui| player::queue_panel(ui, cx, st));
    }

    egui::CentralPanel::default()
        .frame(Frame::none().fill(BG).inner_margin(Margin {
            left: SPACE_XL,
            right: SPACE_LG,
            top: SPACE_SM,
            bottom: SPACE_LG,
        }))
        .show(ctx, |ui| match st.tab {
            Tab::Home => home::show(ui, cx, st),
            Tab::Search => search::show(ui, cx, st),
            Tab::Library => library::show(ui, cx, st),
            Tab::Settings => settings::show(ui, cx, st),
        });

    rename_dialog(ctx, cx, st);
}

fn rename_dialog(ctx: &egui::Context, cx: &mut Cx, st: &mut UiState) {
    let Some((id, mut name)) = st.rename.clone() else { return; };
    let mut open = true;
    let mut close = false;
    egui::Window::new("Rename playlist")
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut name)
                    .desired_width(260.0)
                    .hint_text("Playlist name"),
            );
            let submit = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.add_space(SPACE_SM);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() || submit {
                    cx.push(Action::RenamePlaylist { id: id.clone(), name: name.clone() });
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if close || !open {
        st.rename = None;
    } else {
        st.rename = Some((id, name));
    }
}

fn set_flag(flag: &Arc<Mutex<bool>>, on: bool) {
    if let Ok(mut g) = flag.try_lock() {
        *g = on;
    }
}

fn playlist_name(snap: &Snapshot, id: &str) -> String {
    snap.playlists
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "playlist".to_string())
}

/// Shortens `s` to roughly fit `width` px at font size `size`.
pub fn fit_chars(width: f32, size: f32) -> usize {
    (width / (size * 0.52)).floor().max(4.0) as usize
}

/// Truncates on char boundaries and appends an ellipsis.
pub fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

pub fn fmt_time(secs: f32) -> String {
    let total = secs.max(0.0) as u32;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

pub fn greeting() -> &'static str {
    use chrono::Timelike;
    match chrono::Local::now().hour() {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        18..=21 => "Good evening",
        _ => "Good night",
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "AppFont".to_owned(),
        egui::FontData::from_static(include_bytes!("../../../assets/Inter-Regular.ttf")),
    );
    fonts.font_data.insert(
        "CJK".to_owned(),
        egui::FontData::from_static(include_bytes!("../../../assets/NotoSansCJK.otf")),
    );
    fonts.font_data.insert(
        "Sinhala".to_owned(),
        egui::FontData::from_static(include_bytes!("../../../assets/NotoSansSinhala-Regular.ttf")),
    );
    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        prop.insert(0, "AppFont".to_owned());
        prop.push("CJK".to_owned());
        prop.push("Sinhala".to_owned());
    }
    load_system_fallback_fonts(&mut fonts);
    ctx.set_fonts(fonts);
}

/// Optional system fonts for scripts the bundled fonts do not cover.
fn load_system_fallback_fonts(fonts: &mut egui::FontDefinitions) {
    const DIRS: [&str; 6] = [
        "/usr/share/fonts/truetype/noto",
        "/usr/share/fonts/opentype/noto",
        "/usr/share/fonts/noto",
        "/usr/share/fonts/TTF",
        "/usr/share/fonts",
        "/app/share/fonts",
    ];
    const FILES: [&str; 16] = [
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
        "NotoSansSinhala-Bold.ttf",
        "NotoSerifSinhala-Regular.ttf",
        "NotoSansCJKjp-Regular.otf",
    ];
    let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) else { return; };
    for file in FILES {
        let found = DIRS
            .iter()
            .map(|dir| std::path::Path::new(dir).join(file))
            .find(|p| p.exists());
        if let Some(path) = found {
            if let Ok(bytes) = std::fs::read(&path) {
                let name = file.to_string();
                if !fonts.font_data.contains_key(&name) {
                    fonts.font_data.insert(name.clone(), egui::FontData::from_owned(bytes));
                    prop.push(name);
                }
            }
        }
    }
}

fn setup_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = SURFACE;
    visuals.window_rounding = Rounding::same(RADIUS_LG);
    visuals.window_stroke = egui::Stroke::new(1.0_f32, HAIRLINE);
    visuals.override_text_color = Some(T_PRI);
    visuals.selection.bg_fill = mix(ACCENT, BG, 0.55);
    visuals.extreme_bg_color = Color32::from_rgb(12, 12, 14);
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, T_SEC);
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.hovered.bg_fill = SURFACE_HI;
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HI;
    visuals.widgets.active.bg_fill = SURFACE_PRESS;
    visuals.widgets.active.weak_bg_fill = SURFACE_PRESS;
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        w.rounding = Rounding::same(RADIUS_SM + 2.0);
    }

    ctx.style_mut(|style| {
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.scroll.bar_width = 8.0;
        style.spacing.scroll.handle_min_length = 36.0;
        style.spacing.scroll.bar_inner_margin = 2.0;
        style.spacing.scroll.bar_outer_margin = 2.0;
        style.spacing.scroll.floating = true;
        style.spacing.scroll.floating_width = 6.0;
    });
}
