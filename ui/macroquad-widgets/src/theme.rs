//! Shared palette — the Tokyo-Night-ish dark theme the txmints macroquad
//! surfaces use (one vivid accent).
//!
//! ## The values live in `ui-theme`
//!
//! They used to be ten `Color` literals here and eighteen more in
//! `egui-widgets`, and MEASURED 2026-09-16 the two had already drifted: six
//! colours byte-identical, three diverged (`bg`, `panel`, `muted`), and
//! `success` resolving to a different hue on each side. Nobody decided that; it
//! is what one palette maintained twice does.
//!
//! So the numbers now come from `ui_theme::tokens::raw`, through the `Paint`
//! bridge. **`Theme` keeps its own flat ten fields on purpose** — `p.theme.muted`
//! reads at ~191 call sites across four crates, and renaming them to
//! `ColorTokens`' spelling would be a rewrite that buys nothing this step. What
//! changed is where the values come from, and that
//! `tests/contrast.rs` now holds every preset to a floor.
//!
//! ## Where this deliberately differs from egui
//!
//! - **`success` is teal, not the green ramp.** See [`Theme::success`].
//! - **`bg` / `panel` are darker.** macroquad clears the canvas to its own
//!   background, and the pump's shell HTML matches it; `ui-theme` carries both
//!   as named values (`BG_NEAR_BLACK`, `PANEL_NEAR_BLACK`) so neither side is
//!   guessing.
//!
//! Both are recorded decisions with a constant naming them, which is the
//! difference between a divergence and a drift.

use macroquad::prelude::Color;
use ui_theme::{Paint, tokens::raw};

/// One shared value as this renderer's colour.
///
/// Not `const`: crossing the `Paint` bridge is a trait call. Nothing outside
/// this module read the old constants — the crate root re-exports only
/// [`Theme`] — so they are simply gone rather than kept as `LazyLock`.
fn c(value: ui_theme::Srgb) -> Color {
    Color::from_srgb(value)
}

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
/// colour constants.
///
/// The values come from `ui_theme::tokens::raw` (see the module header for why
/// they are no longer literals here). Presets vary the **accent alone** over the
/// same neutral dark base, which `tests/contrast.rs` asserts — along with a
/// floor for every tier on every surface.
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
    /// Success / settled (teal) — a thing that landed and is now true.
    ///
    /// **Deliberately NOT the accent, and deliberately not egui's green.**
    /// Accent is swapped per preset (lime, ember, iris, aqua, rose), and
    /// surfaces that already spend accent on something else — `block_train`
    /// gives it to the newest block — would otherwise render "your transaction
    /// landed" in the same ink as "this block is newest", and in a different
    /// hue on every skin. Fixed across presets on purpose, and carried in the
    /// shared crate as `raw::SETTLED_TEAL` so the divergence from egui's
    /// `success` is a recorded decision rather than drift.
    pub success: Color,
    pub track: Color,
}

impl Theme {
    pub fn tokyo_night() -> Self {
        Self {
            name: "tokyo night",
            // Darker than egui's `bg_primary`: this is the colour the canvas
            // clears to, and the pump's shell HTML matches it.
            bg: c(raw::BG_NEAR_BLACK),
            panel: c(raw::PANEL_NEAR_BLACK),
            // The lime the presets swap. Shared with egui's `accent_green`.
            accent: c(raw::ACCENT_GREEN),
            link: c(raw::ACCENT_BLUE),
            fg: c(raw::TEXT_PRIMARY),
            // ⚠️ egui's contrast-tuned tier, NOT macroquad's old (120,130,170).
            // The old value was never held to a floor; `tests/contrast.rs` holds
            // this one against every preset background.
            muted: c(raw::TEXT_MUTED),
            danger: c(raw::ACCENT_RED),
            warn: c(raw::ACCENT_YELLOW),
            success: c(raw::SETTLED_TEAL),
            track: c(raw::BG_HIGHLIGHT),
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
    pub const PRESETS: &'static [fn() -> Theme] = &[
        Theme::tokyo_night,
        Theme::ember,
        Theme::iris,
        Theme::aqua,
        Theme::rose,
    ];
}

impl Default for Theme {
    fn default() -> Self {
        Self::tokyo_night()
    }
}
