//! The colours a chart **encodes with** — as distinct from the ones chrome is
//! painted in.
//!
//! # Why this is a separate axis from [`crate::theme::ColorTokens`]
//!
//! The two answer different questions, and collapsing them silently produces
//! wrong charts. A semantic token says *what a thing means* — this failed, this
//! is a warning. An encoding colour says *where this datum sits*, and its job is
//! to preserve a relationship: distinguishable-from, more-than, further-from-
//! zero. Mapping a six-way categorical ramp onto four semantic tokens makes two
//! categories collide, and a stacked chart with two identical bands is not a
//! styling regression — it is a wrong chart that still looks fine to whoever
//! shipped it.
//!
//! # Kinds, because the kind is what carries the invariant
//!
//! There is no single "chart palette". There are five kinds in this suite, each
//! with a different thing a theme must not break:
//!
//! | Kind | Used by | Invariant |
//! |---|---|---|
//! | [`SeriesPalette::categorical`] | `channel_bands`, ring classes | adjacent pairs separable under dichromacy |
//! | [`Diverging`] | flow in/out, price up/down, over/under | arms separable **and** a neutral at surface luminance |
//! | [`Sequential`] | rarity rank, density, depth | **monotonic in luminance** |
//! | [`IdentityEnvelope`] | `utxo_map` policy colours | unbounded hues, bounded legibility |
//! | *fixed / physical* | `asset_card` foil, terrain metaphor | **not themed at all** |
//!
//! That last row is a real category, not an oversight. The holographic foil is a
//! hue rotation simulating a physical effect; a themed rainbow is not a rainbow.
//! Those literals stay literal, and the reason is written where they live.
//!
//! # The enforcement is the deliverable
//!
//! Letting a theme supply chart colours is only safe because each invariant is a
//! test over `Theme::PRESETS` (`tests/series_palette.rs`). That is not
//! belt-and-braces: within an hour of the categorical validator existing it
//! caught a sixth slot colliding at ΔE 7.3 under protanopia, and the ordinal
//! ramp below was found non-monotonic after years in the tree. Without the
//! tests, every new preset is a chance to ship an encoding that reads fine to
//! the person who added it and lies to everyone else.
//!
//! # Colour should not be the only channel
//!
//! Where position, size or shape already carry the encoding, theming colour is
//! low-risk — `flow_ring` deforms the radius, the token view makes depth the
//! headline on a log scale. Where colour is the only channel, the invariant test
//! is the only thing between a theme and an unreadable chart.

// The four encoding kinds, the Oklab helpers and their invariant tests moved
// to `ui-theme` on 2026-09-16, so `macroquad-widgets` renders charts from the
// same validated ramps instead of having none. The rationale above stays here,
// beside the widgets it governs.
//
// `theme.rs` aliases them to `Color32` (`Sequential`, `Diverging`,
// `SeriesPalette`) and re-exports `IdentityEnvelope`, so `theme::SeriesPalette`
// and every widget naming these keeps resolving unchanged.
pub use ui_theme::{Diverging, IdentityEnvelope, Sequential, SeriesPalette};
