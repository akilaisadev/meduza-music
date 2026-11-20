#![allow(dead_code)] // tokens are shared vocabulary; not every token is used everywhere

//! Design tokens for the Meduza UI.
//!
//! Colours, radii, spacing and type sizes live here so the whole app shares one
//! visual language. Surfaces are layered (base → side → card → hover) instead of
//! using flat borders, and the accent can be re-tinted per track from its artwork.

use eframe::egui::{Color32, FontFamily, FontId};

// ── Surfaces (darkest → lightest) ────────────────────────────────────────────
pub const BG: Color32 = Color32::from_rgb(9, 9, 11);
pub const BG_SIDE: Color32 = Color32::from_rgb(14, 14, 17);
pub const BG_PLAYER: Color32 = Color32::from_rgb(16, 16, 20);
pub const SURFACE: Color32 = Color32::from_rgb(22, 22, 27);
pub const SURFACE_HI: Color32 = Color32::from_rgb(31, 31, 38);
pub const SURFACE_PRESS: Color32 = Color32::from_rgb(40, 40, 48);
pub const HAIRLINE: Color32 = Color32::from_rgb(36, 36, 44);

// ── Accent & status ──────────────────────────────────────────────────────────
pub const ACCENT: Color32 = Color32::from_rgb(30, 215, 96);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(30, 215, 96);
pub const DANGER: Color32 = Color32::from_rgb(255, 92, 92);
pub const WARN: Color32 = Color32::from_rgb(255, 184, 77);
pub const LIKE: Color32 = Color32::from_rgb(255, 64, 110);

// ── Text ─────────────────────────────────────────────────────────────────────
pub const T_PRI: Color32 = Color32::from_rgb(242, 242, 245);
pub const T_SEC: Color32 = Color32::from_rgb(170, 170, 180);
pub const T_DIM: Color32 = Color32::from_rgb(112, 112, 124);

// ── Geometry ─────────────────────────────────────────────────────────────────
pub const RADIUS_SM: f32 = 6.0;
pub const RADIUS_MD: f32 = 10.0;
pub const RADIUS_LG: f32 = 16.0;
pub const RADIUS_PILL: f32 = 99.0;

pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 20.0;
pub const SPACE_XL: f32 = 32.0;

pub const SIDEBAR_FULL: f32 = 236.0;
pub const SIDEBAR_RAIL: f32 = 72.0;
pub const TOPBAR_H: f32 = 60.0;
pub const PLAYER_H: f32 = 96.0;
pub const QUEUE_W: f32 = 320.0;

/// Shelf / grid card artwork size (logical px before zoom).
pub const CARD_W: f32 = 170.0;
/// Track-row artwork size.
pub const ROW_ART: f32 = 44.0;
pub const ROW_H: f32 = 60.0;

// ── Scale ────────────────────────────────────────────────────────────────────
/// Default UI zoom. Kept below 1.0 so type and cards open at a compact size;
/// pinch / Ctrl+scroll scales from here and Ctrl+0 returns to it.
pub const DEFAULT_ZOOM: f32 = 0.8;
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 2.0;

// ── Type scale ───────────────────────────────────────────────────────────────
pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub const TEXT_XS: f32 = 11.0;
pub const TEXT_SM: f32 = 12.5;
pub const TEXT_MD: f32 = 14.0;
pub const TEXT_LG: f32 = 17.0;
pub const TEXT_XL: f32 = 22.0;
pub const TEXT_HERO: f32 = 34.0;

// ── Colour helpers ───────────────────────────────────────────────────────────

/// Linear blend of two colours; `t = 0` gives `a`, `t = 1` gives `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Scales a colour's brightness; `k < 1` darkens.
pub fn shade(c: Color32, k: f32) -> Color32 {
    let s = |x: u8| (x as f32 * k).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(s(c.r()), s(c.g()), s(c.b()))
}

/// Perceived brightness in 0..=1.
pub fn luma(c: Color32) -> f32 {
    (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
}

/// Makes an accent usable on a dark UI: lifts very dark colours so the progress
/// bar, play button and highlighted titles stay readable.
pub fn readable_accent(c: Color32) -> Color32 {
    let l = luma(c);
    if l >= 0.45 {
        return c;
    }
    mix(c, Color32::WHITE, (0.45 - l) * 2.2)
}
