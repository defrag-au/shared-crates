//! Shared palette — the Tokyo-Night-ish dark theme the txmints macroquad
//! surfaces use (one vivid accent).
//!
//! ## The vocabulary lives in `ui-theme`
//!
//! The colours used to be ten `Color` literals here and eighteen more in
//! `egui-widgets`, and MEASURED 2026-09-16 the two had already drifted: six
//! colours byte-identical, three diverged (`bg`, `panel`, `muted`), and
//! `success` resolving to a different hue on each side. Nobody decided that; it
//! is what one palette maintained twice does.
//!
//! The first pass took the *values* from `ui_theme::tokens::raw` but kept this
//! crate's own flat ten fields, so the two renderers shared numbers and not
//! names. This pass finishes the job: [`Theme`] now carries
//! [`ColorTokens`](ui_theme::ColorTokens), [`SeriesPalette`](ui_theme::SeriesPalette)
//! and [`TypeScale`](ui_theme::TypeScale), and implements
//! [`Palette`](ui_theme::Palette) — so [`Ink`](ui_theme::Ink) resolves here
//! exactly as it does on the egui side, and a widget written against the
//! vocabulary reads the same on both.
//!
//! ## The ten flat fields are deprecated, not gone
//!
//! `p.theme.muted` reads at ~169 sites in this repo and more in the macroquad
//! hosts downstream (txmints, pump-the-lump). Deleting the fields would break
//! every one of them at once with a hard error and no list; deprecating them
//! makes **cargo enumerate the migration** and lets the hosts move on their own
//! schedule. They are derived from the tokens at construction, so there is
//! still exactly one source of truth.
//!
//! ## Where this deliberately differs from egui
//!
//! - **`success` is teal, not the green ramp.** A preset swaps `accent`, and a
//!   surface that already spends accent on something else (`block_train` gives
//!   it to the newest block) would otherwise render "your transaction landed"
//!   in the same ink as "this block is newest", in a different hue per skin.
//! - **`accent` is the green, not the blue.** egui's default accent is
//!   `ACCENT_BLUE`; these surfaces have always led with lime.
//! - **`bg_primary` / `bg_secondary` are darker.** macroquad clears the canvas
//!   to its own background, and the pump's shell HTML matches it; `ui-theme`
//!   carries both as named values (`BG_NEAR_BLACK`, `PANEL_NEAR_BLACK`) so
//!   neither side is guessing.
//! - **The type ramp is [`TextScale::canvas`](ui_theme::TextScale::canvas),**
//!   which is the ramp these sources measurably already used — 11–16px with one
//!   22px hero, and no 9px dense-table tier.
//!
//! All four are recorded decisions with a constant naming them, which is the
//! difference between a divergence and a drift.

use macroquad::prelude::Color;
use ui_theme::{ColorTokens, Paint, Palette, SeriesPalette, TypeScale, tokens::raw};

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

/// A full theme. Carried on [`crate::Painter`] so widgets read `p.theme.*` and
/// the whole UI can be re-skinned by swapping one value — no per-widget colour
/// constants.
///
/// Presets vary the **accent alone** over the same neutral dark base, which
/// `tests/contrast.rs` asserts — along with a floor for every tier on every
/// surface.
#[derive(Clone, Copy)]
pub struct Theme {
    pub name: &'static str,

    /// The chrome palette: what structure is painted in.
    pub color: ColorTokens<Color>,
    /// The encoding palette: what charts encode *data* with. Separate from
    /// chrome on purpose — see `ui_theme::encoding`.
    pub series: SeriesPalette<Color>,
    /// The typography axis. Point sizes come from here, never from a literal —
    /// `tests/text_sizes.rs` holds that line.
    pub text: TypeScale,

    /// Deprecated: use `color.bg_primary`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.bg_primary`")]
    pub bg: Color,
    /// Deprecated: use `color.bg_secondary`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.bg_secondary`")]
    pub panel: Color,
    /// Deprecated: use `color.accent`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.accent`")]
    pub accent: Color,
    /// Deprecated: use `color.accent_blue`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.accent_blue`")]
    pub link: Color,
    /// Deprecated: use `color.text_primary`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.text_primary`")]
    pub fg: Color,
    /// Deprecated: use `color.text_muted`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.text_muted`")]
    pub muted: Color,
    /// Deprecated: use `color.error`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.error`")]
    pub danger: Color,
    /// Deprecated: use `color.warning`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.warning`")]
    pub warn: Color,
    /// Deprecated: use `color.success`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.success`")]
    pub success: Color,
    /// Deprecated: use `color.bg_highlight`.
    #[deprecated(since = "0.2.0", note = "use `theme.color.bg_highlight`")]
    pub track: Color,
}

impl Theme {
    /// The chrome palette these surfaces ship, with the three deliberate
    /// departures from `ColorTokens::tokyo_night` applied.
    fn tokens() -> ColorTokens<Color> {
        ColorTokens {
            // Darker than egui's `bg_primary`: this is the colour the canvas
            // clears to, and the pump's shell HTML matches it.
            bg_primary: c(raw::BG_NEAR_BLACK),
            bg_secondary: c(raw::PANEL_NEAR_BLACK),
            // The lime the presets swap. egui's default accent is the blue.
            accent: c(raw::ACCENT_GREEN),
            // Fixed across presets on purpose — see the module header.
            success: c(raw::SETTLED_TEAL),
            ..ColorTokens::tokyo_night()
        }
    }

