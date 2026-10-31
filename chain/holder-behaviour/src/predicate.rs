//! The typed predicate vocabulary this fold can answer today.
//!
//! The frozen Perps catalogue carries 42 kinds across five axes. This crate implements
//! the five that exercise each distinct shape of the fold — a one-way fact, an open
//! interval, a global rank read, a market count and a metadata join — so the tier
//! boundaries are pinned before the remaining kinds become mechanical:
//!
//! | kind | what it proves | reads |
//! | --- | --- | --- |
//! | [`Predicate::NeverDisposed`] | a one-way fact about the whole history | lots |
//! | [`Predicate::HeldSinceMint`] | an open interval that started at the mint | lots |
//! | [`Predicate::TotalTenureAtLeast`] | summed interval duration, open and closed | lots |
//! | [`Predicate::RoundTripsAtLeast`] | a count of completed intervals | lots |
//! | [`Predicate::HasAllTraits`] | a set carried by one held unit | lots + traits |
//! | [`Predicate::HasTrait`] | one value ever held | lots + traits |
//! | [`Predicate::HasTraitValues`] | a set of values ever held across assets | lots + traits |
//! | [`Predicate::HeldWithTraitAtOnce`] | a count held simultaneously, as a peak | lots + traits |
//! | [`Predicate::SameValueAcrossAssets`] | assets sharing a value, ever held | lots + traits |
//! | [`Predicate::MintWithin`] … [`Predicate::MintAfterGap`] | the collection's mint order | lots + mint facts |
//! | [`Predicate::MintedCount`] … [`Predicate::NeverBought`] | what the subject did at the mint | lots |
//!
//! A kind that is not here is **not silently false**: the catalogue loader is expected to
//! refuse a definition it cannot build, which is the same rule the design doc states for
//! an unknown `kind` — "an unknown kind is an error, never a silent skip".

use serde::{Deserialize, Serialize};

