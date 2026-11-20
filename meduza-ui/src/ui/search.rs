//! Search: a large field with live suggestions, a top result, a song list, and
//! genre browsing when nothing has been searched yet.

use std::sync::atomic::Ordering;
use std::time::Instant;

use eframe::egui::{self, Align2, Frame, Margin, Rect, Rounding, RichText, Sense};
use eframe::egui::{pos2, vec2};

use meduza_core::innertube::TrackItem;

use super::icons::{self, Icon};
use super::widgets::{self, art, pill, tile, BtnStyle};
use super::{fit_chars, trunc, Action, Cx, UiState, SEARCH_FIELD_ID};
use crate::theme::*;

const GENRES: [&str; 10] = [
    "Pop", "Hip-Hop", "Rock", "Electronic", "R&B", "Latin", "Indie", "Chill", "Workout", "Podcasts",
];

pub fn show(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    let vm = cx.vm;
    let snap = cx.snap;

    ui.add_space(SPACE_SM);
    search_field(ui, cx, st);

    let searching = vm.is_searching.load(Ordering::SeqCst);
    ui.add_space(SPACE_LG);

    egui::ScrollArea::vertical()
        .id_source("search_results")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if searching {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(18.0).color(ACCENT));
                    ui.label(RichText::new("Searching…").color(T_SEC).font(font(TEXT_MD)));
                });
                ui.add_space(SPACE_MD);
                for _ in 0..6 {
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
                    ui.painter().rect_filled(rect.shrink2(vec2(0.0, 8.0)), Rounding::same(RADIUS_MD), SURFACE);
                }
                return;
            }

            if st.last_search.is_empty() {
                browse_genres(ui, cx, st);
                return;
            }

            if snap.search.is_empty() {
                widgets::empty_state(
                    ui,
                    Icon::Search,
                    "No results",
                    &format!("Nothing matched “{}”. Try another spelling.", st.last_search),
                );
                return;
            }

            top_result(ui, cx, &snap.search[0]);
            ui.add_space(SPACE_XL);
            widgets::heading(ui, "Songs");
            ui.add_space(SPACE_SM);
            widgets::track_list(ui, cx, &snap.search, None);
            ui.add_space(SPACE_LG);
        });
}

fn search_field(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    let width = ui.available_width().min(680.0);
    Frame::none()
        .fill(SURFACE)
        .rounding(Rounding::same(RADIUS_PILL))
        .inner_margin(Margin::symmetric(20.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(width - 40.0);
            ui.horizontal(|ui| {
                let (icon_rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                icons::paint(ui.painter(), icon_rect, Icon::Search, T_SEC);
                ui.add_space(6.0);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut st.search_query)
                        .id(egui::Id::new(SEARCH_FIELD_ID))
                        .hint_text("Artists, songs, albums")
                        .font(font(TEXT_LG))
                        .frame(false)
                        .desired_width(f32::INFINITY),
                );
                if field.changed() {
                    let query = st.search_query.trim().to_string();
                    if let Ok(mut g) = st.suggestions.try_lock() {
                        g.clear();
                    }
                    if query.is_empty() {
                        st.pending_suggest = None;
                        st.suggest_visible = false;
                    } else {
                        st.pending_suggest = Some(query);
                        st.suggest_changed_at = Instant::now();
                        st.suggest_visible = true;
                    }
                }
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    cx.push(Action::Search(st.search_query.trim().to_string()));
                }
            });
        });

    if st.suggest_visible {
        let suggestions = st.suggestions.try_lock().map(|g| g.clone()).unwrap_or_default();
        if !suggestions.is_empty() {
            Frame::none()
                .fill(SURFACE_HI)
                .rounding(Rounding::same(RADIUS_MD))
                .inner_margin(Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(width - 16.0);
                    for s in suggestions.iter().take(6) {
                        let row = ui.add(
                            egui::Button::new(
                                RichText::new(format!("🔍  {}", s)).color(T_PRI).font(font(TEXT_MD)),
                            )
                            .frame(false)
                            .min_size(vec2(width - 32.0, 30.0)),
                        );
                        if row.clicked() {
                            st.search_query = s.clone();
                            st.suggest_visible = false;
                            cx.push(Action::Search(s.clone()));
                        }
                    }
                });
        }
    }

    if !st.last_search.is_empty() && !st.suggest_visible {
        ui.add_space(SPACE_SM);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Last search").color(T_DIM).font(font(TEXT_XS)));
            let chip = pill(ui, &trunc(&st.last_search, 30), BtnStyle::Secondary, Some(Icon::Search));
            if chip.clicked() {
                cx.push(Action::Search(st.last_search.clone()));
            }
        });
    }
}

/// Large card for the best match: click to play its radio.
fn top_result(ui: &mut egui::Ui, cx: &mut Cx, track: &TrackItem) {
    ui.label(RichText::new("Top result").font(font(TEXT_LG)).color(T_PRI).strong());
    ui.add_space(SPACE_SM);
    let width = ui.available_width().min(720.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 150.0), Sense::click());
    let hot = ui.ctx().animate_bool(resp.id.with("hot"), resp.hovered());
    widgets::prefetch_on_dwell(ui, cx, resp.id, resp.hovered(), track);
    ui.painter().rect_filled(rect, Rounding::same(RADIUS_LG), mix(SURFACE, SURFACE_HI, hot));

    let art_rect = Rect::from_min_size(rect.min + vec2(18.0, 18.0), vec2(114.0, 114.0));
    art(ui, cx, &track.media_id, &track.thumbnail_url, art_rect, RADIUS_MD);

    let painter = ui.painter();
    let tx = art_rect.right() + 24.0;
    let text_w = rect.right() - tx - 120.0;
    painter.text(
        pos2(tx, rect.top() + 40.0),
        Align2::LEFT_CENTER,
        trunc(&track.title, fit_chars(text_w, TEXT_XL)),
        font(TEXT_XL),
        if cx.is_now(&track.media_id) { cx.accent } else { T_PRI },
    );
    painter.text(
        pos2(tx, rect.top() + 70.0),
        Align2::LEFT_CENTER,
        trunc(&track.artist, fit_chars(text_w, TEXT_MD)),
        font(TEXT_MD),
        T_SEC,
    );
    painter.text(
        pos2(tx, rect.top() + 96.0),
        Align2::LEFT_CENTER,
        "Song",
        font(TEXT_XS),
        T_DIM,
    );
    let play_c = pos2(rect.right() - 52.0, rect.center().y);
    painter.circle_filled(play_c, 26.0, ACCENT);
    icons::paint(painter, Rect::from_center_size(play_c, vec2(22.0, 22.0)), Icon::Play, BG);

    if resp.clicked() {
        cx.push(Action::Play(track.clone()));
    }
    resp.context_menu(|ui| widgets::track_menu(ui, cx, track, None));
}

/// Genre tiles shown before anything has been searched.
fn browse_genres(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    widgets::heading(ui, "Browse all");
    ui.add_space(SPACE_MD);
    let gap = SPACE_MD;
    let avail = ui.available_width();
    let cols = ((avail + gap) / 230.0).floor().clamp(2.0, 5.0) as usize;
    let w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
    let h = w * 0.55;

    for row in GENRES.chunks(cols) {
        ui.horizontal(|ui| {
            for name in row {
                let resp = tile(ui, vec2(w, h), name, name, "Browse");
                if resp.clicked() {
                    st.search_query = name.to_string();
                    cx.push(Action::Search(name.to_string()));
                }
                ui.add_space(gap);
            }
        });
        ui.add_space(gap);
    }
}
