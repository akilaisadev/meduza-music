//! Reusable building blocks: artwork, cards, track rows, controls and states.
//!
//! Overlays (icon buttons on a row, the play badge on a card) are placed with
//! `ui.interact` so they never move the layout cursor.

use eframe::egui::{self, Align2, Color32, CursorIcon, Frame, Margin, Rect, Rounding, Sense};
use eframe::egui::{pos2, vec2, RichText};

use meduza_core::innertube::TrackItem;

use super::icons::{self, Icon};
use super::{fit_chars, trunc, Action, Cx};
use crate::theme::*;

/// Muted palette used for placeholder artwork and generated tiles.
const SEEDS: [Color32; 8] = [
    Color32::from_rgb(74, 48, 120),
    Color32::from_rgb(30, 90, 140),
    Color32::from_rgb(130, 50, 70),
    Color32::from_rgb(30, 110, 95),
    Color32::from_rgb(150, 90, 30),
    Color32::from_rgb(60, 100, 50),
    Color32::from_rgb(120, 40, 110),
    Color32::from_rgb(40, 70, 130),
];

/// Starts resolving a track's stream once the cursor has rested on it for a
/// moment, so clicking it plays without waiting for yt-dlp. Sweeping across a
/// list does not trigger it, and each hover prefetches at most once.
pub fn prefetch_on_dwell(ui: &egui::Ui, cx: &Cx, id: egui::Id, hovered: bool, track: &TrackItem) {
    const DWELL_SECS: f64 = 0.25;
    let key = id.with("dwell");
    let ctx = ui.ctx();
    if !hovered {
        ctx.data_mut(|d| d.remove_temp::<f64>(key));
        return;
    }
    let now = ui.input(|i| i.time);
    let started = ctx.data_mut(|d| *d.get_temp_mut_or(key, now));
    if started < 0.0 {
        return; // already prefetched during this hover
    }
    if now - started >= DWELL_SECS {
        cx.vm.playback.prefetch(track);
        ctx.data_mut(|d| d.insert_temp(key, -1.0));
    } else {
        ctx.request_repaint_after(std::time::Duration::from_millis(260));
    }
}

/// Stable colour for an id, so the same playlist always gets the same tile.
pub fn seed_color(id: &str) -> Color32 {
    let mut hash: u32 = 2_166_136_261;
    for b in id.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    SEEDS[hash as usize % SEEDS.len()]
}

/// Animation factor (0..1) for a hover state, keyed by a widget id.
fn hot(ui: &egui::Ui, id: egui::Id, on: bool) -> f32 {
    ui.ctx().animate_bool(id.with("hot"), on)
}

/// Vertical gradient over a rectangle (sharp corners; use for full-bleed areas).
pub fn vertical_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Draws artwork for `id` into `rect`, or a tinted placeholder while it loads.
pub fn art(ui: &mut egui::Ui, cx: &mut Cx, id: &str, url: &str, rect: Rect, rounding: f32) {
    let max_dim = cx.max_dim;
    if let Some(tex) = cx.images.texture(ui.ctx(), id, url, max_dim) {
        // Centre-crop to the frame instead of stretching non-square artwork.
        let size = tex.size_vec2();
        let uv = if size.x > size.y {
            let m = (1.0 - size.y / size.x) / 2.0;
            Rect::from_min_max(pos2(m, 0.0), pos2(1.0 - m, 1.0))
        } else if size.y > 0.0 {
            let m = (1.0 - size.x / size.y) / 2.0;
            Rect::from_min_max(pos2(0.0, m), pos2(1.0, 1.0 - m))
        } else {
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
        };
        ui.put(
            rect,
            egui::Image::new(tex)
                .uv(uv)
                .fit_to_exact_size(rect.size())
                .rounding(Rounding::same(rounding)),
        );
        return;
    }
    let seed = seed_color(id);
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(rounding), mix(seed, SURFACE, 0.55));
    icons::paint(
        painter,
        Rect::from_center_size(rect.center(), vec2(rect.width() * 0.4, rect.height() * 0.4)),
        Icon::Lyrics,
        mix(seed, T_PRI, 0.55),
    );
}

