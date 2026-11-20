//! Left navigation rail / sidebar and the top bar of the main area.

use eframe::egui::{self, Align2, Layout, Margin, Rect, Rounding, Sense};
use eframe::egui::{pos2, vec2, RichText};

use super::icons::{self, Icon};
use super::widgets::{self, icon_button};
use super::{fit_chars, trunc, Action, Cx, Tab, UiState};
use crate::theme::*;

const NAV: [(Tab, &str, Icon); 3] = [
    (Tab::Home, "Home", Icon::Home),
    (Tab::Search, "Search", Icon::Search),
    (Tab::Library, "Your Library", Icon::Library),
];

pub fn show(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    let full = st.sidebar_expanded;

    ui.add_space(SPACE_LG);
    brand(ui, cx, st, full);
    ui.add_space(SPACE_XL);

    for (tab, label, icon) in NAV {
        nav_item(ui, st, tab, label, icon, full);
    }
    nav_item(ui, st, Tab::Settings, "Settings", Icon::Settings, full);

    if !full {
        return;
    }

    ui.add_space(SPACE_LG);
    ui.horizontal(|ui| {
        ui.add_space(22.0);
        ui.label(RichText::new("PLAYLISTS").font(font(TEXT_XS)).color(T_DIM));
    });
    ui.add_space(SPACE_XS);

    let snap = cx.snap;
    egui::ScrollArea::vertical()
        .id_source("sidebar_playlists")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if snap.playlists.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    ui.label(RichText::new("Your playlists appear here").font(font(TEXT_SM)).color(T_DIM));
                });
            }
            for pl in &snap.playlists {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
                let hot = ui.ctx().animate_bool(resp.id.with("hot"), resp.hovered());
                let painter = ui.painter();
                if hot > 0.01 {
                    painter.rect_filled(rect.shrink2(vec2(10.0, 2.0)), Rounding::same(RADIUS_SM), mix(BG_SIDE, SURFACE_HI, hot));
                }
                let thumb = Rect::from_min_size(rect.min + vec2(18.0, 8.0), vec2(28.0, 28.0));
                let seed = widgets::seed_color(&pl.id);
                painter.rect_filled(thumb, Rounding::same(RADIUS_SM), mix(seed, BG, 0.3));
                icons::paint(painter, thumb.shrink(7.0), Icon::Lyrics, mix(seed, T_PRI, 0.6));
                painter.text(
                    pos2(thumb.right() + 10.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    trunc(&pl.name, fit_chars(rect.width() - 80.0, TEXT_SM)),
                    font(TEXT_SM),
                    T_SEC,
                );
                if resp.clicked() {
                    cx.push(Action::OpenPlaylist(Some(pl.id.clone())));
                }
            }
        });
}

fn brand(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState, full: bool) {
    ui.horizontal(|ui| {
        ui.add_space(if full { 18.0 } else { 21.0 });
        match cx.logo {
            Some(logo) => {
                ui.add(
                    egui::Image::new(logo)
                        .fit_to_exact_size(vec2(30.0, 30.0))
                        .rounding(Rounding::same(RADIUS_SM)),
                );
            }
            None => {
                ui.label(RichText::new("♪").color(ACCENT).font(font(24.0)));
            }
        }
        if full {
            ui.add_space(8.0);
            ui.label(RichText::new("Meduza").color(T_PRI).font(font(TEXT_LG)).strong());
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                if icon_button(ui, Icon::ChevronLeft, 28.0, T_SEC, "Collapse sidebar").clicked() {
                    st.sidebar_expanded = false;
                }
            });
        }
    });
    if !full {
        ui.add_space(SPACE_SM);
        ui.horizontal(|ui| {
            ui.add_space(21.0);
            if icon_button(ui, Icon::ChevronRight, 28.0, T_SEC, "Expand sidebar").clicked() {
                st.sidebar_expanded = true;
            }
        });
    }
}

fn nav_item(ui: &mut egui::Ui, st: &mut UiState, tab: Tab, label: &str, icon: Icon, full: bool) {
    let active = st.tab == tab;
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let width = ui.available_width() - 12.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 42.0), Sense::click());
        let hot = ui.ctx().animate_bool(resp.id.with("hot"), resp.hovered());
        let painter = ui.painter();

        let fill = if active { SURFACE_PRESS } else { mix(BG_SIDE, SURFACE, hot) };
        painter.rect_filled(rect, Rounding::same(RADIUS_MD), fill);
        if active {
            let bar = Rect::from_min_max(rect.left_top() + vec2(0.0, 12.0), rect.left_bottom() - vec2(0.0, 12.0));
            let bar = Rect::from_min_max(bar.min, bar.min + vec2(3.0, bar.height()));
            painter.rect_filled(bar, Rounding::same(2.0), ACCENT);
        }

        let color = if active { T_PRI } else { T_SEC };
        let (icon_rect, text_x) = if full {
            (
                Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), vec2(18.0, 18.0)),
                rect.left() + 46.0,
            )
        } else {
            (Rect::from_center_size(rect.center(), vec2(18.0, 18.0)), 0.0)
        };
        icons::paint(painter, icon_rect, icon, if active { ACCENT } else { color });
        if full {
            painter.text(pos2(text_x, rect.center().y), Align2::LEFT_CENTER, label, font(TEXT_MD), color);
        }

        if resp.clicked() {
            st.tab = tab;
        }
        if !full {
            resp.on_hover_text(label);
        }
    });
    ui.add_space(2.0);
}

/// Top bar: page title on the left, global search on the right (hidden on the
/// Search page, which has its own large field).
pub fn topbar(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    let title = match st.tab {
        Tab::Home => "Home",
        Tab::Search => "Search",
        Tab::Library => "Your Library",
        Tab::Settings => "Settings",
    };

    egui::Frame::none()
        .inner_margin(Margin::symmetric(SPACE_XL, 0.0))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new(title).color(T_PRI).font(font(TEXT_LG)).strong());

                if st.tab == Tab::Search {
                    return;
                }
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    global_search(ui, cx, st);
                });
            });
        });
}

fn global_search(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    egui::Frame::none()
        .fill(SURFACE)
        .rounding(Rounding::same(RADIUS_PILL))
        .inner_margin(Margin::symmetric(14.0, 7.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (icon_rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                icons::paint(ui.painter(), icon_rect, Icon::Search, T_DIM);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut st.search_query)
                        .id(egui::Id::new("topbar_search"))
                        .hint_text("Search   Ctrl K")
                        .font(font(TEXT_SM))
                        .frame(false)
                        .desired_width(240.0),
                );
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let query = st.search_query.trim().to_string();
                    if !query.is_empty() {
                        cx.push(Action::Search(query));
                    }
                }
            });
        });
}
