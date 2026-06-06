//! Shared palette — the Tokyo-Night-ish dark theme the txmints macroquad
//! surfaces use (one vivid accent). Mirrors `egui-widgets`'s palette intent so
//! the two renderers feel like one product.

use macroquad::prelude::Color;

/// Page background.
pub const BG: Color = Color::new(0.039, 0.039, 0.102, 1.0);
/// Raised panel / card fill.
pub const PANEL: Color = Color::new(0.078, 0.086, 0.149, 1.0);
/// Primary accent (lime green) — progress, CTAs, the live heartbeat.
pub const ACCENT: Color = Color::new(0.620, 0.808, 0.416, 1.0);
/// Link / tappable tx colour (blue).
pub const LINK: Color = Color::new(0.478, 0.635, 0.968, 1.0);
/// Primary text.
pub const FG: Color = Color::new(0.752, 0.792, 0.960, 1.0);
/// Secondary / muted text.
pub const MUTED: Color = Color::new(0.470, 0.510, 0.667, 1.0);
/// Error / danger.
pub const DANGER: Color = Color::new(0.969, 0.463, 0.557, 1.0);
/// Inactive track (progress background, disabled button).
pub const TRACK: Color = Color::new(0.160, 0.180, 0.260, 1.0);

/// Return `color` with its alpha replaced — for the pulsing heartbeat dot.
pub fn with_alpha(color: Color, a: f32) -> Color {
    Color::new(color.r, color.g, color.b, a)
}

/// Scale a colour's brightness — `f > 1.0` lightens (hover), `< 1.0` darkens
/// (pressed). Alpha is preserved.
pub fn shade(color: Color, f: f32) -> Color {
    Color::new(
        (color.r * f).clamp(0.0, 1.0),
        (color.g * f).clamp(0.0, 1.0),
        (color.b * f).clamp(0.0, 1.0),
        color.a,
    )
}