/// Large square tile with a gradient and a title, used for playlists and genres.
pub fn tile(ui: &mut egui::Ui, size: egui::Vec2, id: &str, title: &str, subtitle: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let h = hot(ui, resp.id, resp.hovered());
    let seed = seed_color(id);
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(RADIUS_LG), SURFACE);
    let inner = rect.shrink(1.0);
    let top = mix(seed, BG, 0.25 - 0.1 * h);
    let bottom = mix(seed, BG, 0.75);
    vertical_gradient(painter, inner, top, bottom);
    painter.text(
        rect.left_bottom() + vec2(16.0, -44.0),
        Align2::LEFT_BOTTOM,
        trunc(title, fit_chars(rect.width() - 32.0, TEXT_LG)),
        font(TEXT_LG),
        T_PRI,
    );
    painter.text(
        rect.left_bottom() + vec2(16.0, -22.0),
        Align2::LEFT_BOTTOM,
        trunc(subtitle, fit_chars(rect.width() - 32.0, TEXT_SM)),
        font(TEXT_SM),
        mix(T_PRI, T_DIM, 0.3),
    );
    if h > 0.01 {
        let c = pos2(rect.right() - 30.0, rect.top() + 30.0);
        painter.circle_filled(c, 20.0 * h, ACCENT);
        icons::paint(painter, Rect::from_center_size(c, vec2(18.0, 18.0)), Icon::Play, BG);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Album-style card used in shelves. Click plays the track (radio queue).
pub fn track_card(ui: &mut egui::Ui, cx: &mut Cx, track: &TrackItem, width: f32) -> egui::Response {
    let height = width + 60.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let h = hot(ui, resp.id, resp.hovered());
    prefetch_on_dwell(ui, cx, resp.id, resp.hovered(), track);
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(RADIUS_LG), mix(SURFACE, SURFACE_HI, h));

    let pad = 12.0;
    let art_rect = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(width - pad * 2.0, width - pad * 2.0));
    art(ui, cx, &track.media_id, &track.thumbnail_url, art_rect, RADIUS_MD);

    if h > 0.01 {
        let c = art_rect.right_bottom() - vec2(22.0, 22.0);
        let painter = ui.painter();
        painter.circle_filled(c, 20.0 * h.max(0.6), ACCENT);
        icons::paint(painter, Rect::from_center_size(c, vec2(16.0, 16.0)), Icon::Play, BG);
    }

    let painter = ui.painter();
    let title_col = if cx.is_now(&track.media_id) { cx.accent } else { T_PRI };
    let text_w = width - pad * 2.0;
    painter.text(
        pos2(rect.left() + pad, art_rect.bottom() + 12.0),
        Align2::LEFT_TOP,
        trunc(&track.title, fit_chars(text_w, TEXT_MD)),
        font(TEXT_MD),
        title_col,
    );
    painter.text(
        pos2(rect.left() + pad, art_rect.bottom() + 32.0),
        Align2::LEFT_TOP,
        trunc(&track.artist, fit_chars(text_w, TEXT_SM)),
        font(TEXT_SM),
        T_SEC,
    );

    if resp.clicked() {
        cx.push(Action::Play(track.clone()));
    }
    resp.context_menu(|ui| track_menu(ui, cx, track, None));
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Horizontal shelf of cards with a heading.
pub fn shelf(ui: &mut egui::Ui, cx: &mut Cx, id: &str, title: &str, tracks: &[TrackItem]) {
    heading(ui, title);
    ui.add_space(SPACE_SM);
    egui::ScrollArea::horizontal()
        .id_source(id)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for track in tracks.iter().take(30) {
                    track_card(ui, cx, track, CARD_W);
                    ui.add_space(SPACE_MD);
                }
            });
        });
    ui.add_space(SPACE_XL);
}

/// Placeholder cards shown while a shelf is loading.
pub fn skeleton_shelf(ui: &mut egui::Ui, time: f64, title_w: f32, count: usize) {
    let (rect, _) = ui.allocate_exact_size(vec2(title_w, 22.0), Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(RADIUS_SM), SURFACE);
    ui.add_space(SPACE_SM);
    ui.horizontal(|ui| {
        for i in 0..count {
            let (rect, _) = ui.allocate_exact_size(vec2(CARD_W, CARD_W + 60.0), Sense::hover());
            let phase = (time * 1.6 + i as f64 * 0.35).sin() as f32 * 0.5 + 0.5;
            ui.painter()
                .rect_filled(rect, Rounding::same(RADIUS_LG), mix(SURFACE, SURFACE_HI, phase));
            ui.add_space(SPACE_MD);
        }
    });
    ui.add_space(SPACE_XL);
}