/// One achievement's condition, as the catalogue states it.
///
/// Tagged on `kind` so the TOML `predicate = { kind = "…", … }` table deserialises
/// directly, with no hand-written adapter between the published rules and the enforced
/// ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Predicate {
    /// Acquired at least one unit, and never let one go.
    ///
    /// One-way: once a disposal is seen the answer is `Missed` forever, because the
    /// history it describes cannot change.
    NeverDisposed,

    /// Still holds a unit that arrived at the primary mint.
    ///
    /// An **open** interval that started at the mint, not "minted once" — an asset
    /// minted and sold does not satisfy it, which is the difference between this and
    /// `minted_count`.
    HeldSinceMint,

    /// Total held time across every unit, open and closed, reaches `days`.
    ///
    /// Summed over lots, so two units held for a year each is two years of tenure. The
    /// open part grows with the clock, which is what makes the near-miss reportable
    /// with a date: `eta_unix` is the unix second at which the currently open lots reach
    /// the threshold, if they ever do. Days are unix days, not heights — see
    /// [`crate::SECONDS_PER_DAY`].
    TotalTenureAtLeast {
        /// The threshold, in days of chain time.
        days: u32,
    },

    /// At least `n` units arrived from another party and have since fully left.
    ///
    /// "Completed round trip", not "sold once": a lot that arrived at the mint is not a
    /// round trip, and a lot that still holds part of the position is not complete. A
    /// disposal the fold cannot attribute does not count, so a coverage gap shrinks the
    /// count rather than inventing one.
    RoundTripsAtLeast {
        /// The threshold, in completed lots.
        n: u32,
    },

    /// One held unit carries every one of `values` in `category`.
    ///
    /// `category = "*"` matches across categories, which is what a set spanning
    /// categories needs. An asset with no trait rows at all makes the predicate
    /// `Unavailable` rather than `Missed` — the evaluator cannot distinguish "does not
    /// have it" from "was never told".
    /// Every one of `values` was carried by some unit the subject held.
    ///
    /// A set across assets, not one asset carrying all of them: `tri-faction` is "a Perp
    /// from each faction", and no Perp carries three factions. Each value needs one unit
    /// that carried it at any point, and the answer is earned at the slot the last value
    /// arrived. Selling one afterwards does not un-earn it.
    ///
    /// `category = "*"` matches across categories, which is what a set like the three
    /// crowns needs. An asset with no trait rows at all makes the answer `Unavailable`
    /// rather than `Missed` — the evaluator cannot tell "does not have it" from "was never
    /// told".
    HasAllTraits {
        /// The trait category, or `*` for any.
        category: String,
        /// The values, each carried by some asset the subject held.
        values: Vec<String>,
    },

    /// A unit carrying `value` in `category` was ever held.
    ///
    /// `value = "*"` matches any value in the category. Every lot counts, open or closed:
    /// the statement is "held", so selling the asset afterwards does not un-earn it, and
    /// the answer is earned at the slot the first qualifying lot arrived.
    HasTrait {
        /// The trait category. `*` matches across categories.
        category: String,
        /// The value, or `*` for any value in the category.
        value: String,
    },

    /// `at_least` of `values` in `category` were ever held, across assets.
    ///
    /// Each value needs one asset that carried it; unlike [`Predicate::HasAllTraits`] they
    /// need not be held on the same asset, and unlike
    /// [`Predicate::HeldWithTraitAtOnce`] they need not be held at the same instant.
    HasTraitValues {
        /// The trait category. `*` matches across categories.
        category: String,
        /// The values to look for. A repeated value counts once.
        values: Vec<String>,
        /// How many distinct values must have been seen.
        at_least: u32,
    },

    /// `at_least` distinct units carrying `category` were held at one instant.
    ///
    /// A **historical peak**, not the current count: the achievement is earned at the first
    /// slot the running count reaches the threshold and is never un-earned. `value` omitted
    /// means any value in the category; intervals are half-open `[from, to)`, so an
    /// acquisition and a disposal at the same slot are not simultaneous.
    HeldWithTraitAtOnce {
        /// The trait category.
        category: String,
        /// The value, or `None` for any value in the category.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        /// How many distinct units must be held together.
        at_least: u32,
    },

    /// `assets` distinct units sharing one value in `category` were ever held.
    ///
    /// "Ever", not "at once": the pair need not have overlapped. The answer is earned at
    /// the slot the `assets`-th such unit arrived, for the first value to reach it.
    SameValueAcrossAssets {
        /// The trait category. `*` matches across categories.
        category: String,
        /// How many distinct units must share the value.
        assets: u32,
    },

    /// One of the subject's own mints happened within `seconds` of the collection's first.
    MintWithin {
        /// The window, in chain seconds.
        seconds: u64,
    },

    /// One of the subject's own mints was among the first `n` of the collection.
    MintRankAtMost {
        /// The 1-based rank ceiling.
        n: u32,
    },

    /// One of the subject's own mints was at rank `n` or later.
    MintRankAtLeast {
        /// The 1-based rank floor.
        n: u32,
    },

    /// One of the subject's own mints fell in the last `1 - fraction` of the mint.
    ///
    /// `rank / minted-so-far > fraction`, which is only a fact about the *window* on a
    /// complete fold — an incremental reader mid-mint divides by a partial count.
    MintRankAfterFraction {
        /// The fraction of the mint that must precede it, in `(0, 1)`.
        fraction: f64,
    },

    /// One of the subject's own mints came after the mint had stalled for `days`.
    MintAfterGap {
        /// The quiet period, in days of chain time.
        days: u32,
    },

    /// The subject minted at least `at_least` assets at primary.
    MintedCount {
        /// The threshold, in minted lots.
        at_least: u32,
    },

    /// The subject minted at least `at_least` assets at primary and still holds them.
    MintedAndStillHeld {
        /// The threshold, in open mint lots.
        at_least: u32,
    },

    /// Every asset the subject minted is still held by them.
    AllMintsRetained,

    /// Every asset the subject ever held arrived at the primary mint.
    ///
    /// A gift and a purchase are both an arrival from another party, and the fold cannot
    /// tell them apart — so "bought" here means "did not mint it".
    NeverBought,
}

