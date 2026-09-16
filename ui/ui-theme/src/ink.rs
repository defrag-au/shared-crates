//! How a widget **decides** a colour: from the theme, or overridden.
//!
//! This replaced `Option<Color32>`, which was the wrong type for the job in two
//! ways. `None` says *absent* — but a themed default is the opposite of absent,
//! it is the considered answer. And because `None` carries nothing, the token it
//! stood for had to be named at the far-away resolve site (`x.unwrap_or(c.y)`),
//! so reading a widget's `Default` told you nothing about what it would look
//! like, and two resolve sites for one field could silently disagree.

use crate::color::{Paint, with_alpha};
use crate::encoding::{Series, SeriesPalette};
use crate::tokens::{ColorTokens, Token};

/// What [`Ink`] resolves against.
///
/// A trait rather than a concrete `Theme` because a theme is more than colour —
/// typography, spacing, radius and motion are real axes that belong to the
/// renderer, not here. Each front end implements this for its own theme type and
/// `ink.resolve(&theme)` keeps working unchanged.
pub trait Palette<P> {
    fn color(&self) -> &ColorTokens<P>;
    fn series(&self) -> &SeriesPalette<P>;
}

/// How a widget decides a colour: **from the theme, or overridden**.
///
/// Each arm is a resolution *strategy*, evaluated against the active theme:
///
/// - [`Token`](Self::Token) — take a named token as-is.
/// - [`Wash`](Self::Wash) — a token at reduced alpha: tracks, scrims, webs,
///   hairlines. Goes through [`with_alpha`], so it cannot reintroduce the
///   premultiplication bug.
/// - [`On`](Self::On) — whatever reads legibly *on* that token's surface. For
///   text over a semantic fill.
/// - [`Series`](Self::Series) — the encoding palette instead of the chrome one,
///   for colours that carry data rather than structure.
/// - [`Fixed`](Self::Fixed) — a literal, escaping the theme deliberately. This
///   is the arm a reviewer should be suspicious of, which is the point: it is
///   now a *named* choice rather than the absence of one.
///
/// ⚠️ `Eq` is absent because a renderer's colour may be float-backed. Every arm
/// but `Fixed` is colour-free, so a `const` like
/// `const OFF: Ink = Ink::Wash(Token::TextMuted, 80)` still works.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ink<P> {
    /// A named theme token, taken as-is.
    Token(Token),
    /// A named theme token at reduced alpha (0–255).
    Wash(Token, u8),
    /// A legible foreground over the named token's surface.
    On(Token),
    /// An entry from the encoding palette.
    Series(Series),
    /// A fixed colour, overriding the theme.
    Fixed(P),
}

impl<P: Paint> Ink<P> {
    /// Resolve against anything that can supply the two palettes.
    pub fn resolve<T: Palette<P>>(self, theme: &T) -> P {
        let c = theme.color();
        match self {
            Ink::Token(t) => t.get(c),
            Ink::Wash(t, a) => with_alpha(t.get(c), a),
            Ink::On(t) => c.on(t.get(c)),
            Ink::Series(s) => s.get(theme.series()),
            Ink::Fixed(color) => color,
        }
    }

    /// A literal, escaping the theme.
    ///
    /// A named constructor because `From<P> for Ink<P>` cannot be written — it
    /// would overlap with `From<Token>` under coherence. Per-renderer `From`
    /// impls live behind this crate's feature gates instead.
    pub fn fixed(color: P) -> Self {
        Ink::Fixed(color)
    }
}

impl<P> Ink<P> {
    /// The chrome token this ink defers to — `None` for [`Ink::Series`] and
    /// [`Ink::Fixed`].
    ///
    /// Lets a test assert that a widget's defaults all go through the theme.
    pub const fn token(self) -> Option<Token>
    where
        P: Copy,
    {
        match self {
            Ink::Token(t) | Ink::Wash(t, _) | Ink::On(t) => Some(t),
            Ink::Series(_) | Ink::Fixed(_) => None,
        }
    }

    /// Whether this ink escapes the theme entirely.
    pub const fn is_fixed(self) -> bool
    where
        P: Copy,
    {
        matches!(self, Ink::Fixed(_))
    }
}