/// Rows of tracks. Clicking a row plays the list from that row. `playlist` is
/// set when the list is a playlist, enabling "Remove from playlist".
pub fn track_list(ui: &mut egui::Ui, cx: &mut Cx, tracks: &[TrackItem], playlist: Option<&str>) {
    for (i, track) in tracks.iter().enumerate() {
        track_row(ui, cx, tracks, i, track, playlist);
    }
}

fn track_row(
    ui: &mut egui::Ui,
    cx: &mut Cx,
    list: &[TrackItem],
    index: usize,
    track: &TrackItem,
    playlist: Option<&str>,
) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let h = hot(ui, resp.id, resp.hovered());
    prefetch_on_dwell(ui, cx, resp.id, resp.hovered(), track);
    let is_now = cx.is_now(&track.media_id);
    let painter = ui.painter();

    let base = if is_now { SURFACE } else { BG };
    painter.rect_filled(rect, Rounding::same(RADIUS_MD), mix(base, SURFACE_HI, h));

    // Leading column: index, play glyph, or live equaliser for the current track.
    let lead = Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), vec2(18.0, 18.0));
    if is_now && cx.playing {
        icons::equalizer(painter, lead, cx.time, cx.accent);
    } else if is_now {
        icons::paint(painter, lead, Icon::Pause, cx.accent);
    } else if h > 0.01 {
        icons::paint(painter, lead, Icon::Play, T_PRI);
    } else {
        painter.text(lead.center(), Align2::CENTER_CENTER, format!("{}", index + 1), font(TEXT_SM), T_DIM);
    }

    let art_rect = Rect::from_min_size(
        pos2(rect.left() + 46.0, rect.center().y - ROW_ART / 2.0),
        vec2(ROW_ART, ROW_ART),
    );
    art(ui, cx, &track.media_id, &track.thumbnail_url, art_rect, RADIUS_SM);

    let painter = ui.painter();
    let text_x = art_rect.right() + 14.0;
    let text_w = (rect.right() - 260.0 - text_x).max(80.0);
    let title_col = if is_now { cx.accent } else { T_PRI };
    painter.text(
        pos2(text_x, rect.center().y - 10.0),
        Align2::LEFT_CENTER,
        trunc(&track.title, fit_chars(text_w, TEXT_MD)),
        font(TEXT_MD),
        title_col,
    );
    painter.text(
        pos2(text_x, rect.center().y + 10.0),
        Align2::LEFT_CENTER,
        trunc(&track.artist, fit_chars(text_w, TEXT_SM)),
        font(TEXT_SM),
        T_SEC,
    );

    let painter = ui.painter();
    if track.duration_seconds > 0 {
        painter.text(
            pos2(rect.right() - 120.0, rect.center().y),
            Align2::RIGHT_CENTER,
            format!("{:02}:{:02}", track.duration_seconds / 60, track.duration_seconds % 60),
            font(TEXT_SM),
            T_DIM,
        );
    }

    let liked = cx.is_liked(&track.media_id);
    let heart_rect = Rect::from_center_size(pos2(rect.right() - 80.0, rect.center().y), vec2(28.0, 28.0));
    if overlay_icon(ui, heart_rect, resp.id.with("heart"), if liked { Icon::Heart } else { Icon::HeartOutline },
        if liked { LIKE } else { T_SEC }, if liked { "Remove from Liked" } else { "Save to Liked" })
        .clicked()
    {
        cx.push(Action::ToggleLike(track.clone()));
    }

    let more_rect = Rect::from_center_size(pos2(rect.right() - 40.0, rect.center().y), vec2(28.0, 28.0));
    let more = overlay_icon(ui, more_rect, resp.id.with("more"), Icon::More, T_SEC, "More");
    let menu_id = resp.id.with("menu");
    if more.clicked() {
        ui.memory_mut(|m| m.toggle_popup(menu_id));
    }
    egui::popup::popup_below_widget(ui, menu_id, &more, |ui| {
        ui.set_min_width(220.0);
        track_menu(ui, cx, track, playlist);
    });

    if resp.clicked() {
        cx.push(Action::PlayFrom { tracks: list.to_vec(), start: index, shuffle: false });
    }
    resp.context_menu(|ui| track_menu(ui, cx, track, playlist));
}

