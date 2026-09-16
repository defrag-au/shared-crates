//! The **chrome** palette: what structure is painted in, and a name for each
//! entry.
//!
//! Distinct from [`crate::encoding`], which is what charts encode *data* with.
//! Collapsing the two silently produces wrong charts — see that module's header.

use crate::color::{Paint, contrast_ratio};

/// The raw values, as sRGB. Generic palettes are built from these, so a theme
/// is a list of colours in one place rather than eighteen literals per renderer.
pub mod raw {
    use crate::color::Srgb;

    // ── Tokyo Night Dark — the values both crates have always shipped ──────
    pub const BG_PRIMARY: Srgb = Srgb::rgb(26, 27, 38);
    pub const BG_SECONDARY: Srgb = Srgb::rgb(36, 40, 59);
    pub const BG_HIGHLIGHT: Srgb = Srgb::rgb(41, 46, 66);

    pub const TEXT_PRIMARY: Srgb = Srgb::rgb(192, 202, 245);
    /// Tokyo Night `fg_dark` — ~6.9:1 on the secondary background.
    pub const TEXT_SECONDARY: Srgb = Srgb::rgb(169, 177, 214);
    /// De-emphasis tier, but still AA at small sizes — ~5.0:1 on the secondary
    /// background. De-emphasis is expressed *within* the passing range, never by
    /// dropping below it.
    pub const TEXT_MUTED: Srgb = Srgb::rgb(139, 149, 196);

    pub const ACCENT_BLUE: Srgb = Srgb::rgb(122, 162, 247);
    pub const ACCENT_CYAN: Srgb = Srgb::rgb(125, 207, 255);
    pub const ACCENT_GREEN: Srgb = Srgb::rgb(158, 206, 106);
    pub const ACCENT_YELLOW: Srgb = Srgb::rgb(224, 175, 104);
    pub const ACCENT_ORANGE: Srgb = Srgb::rgb(255, 158, 100);
    pub const ACCENT_RED: Srgb = Srgb::rgb(247, 118, 142);
    pub const ACCENT_MAGENTA: Srgb = Srgb::rgb(187, 154, 247);

    /// Deliberately its own value: when this aliased the highlight background it
    /// sat at 1.24:1 against the primary background and panel edges were
    /// effectively invisible.
    pub const BORDER: Srgb = Srgb::rgb(65, 72, 104);

    /// Settled / landed — a fixed teal, NOT the accent.
    ///
    /// `macroquad-widgets` introduced this because its presets swap `accent` per
    /// theme, so "your transaction landed" would change hue per skin. egui
    /// aliased `success` to the green ramp instead, which is how the same
    /// semantic token came to resolve to two different colours across the two
    /// renderers. Named here so the divergence is visible and decidable.
    pub const SETTLED_TEAL: Srgb = Srgb::rgb(115, 218, 202);

    /// The near-black background the macroquad surfaces clear to. It is also
    /// the pump shell's `#0A0A1A`, which is hand-typed in `index.html` today —
    /// having it here is what lets that be stamped rather than duplicated.
    pub const BG_NEAR_BLACK: Srgb = Srgb::rgb(10, 10, 26);
    /// The macroquad panel fill that pairs with [`BG_NEAR_BLACK`].
    pub const PANEL_NEAR_BLACK: Srgb = Srgb::rgb(20, 22, 38);

