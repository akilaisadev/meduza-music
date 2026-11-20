//! Bottom player bar and the queue drawer.

use eframe::egui::{self, Align2, Color32, Layout, Rect, Rounding, Sense, Stroke};
use eframe::egui::{pos2, vec2, RichText};

use meduza_core::innertube::TrackItem;
use meduza_playback::playback_manager::{PlaybackState, RepeatMode};

use super::icons::{self, Icon};
use super::widgets::{self, bar, icon_button};
use super::{fit_chars, fmt_time, trunc, Action, Cx, UiState};
use crate::theme::*;

const PLAY_BUTTON: f32 = 42.0;

pub fn show(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState, now: Option<&TrackItem>) {
    let vm = cx.vm;
    let pb = &vm.playback;
    let full = ui.max_rect();
    ui.painter()
        .line_segment([full.left_top(), full.right_top()], Stroke::new(1.0_f32, HAIRLINE));

    let pos = pb.progress_secs.try_lock().map(|g| *g).unwrap_or(0.0);
    let dur = pb.duration_secs.try_lock().map(|g| *g).unwrap_or(0.0);
    let volume = pb.volume.try_lock().map(|g| *g).unwrap_or(80.0);
    let shuffle = pb.is_shuffle.try_lock().map(|g| *g).unwrap_or(false);
    let repeat = pb.repeat_mode.try_lock().map(|g| *g).unwrap_or(RepeatMode::Off);
    let loading = pb
        .state
        .try_lock()
        .map(|s| matches!(*s, PlaybackState::Loading))
        .unwrap_or(false);

    let w = full.width();
    let left_w = (w * 0.28).clamp(220.0, 400.0);
    let right_w = (w * 0.27).clamp(230.0, 340.0);
    let center_w = (w - left_w - right_w).max(260.0);
    let left = Rect::from_min_size(full.min, vec2(left_w, full.height()));
    let center = Rect::from_min_size(pos2(full.left() + left_w, full.top()), vec2(center_w, full.height()));
    let right = Rect::from_min_size(pos2(center.right(), full.top()), vec2(full.width() - left_w - center_w, full.height()));

    // ── Left: now playing summary ──────────────────────────────────────────
    ui.allocate_ui_at_rect(left.shrink2(vec2(18.0, 0.0)), |ui| {
        ui.horizontal_centered(|ui| {
            let (art_rect, art_resp) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::click());
            match now {
                Some(track) => {
                    widgets::art(ui, cx, &track.media_id, &track.thumbnail_url, art_rect, RADIUS_SM);
                    if art_resp.clicked() {
                        cx.push(Action::ShowNowPlaying(true));
                    }
                    art_resp.on_hover_text("Open full-screen player");
                }
                None => {
                    ui.painter().rect_filled(art_rect, Rounding::same(RADIUS_SM), SURFACE_PRESS);
                    icons::paint(ui.painter(), art_rect.shrink(16.0), Icon::Lyrics, T_DIM);
                }
            }
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.add_space(10.0);
                match now {
                    Some(track) => {
                        let text_w = left_w - 150.0;
                        ui.label(
                            RichText::new(trunc(&track.title, fit_chars(text_w, TEXT_MD)))
                                .color(if cx.playing { cx.accent } else { T_PRI })
                                .font(font(TEXT_MD))
                                .strong(),
                        );
                        ui.label(
                            RichText::new(trunc(&track.artist, fit_chars(text_w, TEXT_SM)))
                                .color(T_SEC)
                                .font(font(TEXT_SM)),
                        );
                    }
                    None => {
                        ui.label(RichText::new("Nothing playing").color(T_DIM).font(font(TEXT_MD)));
                        ui.label(RichText::new("Pick a song to start").color(T_DIM).font(font(TEXT_SM)));
                    }
                }
            });
            if let Some(track) = now {
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    let liked = cx.is_liked(&track.media_id);
                    let icon = if liked { Icon::Heart } else { Icon::HeartOutline };
                    let color = if liked { LIKE } else { T_SEC };
                    let tip = if liked { "Remove from Liked" } else { "Save to Liked" };
                    if icon_button(ui, icon, 30.0, color, tip).clicked() {
                        cx.push(Action::ToggleLike(track.clone()));
                    }
                });
            }
        });
    });

    // ── Centre: transport + seek ───────────────────────────────────────────
    ui.allocate_ui_at_rect(center.shrink2(vec2(24.0, 0.0)), |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let shuffle_color = if shuffle { cx.accent } else { T_DIM };
                if icon_button(ui, Icon::Shuffle, 30.0, shuffle_color, "Shuffle").clicked() {
                    cx.push(Action::ToggleShuffle);
                }
                if icon_button(ui, Icon::Prev, 34.0, T_PRI, "Previous").clicked() {
                    cx.push(Action::SkipPrev);
                }
                if play_button(ui, cx, loading) {
                    cx.push(Action::TogglePause);
                }
                if icon_button(ui, Icon::Next, 34.0, T_PRI, "Next").clicked() {
                    cx.push(Action::SkipNext);
                }
                let (icon, color, tip) = match repeat {
                    RepeatMode::Off => (Icon::Repeat, T_DIM, "Repeat off"),
                    RepeatMode::All => (Icon::Repeat, cx.accent, "Repeat all"),
                    RepeatMode::One => (Icon::RepeatOne, cx.accent, "Repeat one"),
                };
                if icon_button(ui, icon, 30.0, color, tip).clicked() {
                    cx.push(Action::ToggleRepeat);
                }
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let label_w = 52.0;
                let bar_w = (center.width() - 48.0 - label_w * 2.0 - 16.0).max(120.0);
                ui.add_sized(vec2(label_w, 16.0), egui::Label::new(time_text(pos)));
                let ratio = if dur > 0.0 { pos / dur } else { 0.0 };
                if let Some(frac) = bar(ui, bar_w, ratio, cx.accent) {
                    if dur > 0.0 {
                        cx.push(Action::Seek(frac * dur));
                    }
                }
                ui.add_sized(vec2(label_w, 16.0), egui::Label::new(time_text(dur)));
            });
        });
    });

    // ── Right: queue, full-screen, volume ──────────────────────────────────
    ui.allocate_ui_at_rect(right.shrink2(vec2(18.0, 0.0)), |ui| {
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            let queue_color = if st.show_queue { cx.accent } else { T_SEC };
            if icon_button(ui, Icon::Queue, 30.0, queue_color, "Queue").clicked() {
                st.show_queue = !st.show_queue;
            }
            if icon_button(ui, Icon::Expand, 30.0, T_SEC, "Full-screen player").clicked() {
                cx.push(Action::ShowNowPlaying(true));
            }
            let muted = volume <= 0.5;
            let vol_icon = if muted { Icon::Mute } else { Icon::Volume };
            if icon_button(ui, vol_icon, 30.0, T_SEC, if muted { "Unmute" } else { "Mute" }).clicked() {
                if muted {
                    cx.push(Action::SetVolume(st.volume_before_mute.max(10.0)));
                } else {
                    cx.push(Action::SetVolume(0.0));
                }
            }
            ui.add_space(4.0);
            if let Some(frac) = bar(ui, 110.0, volume / 100.0, cx.accent) {
                cx.push(Action::SetVolume(frac * 100.0));
            }
        });
    });
}

