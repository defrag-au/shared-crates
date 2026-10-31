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
//! | [`Predicate::HasAllTraits`] | a join from held units to metadata | lots + traits |
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
    /// with a date: `eta_slot` is the slot at which the currently open lots reach the
    /// threshold, if they ever do.
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
    HasAllTraits {
        /// The trait category, or `*` for any.
        category: String,
        /// The values, all of which must be present on the one asset.
        values: Vec<String>,
    },
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
        }
    }

    /// Whether the answer depends on the holder history being complete.
    ///
    /// True for every predicate about a span or a "never": a missing movement truncates
    /// a span or hides a disposal, and a span that is short because a source was
    /// incomplete must be reported as a gap rather than as a near miss.
    pub fn needs_holder_coverage(&self) -> bool {
        !matches!(self, Self::HasAllTraits { .. })
    }

    /// Whether the answer depends on metadata being recorded.
    pub fn needs_traits(&self) -> bool {
        matches!(self, Self::HasAllTraits { .. })
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