    // ── opensea — near-black and neutral, for a browsing surface ───────────
    pub const OS_BG_PRIMARY: Srgb = Srgb::rgb(12, 13, 16);
    pub const OS_BG_SECONDARY: Srgb = Srgb::rgb(22, 24, 29);
    pub const OS_BG_HIGHLIGHT: Srgb = Srgb::rgb(34, 37, 44);
    pub const OS_TEXT_PRIMARY: Srgb = Srgb::rgb(247, 248, 249);
    pub const OS_TEXT_SECONDARY: Srgb = Srgb::rgb(180, 186, 196);
    pub const OS_TEXT_MUTED: Srgb = Srgb::rgb(142, 150, 163);
    pub const OS_ACCENT_BLUE: Srgb = Srgb::rgb(59, 142, 240);
    pub const OS_ACCENT_CYAN: Srgb = Srgb::rgb(56, 189, 248);
    pub const OS_ACCENT_GREEN: Srgb = Srgb::rgb(52, 199, 123);
    pub const OS_ACCENT_YELLOW: Srgb = Srgb::rgb(245, 181, 68);
    pub const OS_ACCENT_ORANGE: Srgb = Srgb::rgb(251, 146, 60);
    pub const OS_ACCENT_RED: Srgb = Srgb::rgb(244, 88, 110);
    pub const OS_ACCENT_MAGENTA: Srgb = Srgb::rgb(192, 132, 252);
    /// Lighter than the reference UI's own hairlines, which sit near 1.4:1
    /// against the page — under this crate's visibility floor.
    pub const OS_BORDER: Srgb = Srgb::rgb(56, 61, 71);
}

/// The colour axis.
///
/// The semantic entries (`accent`, `success`, `warning`, `error`) are **fields,
/// not accessors that alias the ramp**. A theme must be able to say that success
/// is not green without redefining the ramp it borrows from.
///
/// Generic over the renderer's colour type, so each front end aliases it —
/// `type ColorTokens = ui_theme::ColorTokens<Color32>` — and field access still
/// yields that renderer's own colour.
///
/// ⚠️ `Eq` is deliberately absent: macroquad's colour is four `f32`s, so the
/// bound could not be satisfied there. `PartialEq` is what comparisons need.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorTokens<P> {
    pub bg_primary: P,
    pub bg_secondary: P,
    pub bg_highlight: P,

    pub text_primary: P,
    pub text_secondary: P,
    pub text_muted: P,

    pub accent_blue: P,
    pub accent_cyan: P,
    pub accent_green: P,
    pub accent_yellow: P,
    pub accent_orange: P,
    pub accent_red: P,
    pub accent_magenta: P,

    pub accent: P,
    pub success: P,
    pub warning: P,
    pub error: P,
    pub border: P,
}

impl<P: Paint> ColorTokens<P> {
    /// The Tokyo Night Dark palette — the values this suite has always shipped.
    ///
    /// Not `const fn` any more: building through [`Paint`] means a trait call,
    /// and trait methods cannot be const. Verified free — nothing bound a
    /// `ColorTokens`, `SeriesPalette` or `Theme` in a `const`/`static`, and the
    /// preset lists hold function pointers.
    pub fn tokyo_night() -> Self {
        let c = P::from_srgb;
        Self {
            bg_primary: c(raw::BG_PRIMARY),
            bg_secondary: c(raw::BG_SECONDARY),
            bg_highlight: c(raw::BG_HIGHLIGHT),
            text_primary: c(raw::TEXT_PRIMARY),
            text_secondary: c(raw::TEXT_SECONDARY),
            text_muted: c(raw::TEXT_MUTED),
            accent_blue: c(raw::ACCENT_BLUE),
            accent_cyan: c(raw::ACCENT_CYAN),
            accent_green: c(raw::ACCENT_GREEN),
            accent_yellow: c(raw::ACCENT_YELLOW),
            accent_orange: c(raw::ACCENT_ORANGE),
            accent_red: c(raw::ACCENT_RED),
            accent_magenta: c(raw::ACCENT_MAGENTA),
            accent: c(raw::ACCENT_BLUE),
            success: c(raw::ACCENT_GREEN),
            warning: c(raw::ACCENT_YELLOW),
            error: c(raw::ACCENT_RED),
            border: c(raw::BORDER),
        }
    }

