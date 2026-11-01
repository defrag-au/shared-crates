//! The typed predicate vocabulary this fold can answer today.
//!
//! The frozen Perps catalogue carries 42 kinds across five axes. This crate implements the
//! kinds the fold's own facts can answer — the subject's lots, the collection's mint order
//! and the metadata join — and refuses the rest, so a kind is never silently false:
//!
//! | kind | what it proves | reads |
//! | --- | --- | --- |
//! | [`Predicate::NeverDisposed`] | a one-way fact about the whole history | lots |
//! | [`Predicate::HeldSinceMint`] | an open interval that started at the mint | lots |
//! | [`Predicate::TotalTenureAtLeast`] | summed interval duration, open and closed | lots |
//! | [`Predicate::AnySpanAtLeast`] / [`Predicate::SpanAtLeast`] | one or more continuous intervals | lots |
//! | [`Predicate::HoldsNeverMoved`] / [`Predicate::ReacquiredSameAsset`] / [`Predicate::ReturnedAfterIdle`] | a shape over the subject's own lots | lots |
//! | [`Predicate::RoundTripsAtLeast`] | a count of completed intervals | lots |
//! | [`Predicate::HasAllTraits`] | a set carried by one held unit | lots + traits |
//! | [`Predicate::HasTrait`] | one value ever held | lots + traits |
//! | [`Predicate::HasTraitValues`] | a set of values ever held across assets | lots + traits |
//! | [`Predicate::HeldWithTraitAtOnce`] | a count held simultaneously, as a peak | lots + traits |
//! | [`Predicate::SameValueAcrossAssets`] | assets sharing a value, ever held | lots + traits |
//! | [`Predicate::MintWithin`] … [`Predicate::MintAfterGap`] | the collection's mint order | lots + mint facts |
//! | [`Predicate::MintedCount`] … [`Predicate::NeverBought`] | what the subject did at the mint | lots |
//! | [`Predicate::NoSaleBelowMint`] | a sale below a declared mint price | market |
//! | [`Predicate::MovementRatioAtLeast`] | movements per asset ever held | lots + movement count |
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

    /// One lot stayed open for at least `days`.
    ///
    /// A **continuous** span, not a sum: a unit held for 60 days, sold, and reacquired for
    /// another 60 does not satisfy a 100-day `span_at_least`, while it does satisfy
    /// `total_tenure_at_least`. The clock is the lot's own interval, so a partial disposal
    /// does not restart it.
    AnySpanAtLeast {
        /// The threshold, in days of chain time.
        days: u32,
    },

    /// At least `assets` distinct units were each held for `days` continuously.
    ///
    /// Per unit, the **longest** span of any one of its lots counts; a unit held twice for
    /// 60 days is not a 100-day asset. This is `any_span_at_least` with a breadth
    /// requirement, and the statement it proves is "five assets each held 100+ days".
    SpanAtLeast {
        /// The threshold, in days of chain time.
        days: u32,
        /// How many distinct units must each reach it.
        assets: u32,
    },

    /// The subject holds a unit that arrived at the mint and has not left since.
    ///
    /// An **open** lot acquired at the primary mint. Existential, so it is never `Missed`:
    /// a wallet that holds only traded assets can still acquire an untouched one.
    HoldsNeverMoved,

    /// The subject sold a unit and later acquired that same unit from another party.
    ///
    /// `times` counts the **reacquisitions**, not the units: buying the same asset back
    /// twice is two. The fold cannot tell a purchase from a gift, so the later lot is
    /// `Transfer` ("came from someone else"), the same reading `never_bought` uses.
    ReacquiredSameAsset {
        /// The threshold, in reacquisition events.
        times: u32,
    },

    /// The subject held nothing for `days`, then acquired again.
    ///
    /// Holding intervals are merged before the gap is measured, so overlapping lots are
    /// one holding period. A wallet that has never held anything is `Pending`, not `Missed`:
    /// the idle-then-return shape needs a holding to return from.
    ReturnedAfterIdle {
        /// The quiet period, in days of chain time.
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

    /// The subject appeared as the buyer in a settled trade.
    ///
    /// Any side: buying a listing and accepting a bid both make the subject the buyer.
    /// A trade the source could not attribute is not a purchase, so this reads the fill's
    /// parties rather than a holder movement.
    MarketPurchase,

    /// The subject placed at least `at_least` listings.
    ///
    /// Distinct orders, not placement events: a relist reuses the order identity and does
    /// not count twice. A withdrawal does not un-list it — the statement is "listed", so
    /// the order having existed is the fact.
    ListedCount {
        /// The threshold, in listed assets.
        at_least: u32,
    },

    /// The subject bought a listing within `seconds` of it being placed.
    ///
    /// Needs an order the source keyed on both ends: a fill that does not name its order, or
    /// one whose placement predates the walk, reports the gap rather than counting as fast.
    BoughtEscrowWithin {
        /// The window, in unix seconds.
        seconds: u64,
    },

    /// The subject bought a listing that had stood for at least `days`.
    ///
    /// The same order-age requirement as [`Predicate::BoughtEscrowWithin`], read the other
    /// way round.
    BoughtListingOlderThan {
        /// The age, in unix days.
        days: u32,
    },

    /// The subject bought through a bid scoped to the whole collection.
    BoughtViaCollectionOffer,

    /// The subject had a bid on one asset accepted.
    OfferAcceptedAsBuyer,

    /// The subject placed a bid on one asset.
    OfferCreated,

    /// The subject never sold below the primary mint price.
    ///
    /// One-way: a single sale below `amount` makes the answer `Missed` for good. A subject
    /// who has not sold is `Pending`, not earned — `never_disposed` is the achievement for
    /// holding, and this one is for selling without dumping.
    ///
    /// The price is stated here rather than derived, because a mint price is not in the
    /// movement archive. A two-stage mint states the price it ended at; see the catalogue.
    NoSaleBelowMint {
        /// The primary mint price, in the chain's own coin's smallest unit.
        amount: u64,
        /// The scale `amount` is expressed at.
        decimals: u8,
    },

    /// The subject's holder movements are at least `ratio` times the assets it held.
    ///
    /// A movement is one holder event the subject was a party to; the denominator is the
    /// distinct units it ever held. A subject that never held a unit has nothing to have
    /// moved per asset, so it stays `Pending` rather than dividing by zero.
    MovementRatioAtLeast {
        /// The multiple of the assets held that must be reached.
        ratio: f64,
    },

    /// The subject sold at or below the collection's floor.
    ///
    /// A sale is any fill where the subject is the seller, whether it consumed a listing or
    /// accepted a bid. The comparison needs a priced fill and a known floor at that instant;
    /// a sale that cannot be judged reports the gap rather than becoming a miss.
    SoldAtOrBelowFloor,

    /// The subject sold above the collection's floor.
    SoldAboveFloor,

    /// The subject sold above the floor while the floor's last move was up.
    ///
    /// "Rising" is the last **change** the book recorded, not a window: the floor at the sale
    /// is higher than the previous observation, and a book that emptied in between is not a
    /// rise.
    SoldIntoStrength,

    /// One of the subject's acquisitions happened while the floor was in a drawdown.
    ///
    /// The drawdown is measured from the highest floor seen up to that instant, which may
    /// predate the acquisition — the market was already down when they bought.
    AcquiredDuringDrawdown {
        /// The drawdown, as a fraction of the peak, in `(0, 1)`.
        fraction: f64,
    },

    /// One of the subject's holding spans covered a floor drawdown of `fraction`.
    ///
    /// Both the peak and the trough must fall inside the span. A fall that began before the
    /// subject acquired does not count, and the floor at the acquisition seeds the peak.
    DrawdownCovered {
        /// The drawdown, as a fraction of the peak, in `(0, 1)`.
        fraction: f64,
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

    /// `at_least` distinct units were held at one instant.
    ///
    /// A **historical peak**, not the current count: the achievement is earned at the first
    /// slot the running count reaches the threshold and is never un-earned. Intervals are
    /// half-open `[from, to)`, and a unit held through overlapping lots counts once.
    ///
    /// This is the trait-free sibling of [`Predicate::HeldWithTraitAtOnce`], for a collection
    /// whose holding ladder is about the collection itself rather than one trait value.
    HoldsAtLeast {
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
            Self::AnySpanAtLeast { .. } => "any_span_at_least",
            Self::SpanAtLeast { .. } => "span_at_least",
            Self::HoldsNeverMoved => "holds_never_moved",
            Self::ReacquiredSameAsset { .. } => "reacquired_same_asset",
            Self::ReturnedAfterIdle { .. } => "returned_after_idle",
            Self::RoundTripsAtLeast { .. } => "round_trips_at_least",
            Self::MarketPurchase => "market_purchase",
            Self::ListedCount { .. } => "listed_count",
            Self::BoughtEscrowWithin { .. } => "bought_escrow_within",
            Self::BoughtListingOlderThan { .. } => "bought_listing_older_than",
            Self::BoughtViaCollectionOffer => "bought_via_collection_offer",
            Self::OfferAcceptedAsBuyer => "offer_accepted_as_buyer",
            Self::OfferCreated => "offer_created",
            Self::NoSaleBelowMint { .. } => "no_sale_below_mint",
            Self::MovementRatioAtLeast { .. } => "movement_ratio_at_least",
            Self::SoldAtOrBelowFloor => "sold_at_or_below_floor",
            Self::SoldAboveFloor => "sold_above_floor",
            Self::SoldIntoStrength => "sold_into_strength",
            Self::AcquiredDuringDrawdown { .. } => "acquired_during_drawdown",
            Self::DrawdownCovered { .. } => "drawdown_covered",
            Self::HasAllTraits { .. } => "has_all_traits",
            Self::HasTrait { .. } => "has_trait",
            Self::HasTraitValues { .. } => "has_trait_values",
            Self::HeldWithTraitAtOnce { .. } => "held_with_trait_at_once",
            Self::HoldsAtLeast { .. } => "holds_at_least",
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
                | Self::MarketPurchase
                | Self::ListedCount { .. }
                | Self::BoughtEscrowWithin { .. }
                | Self::BoughtListingOlderThan { .. }
                | Self::BoughtViaCollectionOffer
                | Self::OfferAcceptedAsBuyer
                | Self::OfferCreated
                | Self::NoSaleBelowMint { .. }
                | Self::SoldAtOrBelowFloor
                | Self::SoldAboveFloor
                | Self::SoldIntoStrength
        )
    }

    /// Whether the answer needs marketplace facts, not just the subject's lots.
    ///
    /// A store built from a holder-only stream answers every one of these as
    /// `Unavailable(Gap::MarketFacts)` rather than as "did not trade".
    pub fn needs_market_facts(&self) -> bool {
        matches!(
            self,
            Self::MarketPurchase
                | Self::ListedCount { .. }
                | Self::BoughtEscrowWithin { .. }
                | Self::BoughtListingOlderThan { .. }
                | Self::BoughtViaCollectionOffer
                | Self::OfferAcceptedAsBuyer
                | Self::OfferCreated
                | Self::NoSaleBelowMint { .. }
                | Self::SoldAtOrBelowFloor
                | Self::SoldAboveFloor
                | Self::SoldIntoStrength
                | Self::AcquiredDuringDrawdown { .. }
                | Self::DrawdownCovered { .. }
        )
    }

    /// Whether the answer needs the collection's floor over time.
    ///
    /// A store built from a holder-only stream — or a bundle row — answers these as
    /// `Unavailable(Gap::FloorFacts)`; the earned snapshot may still have answered them.
    pub fn needs_floor_facts(&self) -> bool {
        matches!(
            self,
            Self::SoldAtOrBelowFloor
                | Self::SoldAboveFloor
                | Self::SoldIntoStrength
                | Self::AcquiredDuringDrawdown { .. }
                | Self::DrawdownCovered { .. }
        )
    }

    /// Whether the answer needs the subject's movement count, not just its lots.
    ///
    /// A bundle row carries lots, so it answers these as
    /// `Unavailable(Gap::MovementFacts)`; the earned snapshot may still have answered them.
    pub fn needs_movements(&self) -> bool {
        matches!(self, Self::MovementRatioAtLeast { .. })
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
    fn the_lot_only_kinds_parse_from_the_catalogue_table() {
        let span: Predicate =
            serde_json::from_str(r#"{"kind":"span_at_least","days":100,"assets":5}"#).unwrap();
        assert_eq!(
            span,
            Predicate::SpanAtLeast {
                days: 100,
                assets: 5
            }
        );
        let any: Predicate =
            serde_json::from_str(r#"{"kind":"any_span_at_least","days":365}"#).unwrap();
        assert_eq!(any, Predicate::AnySpanAtLeast { days: 365 });
        let returned: Predicate =
            serde_json::from_str(r#"{"kind":"returned_after_idle","days":90}"#).unwrap();
        assert_eq!(returned, Predicate::ReturnedAfterIdle { days: 90 });
        let untouched: Predicate = serde_json::from_str(r#"{"kind":"holds_never_moved"}"#).unwrap();
        assert_eq!(untouched, Predicate::HoldsNeverMoved);
        let came_back: Predicate =
            serde_json::from_str(r#"{"kind":"reacquired_same_asset","times":1}"#).unwrap();
        assert_eq!(came_back, Predicate::ReacquiredSameAsset { times: 1 });
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
        assert!(Predicate::AnySpanAtLeast { days: 1 }.needs_holder_coverage());
        assert!(
            !Predicate::HasAllTraits {
                category: "Faction".into(),
                values: vec!["Genovault".into()]
            }
            .needs_holder_coverage()
        );
        // A marketplace fact does not depend on the holder walk being complete.
        assert!(!Predicate::MarketPurchase.needs_holder_coverage());
        assert!(Predicate::MarketPurchase.needs_market_facts());
        assert!(!Predicate::NeverDisposed.needs_market_facts());
    }
}