impl Predicate {
    /// The catalogue id of the kind, as the TOML spells it.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NeverDisposed => "never_disposed",
            Self::HeldSinceMint => "held_since_mint",
            Self::TotalTenureAtLeast { .. } => "total_tenure_at_least",
            Self::RoundTripsAtLeast { .. } => "round_trips_at_least",
            Self::HasAllTraits { .. } => "has_all_traits",
            Self::HasTrait { .. } => "has_trait",
            Self::HasTraitValues { .. } => "has_trait_values",
            Self::HeldWithTraitAtOnce { .. } => "held_with_trait_at_once",
            Self::SameValueAcrossAssets { .. } => "same_value_across_assets",
            Self::MintWithin { .. } => "mint_within",
            Self::MintRankAtMost { .. } => "mint_rank_at_most",
            Self::MintRankAtLeast { .. } => "mint_rank_at_least",
            Self::MintRankAfterFraction { .. } => "mint_rank_after_fraction",
            Self::MintAfterGap { .. } => "mint_after_gap",
            Self::MintedCount { .. } => "minted_count",
            Self::MintedAndStillHeld { .. } => "minted_and_still_held",
            Self::AllMintsRetained => "all_mints_retained",
            Self::NeverBought => "never_bought",
        }
    }

    /// Whether the answer depends on the holder history being complete.
    ///
    /// True for every predicate about a span or a "never": a missing movement truncates
    /// a span or hides a disposal, and a span that is short because a source was
    /// incomplete must be reported as a gap rather than as a near miss.
    pub fn needs_holder_coverage(&self) -> bool {
        !matches!(
            self,
            Self::HasAllTraits { .. }
                | Self::HasTrait { .. }
                | Self::HasTraitValues { .. }
                | Self::HeldWithTraitAtOnce { .. }
                | Self::SameValueAcrossAssets { .. }
        )
    }

    /// Whether the answer depends on metadata being recorded.
    pub fn needs_traits(&self) -> bool {
        matches!(
            self,
            Self::HasAllTraits { .. }
                | Self::HasTrait { .. }
                | Self::HasTraitValues { .. }
                | Self::HeldWithTraitAtOnce { .. }
                | Self::SameValueAcrossAssets { .. }
        )
    }

    /// Whether the answer needs the collection's mint order, not just the subject's lots.
    pub fn needs_mint_facts(&self) -> bool {
        matches!(
            self,
            Self::MintWithin { .. }
                | Self::MintRankAtMost { .. }
                | Self::MintRankAtLeast { .. }
                | Self::MintRankAfterFraction { .. }
                | Self::MintAfterGap { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toml_inline_table_deserialises_straight_to_the_variant() {
        // The exact shape the frozen catalogue uses.
        let predicate: Predicate =
            serde_json::from_str(r#"{"kind":"total_tenure_at_least","days":1000}"#).unwrap();
        assert_eq!(predicate, Predicate::TotalTenureAtLeast { days: 1000 });
    }

    #[test]
    fn a_unit_variant_needs_only_its_tag() {
        let predicate: Predicate = serde_json::from_str(r#"{"kind":"never_disposed"}"#).unwrap();
        assert_eq!(predicate, Predicate::NeverDisposed);
    }

    #[test]
    fn a_trait_predicate_round_trips_its_values_in_order() {
        let text = r#"{"kind":"has_all_traits","category":"Faction",
                       "values":["Astro Schism","Genovault","Kraashers"]}"#;
        let predicate: Predicate = serde_json::from_str(text).unwrap();
        assert_eq!(
            predicate,
            Predicate::HasAllTraits {
                category: "Faction".into(),
                values: vec![
                    "Astro Schism".into(),
                    "Genovault".into(),
                    "Kraashers".into()
                ],
            }
        );
    }

    #[test]
    fn an_unknown_kind_is_an_error_and_not_a_silent_skip() {
        // The catalogue-loader contract: a kind this crate cannot answer must fail to
        // parse, so it is caught when the catalogue is read rather than scoring zero.
        let missing = serde_json::from_str::<Predicate>(r#"{"kind":"held_root_token"}"#);
        assert!(missing.is_err());
    }

    #[test]
    fn coverage_matters_to_spans_and_not_to_trait_reads() {
        assert!(Predicate::NeverDisposed.needs_holder_coverage());
        assert!(Predicate::TotalTenureAtLeast { days: 1 }.needs_holder_coverage());
        assert!(
            !Predicate::HasAllTraits {
                category: "Faction".into(),
                values: vec!["Genovault".into()]
            }
            .needs_holder_coverage()
        );
    }
}