impl<P> From<Token> for Ink<P> {
    fn from(t: Token) -> Self {
        Ink::Token(t)
    }
}

impl<P> From<Series> for Ink<P> {
    fn from(s: Series) -> Self {
        Ink::Series(s)
    }
}

/// `.color(Color32::RED)` keeps compiling at call sites taking `impl Into<Ink>`.
/// Lives here rather than in `egui-widgets` because both `From` and `Color32`
/// would be foreign there.
#[cfg(feature = "egui")]
impl From<egui::Color32> for Ink<egui::Color32> {
    fn from(c: egui::Color32) -> Self {
        Ink::Fixed(c)
    }
}

#[cfg(feature = "macroquad")]
impl From<macroquad::color::Color> for Ink<macroquad::color::Color> {
    fn from(c: macroquad::color::Color) -> Self {
        Ink::Fixed(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Srgb, contrast_ratio};

    /// A minimal theme: exactly the two axes `Ink` needs, nothing else.
    struct TestTheme {
        color: ColorTokens<Srgb>,
        series: SeriesPalette<Srgb>,
    }

    impl Palette<Srgb> for TestTheme {
        fn color(&self) -> &ColorTokens<Srgb> {
            &self.color
        }
        fn series(&self) -> &SeriesPalette<Srgb> {
            &self.series
        }
    }

    fn theme() -> TestTheme {
        TestTheme {
            color: ColorTokens::tokyo_night(),
            series: SeriesPalette::tokyo_night(),
        }
    }

    /// The const that `seven_segment` relies on — every arm but `Fixed` is
    /// colour-free, so this must stay constructible in a const.
    const OFF_SEGMENT: Ink<Srgb> = Ink::Wash(Token::TextMuted, 80);

    #[test]
    fn a_colour_free_ink_is_const_constructible() {
        assert_eq!(OFF_SEGMENT.token(), Some(Token::TextMuted));
        assert!(!OFF_SEGMENT.is_fixed());
    }

    #[test]
    fn a_token_resolves_to_its_palette_entry() {
        let t = theme();
        assert_eq!(
            Ink::Token(Token::TextPrimary).resolve(&t),
            t.color.text_primary
        );
    }

    /// A wash keeps the hue and takes the alpha — the premultiply happens once,
    /// inside `Paint`, so no caller can reach for the wrong constructor.
    #[test]
    fn a_wash_keeps_the_hue_and_takes_the_alpha() {
        let t = theme();
        let Srgb { r, g, b, a } = Ink::Wash(Token::TextMuted, 80).resolve(&t);
        assert_eq!(a, 80);
        let m = t.color.text_muted;
        assert_eq!((r, g, b), (m.r, m.g, m.b));
    }

    /// `On` is measured, not chosen. Over the error fill it must return
    /// something that actually reads.
    #[test]
    fn on_resolves_to_a_legible_foreground() {
        let t = theme();
        let fill = t.color.error;
        let fg = Ink::On(Token::Error).resolve(&t);
        assert!(
            contrast_ratio(fg, fill) >= 4.5,
            "{}",
            contrast_ratio(fg, fill)
        );
    }

    #[test]
    fn series_resolves_from_the_encoding_palette_not_the_chrome_one() {
        let t = theme();
        assert_eq!(Ink::Series(Series::Inbound).resolve(&t), t.series.inbound());
        assert_eq!(Ink::Series(Series::Nth(0)).resolve(&t), t.series.nth(0));
    }

    /// The arm a reviewer should be suspicious of — and it is reportable, which
    /// is what lets a test assert a widget's defaults all go through the theme.
    #[test]
    fn a_fixed_ink_says_it_escaped_the_theme() {
        let t = theme();
        let red = Srgb::rgb(255, 0, 0);
        let ink = Ink::fixed(red);
        assert_eq!(ink.resolve(&t), red);
        assert!(ink.is_fixed());
        assert_eq!(ink.token(), None);
    }

    #[test]
    fn from_impls_route_to_the_arms_a_caller_means() {
        assert_eq!(Ink::<Srgb>::from(Token::Accent), Ink::Token(Token::Accent));
        assert_eq!(
            Ink::<Srgb>::from(Series::Outbound),
            Ink::Series(Series::Outbound)
        );
    }
}