    /// Near-black and neutral rather than Tokyo Night's indigo cast.
    pub fn opensea() -> Self {
        let c = P::from_srgb;
        Self {
            bg_primary: c(raw::OS_BG_PRIMARY),
            bg_secondary: c(raw::OS_BG_SECONDARY),
            bg_highlight: c(raw::OS_BG_HIGHLIGHT),
            text_primary: c(raw::OS_TEXT_PRIMARY),
            text_secondary: c(raw::OS_TEXT_SECONDARY),
            text_muted: c(raw::OS_TEXT_MUTED),
            accent_blue: c(raw::OS_ACCENT_BLUE),
            accent_cyan: c(raw::OS_ACCENT_CYAN),
            accent_green: c(raw::OS_ACCENT_GREEN),
            accent_yellow: c(raw::OS_ACCENT_YELLOW),
            accent_orange: c(raw::OS_ACCENT_ORANGE),
            accent_red: c(raw::OS_ACCENT_RED),
            accent_magenta: c(raw::OS_ACCENT_MAGENTA),
            accent: c(raw::OS_ACCENT_BLUE),
            success: c(raw::OS_ACCENT_GREEN),
            warning: c(raw::OS_ACCENT_YELLOW),
            error: c(raw::OS_ACCENT_RED),
            border: c(raw::OS_BORDER),
        }
    }

    /// Every background a text colour can land on.
    ///
    /// The contrast floors have to be checked against all three, and listing
    /// them here means a new theme cannot forget one.
    pub fn backgrounds(&self) -> [P; 3] {
        [self.bg_primary, self.bg_secondary, self.bg_highlight]
    }

    /// Every tier of the text ramp, for the same reason.
    pub fn text_ramp(&self) -> [P; 3] {
        [self.text_primary, self.text_secondary, self.text_muted]
    }

    /// A foreground **from this palette** that reads on `fill`.
    ///
    /// For solid semantic fills — a danger chip, a status pill — where the
    /// caller knows the background and needs text that survives it. Returns
    /// whichever end of the theme's own ramp contrasts more, so the answer moves
    /// with the theme instead of being a hardcoded white.
    ///
    /// Picking by measured ratio rather than a luminance threshold matters for
    /// the mid-tone fills (a 60%-luminance amber) where the two are close and a
    /// threshold guesses wrong.
    pub fn on(&self, fill: P) -> P {
        if contrast_ratio(self.bg_primary, fill) >= contrast_ratio(self.text_primary, fill) {
            self.bg_primary
        } else {
            self.text_primary
        }
    }
}

/// A name for one entry in [`ColorTokens`].
///
/// Exists so a colour choice can be *written down* somewhere that has no theme
/// to ask — a `Default` impl, a const config, a consumer's struct literal. The
/// value is fetched later, from whichever theme is actually active.
///
/// [`ALL`](Self::ALL) is exhaustive and [`get`](Self::get) matches without a
/// wildcard, so adding a token to [`ColorTokens`] fails to compile until it is
/// named here too.
///
/// Carries no colour, so it is renderer-free and `const`-constructible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    BgPrimary,
    BgSecondary,
    BgHighlight,
    TextPrimary,
    TextSecondary,
    TextMuted,
    AccentBlue,
    AccentCyan,
    AccentGreen,
    AccentYellow,
    AccentOrange,
    AccentRed,
    AccentMagenta,
    Accent,
    Success,
    Warning,
    Error,
    Border,
}

impl Token {
    /// Every token, in [`ColorTokens`] declaration order. Drives token
    /// inspectors and the contrast suite.
    pub const ALL: [Token; 18] = [
        Token::BgPrimary,
        Token::BgSecondary,
        Token::BgHighlight,
        Token::TextPrimary,
        Token::TextSecondary,
        Token::TextMuted,
        Token::AccentBlue,
        Token::AccentCyan,
        Token::AccentGreen,
        Token::AccentYellow,
        Token::AccentOrange,
        Token::AccentRed,
        Token::AccentMagenta,
        Token::Accent,
        Token::Success,
        Token::Warning,
        Token::Error,
        Token::Border,
    ];