/// Context menu shared by cards, rows and the player.
pub fn track_menu(ui: &mut egui::Ui, cx: &mut Cx, track: &TrackItem, playlist: Option<&str>) {
    ui.set_min_width(200.0);
    if ui.button("Play now").clicked() {
        cx.push(Action::Play(track.clone()));
        ui.close_menu();
    }
    if ui.button("Play next").clicked() {
        cx.push(Action::QueueNext(track.clone()));
        ui.close_menu();
    }
    if ui.button("Add to queue").clicked() {
        cx.push(Action::QueueLast(track.clone()));
        ui.close_menu();
    }
    ui.separator();
    let liked = cx.is_liked(&track.media_id);
    if ui.button(if liked { "Remove from Liked" } else { "Save to Liked" }).clicked() {
        cx.push(Action::ToggleLike(track.clone()));
        ui.close_menu();
    }
    let snap = cx.snap;
    ui.menu_button("Add to playlist", |ui| {
        if snap.playlists.is_empty() {
            ui.label(RichText::new("No playlists yet").color(T_DIM));
        }
        for pl in &snap.playlists {
            if ui.button(trunc(&pl.name, 28)).clicked() {
                cx.push(Action::AddToPlaylist { playlist: pl.id.clone(), track: track.clone() });
                ui.close_menu();
            }
        }
    });
    if let Some(pid) = playlist {
        if ui.button("Remove from playlist").clicked() {
            cx.push(Action::RemoveFromPlaylist { playlist: pid.to_string(), media_id: track.media_id.clone() });
            ui.close_menu();
        }
    }
}


/// Small icon button placed over an existing row/card without moving layout.
pub fn overlay_icon(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    icon: Icon,
    color: Color32,
    tip: &str,
) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = hot(ui, id, resp.hovered());
    let painter = ui.painter();
    if h > 0.01 {
        painter.circle_filled(rect.center(), rect.width() * 0.5, mix(SURFACE, SURFACE_PRESS, h));
    }
    icons::paint(painter, rect.shrink(rect.width() * 0.25), icon, color);
    resp.on_hover_text(tip)
}

/// Round icon button for transport and toolbars.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let h = hot(ui, resp.id, resp.hovered());
    let painter = ui.painter();
    if h > 0.01 {
        painter.circle_filled(rect.center(), size * 0.5, mix(BG_PLAYER, SURFACE_HI, h));
    }
    icons::paint(painter, rect.shrink(size * 0.2), icon, color);
    resp.on_hover_text(tip)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnStyle {
    Primary,
    Secondary,
    Danger,
}

/// Text pill button with an optional leading icon.
pub fn pill(ui: &mut egui::Ui, label: &str, style: BtnStyle, icon: Option<Icon>) -> egui::Response {
    let (fill, text_col) = match style {
        BtnStyle::Primary => (ACCENT, BG),
        BtnStyle::Secondary => (SURFACE_PRESS, T_PRI),
        BtnStyle::Danger => (mix(BG, DANGER, 0.22), DANGER),
    };
    let text_size = ui
        .fonts(|f| f.layout_no_wrap(label.to_string(), font(TEXT_SM), text_col))
        .size();
    let icon_w = if icon.is_some() { 18.0 } else { 0.0 };
    let width = text_size.x + icon_w + 34.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 34.0), Sense::click());
    let h = hot(ui, resp.id, resp.hovered());
    let fill = mix(fill, Color32::WHITE, 0.12 * h);
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(RADIUS_PILL), fill);
    let mut x = rect.center().x - (text_size.x + icon_w) / 2.0;
    if let Some(icon) = icon {
        icons::paint(painter, Rect::from_center_size(pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0)), icon, text_col);
        x += icon_w;
    }
    painter.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, font(TEXT_SM), text_col);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Horizontal progress/volume bar. Returns the fraction (0..1) while the user
