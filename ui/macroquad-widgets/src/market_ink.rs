//! The colours a trading view **encodes with**, defaulted from the theme.
//!
//! Chrome (panels, rules, labels) comes from [`Painter::theme`](crate::Painter),
//! never from here. This is the other axis: what a buy is, what a sell is,
//! what ADA is, and the marks a token chart adds. A host that has its own
//! conventions (the token arena's green/red/amber) overrides the fields; a
//! host that has none gets the theme's, so a chart is never colourless.
//!
//! The defaults are deliberately *semantic* rather than a new palette:
//! `buy` is the theme's success, `sell` its error, `ada` its orange, and the
//! rest are accent entries. Changing a theme still moves them.
//!
//! # Why every field takes a *different* token
//!
//! The first draft pointed `pool` at `accent` and `crown` at `accent_yellow`,
//! which are also `accent` and `ada` — so a pool tile and a handle were the same
//! lime, and a new high and an ADA amount were the same gold. None of those
//! pairs is ever on screen together in the arena, because the arena overrides
//! every field; a host that does not override them puts them side by side
//! immediately. `encodings_stay_tellable_apart` holds that line.
//!
//! `accent` is exempt, and deliberately: it *is* the skin, the presets span the
//! hue circle, and `rose` lands on `sell` exactly. A host whose accent must not
//! be mistaken for a market colour overrides the field — which is one setter, and
//! is what the arena already does.

use macroquad::prelude::*;

use crate::Theme;

/// The semantic colours a market view draws with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarketInk {
    /// Tokens leaving the pool for a holder.
    pub buy: Color,
    /// Tokens going into the pool.
    pub sell: Color,
    /// ADA in flight, and a value's unit mark.
    pub ada: Color,
    /// A transfer between holders.
    pub transfer: Color,
    /// A pool, and liquidity.
    pub pool: Color,
    /// Handles, crowns, the accent.
    pub accent: Color,
    /// A high-water mark, a new #1.
    pub crown: Color,
}

impl MarketInk {
    /// The theme's own semantics, with no market-specific convention imposed.
    pub fn from_theme(t: &Theme) -> Self {
        Self {
            buy: t.color.success,
            sell: t.color.error,
            ada: t.color.accent_orange,
            transfer: t.color.accent_magenta,
            pool: t.color.accent_cyan,
            accent: t.color.accent,
            crown: t.color.accent_yellow,
        }
    }

    pub fn buy(mut self, color: Color) -> Self {
        self.buy = color;
        self
    }

    pub fn sell(mut self, color: Color) -> Self {
        self.sell = color;
        self
    }

    pub fn ada(mut self, color: Color) -> Self {
        self.ada = color;
        self
    }

    pub fn transfer(mut self, color: Color) -> Self {
        self.transfer = color;
        self
    }

    pub fn pool(mut self, color: Color) -> Self {
        self.pool = color;
        self
    }

    pub fn accent(mut self, color: Color) -> Self {
        self.accent = color;
        self
    }

    pub fn crown(mut self, color: Color) -> Self {
        self.crown = color;
        self
    }
}

impl Default for MarketInk {
    /// The Tokyo-Night theme's semantics. VM constructors take an explicit
    /// `MarketInk`; this is for a story or a first draft, not a production
    /// default that silently ignores the host's theme.
    fn default() -> Self {
        Self::from_theme(&Theme::tokyo_night())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_theme::{Paint, delta_e};

    /// Two market encodings that read as one colour are one encoding. The floor
    /// is the one `tests/contrast.rs` uses for `success` vs `accent`, where the
    /// same argument applies: a reader must never have to know which field a
    /// mark came from to know what it means.
    const APART: f64 = 15.0;

    /// The six colours that carry a *meaning*, as opposed to [`MarketInk::accent`],
    /// which carries the skin.
    fn encodings(ink: &MarketInk) -> [(&'static str, Color); 6] {
        [
            ("buy", ink.buy),
            ("sell", ink.sell),
            ("ada", ink.ada),
            ("transfer", ink.transfer),
            ("pool", ink.pool),
            ("crown", ink.crown),
        ]
    }

    /// Every preset, because the theme is what supplies these and a future one
    /// could move a token behind two meanings.
    #[test]
    fn encodings_stay_tellable_apart() {
        for preset in Theme::PRESETS {
            let theme = preset();
            let ink = MarketInk::from_theme(&theme);
            let fields = encodings(&ink);
            for (i, (a_name, a)) in fields.iter().enumerate() {
                for (b_name, b) in &fields[i + 1..] {
                    let d = delta_e(a.srgb(), b.srgb());
                    assert!(
                        d >= APART,
                        "{}: {a_name} vs {b_name} is ΔE {d:.1}, below {APART}",
                        theme.name
                    );
                }
            }
        }
    }

    /// The host override has to be able to collapse a field back to one colour
    /// without the rest moving — that is what makes the arena's own constants a
    /// drop-in.
    #[test]
    fn a_setter_moves_exactly_one_field() {
        let base = MarketInk::default();
        let moved = base.pool(Color::new(1.0, 0.0, 1.0, 1.0));
        assert_eq!(moved.pool, Color::new(1.0, 0.0, 1.0, 1.0));
        for ((a_name, a), (_, b)) in encodings(&base).iter().zip(encodings(&moved).iter()) {
            if *a_name != "pool" {
                assert_eq!(a, b, "{a_name} moved with pool");
            }
        }
        assert_eq!(base.accent, moved.accent, "accent moved with pool");
    }
}
