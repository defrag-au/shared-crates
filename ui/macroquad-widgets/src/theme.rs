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
/// Warning / caution (amber) — ineligible notices etc.
pub const WARN: Color = Color::new(0.878, 0.686, 0.408, 1.0);
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

// ============================================================================
// Theme — the runtime-swappable palette carried by `Painter`
// ============================================================================

/// A full palette. Carried on [`crate::Painter`] so widgets read `p.theme.*`
/// and the whole UI can be re-skinned by swapping one value — no per-widget
/// colour constants. The module-level consts above are the `tokyo_night`
/// defaults; presets vary the accent (and link) over the same neutral dark base.
#[derive(Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub panel: Color,
    pub accent: Color,
    pub link: Color,
    pub fg: Color,
    pub muted: Color,
    pub danger: Color,
    pub warn: Color,
    pub track: Color,
}

impl Theme {
    pub fn tokyo_night() -> Self {
        Self {
            name: "tokyo night",
            bg: BG,
            panel: PANEL,
            accent: ACCENT,
            link: LINK,
            fg: FG,
            muted: MUTED,
            danger: DANGER,
            warn: WARN,
            track: TRACK,
        }
    }

    /// Same neutral dark base, different accent.
    fn with_accent(name: &'static str, accent: Color) -> Self {
        Self {
            name,
            accent,
            ..Self::tokyo_night()
        }
    }

    pub fn ember() -> Self {
        Self::with_accent("ember", Color::new(0.964, 0.620, 0.300, 1.0))
    }
    pub fn iris() -> Self {
        Self::with_accent("iris", Color::new(0.733, 0.604, 0.969, 1.0))
    }
    pub fn aqua() -> Self {
        Self::with_accent("aqua", Color::new(0.486, 0.808, 1.0, 1.0))
    }
    pub fn rose() -> Self {
        Self::with_accent("rose", Color::new(0.969, 0.463, 0.557, 1.0))
    }

    /// All presets, for a theme switcher.
    pub const PRESETS: &'static [fn() -> Theme] =
        &[Theme::tokyo_night, Theme::ember, Theme::iris, Theme::aqua, Theme::rose];
}

impl Default for Theme {
    fn default() -> Self {
        Self::tokyo_night()
    }
}
