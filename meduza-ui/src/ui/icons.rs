//! Vector icons drawn with the painter.
//!
//! Text glyphs (⏮, 🔀, ♥ …) depend on which fonts the system provides and render
//! inconsistently; drawing the shapes keeps every icon identical on every
//! machine and lets them scale cleanly with the UI zoom.

#![allow(dead_code)] // the full icon set is kept even where a view does not use every glyph

use std::f32::consts::PI;

use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Shape, Stroke};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Home,
    Search,
    Library,
    Settings,
    Play,
    Pause,
    Prev,
    Next,
    Shuffle,
    Repeat,
    RepeatOne,
    Heart,
    HeartOutline,
    Plus,
    Queue,
    Volume,
    Mute,
    Expand,
    Collapse,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    Lyrics,
    Close,
    More,
    Trash,
    Pencil,
    Export,
    Import,
    Shuffle2,
}

/// Paints `icon` centred in `rect`, sized to the smaller side of it.
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.5;
    let stroke = Stroke::new((s * 0.13).max(1.4), color);
    let fill = color;

    match icon {
        Icon::Home => {
            // House outline: roof apex, eaves, walls and door-less base.
            let pts = vec![
                pos(c, 0.0, -0.66, s),
                pos(c, 0.66, -0.02, s),
                pos(c, 0.46, -0.02, s),
                pos(c, 0.46, 0.62, s),
                pos(c, -0.46, 0.62, s),
                pos(c, -0.46, -0.02, s),
                pos(c, -0.66, -0.02, s),
            ];
            painter.add(Shape::closed_line(pts, stroke));
        }
        Icon::Search => {
            painter.circle_stroke(pos(c, -0.1, -0.1, s), s * 0.5, stroke);
            painter.line_segment([pos(c, 0.28, 0.28, s), pos(c, 0.62, 0.62, s)], stroke);
        }
        Icon::Library => {
            // Three upright "spines" of different heights.
            let w = s * 0.34;
            for (x, h) in [(-0.6, 0.85), (-0.12, 1.1), (0.36, 0.7)] {
                let r = Rect::from_center_size(pos(c, x + 0.17, 0.0, s), egui::vec2(w, h * s * 1.3));
                painter.rect_stroke(r, Rounding::same(s * 0.08), stroke);
            }
        }
        Icon::Settings => {
            // Three sliders with knobs at different positions.
            for (y, knob) in [(-0.5, -0.4), (0.0, 0.35), (0.5, -0.1)] {
                painter.line_segment([pos(c, -0.65, y, s), pos(c, 0.65, y, s)], stroke);
                painter.circle_filled(pos(c, knob, y, s), s * 0.17, fill);
            }
        }
        Icon::Play => {
            // Points are offset left so the triangle's visual mass, not its
            // bounding box, sits on the circle's centre.
            painter.add(Shape::convex_polygon(
                vec![pos(c, -0.4, -0.62, s), pos(c, 0.5, 0.0, s), pos(c, -0.4, 0.62, s)],
                fill,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            let bw = s * 0.3;
            for x in [-0.28, 0.28] {
                let r = Rect::from_center_size(pos(c, x, 0.0, s), egui::vec2(bw, s * 1.1));
                painter.rect_filled(r, Rounding::same(bw * 0.3), fill);
            }
        }
        Icon::Prev | Icon::Next => {
            let dir = if icon == Icon::Next { 1.0 } else { -1.0 };
            // A bar on the far edge plus a triangle pointing away from the centre
            // of the glyph: ⏭ for Next (▶|), ⏮ for Prev (|◀).
            let bar = Rect::from_center_size(pos(c, dir * 0.62, 0.0, s), egui::vec2(s * 0.2, s * 1.1));
            painter.rect_filled(bar, Rounding::same(s * 0.06), fill);
            painter.add(Shape::convex_polygon(
                vec![
                    pos(c, -dir * 0.55, -0.55, s),
                    pos(c, -dir * 0.55, 0.55, s),
                    pos(c, dir * 0.4, 0.0, s),
                ],
                fill,
                Stroke::NONE,
            ));
        }
        Icon::Shuffle | Icon::Shuffle2 => {
            painter.line_segment([pos(c, -0.6, -0.4, s), pos(c, 0.5, 0.4, s)], stroke);
            painter.line_segment([pos(c, -0.6, 0.4, s), pos(c, 0.5, -0.4, s)], stroke);
            arrow_head(painter, pos(c, 0.62, 0.45, s), 1.0, s, fill);
            arrow_head(painter, pos(c, 0.62, -0.45, s), -1.0, s, fill);
        }
        Icon::Repeat | Icon::RepeatOne => {
            let r = Rect::from_center_size(c, egui::vec2(s * 1.2, s * 0.95));
            painter.rect_stroke(r, Rounding::same(s * 0.3), stroke);
            // Arrow head on the upper-right corner.
            arrow_head(painter, pos(c, 0.6, -0.48, s), -1.0, s, fill);
            if icon == Icon::RepeatOne {
                painter.text(
                    c,
                    egui::Align2::CENTER_CENTER,
                    "1",
                    egui::FontId::proportional(s * 0.9),
                    fill,
                );
            }
        }
        Icon::Heart | Icon::HeartOutline => {
            let pts = heart_points(c, s);
            if icon == Icon::Heart {
                painter.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
            } else {
                painter.add(Shape::closed_line(pts, stroke));
            }
        }
        Icon::Plus => {
            painter.line_segment([pos(c, -0.6, 0.0, s), pos(c, 0.6, 0.0, s)], stroke);
            painter.line_segment([pos(c, 0.0, -0.6, s), pos(c, 0.0, 0.6, s)], stroke);
        }
        Icon::Queue => {
            for (y, len) in [(-0.5, 1.2), (0.0, 1.2), (0.5, 0.7)] {
                painter.line_segment([pos(c, -0.7, y, s), pos(c, -0.7 + len, y, s)], stroke);
            }
            painter.add(Shape::convex_polygon(
                vec![pos(c, 0.35, 0.3, s), pos(c, 0.75, 0.5, s), pos(c, 0.35, 0.7, s)],
                fill,
                Stroke::NONE,
            ));
        }
        Icon::Volume | Icon::Mute => {
            speaker(painter, c, s, fill);
            if icon == Icon::Mute {
                painter.line_segment([pos(c, 0.3, -0.4, s), pos(c, 0.8, 0.4, s)], stroke);
                painter.line_segment([pos(c, 0.3, 0.4, s), pos(c, 0.8, -0.4, s)], stroke);
            } else {
                let arc = |r: f32| -> Vec<Pos2> {
                    (0..=12)
                        .map(|i| {
                            let a = -0.6 + 1.2 * (i as f32 / 12.0);
                            pos(c, 0.35 + r * a.cos(), r * a.sin(), s)
                        })
                        .collect()
                };
                painter.add(Shape::line(arc(0.45), stroke));
                painter.add(Shape::line(arc(0.85), stroke));
            }
        }
        Icon::Expand => chevron(painter, c, s, stroke, ChevronDir::Up),
        Icon::Collapse | Icon::ChevronDown => chevron(painter, c, s, stroke, ChevronDir::Down),
        Icon::ChevronLeft => chevron(painter, c, s, stroke, ChevronDir::Left),
        Icon::ChevronRight => chevron(painter, c, s, stroke, ChevronDir::Right),
        Icon::Lyrics => {
            // Music note: head, stem and flag.
            painter.circle_filled(pos(c, -0.3, 0.4, s), s * 0.25, fill);
            painter.line_segment([pos(c, -0.05, 0.4, s), pos(c, -0.05, -0.55, s)], stroke);
            painter.line_segment([pos(c, -0.05, -0.55, s), pos(c, 0.45, -0.35, s)], stroke);
        }
        Icon::Close => {
            painter.line_segment([pos(c, -0.5, -0.5, s), pos(c, 0.5, 0.5, s)], stroke);
            painter.line_segment([pos(c, -0.5, 0.5, s), pos(c, 0.5, -0.5, s)], stroke);
        }
        Icon::More => {
            for x in [-0.55, 0.0, 0.55] {
                painter.circle_filled(pos(c, x, 0.0, s), s * 0.13, fill);
            }
        }
        Icon::Trash => {
            painter.line_segment([pos(c, -0.5, -0.4, s), pos(c, 0.5, -0.4, s)], stroke);
            painter.line_segment([pos(c, -0.15, -0.6, s), pos(c, 0.15, -0.6, s)], stroke);
            let body = Rect::from_min_max(pos(c, -0.4, -0.25, s), pos(c, 0.4, 0.6, s));
            painter.rect_stroke(body, Rounding::same(s * 0.08), stroke);
        }
        Icon::Pencil => {
            painter.line_segment([pos(c, -0.5, 0.5, s), pos(c, 0.4, -0.4, s)], stroke);
            painter.line_segment([pos(c, -0.5, 0.5, s), pos(c, -0.6, 0.6, s)], stroke);
            painter.line_segment([pos(c, 0.3, -0.5, s), pos(c, 0.5, -0.3, s)], stroke);
        }
        Icon::Export => {
            painter.line_segment([pos(c, 0.0, -0.55, s), pos(c, 0.0, 0.3, s)], stroke);
            arrow_head(painter, pos(c, 0.0, 0.45, s), 1.0, s, fill);
            painter.line_segment([pos(c, -0.5, 0.6, s), pos(c, 0.5, 0.6, s)], stroke);
        }
        Icon::Import => {
            painter.line_segment([pos(c, 0.0, -0.3, s), pos(c, 0.0, 0.5, s)], stroke);
            arrow_head(painter, pos(c, 0.0, -0.5, s), -1.0, s, fill);
            painter.line_segment([pos(c, -0.5, 0.6, s), pos(c, 0.5, 0.6, s)], stroke);
        }
    }
}

/// Animated three-bar equaliser used to mark the playing track.
pub fn equalizer(painter: &egui::Painter, rect: Rect, time: f64, color: Color32) {
    let bar_w = rect.width() / 5.0;
    for i in 0..3 {
        let phase = time * 4.0 + i as f64 * 1.7;
        let h = 0.35 + 0.65 * (0.5 + 0.5 * phase.sin()) as f32;
        let x = rect.left() + bar_w * (1.0 + i as f32 * 1.2);
        let r = Rect::from_min_max(
            Pos2::new(x, rect.bottom() - rect.height() * h),
            Pos2::new(x + bar_w, rect.bottom()),
        );
        painter.rect_filled(r, Rounding::same(bar_w * 0.4), color);
    }
}

#[derive(Clone, Copy)]
enum ChevronDir {
    Up,
    Down,
    Left,
    Right,
}

fn chevron(painter: &egui::Painter, c: Pos2, s: f32, stroke: Stroke, dir: ChevronDir) {
    let (a, b, d) = match dir {
        ChevronDir::Up => ((-0.5, 0.3), (0.0, -0.3), (0.5, 0.3)),
        ChevronDir::Down => ((-0.5, -0.3), (0.0, 0.3), (0.5, -0.3)),
        ChevronDir::Left => ((0.3, -0.5), (-0.3, 0.0), (0.3, 0.5)),
        ChevronDir::Right => ((-0.3, -0.5), (0.3, 0.0), (-0.3, 0.5)),
    };
    painter.add(Shape::line(
        vec![pos(c, a.0, a.1, s), pos(c, b.0, b.1, s), pos(c, d.0, d.1, s)],
        stroke,
    ));
}

fn speaker(painter: &egui::Painter, c: Pos2, s: f32, fill: Color32) {
    let body = vec![
        pos(c, -0.7, -0.2, s),
        pos(c, -0.35, -0.2, s),
        pos(c, 0.0, -0.55, s),
        pos(c, 0.0, 0.55, s),
        pos(c, -0.35, 0.2, s),
        pos(c, -0.7, 0.2, s),
    ];
    painter.add(Shape::convex_polygon(body, fill, Stroke::NONE));
}

/// Small filled triangle pointing up (`dir < 0`) or down (`dir > 0`).
fn arrow_head(painter: &egui::Painter, tip: Pos2, dir: f32, s: f32, fill: Color32) {
    let h = s * 0.3;
    let base_y = tip.y - dir * h;
    painter.add(Shape::convex_polygon(
        vec![
            Pos2::new(tip.x - h * 0.7, base_y),
            Pos2::new(tip.x + h * 0.7, base_y),
            tip,
        ],
        fill,
        Stroke::NONE,
    ));
}

fn heart_points(c: Pos2, s: f32) -> Vec<Pos2> {
    (0..48)
        .map(|i| {
            let t = i as f32 / 48.0 * 2.0 * PI;
            let x = 16.0 * t.sin().powi(3);
            let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
            Pos2::new(c.x + x / 17.0 * s * 0.95, c.y - y / 17.0 * s * 0.95 + s * 0.08)
        })
        .collect()
}

/// Point at (`x`, `y`) in units of the icon half-size `s`, offset from `c`.
fn pos(c: Pos2, x: f32, y: f32, s: f32) -> Pos2 {
    Pos2::new(c.x + x * s, c.y + y * s)
}