/// Large accent play/pause button. Returns true when clicked.
fn play_button(ui: &mut egui::Ui, cx: &Cx, loading: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(PLAY_BUTTON, PLAY_BUTTON), Sense::click());
    let hot = ui.ctx().animate_bool(resp.id.with("hot"), resp.hovered());
    let painter = ui.painter();
    let fill = mix(cx.accent, Color32::WHITE, 0.12 * hot);
    painter.circle_filled(rect.center(), PLAY_BUTTON / 2.0, fill);
    let glyph = if cx.playing { Icon::Pause } else { Icon::Play };
    let glyph_color = if loading { mix(BG, T_DIM, 0.5) } else { BG };
    icons::paint(painter, rect.shrink(PLAY_BUTTON * 0.22), glyph, glyph_color);
    resp.on_hover_text(if cx.playing { "Pause" } else { "Play" }).clicked()
}


fn time_text(secs: f32) -> RichText {
    RichText::new(fmt_time(secs)).font(font(TEXT_XS)).color(T_DIM)
}

/// Queue drawer: the current track and everything coming up. Click to jump.
pub fn queue_panel(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    ui.add_space(SPACE_LG);
    ui.horizontal(|ui| {
        ui.add_space(SPACE_LG);
        ui.label(RichText::new("Queue").color(T_PRI).font(font(TEXT_XL)).strong());
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(SPACE_SM);
            if icon_button(ui, Icon::Close, 28.0, T_SEC, "Close queue").clicked() {
                st.show_queue = false;
            }
        });
    });
    ui.add_space(SPACE_MD);

    let snap = cx.snap;
    if snap.queue.is_empty() {
        widgets::empty_state(ui, Icon::Queue, "Queue is empty", "Start a song and the queue fills itself.");
        return;
    }
    let current = snap.queue_index.min(snap.queue.len() - 1);

    egui::ScrollArea::vertical()
        .id_source("queue_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            section_label(ui, "NOW PLAYING");
            queue_row(ui, cx, &snap.queue[current], current, true);

            if current + 1 < snap.queue.len() {
                ui.add_space(SPACE_LG);
                section_label(ui, &format!("UP NEXT · {}", snap.queue.len() - current - 1));
                for i in current + 1..snap.queue.len() {
                    queue_row(ui, cx, &snap.queue[i], i, false);
                }
            }
            ui.add_space(SPACE_LG);
        });
}

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(SPACE_LG);
        ui.label(RichText::new(text).font(font(TEXT_XS)).color(T_DIM));
    });
    ui.add_space(SPACE_XS);
}