/// clicks or drags it.
pub fn bar(ui: &mut egui::Ui, width: f32, ratio: f32, accent: Color32) -> Option<f32> {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 18.0), Sense::click_and_drag());
    let h = hot(ui, resp.id, resp.hovered() || resp.dragged());
    let thick = 4.0 + 2.0 * h;
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), thick));
    let ratio = ratio.clamp(0.0, 1.0);
    let painter = ui.painter();
    painter.rect_filled(track, Rounding::same(thick / 2.0), SURFACE_PRESS);
    let fill = Rect::from_min_size(track.min, vec2(track.width() * ratio, thick));
    painter.rect_filled(fill, Rounding::same(thick / 2.0), if h > 0.01 { accent } else { mix(accent, T_PRI, 0.2) });
    if h > 0.01 {
        painter.circle_filled(fill.right_center(), 3.0 + 4.0 * h, T_PRI);
    }
    if resp.clicked() || resp.dragged() {
        if let Some(p) = resp.interact_pointer_pos() {
            return Some(((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
        }
    }
    None
}

/// Settings row with a switch. Returns true when the value was toggled.
pub fn switch_row(ui: &mut egui::Ui, title: &str, desc: &str, value: &mut bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
    let h = hot(ui, resp.id, resp.hovered());
    let knob_t = ui.ctx().animate_bool(resp.id.with("knob"), *value);
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(RADIUS_MD), mix(SURFACE, SURFACE_HI, h * 0.6));
    painter.text(rect.left_top() + vec2(18.0, 14.0), Align2::LEFT_TOP, title, font(TEXT_MD), T_PRI);
    painter.text(rect.left_top() + vec2(18.0, 36.0), Align2::LEFT_TOP, desc, font(TEXT_SM), T_SEC);

    let pill_rect = Rect::from_min_size(pos2(rect.right() - 68.0, rect.center().y - 12.0), vec2(46.0, 24.0));
    painter.rect_filled(pill_rect, Rounding::same(12.0), mix(SURFACE_PRESS, ACCENT, knob_t));
    let knob_x = pill_rect.left() + 12.0 + (pill_rect.width() - 24.0) * knob_t;
    painter.circle_filled(pos2(knob_x, pill_rect.center().y), 9.0, if knob_t > 0.5 { BG } else { T_SEC });

    if resp.clicked() {
        *value = !*value;
        return true;
    }
    false
}

/// Segmented control. Returns the index picked this frame.
pub fn segmented(ui: &mut egui::Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let mut picked = None;
    Frame::none()
        .fill(SURFACE)
        .rounding(Rounding::same(RADIUS_PILL))
        .inner_margin(Margin::same(4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, label) in labels.iter().enumerate() {
                    let on = i == selected;
                    let text = RichText::new(*label)
                        .font(font(TEXT_SM))
                        .color(if on { BG } else { T_SEC });
                    let btn = egui::Button::new(text)
                        .fill(if on { T_PRI } else { Color32::TRANSPARENT })
                        .rounding(Rounding::same(RADIUS_PILL))
                        .min_size(vec2(0.0, 28.0));
                    if ui.add(btn).clicked() && !on {
                        picked = Some(i);
                    }
                }
            });
        });
    picked
}

/// Card container for settings groups.
pub fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    Frame::none()
        .fill(SURFACE)
        .rounding(Rounding::same(RADIUS_LG))
        .inner_margin(Margin::symmetric(10.0, 10.0))
        .show(ui, add);
}

pub fn heading(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).font(font(TEXT_LG)).color(T_PRI).strong());
}

pub fn page_title(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).font(font(TEXT_HERO)).color(T_PRI).strong());
}

pub fn empty_state(ui: &mut egui::Ui, icon: Icon, title: &str, body: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(64.0);
        let (rect, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
        icons::paint(ui.painter(), rect, icon, T_DIM);
        ui.add_space(12.0);
        ui.label(RichText::new(title).font(font(TEXT_LG)).color(T_PRI));
        ui.label(RichText::new(body).font(font(TEXT_SM)).color(T_DIM));
    });
}