    /// This token's value in `c`.
    pub fn get<P: Paint>(self, c: &ColorTokens<P>) -> P {
        match self {
            Token::BgPrimary => c.bg_primary,
            Token::BgSecondary => c.bg_secondary,
            Token::BgHighlight => c.bg_highlight,
            Token::TextPrimary => c.text_primary,
            Token::TextSecondary => c.text_secondary,
            Token::TextMuted => c.text_muted,
            Token::AccentBlue => c.accent_blue,
            Token::AccentCyan => c.accent_cyan,
            Token::AccentGreen => c.accent_green,
            Token::AccentYellow => c.accent_yellow,
            Token::AccentOrange => c.accent_orange,
            Token::AccentRed => c.accent_red,
            Token::AccentMagenta => c.accent_magenta,
            Token::Accent => c.accent,
            Token::Success => c.success,
            Token::Warning => c.warning,
            Token::Error => c.error,
            Token::Border => c.border,
        }
    }

    /// The field name, as written in [`ColorTokens`] — for inspectors and
    /// assertion messages.
    pub const fn name(self) -> &'static str {
        match self {
            Token::BgPrimary => "bg_primary",
            Token::BgSecondary => "bg_secondary",
            Token::BgHighlight => "bg_highlight",
            Token::TextPrimary => "text_primary",
            Token::TextSecondary => "text_secondary",
            Token::TextMuted => "text_muted",
            Token::AccentBlue => "accent_blue",
            Token::AccentCyan => "accent_cyan",
            Token::AccentGreen => "accent_green",
            Token::AccentYellow => "accent_yellow",
            Token::AccentOrange => "accent_orange",
            Token::AccentRed => "accent_red",
            Token::AccentMagenta => "accent_magenta",
            Token::Accent => "accent",
            Token::Success => "success",
            Token::Warning => "warning",
            Token::Error => "error",
            Token::Border => "border",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the tests need a concrete colour — the module itself is generic.
    use crate::color::Srgb;

    /// `ALL` must stay exhaustive, or the contrast suite silently stops
    /// checking whichever token was forgotten.
    #[test]
    fn every_token_is_listed_and_named_once() {
        assert_eq!(Token::ALL.len(), 18);
        let mut names: Vec<&str> = Token::ALL.iter().map(|t| t.name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "a token name is duplicated");
    }

    #[test]
    fn every_token_resolves_against_the_default_palette() {
        let c = ColorTokens::<Srgb>::tokyo_night();
        for token in Token::ALL {
            // Nothing in the palette is fully transparent; a zero alpha here
            // would mean an unset field reached the tokens.
            assert!(token.get(&c).is_opaque(), "{} is not opaque", token.name());
        }
    }

    /// The floor the whole text ramp is built to: AA for small text, on every
    /// background it can land on.
    #[test]
    fn the_text_ramp_clears_aa_on_every_background() {
        for palette in [
            ColorTokens::<Srgb>::tokyo_night(),
            ColorTokens::<Srgb>::opensea(),
        ] {
            for bg in palette.backgrounds() {
                for text in palette.text_ramp() {
                    let ratio = crate::color::contrast_ratio(text, bg);
                    assert!(ratio >= 4.5, "{text:?} on {bg:?} is {ratio}");
                }
            }
        }
    }

    /// A border nobody can see is not a border. This floor is why `border` is
    /// its own value rather than an alias of the highlight background.
    #[test]
    fn the_border_is_visible_against_the_page() {
        for palette in [
            ColorTokens::<Srgb>::tokyo_night(),
            ColorTokens::<Srgb>::opensea(),
        ] {
            let ratio = crate::color::contrast_ratio(palette.border, palette.bg_primary);
            assert!(ratio >= 1.5, "border is {ratio}");
        }
    }

    /// Legibility is derived, so a semantic fill cannot ship unreadable text.
    #[test]
    fn on_returns_something_readable_over_every_semantic_fill() {
        let c = ColorTokens::<Srgb>::tokyo_night();
        for fill in [c.accent, c.success, c.warning, c.error] {
            let ratio = crate::color::contrast_ratio(c.on(fill), fill);
            assert!(ratio >= 4.5, "label on {fill:?} is {ratio}");
        }
    }
}