fn queue_row(ui: &mut egui::Ui, cx: &mut Cx, track: &TrackItem, index: usize, current: bool) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    let hot = ui.ctx().animate_bool(resp.id.with("hot"), resp.hovered());
    ui.painter().rect_filled(
        rect.shrink2(vec2(8.0, 2.0)),
        Rounding::same(RADIUS_MD),
        mix(if current { SURFACE } else { BG_SIDE }, SURFACE_HI, hot),
    );

    let art_rect = Rect::from_min_size(rect.min + vec2(18.0, 10.0), vec2(36.0, 36.0));
    widgets::art(ui, cx, &track.media_id, &track.thumbnail_url, art_rect, RADIUS_SM);

    let painter = ui.painter();
    let text_x = art_rect.right() + 12.0;
    let text_w = rect.width() - 36.0 - 60.0 - 30.0;
    let title_color = if current { cx.accent } else { T_PRI };
    painter.text(
        pos2(text_x, rect.center().y - 9.0),
        Align2::LEFT_CENTER,
        trunc(&track.title, fit_chars(text_w, TEXT_SM)),
        font(TEXT_SM),
        title_color,
    );
    painter.text(
        pos2(text_x, rect.center().y + 9.0),
        Align2::LEFT_CENTER,
        trunc(&track.artist, fit_chars(text_w, TEXT_XS)),
        font(TEXT_XS),
        T_SEC,
    );
    if current && cx.playing {
        let eq = Rect::from_center_size(pos2(rect.right() - 26.0, rect.center().y), vec2(14.0, 14.0));
        icons::equalizer(painter, eq, cx.time, cx.accent);
    }

    if resp.clicked() && !current {
        cx.push(Action::PlayQueueAt(index));
    }
    if resp.clicked() && current {
        cx.push(Action::TogglePause);
    }
}
