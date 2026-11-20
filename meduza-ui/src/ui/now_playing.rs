//! Full-screen player: artwork-tinted backdrop, transport, seek and synced lyrics.

use eframe::egui::{self, Align2, Color32, Rect, Rounding, Sense};
use eframe::egui::{pos2, vec2, RichText};

use meduza_core::innertube::TrackItem;
use meduza_playback::playback_manager::RepeatMode;

use super::icons::{self, Icon};
use super::widgets::{self, bar, icon_button, pill, BtnStyle};
use super::{fmt_time, Action, Cx, UiState};
use crate::theme::*;

const TOP_H: f32 = 64.0;
const HERO: f32 = 26.0;

pub fn show(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState, now: Option<&TrackItem>) {
    let vm = cx.vm;
    let rect = ui.max_rect();

    // Backdrop: the artwork's colour fading into the app background.
    widgets::vertical_gradient(ui.painter(), rect, mix(cx.accent, BG, 0.55), mix(BG, cx.accent, 0.05));

    top_bar(ui, cx, st, rect);

    let body = Rect::from_min_max(rect.min + vec2(0.0, TOP_H), rect.max - vec2(0.0, SPACE_LG));
    let content = body.shrink2(vec2(56.0, 0.0));
    let show_lyrics = st.show_lyrics && content.width() > 760.0;
    let left_w = if show_lyrics { content.width() * 0.5 } else { content.width() };
    let left = Rect::from_min_size(content.min, vec2(left_w, content.height()));

    // ── Left: artwork, title, seek, transport ─────────────────────────────
    let art_size = (left.height() - 230.0).min(left.width() - 20.0).clamp(160.0, 480.0);
    let art_rect = Rect::from_center_size(
        pos2(left.center().x, left.top() + art_size / 2.0 + 8.0),
        vec2(art_size, art_size),
    );
    if let Some(track) = now {
        let shadow = art_rect.translate(vec2(0.0, 16.0)).expand(4.0);
        ui.painter().rect_filled(shadow, Rounding::same(18.0), Color32::from_black_alpha(90));
        widgets::art(ui, cx, &track.media_id, &track.thumbnail_url, art_rect, 14.0);
    } else {
        ui.painter().rect_filled(art_rect, Rounding::same(14.0), SURFACE);
        icons::paint(ui.painter(), art_rect.shrink(art_size * 0.35), Icon::Lyrics, T_DIM);
    }

    let info_top = art_rect.bottom() + 22.0;
    let info_w = left.width();
    let title = now.map(|t| t.title.clone()).unwrap_or_else(|| "Nothing playing".to_string());
    let artist = now.map(|t| t.artist.clone()).unwrap_or_default();
    ui.painter().text(
        pos2(left.left() + 4.0, info_top),
        Align2::LEFT_TOP,
        super::trunc(&title, super::fit_chars(info_w - 60.0, HERO)),
        font(HERO),
        T_PRI,
    );
    ui.painter().text(
        pos2(left.left() + 4.0, info_top + HERO + 8.0),
        Align2::LEFT_TOP,
        super::trunc(&artist, super::fit_chars(info_w - 60.0, TEXT_LG)),
        font(TEXT_LG),
        T_SEC,
    );

    // Like button sits at the right end of the title row.
    if let Some(track) = now {
        let liked = cx.is_liked(&track.media_id);
        let like_rect = Rect::from_center_size(pos2(left.right() - 18.0, info_top + 14.0), vec2(34.0, 34.0));
        let resp = ui.interact(like_rect, ui.id().with("np_like"), Sense::click());
        let color = if liked { LIKE } else { T_SEC };
        icons::paint(ui.painter(), like_rect.shrink(7.0), if liked { Icon::Heart } else { Icon::HeartOutline }, color);
        if resp.clicked() {
            cx.push(Action::ToggleLike(track.clone()));
        }
        resp.on_hover_text(if liked { "Remove from Liked" } else { "Save to Liked" });
    }

    let pb = &vm.playback;
    let pos = pb.progress_secs.try_lock().map(|g| *g).unwrap_or(0.0);
    let dur = pb.duration_secs.try_lock().map(|g| *g).unwrap_or(0.0);

    let seek_top = info_top + HERO + 8.0 + 34.0;
    ui.allocate_ui_at_rect(
        Rect::from_min_size(pos2(left.left(), seek_top), vec2(left.width(), 40.0)),
        |ui| {
            ui.horizontal(|ui| {
                ui.add_sized(vec2(48.0, 16.0), egui::Label::new(time(pos)));
                let bar_w = (left.width() - 120.0).max(120.0);
                let ratio = if dur > 0.0 { pos / dur } else { 0.0 };
                if let Some(frac) = bar(ui, bar_w, ratio, cx.accent) {
                    if dur > 0.0 {
                        cx.push(Action::Seek(frac * dur));
                    }
                }
                ui.add_sized(vec2(48.0, 16.0), egui::Label::new(time(dur)));
            });
        },
    );

    let transport_top = seek_top + 44.0;
    ui.allocate_ui_at_rect(
        Rect::from_min_size(pos2(left.left(), transport_top), vec2(left.width(), 64.0)),
        |ui| {
            transport(ui, cx);
        },
    );

    // ── Right: synced lyrics ─────────────────────────────────────────────
    if show_lyrics {
        let panel = Rect::from_min_max(
            pos2(left.right() + 40.0, body.top() + 8.0),
            pos2(content.right(), body.bottom() - 8.0),
        );
        ui.painter().rect_filled(panel, Rounding::same(RADIUS_LG), Color32::from_black_alpha(70));
        ui.allocate_ui_at_rect(panel.shrink2(vec2(28.0, 22.0)), |ui| {
            lyrics(ui, cx, st, pos);
        });
    }
}