    pub fn tokyo_night() -> Self {
        Self::from_parts("tokyo night", Self::tokens())
    }

    /// Build a theme from its chrome palette, deriving the deprecated flat
    /// fields so the two spellings cannot disagree.
    ///
    /// This is the only place the old names are written down, which is what
    /// makes keeping them cheap: they are a view of `color`, not a second
    /// palette that a preset could forget to update.
    #[allow(deprecated)]
    fn from_parts(name: &'static str, color: ColorTokens<Color>) -> Self {
        Self {
            name,
            bg: color.bg_primary,
            panel: color.bg_secondary,
            accent: color.accent,
            link: color.accent_blue,
            fg: color.text_primary,
            muted: color.text_muted,
            danger: color.error,
            warn: color.warning,
            success: color.success,
            track: color.bg_highlight,
            color,
            series: SeriesPalette::tokyo_night(),
            // The ramp these surfaces already used, measured rather than
            // chosen — see `TextScale::canvas`. Picking a different one would
            // have restyled every macroquad surface under cover of a rename.
            text: TypeScale::canvas(),
        }
    }

    /// Same neutral dark base, different accent.
    fn with_accent(name: &'static str, accent: Color) -> Self {
        Self::from_parts(
            name,
            ColorTokens {
                accent,
                ..Self::tokens()
            },
        )
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

    /// Point size for a step of the type ramp — the one place a size is
    /// allowed to come from.
    pub fn text_size(&self, size: ui_theme::TextSize) -> f32 {
        self.text.at(size)
    }

    /// Point size for a text role. Prefer this when the call site knows what
    /// its text *is*.
    pub fn role_size(&self, role: ui_theme::TextRole) -> f32 {
        self.text.size(role)
    }

    /// Same theme, a different type ramp. The rungs each role stands on are
    /// unchanged, so this opens the whole product out at once.
    pub fn with_text_scale(mut self, steps: ui_theme::TextScale) -> Self {
        self.text.steps = steps;
        self
    }
}

/// What [`ui_theme::Ink`] resolves against — the whole reason this crate can
/// share widget code shapes with the egui side.
impl Palette<Color> for Theme {
    fn color(&self) -> &ColorTokens<Color> {
        &self.color
    }

    fn series(&self) -> &SeriesPalette<Color> {
        &self.series
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::tokyo_night()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_theme::{Ink, Token};

    /// The deprecated names are a VIEW of the tokens, not a second palette. If
    /// a preset could set one without the other, keeping them would be the
    /// drift this whole exercise was about.
    #[test]
    #[allow(deprecated)]
    fn the_deprecated_fields_agree_with_the_tokens_in_every_preset() {
        for make in Theme::PRESETS {
            let t = make();
            assert_eq!(t.bg, t.color.bg_primary, "{}: bg", t.name);
            assert_eq!(t.panel, t.color.bg_secondary, "{}: panel", t.name);
            assert_eq!(t.accent, t.color.accent, "{}: accent", t.name);
            assert_eq!(t.link, t.color.accent_blue, "{}: link", t.name);
            assert_eq!(t.fg, t.color.text_primary, "{}: fg", t.name);
            assert_eq!(t.muted, t.color.text_muted, "{}: muted", t.name);
            assert_eq!(t.danger, t.color.error, "{}: danger", t.name);
            assert_eq!(t.warn, t.color.warning, "{}: warn", t.name);
            assert_eq!(t.success, t.color.success, "{}: success", t.name);
            assert_eq!(t.track, t.color.bg_highlight, "{}: track", t.name);
        }
    }

    /// The point of implementing `Palette`: an `Ink` written on either side
    /// resolves here.
    #[test]
    fn ink_resolves_against_this_theme() {
        let t = Theme::tokyo_night();
        assert_eq!(Ink::Token(Token::Accent).resolve(&t), t.color.accent);
        // A wash takes its alpha as sRGB's u8; this renderer's colour is f32.
        let washed = Ink::Wash(Token::TextMuted, 128).resolve(&t);
        assert!(
            (washed.a - 128.0 / 255.0).abs() < 1e-6,
            "a wash keeps the alpha it was given, got {}",
            washed.a
        );
        assert_eq!(washed.r, t.color.text_muted.r, "and keeps the hue");
    }

    /// The departures from `ColorTokens::tokyo_night` are deliberate, so they
    /// are asserted rather than left to be re-litigated by the next reader.
    #[test]
    fn the_departures_from_the_shared_default_are_the_documented_four() {
        let mine = Theme::tokyo_night().color;
        let shared = ColorTokens::<Color>::tokyo_night();

        assert_eq!(mine.bg_primary, c(raw::BG_NEAR_BLACK));
        assert_eq!(mine.bg_secondary, c(raw::PANEL_NEAR_BLACK));
        assert_eq!(mine.accent, c(raw::ACCENT_GREEN));
        assert_eq!(mine.success, c(raw::SETTLED_TEAL));

        // Everything else is the shared palette, untouched.
        assert_eq!(mine.text_primary, shared.text_primary);
        assert_eq!(mine.text_muted, shared.text_muted);
        assert_eq!(mine.bg_highlight, shared.bg_highlight);
        assert_eq!(mine.error, shared.error);
        assert_eq!(mine.warning, shared.warning);
        assert_eq!(mine.border, shared.border);
    }
}
