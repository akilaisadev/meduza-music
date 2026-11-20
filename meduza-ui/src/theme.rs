#![allow(dead_code)] // tokens are shared vocabulary; not all used in every pass

//! Design tokens for the Meduza UI.
//!
//! Keeping colors, radii and spacing in one place lets the whole app get a
//! coherent "modern Linux desktop" look without touching dozens of literals.

use eframe::egui::Color32;

// ── Semantic palette ─────────────────────────────────────────────────────────
pub const BG: Color32 = Color32::from_rgb(10, 10, 12);
pub const BG_SIDE: Color32 = Color32::from_rgb(15, 15, 18);
pub const BG_TOP: Color32 = Color32::from_rgb(13, 13, 16);
pub const BG_CARD: Color32 = Color32::from_rgb(20, 20, 24);
pub const BG_CARD_HV: Color32 = Color32::from_rgb(28, 28, 34);
pub const SURFACE: Color32 = Color32::from_rgb(18, 18, 22);
pub const SURFACE_RAISED: Color32 = Color32::from_rgb(24, 24, 30);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(34, 34, 42);
pub const BORDER: Color32 = Color32::from_rgb(38, 38, 46);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(58, 58, 70);

pub const ACCENT: Color32 = Color32::from_rgb(29, 185, 84);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(48, 220, 112);
pub const DANGER: Color32 = Color32::from_rgb(255, 90, 90);

pub const T_PRI: Color32 = Color32::from_rgb(245, 245, 247);
pub const T_SEC: Color32 = Color32::from_rgb(165, 165, 175);
pub const T_DIM: Color32 = Color32::from_rgb(105, 105, 115);

// ── Geometry ─────────────────────────────────────────────────────────────────
pub const RADIUS_SM: f32 = 6.0;
pub const RADIUS_MD: f32 = 10.0;
pub const RADIUS_LG: f32 = 14.0;
pub const RADIUS_PILL: f32 = 22.0;

pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 20.0;
pub const SPACE_XL: f32 = 28.0;

pub const SIDEBAR_FULL: f32 = 220.0;
pub const SIDEBAR_RAIL: f32 = 68.0;