fn top_bar(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState, rect: Rect) {
    let bar_rect = Rect::from_min_size(rect.min + vec2(20.0, 12.0), vec2(40.0, 40.0));
    ui.allocate_ui_at_rect(bar_rect, |ui| {
        if icon_button(ui, Icon::ChevronDown, 40.0, T_PRI, "Close (Esc)").clicked() {
            cx.push(Action::ShowNowPlaying(false));
        }
    });

    let label_rect = Rect::from_min_size(rect.min + vec2(72.0, 22.0), vec2(rect.width() - 300.0, 24.0));
    ui.painter().text(
        label_rect.center_top(),
        Align2::CENTER_TOP,
        "NOW PLAYING",
        font(TEXT_XS),
        mix(T_PRI, T_DIM, 0.4),
    );

    let toggle_rect = Rect::from_min_size(rect.right_top() + vec2(-190.0, 16.0), vec2(170.0, 34.0));
    ui.allocate_ui_at_rect(toggle_rect, |ui| {
        let label = if st.show_lyrics { "Hide lyrics" } else { "Show lyrics" };
        if pill(ui, label, BtnStyle::Secondary, Some(Icon::Lyrics)).clicked() {
            st.show_lyrics = !st.show_lyrics;
            st.last_lyric = None;
        }
    });
}

fn transport(ui: &mut egui::Ui, cx: &mut Cx) {
    let vm = cx.vm;
    let shuffle = vm.playback.is_shuffle.try_lock().map(|g| *g).unwrap_or(false);
    let repeat = vm.playback.repeat_mode.try_lock().map(|g| *g).unwrap_or(RepeatMode::Off);
    ui.horizontal_centered(|ui| {
        ui.add_space(0.0);
        let shuffle_color = if shuffle { cx.accent } else { T_DIM };
        if icon_button(ui, Icon::Shuffle, 40.0, shuffle_color, "Shuffle").clicked() {
            cx.push(Action::ToggleShuffle);
        }
        if icon_button(ui, Icon::Prev, 46.0, T_PRI, "Previous").clicked() {
            cx.push(Action::SkipPrev);
        }
        let (play_rect, play) = ui.allocate_exact_size(vec2(68.0, 68.0), Sense::click());
        let hot = ui.ctx().animate_bool(play.id.with("hot"), play.hovered());
        let painter = ui.painter();
        painter.circle_filled(play_rect.center(), 34.0, mix(cx.accent, Color32::WHITE, 0.14 * hot));
        let glyph = if cx.playing { Icon::Pause } else { Icon::Play };
        icons::paint(painter, play_rect.shrink(22.0), glyph, BG);
        if play.clicked() {
            cx.push(Action::TogglePause);
        }
        if icon_button(ui, Icon::Next, 46.0, T_PRI, "Next").clicked() {
            cx.push(Action::SkipNext);
        }
        let (icon, color, tip) = match repeat {
            RepeatMode::Off => (Icon::Repeat, T_DIM, "Repeat off"),
            RepeatMode::All => (Icon::Repeat, cx.accent, "Repeat all"),
            RepeatMode::One => (Icon::RepeatOne, cx.accent, "Repeat one"),
        };
        if icon_button(ui, icon, 40.0, color, tip).clicked() {
            cx.push(Action::ToggleRepeat);
        }
    });
}

fn lyrics(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState, pos: f32) {
    let vm = cx.vm;
    let Ok(lyrics) = vm.lyrics.read() else {
        ui.label(RichText::new("Lyrics unavailable").color(T_DIM));
        return;
    };

    if lyrics.is_empty() {
        let text = if vm.lyrics_track_id.read().ok().and_then(|g| g.clone()).is_some() {
            "No lyrics found for this track."
        } else {
            "Lyrics appear here."
        };
        widgets::empty_state(ui, Icon::Lyrics, "No lyrics", text);
        return;
    }

    ui.label(RichText::new(format!("LYRICS · {}", lyrics.source)).font(font(TEXT_XS)).color(T_DIM));
    ui.add_space(SPACE_SM);

    if lyrics.lines.is_empty() {
        // Plain-text lyrics: no timing, just scroll.
        egui::ScrollArea::vertical()
            .id_source("plain_lyrics")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(text) = &lyrics.plain {
                    ui.label(RichText::new(text).font(font(TEXT_LG)).color(T_SEC));
                }
            });
        return;
    }

    let active = lyrics.active_index(pos);
    let changed = active != st.last_lyric;
    st.last_lyric = active;

    egui::ScrollArea::vertical()
        .id_source("synced_lyrics")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(ui.available_height() * 0.35);
            for (i, line) in lyrics.lines.iter().enumerate() {
                let is_active = active == Some(i);
                let (size, color) = if is_active {
                    (22.0, cx.accent)
                } else if active.map_or(false, |a| i < a) {
                    (17.0, mix(T_DIM, BG, 0.1))
                } else {
                    (17.0, T_SEC)
                };
                let text = if line.text.is_empty() { "♪".to_string() } else { line.text.clone() };
                ui.vertical_centered(|ui| {
                    let label = ui.add(
                        egui::Label::new(RichText::new(text).font(font(size)).color(color))
                            .wrap(true)
                            .sense(Sense::click()),
                    );
                    if label.clicked() {
                        cx.push(Action::Seek(line.time));
                    }
                    if is_active && changed {
                        ui.scroll_to_rect(label.rect, Some(egui::Align::Center));
                    }
                });
                ui.add_space(14.0);
            }
            ui.add_space(ui.available_height() * 0.5);
        });
}

fn time(secs: f32) -> RichText {
    RichText::new(fmt_time(secs)).font(font(TEXT_XS)).color(T_DIM)
}
