//! Home: quick picks, recently played, then the personalised shelves.

use std::sync::atomic::Ordering;

use eframe::egui::{self, Align2, Rect, Rounding, Sense};
use eframe::egui::{pos2, vec2};

use meduza_core::innertube::TrackItem;

use super::icons::Icon;
use super::widgets::{self, art, pill, BtnStyle};
use super::{greeting, trunc, Action, Cx, UiState};
use crate::theme::*;

pub fn show(ui: &mut egui::Ui, cx: &mut Cx, _st: &mut UiState) {
    let vm = cx.vm;
    let snap = cx.snap;
    let loading = vm.is_loading_home.load(Ordering::SeqCst);

    ui.add_space(SPACE_SM);
    widgets::page_title(ui, greeting());
    ui.add_space(SPACE_LG);

    egui::ScrollArea::vertical()
        .id_source("home_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let recent = &snap.history;
            if !recent.is_empty() {
                quick_picks(ui, cx, &recent[..recent.len().min(8)]);
            }

            if snap.sections.is_empty() {
                if loading {
                    for _ in 0..3 {
                        widgets::skeleton_shelf(ui, cx.time, 180.0, 6);
                    }
                } else {
                    widgets::empty_state(
                        ui,
                        Icon::Home,
                        "Couldn't load your feed",
                        "Check your connection, then try again.",
                    );
                    ui.vertical_centered(|ui| {
                        if pill(ui, "Try again", BtnStyle::Primary, None).clicked() {
                            cx.push(Action::ReloadHome);
                        }
                    });
                }
                return;
            }

            for section in &snap.sections {
                widgets::shelf(ui, cx, &section.title, &section.title, &section.items);
            }
            ui.add_space(SPACE_LG);
        });
}

/// Compact tiles for the most recent tracks, laid out as a grid.
fn quick_picks(ui: &mut egui::Ui, cx: &mut Cx, tracks: &[TrackItem]) {
    let gap = SPACE_SM;
    let avail = ui.available_width();
    let cols = ((avail + gap) / 300.0).floor().clamp(1.0, 4.0) as usize;
    let tile_w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;

    egui::Grid::new("quick_picks")
        .num_columns(cols)
        .spacing([gap, gap])
        .show(ui, |ui| {
            for (i, track) in tracks.iter().enumerate() {
                pick_tile(ui, cx, track, tile_w);
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
    ui.add_space(SPACE_XL);
}

fn pick_tile(ui: &mut egui::Ui, cx: &mut Cx, track: &TrackItem, width: f32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 60.0), Sense::click());
    let hot = ui.ctx().animate_bool(resp.id.with("hot"), resp.hovered());
    ui.painter().rect_filled(
        rect,
        Rounding::same(RADIUS_MD),
        mix(SURFACE, SURFACE_HI, hot),
    );
    let art_rect = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(44.0, 44.0));
    art(ui, cx, &track.media_id, &track.thumbnail_url, art_rect, RADIUS_SM);

    let painter = ui.painter();
    let text_w = width - 44.0 - 40.0;
    let color = if cx.is_now(&track.media_id) { cx.accent } else { T_PRI };
    painter.text(
        pos2(art_rect.right() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        trunc(&track.title, super::fit_chars(text_w, TEXT_MD)),
        font(TEXT_MD),
        color,
    );
    if hot > 0.01 {
        let c = pos2(rect.right() - 24.0, rect.center().y);
        painter.circle_filled(c, 14.0 * hot, ACCENT);
        super::icons::paint(painter, Rect::from_center_size(c, vec2(12.0, 12.0)), Icon::Play, BG);
    }

    if resp.clicked() {
        cx.push(Action::Play(track.clone()));
    }
    resp.context_menu(|ui| widgets::track_menu(ui, cx, track, None));
}
