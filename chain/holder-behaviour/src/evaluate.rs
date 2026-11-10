//! Reading a subject's lots as achievements, and saying what is missing.
//!
//! # Earned, pending, missed, unavailable
//!
//! Four answers, and the difference between the last three is the whole point:
//!
//! - [`Progress::Earned`] — the predicate is true, and the outcome cites what proves it.
//! - [`Progress::Pending`] — not yet true, and the subject is on a route to it. For a
//!   tenure predicate this carries the number reached, the number needed and **the unix
//!   second at which the currently open lots get there**, which is free: the interval is
//!   already stored, so no clock and no nightly job are needed to keep a near-miss fresh.
//! - [`Progress::Missed`] — a one-way predicate the subject has already failed. Only
//!   `never_disposed` can produce this, because it is the only predicate whose answer
//!   cannot change back.
//! - [`Progress::Unavailable`] — the source cannot answer, named by [`Gap`]. Never a zero:
//!   an achievement scored zero because a metadata pass had not run is indistinguishable
//!   from a holder who did not earn it, and that is the failure the design doc forbids.
//!
//! # When it became true
//!
//! [`EarnedSince::Exact`] is the slot the fact itself happened — an acquisition, a mint.
//! [`EarnedSince::AsAt`] is "true as at this reading": a summed total crossed its
//! threshold somewhere between the last event and now, and this fold does not reconstruct
//! which disposal did it. Saying so is cheaper than a guess, and a runtime that evaluates
//! per event latches the exact slot itself.

//! # Two clocks
//!
//! A chain coordinate is a height, and a height is not a duration: one Cardano slot is
//! one second, but one EVM block is not, so a predicate stated in days is measured on
//! **unix seconds** and never on the height. [`Assessment`] therefore carries both: `now`
//! is the height the lots are read at (what an `EarnedSince` cites), and `now_unix` is the
//! wall clock the open intervals are measured at. A pending tenure reports its ETA on the
//! unix axis, which is a date a reader can show; a rank still sorts by height.

use chain_events::{Coverage, Currency, PartyRole, TxRef};

use std::collections::{BTreeMap, BTreeSet};

use crate::predicate::Predicate;
use crate::state::{
    AcquiredBy, Fill, FloorSeries, Holding, LotId, MarketHistory, MarketSide, MintFact,
    MintSummary, Placement, Slot, Store, SubjectId, UnitId,
};

/// One subject's trait rows, read once per evaluation.
///
/// Keyed by unit so a predicate reads the map instead of the store. A unit present with an
/// empty `Vec` is a unit whose metadata was never recorded — "no rows", which every trait
/// predicate reports as `Unavailable` rather than as a miss.
type TraitMap = BTreeMap<UnitId, Vec<(String, String)>>;

/// Seconds in a day.
///
/// The predicate vocabulary states durations in days, and this is the only conversion — to
/// seconds, never to a height. Cardano also keeps one slot per second (verified against the
/// archive: a 25,198,962-slot span of Perps history is exactly 25,198,962 unix seconds), but
/// nothing here relies on that, because an EVM block is not a second.
pub const SECONDS_PER_DAY: u64 = 86_400;

/// What the caller believes about the store it is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assessment {
    /// The chain height to read the open intervals at. A rank and an evidence citation use
    /// this axis; it is not a duration.
    pub now: Slot,
    /// Unix seconds to read the open intervals at — the clock every duration is measured on.
    ///
    /// Separate from [`Assessment::now`] because a block number is not a second. On Cardano
    /// the two advance together, one slot per second; nothing here assumes that.
    pub now_unix: u64,
    /// How much of the holder family this store's source can account for.
    ///
    /// [`Coverage::Partial`] makes every span and `never_*` predicate `Unavailable`
    /// rather than short: a span truncated by a missing movement is not a near miss.
    pub holder_coverage: Coverage,
}

impl Assessment {
    /// An assessment at `now` (height) and `now_unix` (seconds) over a source that has
    /// proven completeness.
    pub const fn complete(now: Slot, now_unix: u64) -> Self {
        Self {
            now,
            now_unix,
            holder_coverage: Coverage::Complete,
        }
    }
}

/// One achievement, as the catalogue states it.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition<'a> {
    /// The catalogue id, copied onto every outcome.
    pub id: &'a str,
    /// What the achievement requires.
    pub predicate: Predicate,
}

/// Why a predicate cannot be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    /// The holder history is not complete, so a span or a "never" would be wrong.
    HolderCoverage,
    /// No trait rows are recorded for any held asset.
    Traits,
    /// The store never saw the collection mint, so its mint order is unknown.
    MintFacts,
    /// The store carries no marketplace facts — it was built from a holder-only stream.
    MarketFacts,
    /// A fill the source did not key to its order, so the listing's age is unknown.
    OrderAge,
    /// A fill the source did not price, so a sale cannot be compared with anything.
    FillPrice,
    /// The store does not carry per-subject movement counts — a bundle row does not.
    MovementFacts,
    /// The collection's floor is not known at the moment the predicate reads.
    FloorFacts,
}

/// What a predicate currently says about a subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// True, and observed at the stated slot.
    Earned {
        /// When it became true, as far as this reading can tell.
        since: EarnedSince,
    },
    /// Not yet true, with how far along and — when the open lots will get there — when.
    Pending {
        /// How much has been accumulated.
        have: u64,
        /// How much is required.
        need: u64,
        /// Unix seconds at which the currently open lots reach `need`, when they ever do.
        ///
        /// A timestamp, not a height: it is what a reader renders as a date. `None` means
        /// nothing in hand will change the answer: the subject holds none, or holds
        /// nothing that accrues toward this predicate.
        eta_unix: Option<u64>,
    },
    /// A one-way predicate the subject has already failed.
    Missed,
    /// The source cannot answer, and the reason is named.
    Unavailable(Gap),
}

/// When an earned predicate became true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarnedSince {
    /// The coordinate the fact itself happened at.
    Exact(Slot),
    /// True as at this reading; the crossing coordinate was not reconstructed.
    AsAt {
        /// The chain height of the reading.
        slot: Slot,
        /// Unix seconds of the reading.
        unix: u64,
    },
}

/// A pointer at the thing that proves an outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    /// One acquisition. `disposed_at` is `None` while any of it is still held.
    Lot {
        /// Which asset.
        unit: UnitId,
        /// Which acquisition of it.
        lot: LotId,
        /// When it arrived.
        acquired_at: Slot,
        /// When it finished leaving, if it has.
        disposed_at: Option<Slot>,
        /// The transaction that delivered it — the citation a holder can open.
        acquired_tx: Option<TxRef>,
    },
    /// One asset's traits that satisfied a set predicate.
    Traits {
        /// Which asset.
        unit: UnitId,
        /// The values that were found, in the predicate's order.
        values: Vec<String>,
    },
    /// The units open at the instant a simultaneous holding crossed its threshold.
    HeldAtOnce {
        /// One entry per distinct unit open at that slot.
        units: Vec<UnitId>,
    },
    /// One marketplace fact.
    Market {
        /// The order, when the source named one.
        order: Option<String>,
        /// What the order was, or what was taken.
        side: MarketSide,
        /// `true` for a settled trade, `false` for a placement.
        filled: bool,
        /// When it happened, where the chain coordinate is known.
        at: Option<Slot>,
        /// Unix seconds it happened.
        unix: u64,
    },
    /// A movement count against the assets it was measured over.
    Movements {
        /// How many holder movements the subject was a party to.
        movements: u64,
        /// How many distinct units it ever held.
        units: u64,
    },
}

/// One achievement's answer for one subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome<'a> {
    /// The catalogue id this answers.
    pub id: &'a str,
    /// Earned, pending, missed or unavailable.
    pub progress: Progress,
    /// What proves it. Empty for a pending or unavailable outcome.
    pub evidence: Vec<Evidence>,
    /// For an earned holding, whether it is still held. [`Standing::Record`] for everything
    /// else.
    pub standing: Standing,
    /// For a holding held now, how many pieces satisfy it: each Leader held, each Perp carrying
    /// the trait. Zero for anything not held now. A grant can be per piece.
    pub held: u16,
}

/// Whether an earned achievement is a record of an act or a holding, and if a holding, whether
/// the subject still has it.
///
/// A score reads this: "holds a faction head" is worth more than "once held one", while "minted
/// in the first hour" has no present tense to lose. See [`Predicate::is_standing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Standing {
    /// Something that happened, or anything not earned. Has no "now".
    #[default]
    Record,
    /// A holding, and the open lots still satisfy it.
    Now,
    /// A holding the subject once satisfied and no longer does.
    Past,
    /// A holding the subject still owns, but only through units it has listed for sale. It
    /// scores as [`Standing::Past`] — a Perp for sale is not a Perp kept — and unlisting is
    /// what turns it back into [`Standing::Now`], which is what a page can tell the holder.
    Listed,
}

impl Standing {
    /// The tightest of two readings of one achievement: an identity that holds it through any
    /// wallet holds it now, and one that has it only for sale is closer to holding it than one
    /// that sold it.
    pub fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Now, _) | (_, Self::Now) => Self::Now,
            (Self::Listed, _) | (_, Self::Listed) => Self::Listed,
            (Self::Past, _) | (_, Self::Past) => Self::Past,
            _ => Self::Record,
        }
    }

    /// Whether it is a holding not held now: sold, or for sale. Neither grants, and both score
    /// at the past share.
    pub fn is_lapsed_holding(self) -> bool {
        matches!(self, Self::Past | Self::Listed)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Now => "now",
            Self::Past => "past",
            Self::Listed => "listed",
        }
    }
}

/// Evaluates every definition for one subject.
///
/// One read of the subject's lots serves every predicate, so adding a definition does not
/// add a query. Trait rows are read per held asset, not per definition.
pub fn evaluate<'a, S: Store>(
    store: &S,
    subject: SubjectId,
    at: &Assessment,
    definitions: &[Definition<'a>],
) -> Result<Vec<Outcome<'a>>, S::Error> {
    let lots = store.lots(subject)?;
    // What is held now, and which of it is for sale: read once for every standing predicate.
    let open: Vec<Holding> = lots.iter().filter(|lot| lot.is_open()).cloned().collect();
    let listed: Vec<UnitId> = store.listed_units(subject)?;
    // One read per held unit, not one per definition — the metadata join is the expensive
    // part, and nineteen curation predicates must not pay for it nineteen times.
    let traits = if definitions
        .iter()
        .any(|definition| definition.predicate.needs_traits())
    {
        read_traits(store, &lots)?
    } else {
        TraitMap::new()
    };
    // The collection's mint order, read once for the same reason the traits are.
    let mints = if definitions
        .iter()
        .any(|definition| definition.predicate.needs_mint_facts())
    {
        read_mints(store, &lots)?
    } else {
        MintView::default()
    };
    // Marketplace facts, read once. `None` is "this store has no market source", which
    // every market predicate reports as a gap rather than as no activity.
    let market = if definitions
        .iter()
        .any(|definition| definition.predicate.needs_market_facts())
    {
        store.market_history(subject)?
    } else {
        None
    };
    // The floor over time, read once. It is derived from the same marketplace feed, but it
    // is a collection-level series rather than a per-subject history.
    let floor = if definitions
        .iter()
        .any(|definition| definition.predicate.needs_floor_facts())
    {
        store.floor_series()?
    } else {
        None
    };
    // The subject's movement count, read once. `None` is "this store does not carry one",
    // which a ratio predicate reports as a gap rather than as no activity.
    let movements = if definitions
        .iter()
        .any(|definition| definition.predicate.needs_movements())
    {
        store.movements_of(subject)?
    } else {
        None
    };
    let mut outcomes = Vec::with_capacity(definitions.len());
    for definition in definitions {
        let (progress, evidence) = match &definition.predicate {
            Predicate::NeverDisposed => never_disposed(&lots, at),
            Predicate::HeldSinceMint => held_since_mint(&lots, at),
            Predicate::TotalTenureAtLeast { days } => {
                total_tenure_at_least(&lots, at, u64::from(*days) * SECONDS_PER_DAY)
            }
            Predicate::AnySpanAtLeast { days } => {
                any_span_at_least(&lots, at, u64::from(*days) * SECONDS_PER_DAY)
            }
            Predicate::SpanAtLeast { days, assets } => span_at_least(
                &lots,
                at,
                u64::from(*days) * SECONDS_PER_DAY,
                u64::from(*assets),
            ),
            Predicate::HoldsNeverMoved => holds_never_moved(&lots, at),
            Predicate::ReacquiredSameAsset { times } => {
                reacquired_same_asset(&lots, at, u64::from(*times))
            }
            Predicate::ReturnedAfterIdle { days } => {
                returned_after_idle(&lots, at, u64::from(*days) * SECONDS_PER_DAY)
            }
            Predicate::RoundTripsAtLeast { n } => round_trips_at_least(&lots, at, u64::from(*n)),
            Predicate::MarketPurchase => market_purchase(market.as_ref(), at),
            Predicate::ListedCount { at_least } => {
                listed_count(market.as_ref(), at, u64::from(*at_least))
            }
            Predicate::BoughtEscrowWithin { seconds } => {
                bought_escrow_within(market.as_ref(), at, *seconds)
            }
            Predicate::BoughtListingOlderThan { days } => {
                bought_listing_older_than(market.as_ref(), at, u64::from(*days) * SECONDS_PER_DAY)
            }
            Predicate::BoughtViaCollectionOffer => {
                bought_offer(market.as_ref(), at, MarketSide::CollectionBid)
            }
            Predicate::OfferAcceptedAsBuyer => {
                bought_offer(market.as_ref(), at, MarketSide::ItemBid)
            }
            Predicate::OfferCreated => offer_created(market.as_ref(), at),
            Predicate::NoSaleBelowMint { amount, decimals } => {
                no_sale_below_mint(market.as_ref(), at, *amount, *decimals)
            }
            Predicate::MovementRatioAtLeast { ratio } => {
                movement_ratio_at_least(movements, &lots, at, *ratio)
            }
            Predicate::SoldAtOrBelowFloor => {
                sold_against_floor(market.as_ref(), at, |price, floor| price <= floor)
            }
            Predicate::SoldAboveFloor => {
                sold_against_floor(market.as_ref(), at, |price, floor| price > floor)
            }
            Predicate::SoldIntoStrength => sold_into_strength(market.as_ref(), at),
            Predicate::AcquiredDuringDrawdown { fraction } => {
                acquired_during_drawdown(&lots, floor.as_ref(), at, *fraction)
            }
            Predicate::DrawdownCovered { fraction } => {
                drawdown_covered(&lots, floor.as_ref(), at, *fraction)
            }
            Predicate::HasAllTraits { category, values } => {
                has_all_traits(&traits, &lots, at, category, values)
            }
            Predicate::HasTrait { category, value } => {
                has_trait(&traits, &lots, at, category, value)
            }
            Predicate::HasTraitValues {
                category,
                values,
                at_least,
            } => has_trait_values(&traits, &lots, at, category, values, *at_least),
            Predicate::HeldWithTraitAtOnce {
                category,
                value,
                at_least,
            } => held_with_trait_at_once(&traits, &lots, at, category, value.as_deref(), *at_least),
            Predicate::HoldsAtLeast { at_least } => holds_at_least(&lots, at, *at_least),
            Predicate::SameValueAcrossAssets { category, assets } => {
                same_value_across_assets(&traits, &lots, at, category, *assets)
            }
            Predicate::MintWithin { seconds } => mint_within(&mints, &lots, at, *seconds),
            Predicate::MintRankAtMost { n } => mint_rank(&mints, &lots, at, |rank| rank <= *n),
            Predicate::MintRankAtLeast { n } => mint_rank(&mints, &lots, at, |rank| rank >= *n),
            Predicate::MintRankAfterFraction { fraction } => {
                match mints.summary {
                    // The fraction is of the whole mint, so a partial count is not an answer.
                    None => (Progress::Unavailable(Gap::MintFacts), Vec::new()),
                    Some(summary) => {
                        let total = f64::from(summary.count);
                        mint_rank(&mints, &lots, at, |rank| {
                            f64::from(rank) / total > *fraction
                        })
                    }
                }
            }
            Predicate::MintAfterGap { days } => {
                mint_after_gap(&mints, &lots, at, u64::from(*days) * SECONDS_PER_DAY)
            }
            Predicate::MintedCount { at_least } => minted_count(&lots, at, u64::from(*at_least)),
            Predicate::MintedAndStillHeld { at_least } => {
                minted_and_still_held(&lots, at, u64::from(*at_least))
            }
            Predicate::AllMintsRetained => all_mints_retained(&lots, at),
            Predicate::NeverBought => never_bought(&lots, at),
        };
        let (standing, held) = match (definition.predicate.is_standing(), progress) {
            (true, Progress::Earned { .. }) => {
                // Held and not for sale first; then held at all, which is a listing away.
                let kept: Vec<Holding> = open
                    .iter()
                    .filter(|lot| !listed.contains(&lot.unit))
                    .cloned()
                    .collect();
                if held_now(&definition.predicate, &traits, &kept, at) {
                    (
                        Standing::Now,
                        pieces_held(&definition.predicate, &traits, &kept),
                    )
                } else if kept.len() < open.len()
                    && held_now(&definition.predicate, &traits, &open, at)
                {
                    (Standing::Listed, 0)
                } else {
                    (Standing::Past, 0)
                }
            }
            _ => (Standing::Record, 0),
        };
        outcomes.push(Outcome {
            id: definition.id,
            progress,
            evidence,
            standing,
            held,
        });
    }
    Ok(outcomes)
}

/// Whether a standing predicate still holds over the open lots alone.
///
/// The same functions the full answer uses, given only what is held: "ever held" over the open
/// lots is "holds now", and a peak over lots that are all open at once is the count held now.
fn held_now(predicate: &Predicate, traits: &TraitMap, open: &[Holding], at: &Assessment) -> bool {
    let (progress, _) = match predicate {
        Predicate::HasAllTraits { category, values } => {
            has_all_traits(traits, open, at, category, values)
        }
        Predicate::HasTrait { category, value } => has_trait(traits, open, at, category, value),
        Predicate::HasTraitValues {
            category,
            values,
            at_least,
        } => has_trait_values(traits, open, at, category, values, *at_least),
        Predicate::HeldWithTraitAtOnce {
            category,
            value,
            at_least,
        } => held_with_trait_at_once(traits, open, at, category, value.as_deref(), *at_least),
        Predicate::HoldsAtLeast { at_least } => holds_at_least(open, at, *at_least),
        Predicate::SameValueAcrossAssets { category, assets } => {
            same_value_across_assets(traits, open, at, category, *assets)
        }
        _ => return false,
    };
    matches!(progress, Progress::Earned { .. })
}

/// How many pieces satisfy a holding held now: the distinct kept units carrying the trait, for a
/// one-trait marker (each Leader held is its own piece), and one for any other holding, which is
/// met as a whole rather than piece by piece.
fn pieces_held(predicate: &Predicate, traits: &TraitMap, kept: &[Holding]) -> u16 {
    let count = match predicate {
        Predicate::HasTrait { category, value } => kept
            .iter()
            .filter(|lot| matches_value(traits, lot.unit, category, Some(value.as_str())))
            .map(|lot| lot.unit)
            .collect::<BTreeSet<UnitId>>()
            .len(),
        Predicate::HasTraitValues {
            category, values, ..
        } => kept
            .iter()
            .filter(|lot| {
                values
                    .iter()
                    .any(|value| matches_value(traits, lot.unit, category, Some(value.as_str())))
            })
            .map(|lot| lot.unit)
            .collect::<BTreeSet<UnitId>>()
            .len(),
        _ => 1,
    };
    u16::try_from(count.max(1)).unwrap_or(u16::MAX)
}

/// Reads every unit's trait rows once.
fn read_traits<S: Store>(store: &S, lots: &[Holding]) -> Result<TraitMap, S::Error> {
    let mut traits = TraitMap::new();
    for unit in units_ever(lots) {
        traits.insert(unit, store.traits_of(unit)?);
    }
    Ok(traits)
}

/// The collection's mint order, for one subject's units.
#[derive(Default)]
struct MintView {
    facts: BTreeMap<UnitId, MintFact>,
    summary: Option<MintSummary>,
}

fn read_mints<S: Store>(store: &S, lots: &[Holding]) -> Result<MintView, S::Error> {
    let mut facts = BTreeMap::new();
    for unit in units_ever(lots) {
        if let Some(fact) = store.mint_of(unit)? {
            facts.insert(unit, fact);
        }
    }
    Ok(MintView {
        facts,
        summary: store.mint_summary()?,
    })
}

/// The distinct units a subject has ever held, ascending.
fn units_ever(lots: &[Holding]) -> Vec<UnitId> {
    let mut units: Vec<UnitId> = lots.iter().map(|lot| lot.unit).collect();
    units.sort_unstable();
    units.dedup();
    units
}

/// Whether any unit the subject held has no trait rows at all.
///
/// Metadata coverage, not trait membership: an asset that carries no `Faction` is still
/// *known*, and the answer for it is a real no. An asset whose rows were never recorded is
/// unknown, and a predicate that did not match must say so.
fn any_unknown(traits: &TraitMap, units: &[UnitId]) -> bool {
    units
        .iter()
        .any(|unit| traits.get(unit).is_none_or(Vec::is_empty))
}

fn evidence_of(lot: &Holding) -> Evidence {
    Evidence::Lot {
        unit: lot.unit,
        lot: lot.lot,
        acquired_at: lot.acquired_at,
        disposed_at: lot.disposed_at,
        acquired_tx: lot.acquired_tx.clone(),
    }
}

fn never_disposed(lots: &[Holding], at: &Assessment) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let Some(first) = lots.iter().min_by_key(|lot| lot.acquired_at) else {
        // Nothing acquired yet: the route is open, nothing is scheduled to complete it.
        return (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        );
    };
    if lots.iter().any(Holding::is_disposed) {
        return (Progress::Missed, Vec::new());
    }
    (
        Progress::Earned {
            since: EarnedSince::Exact(first.acquired_at),
        },
        vec![evidence_of(first)],
    )
}

fn held_since_mint(lots: &[Holding], at: &Assessment) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let mint_lot = lots
        .iter()
        .filter(|lot| lot.acquired_by == crate::state::AcquiredBy::Mint && lot.is_open())
        .min_by_key(|lot| lot.acquired_at);
    match mint_lot {
        Some(lot) => (
            Progress::Earned {
                since: EarnedSince::Exact(lot.acquired_at),
            },
            vec![evidence_of(lot)],
        ),
        // Never `Missed`: a mint the subject sold out of could be replaced by another
        // mint while the window is open, and this fold is not told when it closes.
        None => (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        ),
    }
}

fn total_tenure_at_least(
    lots: &[Holding],
    at: &Assessment,
    need: u64,
) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let have: u64 = lots.iter().map(|lot| lot.tenure_at(at.now_unix)).sum();
    if have >= need {
        // The threshold was crossed somewhere between the last event and `now`, and the
        // lots alone do not say which disposal did it. Cite the earliest acquisition: it
        // is the transaction that started the clock.
        let earliest = lots.iter().min_by_key(|lot| lot.acquired_at);
        let evidence = earliest.map(evidence_of).into_iter().collect();
        return (
            Progress::Earned {
                since: EarnedSince::AsAt {
                    slot: at.now,
                    unix: at.now_unix,
                },
            },
            evidence,
        );
    }

    // One open lot accrues one second per second however many units it holds, because
    // tenure is the length of an interval and not its size. The ETA is therefore a
    // timestamp: the reading's clock plus the remaining seconds shared across the lots.
    let open_lots = lots.iter().filter(|lot| lot.is_open()).count() as u64;
    let eta_unix = (open_lots > 0).then(|| {
        let remaining = need - have;
        at.now_unix.saturating_add(remaining.div_ceil(open_lots))
    });
    (
        Progress::Pending {
            have,
            need,
            eta_unix,
        },
        Vec::new(),
    )
}

/// The lot with the longest interval as at `now_unix`, earliest acquisition breaking a tie.
fn longest_span(lots: &[Holding], now_unix: u64) -> Option<(&Holding, u64)> {
    let mut best: Option<(&Holding, u64)> = None;
    for lot in lots {
        let span = lot.tenure_at(now_unix);
        let replace = match best {
            None => true,
            Some((held, longest)) => {
                span > longest
                    || (span == longest
                        && (lot.acquired_at, lot.lot) < (held.acquired_at, held.lot))
            }
        };
        if replace {
            best = Some((lot, span));
        }
    }
    best
}

/// One lot's continuous interval reached `need` seconds.
fn any_span_at_least(lots: &[Holding], at: &Assessment, need: u64) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let need = need.max(1);
    let best = longest_span(lots, at.now_unix);
    let have = best.map_or(0, |(_, span)| span);
    if have >= need
        && let Some((lot, _)) = best
    {
        return (
            Progress::Earned {
                since: EarnedSince::AsAt {
                    slot: at.now,
                    unix: at.now_unix,
                },
            },
            vec![evidence_of(lot)],
        );
    }
    // The earliest open lot to reach the threshold. One already at it would have earned.
    let eta_unix = lots
        .iter()
        .filter(|lot| lot.is_open())
        .map(|lot| lot.acquired_unix.saturating_add(need))
        .min();
    (
        Progress::Pending {
            have,
            need,
            eta_unix,
        },
        Vec::new(),
    )
}

/// `assets` distinct units each had a continuous interval of at least `seconds`.
fn span_at_least(
    lots: &[Holding],
    at: &Assessment,
    seconds: u64,
    assets: u64,
) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let need = assets.max(1);
    let seconds = seconds.max(1);
    let mut per_unit: BTreeMap<UnitId, (&Holding, u64)> = BTreeMap::new();
    for lot in lots {
        let span = lot.tenure_at(at.now_unix);
        let replace = match per_unit.get(&lot.unit) {
            None => true,
            Some((held, best)) => {
                span > *best
                    || (span == *best && (lot.acquired_at, lot.lot) < (held.acquired_at, held.lot))
            }
        };
        if replace {
            per_unit.insert(lot.unit, (lot, span));
        }
    }
    let mut reached: Vec<(&Holding, u64)> = per_unit
        .values()
        .filter(|(_, span)| *span >= seconds)
        .copied()
        .collect();
    reached.sort_by_key(|(lot, _)| (lot.acquired_unix, lot.lot));
    let have = reached.len() as u64;
    if have >= need {
        let evidence = reached
            .iter()
            .take(need as usize)
            .map(|(lot, _)| evidence_of(lot))
            .collect();
        return (
            Progress::Earned {
                since: EarnedSince::AsAt {
                    slot: at.now,
                    unix: at.now_unix,
                },
            },
            evidence,
        );
    }

    // Units that will reach it while still open, and the earliest instant each one crosses.
    let mut crossings: BTreeMap<UnitId, u64> = BTreeMap::new();
    for lot in lots {
        if !lot.is_open()
            || per_unit
                .get(&lot.unit)
                .is_some_and(|(_, span)| *span >= seconds)
        {
            continue;
        }
        let crossing = lot.acquired_unix.saturating_add(seconds);
        crossings
            .entry(lot.unit)
            .and_modify(|earliest| *earliest = (*earliest).min(crossing))
            .or_insert(crossing);
    }
    let mut crossings: Vec<u64> = crossings.into_values().collect();
    crossings.sort_unstable();
    let remaining = (need - have) as usize;
    let eta_unix = crossings.get(remaining - 1).copied();
    (
        Progress::Pending {
            have,
            need,
            eta_unix,
        },
        Vec::new(),
    )
}

/// Holds a unit whose only movement since the mint was the mint itself.
fn holds_never_moved(lots: &[Holding], at: &Assessment) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let untouched = lots
        .iter()
        .filter(|lot| lot.acquired_by == AcquiredBy::Mint && lot.is_open())
        .min_by_key(|lot| (lot.acquired_at, lot.lot));
    match untouched {
        Some(lot) => (
            Progress::Earned {
                since: EarnedSince::Exact(lot.acquired_at),
            },
            vec![evidence_of(lot)],
        ),
        None => (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        ),
    }
}

/// Counts reacquisitions: a unit that fully left, then arrived again from another party.
fn reacquired_same_asset(
    lots: &[Holding],
    at: &Assessment,
    times: u64,
) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let need = times.max(1);
    let mut by_unit: BTreeMap<UnitId, Vec<&Holding>> = BTreeMap::new();
    for lot in lots {
        by_unit.entry(lot.unit).or_default().push(lot);
    }
    let mut proved: Vec<&Holding> = Vec::new();
    for held in by_unit.values_mut() {
        held.sort_by_key(|lot| (lot.acquired_at, lot.lot));
        let mut closed: Option<Slot> = None;
        for lot in held.iter().copied() {
            if lot.acquired_by == AcquiredBy::Transfer
                && let Some(disposed) = closed
                && lot.acquired_at >= disposed
            {
                proved.push(lot);
            }
            if let Some(disposed) = lot.disposed_at {
                closed = Some(closed.map_or(disposed, |latest| latest.max(disposed)));
            }
        }
    }
    proved.sort_by_key(|lot| (lot.acquired_at, lot.lot));
    let have = proved.len() as u64;
    if have >= need {
        let proving = proved[(need - 1) as usize];
        return (
            Progress::Earned {
                since: EarnedSince::Exact(proving.acquired_at),
            },
            proved
                .iter()
                .take(need as usize)
                .map(|lot| evidence_of(lot))
                .collect(),
        );
    }
    (
        Progress::Pending {
            have,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// An idle gap of at least `gap` seconds between two holding periods.
fn returned_after_idle(lots: &[Holding], at: &Assessment, gap: u64) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let gap = gap.max(1);
    // Half-open holding intervals, with a zero-length lot dropped: it covers no instant and
    // would otherwise look like a holding period that ended where it started.
    let mut spans: Vec<(u64, u64, &Holding)> = lots
        .iter()
        .filter_map(|lot| {
            let end = lot.disposed_unix.unwrap_or(at.now_unix);
            (end > lot.acquired_unix).then_some((lot.acquired_unix, end, lot))
        })
        .collect();
    spans.sort_by_key(|(start, end, lot)| (*start, *end, lot.lot));

    let mut merged_end: Option<(u64, &Holding)> = None;
    for (start, end, lot) in spans {
        if let Some((previous, _)) = merged_end
            && start.saturating_sub(previous) >= gap
        {
            return (
                Progress::Earned {
                    since: EarnedSince::Exact(lot.acquired_at),
                },
                vec![evidence_of(lot)],
            );
        }
        merged_end = Some(match merged_end {
            Some((previous, held)) if previous >= end => (previous, held),
            _ => (end, lot),
        });
    }
    (
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None,
        },
        Vec::new(),
    )
}

fn round_trips_at_least(lots: &[Holding], at: &Assessment, need: u64) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let mut completed: Vec<&Holding> = lots
        .iter()
        .filter(|lot| lot.acquired_by == crate::state::AcquiredBy::Transfer && lot.is_closed())
        .collect();
    completed.sort_by_key(|lot| (lot.disposed_at, lot.acquired_at, lot.lot));

    let have = completed.len() as u64;
    if have >= need {
        let proving = completed
            .get(need.saturating_sub(1) as usize)
            .map(|lot| evidence_of(lot));
        let evidence = proving.into_iter().collect();
        return (
            Progress::Earned {
                since: EarnedSince::AsAt {
                    slot: at.now,
                    unix: at.now_unix,
                },
            },
            evidence,
        );
    }
    // A round trip needs a purchase and a sale; neither is on a schedule, so there is no
    // timestamp to promise.
    (
        Progress::Pending {
            have,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

// ============================================================================
// Market — what the subject did on a marketplace
// ============================================================================

/// When a marketplace fact became true: its own coordinate, or the reading's when the
/// source did not give one (an OpenSea message names a wall clock and no block).
fn market_since(at: Option<Slot>, reading: &Assessment) -> EarnedSince {
    match at {
        Some(slot) => EarnedSince::Exact(slot),
        None => EarnedSince::AsAt {
            slot: reading.now,
            unix: reading.now_unix,
        },
    }
}

fn fill_evidence(fill: &Fill) -> Evidence {
    Evidence::Market {
        order: fill.order.clone(),
        side: fill.side,
        filled: true,
        at: fill.at,
        unix: fill.unix,
    }
}

fn placement_evidence(placement: &Placement) -> Evidence {
    Evidence::Market {
        order: Some(placement.order.clone()),
        side: placement.side,
        filled: false,
        at: placement.at,
        unix: placement.unix,
    }
}

/// Where the subject was recorded as the buyer in any settled trade.
fn market_purchase(history: Option<&MarketHistory>, at: &Assessment) -> (Progress, Vec<Evidence>) {
    let Some(history) = history else {
        return (Progress::Unavailable(Gap::MarketFacts), Vec::new());
    };
    let bought = history
        .fills
        .iter()
        .filter(|fill| fill.role == PartyRole::Buyer)
        .min_by_key(|fill| fill.unix);
    match bought {
        Some(fill) => (
            Progress::Earned {
                since: market_since(fill.at, at),
            },
            vec![fill_evidence(fill)],
        ),
        None => (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        ),
    }
}

/// How many distinct orders the subject listed.
fn listed_count(
    history: Option<&MarketHistory>,
    at: &Assessment,
    need: u64,
) -> (Progress, Vec<Evidence>) {
    let Some(history) = history else {
        return (Progress::Unavailable(Gap::MarketFacts), Vec::new());
    };
    let need = need.max(1);
    let mut listed: Vec<&Placement> = history
        .placements
        .iter()
        .filter(|placement| placement.side == MarketSide::Ask)
        .collect();
    listed.sort_by_key(|placement| (placement.unix, placement.order.as_str()));
    let have = listed.len() as u64;
    if have >= need {
        let proving = listed[(need - 1) as usize];
        return (
            Progress::Earned {
                since: market_since(proving.at, at),
            },
            listed
                .iter()
                .take(need as usize)
                .map(|placement| placement_evidence(placement))
                .collect(),
        );
    }
    (
        Progress::Pending {
            have,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// A purchase of an ask whose age satisfies `accept`.
///
/// An ask fill the source did not key has no age, so when none qualifies and at least one
/// such fill exists the answer is the gap: an unkeyed fill is not a fast or a stale one.
fn bought_with_age(
    history: Option<&MarketHistory>,
    at: &Assessment,
    accept: impl Fn(u64) -> bool,
) -> (Progress, Vec<Evidence>) {
    let Some(history) = history else {
        return (Progress::Unavailable(Gap::MarketFacts), Vec::new());
    };
    let mut proved: Option<&Fill> = None;
    let mut unkeyed = false;
    for fill in &history.fills {
        if fill.role != PartyRole::Buyer || fill.side != MarketSide::Ask {
            continue;
        }
        match fill.order_age {
            Some(age) if accept(age) => {
                if proved.is_none_or(|held| fill.unix < held.unix) {
                    proved = Some(fill);
                }
            }
            Some(_) => {}
            None => unkeyed = true,
        }
    }
    if let Some(fill) = proved {
        return (
            Progress::Earned {
                since: market_since(fill.at, at),
            },
            vec![fill_evidence(fill)],
        );
    }
    if unkeyed {
        return (Progress::Unavailable(Gap::OrderAge), Vec::new());
    }
    (
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None,
        },
        Vec::new(),
    )
}

fn bought_escrow_within(
    history: Option<&MarketHistory>,
    at: &Assessment,
    seconds: u64,
) -> (Progress, Vec<Evidence>) {
    bought_with_age(history, at, |age| age <= seconds)
}

fn bought_listing_older_than(
    history: Option<&MarketHistory>,
    at: &Assessment,
    seconds: u64,
) -> (Progress, Vec<Evidence>) {
    bought_with_age(history, at, |age| age >= seconds)
}

/// The subject bought through a bid of one particular scope.
fn bought_offer(
    history: Option<&MarketHistory>,
    at: &Assessment,
    side: MarketSide,
) -> (Progress, Vec<Evidence>) {
    let Some(history) = history else {
        return (Progress::Unavailable(Gap::MarketFacts), Vec::new());
    };
    let bought = history
        .fills
        .iter()
        .filter(|fill| fill.role == PartyRole::Buyer && fill.side == side)
        .min_by_key(|fill| fill.unix);
    match bought {
        Some(fill) => (
            Progress::Earned {
                since: market_since(fill.at, at),
            },
            vec![fill_evidence(fill)],
        ),
        None => (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        ),
    }
}

/// The subject placed an offer on one asset.
fn offer_created(history: Option<&MarketHistory>, at: &Assessment) -> (Progress, Vec<Evidence>) {
    let Some(history) = history else {
        return (Progress::Unavailable(Gap::MarketFacts), Vec::new());
    };
    let placed = history
        .placements
        .iter()
        .filter(|placement| placement.side == MarketSide::ItemBid)
        .min_by_key(|placement| placement.unix);
    match placed {
        Some(placement) => (
            Progress::Earned {
                since: market_since(placement.at, at),
            },
            vec![placement_evidence(placement)],
        ),
        None => (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        ),
    }
}

/// The subject never sold below the mint price.
///
/// Strict in the direction that matters: an unpriced sale leaves the answer unprovable
/// rather than earned, because it could have been the below-mint one. A subject that never
/// sold reports progress, not a gap — `never_disposed` is the achievement for holding.
fn no_sale_below_mint(
    history: Option<&MarketHistory>,
    at: &Assessment,
    amount: u64,
    decimals: u8,
) -> (Progress, Vec<Evidence>) {
    let Some(history) = history else {
        return (Progress::Unavailable(Gap::MarketFacts), Vec::new());
    };
    let mint = u128::from(amount);
    let mut first_sale: Option<&Fill> = None;
    let mut unjudged = false;
    for fill in history
        .fills
        .iter()
        .filter(|fill| fill.role == PartyRole::Seller)
    {
        let Some(price) = fill.price.as_ref() else {
            unjudged = true;
            continue;
        };
        if !matches!(price.currency, Currency::Native) || price.decimals != decimals {
            unjudged = true;
            continue;
        }
        if price.amount < mint {
            return (Progress::Missed, Vec::new());
        }
        if first_sale.is_none_or(|held| fill.unix < held.unix) {
            first_sale = Some(fill);
        }
    }
    if unjudged {
        return (Progress::Unavailable(Gap::FillPrice), Vec::new());
    }
    if let Some(fill) = first_sale {
        return (
            Progress::Earned {
                since: market_since(fill.at, at),
            },
            vec![fill_evidence(fill)],
        );
    }
    (
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// Holder movements per asset ever held.
///
/// The denominator is the distinct units the subject ever held, so acquiring more raises the
/// bar as well as the count. A subject that never held one stays pending rather than
/// dividing by zero. `EarnedSince::AsAt` is the honest coordinate: the count crossed the
/// threshold somewhere between the last event and this reading, and no single slot can be
/// named for a ratio whose denominator also moves.
fn movement_ratio_at_least(
    movements: Option<u64>,
    lots: &[Holding],
    at: &Assessment,
    ratio: f64,
) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }
    let Some(movements) = movements else {
        return (Progress::Unavailable(Gap::MovementFacts), Vec::new());
    };
    let units = units_ever(lots).len() as u64;
    let need = (units as f64 * ratio).ceil() as u64;
    if units > 0 && movements >= need {
        return (
            Progress::Earned {
                since: EarnedSince::AsAt {
                    slot: at.now,
                    unix: at.now_unix,
                },
            },
            vec![Evidence::Movements { movements, units }],
        );
    }
    (
        Progress::Pending {
            have: movements,
            need: need.max(1),
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// A sale judged against the floor that stood when it settled.
///
/// The fill carries that floor, captured by the fold **before** the ask it consumed left the
/// book. Looking it up from the series afterwards would judge a sale against a floor its own
/// removal had raised.
fn sold_against_floor(
    history: Option<&MarketHistory>,
    at: &Assessment,
    accept: impl Fn(u128, u128) -> bool,
) -> (Progress, Vec<Evidence>) {
    let Some(history) = history else {
        return (Progress::Unavailable(Gap::MarketFacts), Vec::new());
    };
    let mut proved: Option<&Fill> = None;
    let mut unjudged = false;
    for fill in history
        .fills
        .iter()
        .filter(|fill| fill.role == PartyRole::Seller)
    {
        let Some((price, settled)) = price_against_floor(fill) else {
            unjudged = true;
            continue;
        };
        if accept(price, settled) && proved.is_none_or(|held| fill.unix < held.unix) {
            proved = Some(fill);
        }
    }
    if let Some(fill) = proved {
        return (
            Progress::Earned {
                since: market_since(fill.at, at),
            },
            vec![fill_evidence(fill)],
        );
    }
    if unjudged {
        return (Progress::Unavailable(Gap::FloorFacts), Vec::new());
    }
    (
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// The fill's price and the floor it settled against, when both are comparable.
///
/// `None` means the sale cannot be judged — no price, no floor captured at the trade, or a
/// price in a denomination the floor is not stated in.
fn price_against_floor(fill: &Fill) -> Option<(u128, u128)> {
    let price = fill.price.as_ref()?;
    if !matches!(price.currency, Currency::Native) {
        return None;
    }
    let settled = fill.floor.as_ref()?;
    if price.decimals != settled.decimals {
        return None;
    }
    Some((price.amount, settled.amount))
}

/// A sale above the floor while the floor's last move was up.
fn sold_into_strength(
    history: Option<&MarketHistory>,
    at: &Assessment,
) -> (Progress, Vec<Evidence>) {
    let Some(history) = history else {
        return (Progress::Unavailable(Gap::MarketFacts), Vec::new());
    };
    let mut proved: Option<&Fill> = None;
    let mut unjudged = false;
    for fill in history
        .fills
        .iter()
        .filter(|fill| fill.role == PartyRole::Seller)
    {
        let Some((price, settled)) = price_against_floor(fill) else {
            unjudged = true;
            continue;
        };
        let Some(rising) = fill.floor_rising else {
            unjudged = true;
            continue;
        };
        if price > settled && rising && proved.is_none_or(|held| fill.unix < held.unix) {
            proved = Some(fill);
        }
    }
    if let Some(fill) = proved {
        return (
            Progress::Earned {
                since: market_since(fill.at, at),
            },
            vec![fill_evidence(fill)],
        );
    }
    if unjudged {
        return (Progress::Unavailable(Gap::FloorFacts), Vec::new());
    }
    (
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// An acquisition made while the floor was below its prior peak.
fn acquired_during_drawdown(
    lots: &[Holding],
    floor: Option<&FloorSeries>,
    at: &Assessment,
    fraction: f64,
) -> (Progress, Vec<Evidence>) {
    let Some(floor) = floor else {
        return (Progress::Unavailable(Gap::FloorFacts), Vec::new());
    };
    let need = basis_points(fraction);
    let mut have = 0u64;
    let mut proved: Option<(&Holding, Option<Slot>)> = None;
    let mut unjudged = false;
    for lot in lots {
        let Some(point) = floor.at(lot.acquired_unix) else {
            unjudged = true;
            continue;
        };
        let (Some(current), Some(peak)) = (point.floor, floor.peak_through(lot.acquired_unix))
        else {
            unjudged = true;
            continue;
        };
        if peak == 0 {
            continue;
        }
        let drawdown = 1.0 - current as f64 / peak as f64;
        let reached = basis_points(drawdown);
        if reached > have {
            have = reached;
            proved = Some((lot, point.at));
        }
    }
    if have >= need
        && let Some((lot, slot)) = proved
    {
        return (
            Progress::Earned {
                since: since_or_reading(slot, at),
            },
            vec![evidence_of(lot)],
        );
    }
    if unjudged {
        return (Progress::Unavailable(Gap::FloorFacts), Vec::new());
    }
    (
        Progress::Pending {
            have,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// A holding span that covered a floor drawdown, peak and trough both inside it.
fn drawdown_covered(
    lots: &[Holding],
    floor: Option<&FloorSeries>,
    at: &Assessment,
    fraction: f64,
) -> (Progress, Vec<Evidence>) {
    let Some(floor) = floor else {
        return (Progress::Unavailable(Gap::FloorFacts), Vec::new());
    };
    let need = basis_points(fraction);
    let mut have = 0u64;
    let mut proved: Option<(&Holding, Option<Slot>)> = None;
    let mut unjudged = false;
    for lot in lots {
        let start = lot.acquired_unix;
        let end = lot.disposed_unix.unwrap_or(at.now_unix);
        if end <= start {
            continue;
        }
        match floor.max_drawdown(start, end) {
            None => unjudged = true,
            Some(drawdown) => {
                let reached = basis_points(drawdown.fraction);
                if reached > have {
                    have = reached;
                    proved = Some((lot, drawdown.at));
                }
            }
        }
    }
    if have >= need
        && let Some((lot, slot)) = proved
    {
        return (
            Progress::Earned {
                since: since_or_reading(slot, at),
            },
            vec![evidence_of(lot)],
        );
    }
    if unjudged {
        return (Progress::Unavailable(Gap::FloorFacts), Vec::new());
    }
    (
        Progress::Pending {
            have,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// A fraction as basis points, which is the unit `Progress` reports it in.
fn basis_points(fraction: f64) -> u64 {
    (fraction.clamp(0.0, 1.0) * 10_000.0).round() as u64
}

fn since_or_reading(at: Option<Slot>, reading: &Assessment) -> EarnedSince {
    match at {
        Some(slot) => EarnedSince::Exact(slot),
        None => EarnedSince::AsAt {
            slot: reading.now,
            unix: reading.now_unix,
        },
    }
}

/// Every one of `values` was carried by a unit held **at the same instant**.
///
/// A set across assets, not one asset carrying all of them: `tri-faction` is "a Perp from
/// each faction, at once", and no Perp carries three factions. `has_trait_values` is the
/// ever-held form; this one requires overlap, so holding one, selling it, and buying
/// another later does not complete the set.
fn has_all_traits(
    traits: &TraitMap,
    lots: &[Holding],
    _at: &Assessment,
    category: &str,
    values: &[String],
) -> (Progress, Vec<Evidence>) {
    // A repeated value must not make the set look complete.
    let mut wanted: Vec<&str> = values.iter().map(String::as_str).collect();
    wanted.sort_unstable();
    wanted.dedup();
    if wanted.is_empty() {
        return (
            Progress::Pending {
                have: 0,
                need: 0,
                eta_unix: None,
            },
            Vec::new(),
        );
    }

    struct Mark {
        slot: Slot,
        start: bool,
        unit: UnitId,
    }

    let mut marks: Vec<Mark> = Vec::new();
    let mut values_by_unit: BTreeMap<UnitId, Vec<&str>> = BTreeMap::new();
    for lot in lots {
        let Some(rows) = traits.get(&lot.unit) else {
            continue;
        };
        let carried: Vec<&str> = wanted
            .iter()
            .copied()
            .filter(|want| {
                rows.iter().any(|(found_category, found_value)| {
                    (category == "*" || found_category == category) && found_value == want
                })
            })
            .collect();
        if carried.is_empty() {
            continue;
        }
        // A half-open interval that ends where it starts covers no instant; its end mark
        // would otherwise sort before its own start and corrupt the sweep.
        if lot
            .disposed_at
            .is_some_and(|disposed| disposed <= lot.acquired_at)
        {
            continue;
        }
        let seen = values_by_unit.entry(lot.unit).or_default();
        for value in carried {
            if !seen.contains(&value) {
                seen.push(value);
            }
        }
        marks.push(Mark {
            slot: lot.acquired_at,
            start: true,
            unit: lot.unit,
        });
        if let Some(disposed) = lot.disposed_at {
            marks.push(Mark {
                slot: disposed,
                start: false,
                unit: lot.unit,
            });
        }
    }
    marks.sort_by_key(|mark| (mark.slot, mark.start));

    let need = wanted.len() as u64;
    let mut open: BTreeMap<UnitId, u32> = BTreeMap::new();
    // How many open units carry each wanted value.
    let mut covered: BTreeMap<&str, u64> = BTreeMap::new();
    let mut peak: u64 = 0;
    let mut crossed: Option<(Slot, Vec<Evidence>)> = None;
    let mut index = 0;
    while index < marks.len() {
        let slot = marks[index].slot;
        while index < marks.len() && marks[index].slot == slot && !marks[index].start {
            let unit = marks[index].unit;
            if let Some(count) = open.get_mut(&unit) {
                debug_assert!(*count > 0, "an end mark without an open lot");
                *count = count.saturating_sub(1);
                if *count == 0
                    && let Some(carried) = values_by_unit.get(&unit)
                {
                    for value in carried {
                        if let Some(count) = covered.get_mut(value) {
                            *count = count.saturating_sub(1);
                        }
                    }
                }
            }
            index += 1;
        }
        while index < marks.len() && marks[index].slot == slot && marks[index].start {
            let unit = marks[index].unit;
            let count = open.entry(unit).or_insert(0);
            if *count == 0
                && let Some(carried) = values_by_unit.get(&unit)
            {
                for value in carried {
                    *covered.entry(value).or_insert(0) += 1;
                }
            }
            *count += 1;
            index += 1;
        }
        let have = covered.values().filter(|count| **count > 0).count() as u64;
        peak = peak.max(have);
        if crossed.is_none() && have == need {
            let evidence = open
                .keys()
                .filter_map(|unit| {
                    let carried = values_by_unit.get(unit)?;
                    Some(Evidence::Traits {
                        unit: *unit,
                        values: carried.iter().map(|value| (*value).to_string()).collect(),
                    })
                })
                .collect();
            crossed = Some((slot, evidence));
        }
    }

    if let Some((since, evidence)) = crossed {
        return (
            Progress::Earned {
                since: EarnedSince::Exact(since),
            },
            evidence,
        );
    }
    if any_unknown(traits, &units_ever(lots)) {
        return (Progress::Unavailable(Gap::Traits), Vec::new());
    }
    (
        Progress::Pending {
            have: peak,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// `value` in `category`, on any unit ever held.
fn has_trait(
    traits: &TraitMap,
    lots: &[Holding],
    _at: &Assessment,
    category: &str,
    value: &str,
) -> (Progress, Vec<Evidence>) {
    let matching = lots
        .iter()
        .filter(|lot| matches_value(traits, lot.unit, category, Some(value)))
        .min_by_key(|lot| (lot.acquired_at, lot.lot));
    if let Some(lot) = matching {
        return (
            Progress::Earned {
                since: EarnedSince::Exact(lot.acquired_at),
            },
            vec![evidence_of(lot)],
        );
    }
    if any_unknown(traits, &units_ever(lots)) {
        return (Progress::Unavailable(Gap::Traits), Vec::new());
    }
    (
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// `at_least` of `values`, each on some unit ever held — not necessarily the same one.
fn has_trait_values(
    traits: &TraitMap,
    lots: &[Holding],
    _at: &Assessment,
    category: &str,
    values: &[String],
    at_least: u32,
) -> (Progress, Vec<Evidence>) {
    // One earliest lot per value, so a repeated value cannot count twice.
    let mut earliest: BTreeMap<&str, &Holding> = BTreeMap::new();
    for lot in lots {
        let Some(rows) = traits.get(&lot.unit) else {
            continue;
        };
        for (found_category, found_value) in rows {
            if category != "*" && found_category != category {
                continue;
            }
            if !values.iter().any(|value| value == found_value) {
                continue;
            }
            earliest
                .entry(found_value.as_str())
                .and_modify(|held| {
                    if (lot.acquired_at, lot.lot) < (held.acquired_at, held.lot) {
                        *held = lot;
                    }
                })
                .or_insert(lot);
        }
    }

    let need = u64::from(at_least.max(1));
    let mut reached: Vec<Slot> = earliest.values().map(|lot| lot.acquired_at).collect();
    reached.sort_unstable();
    if reached.len() as u64 >= need {
        let since = reached[(need - 1) as usize];
        let evidence = earliest
            .values()
            .take(need as usize)
            .map(|lot| evidence_of(lot))
            .collect();
        return (
            Progress::Earned {
                since: EarnedSince::Exact(since),
            },
            evidence,
        );
    }
    if any_unknown(traits, &units_ever(lots)) {
        return (Progress::Unavailable(Gap::Traits), Vec::new());
    }
    (
        Progress::Pending {
            have: reached.len() as u64,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// The historical peak of distinct units held at one instant.
///
/// A sweep over the lots' half-open intervals: ends before starts at a shared slot, so an
/// acquisition and a disposal at the same height are never counted as simultaneous. A unit
/// held through two overlapping lots is one unit, which is why the open count is per unit
/// and not per lot.
fn held_with_trait_at_once(
    traits: &TraitMap,
    lots: &[Holding],
    _at: &Assessment,
    category: &str,
    value: Option<&str>,
    at_least: u32,
) -> (Progress, Vec<Evidence>) {
    struct Mark {
        slot: Slot,
        start: bool,
        unit: UnitId,
    }

    let mut marks: Vec<Mark> = Vec::new();
    let mut values_by_unit: BTreeMap<UnitId, Vec<String>> = BTreeMap::new();
    for lot in lots {
        if !matches_value(traits, lot.unit, category, value) {
            continue;
        }
        // A half-open interval that ends where it starts covers no instant. Skipping it
        // matters beyond tidiness: its end mark would otherwise sort before its own start
        // (ends first at a shared slot) and decrement some *other* lot's open count.
        if lot
            .disposed_at
            .is_some_and(|disposed| disposed <= lot.acquired_at)
        {
            continue;
        }
        if let Some(rows) = traits.get(&lot.unit) {
            let seen = values_by_unit.entry(lot.unit).or_default();
            for (found_category, found_value) in rows {
                if (category == "*" || found_category == category) && !seen.contains(found_value) {
                    seen.push(found_value.clone());
                }
            }
        }
        marks.push(Mark {
            slot: lot.acquired_at,
            start: true,
            unit: lot.unit,
        });
        if let Some(disposed) = lot.disposed_at {
            marks.push(Mark {
                slot: disposed,
                start: false,
                unit: lot.unit,
            });
        }
    }
    marks.sort_by_key(|mark| (mark.slot, mark.start));

    let need = u64::from(at_least.max(1));
    let mut open: BTreeMap<UnitId, u32> = BTreeMap::new();
    let mut current: u64 = 0;
    let mut peak: u64 = 0;
    let mut crossed: Option<(Slot, Vec<Evidence>)> = None;
    let mut index = 0;
    while index < marks.len() {
        let slot = marks[index].slot;
        while index < marks.len() && marks[index].slot == slot && !marks[index].start {
            let unit = marks[index].unit;
            if let Some(count) = open.get_mut(&unit) {
                // Every end now has a start already processed: zero-length intervals are
                // skipped above, and `sort_by_key` keeps ends before starts only for
                // intervals that span.
                debug_assert!(*count > 0, "an end mark without an open lot");
                *count = count.saturating_sub(1);
                if *count == 0 {
                    current = current.saturating_sub(1);
                }
            }
            index += 1;
        }
        while index < marks.len() && marks[index].slot == slot && marks[index].start {
            let unit = marks[index].unit;
            let count = open.entry(unit).or_insert(0);
            if *count == 0 {
                current += 1;
            }
            *count += 1;
            index += 1;
        }
        peak = peak.max(current);
        if crossed.is_none() && current >= need {
            let evidence = open
                .keys()
                .map(|unit| Evidence::Traits {
                    unit: *unit,
                    values: values_by_unit.get(unit).cloned().unwrap_or_default(),
                })
                .collect();
            crossed = Some((slot, evidence));
        }
    }

    if let Some((since, evidence)) = crossed {
        return (
            Progress::Earned {
                since: EarnedSince::Exact(since),
            },
            evidence,
        );
    }
    if any_unknown(traits, &units_ever(lots)) {
        return (Progress::Unavailable(Gap::Traits), Vec::new());
    }
    (
        Progress::Pending {
            have: peak,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// The historical peak of distinct units held at one instant, with no trait filter.
///
/// The same sweep as [`held_with_trait_at_once`]: half-open intervals, ends before starts at a
/// shared slot, and a unit held through two overlapping lots counts once. A partial source is
/// a gap rather than a short count, because a missing movement hides a disposal and would
/// overstate the peak.
fn holds_at_least(lots: &[Holding], at: &Assessment, at_least: u32) -> (Progress, Vec<Evidence>) {
    if at.holder_coverage != Coverage::Complete {
        return (Progress::Unavailable(Gap::HolderCoverage), Vec::new());
    }

    struct Mark {
        slot: Slot,
        start: bool,
        unit: UnitId,
    }

    let mut marks: Vec<Mark> = Vec::new();
    for lot in lots {
        // A half-open interval that ends where it starts covers no instant.
        if lot
            .disposed_at
            .is_some_and(|disposed| disposed <= lot.acquired_at)
        {
            continue;
        }
        marks.push(Mark {
            slot: lot.acquired_at,
            start: true,
            unit: lot.unit,
        });
        if let Some(disposed) = lot.disposed_at {
            marks.push(Mark {
                slot: disposed,
                start: false,
                unit: lot.unit,
            });
        }
    }
    marks.sort_by_key(|mark| (mark.slot, mark.start));

    let need = u64::from(at_least.max(1));
    let mut open: BTreeMap<UnitId, u32> = BTreeMap::new();
    let mut current: u64 = 0;
    let mut peak: u64 = 0;
    let mut crossed: Option<(Slot, Vec<UnitId>)> = None;
    let mut index = 0;
    while index < marks.len() {
        let slot = marks[index].slot;
        while index < marks.len() && marks[index].slot == slot && !marks[index].start {
            let unit = marks[index].unit;
            if let Some(count) = open.get_mut(&unit) {
                debug_assert!(*count > 0, "an end mark without an open lot");
                *count = count.saturating_sub(1);
                if *count == 0 {
                    current = current.saturating_sub(1);
                }
            }
            index += 1;
        }
        while index < marks.len() && marks[index].slot == slot && marks[index].start {
            let unit = marks[index].unit;
            let count = open.entry(unit).or_insert(0);
            if *count == 0 {
                current += 1;
            }
            *count += 1;
            index += 1;
        }
        peak = peak.max(current);
        if crossed.is_none() && current >= need {
            crossed = Some((slot, open.keys().copied().collect()));
        }
    }

    if let Some((since, units)) = crossed {
        return (
            Progress::Earned {
                since: EarnedSince::Exact(since),
            },
            vec![Evidence::HeldAtOnce { units }],
        );
    }
    (
        Progress::Pending {
            have: peak,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// `assets` distinct units sharing one value in `category`, ever held.
fn same_value_across_assets(
    traits: &TraitMap,
    lots: &[Holding],
    _at: &Assessment,
    category: &str,
    assets: u32,
) -> (Progress, Vec<Evidence>) {
    // value → unit → its earliest acquisition of that value.
    let mut by_value: BTreeMap<&str, BTreeMap<UnitId, &Holding>> = BTreeMap::new();
    for lot in lots {
        let Some(rows) = traits.get(&lot.unit) else {
            continue;
        };
        for (found_category, found_value) in rows {
            if category != "*" && found_category != category {
                continue;
            }
            by_value
                .entry(found_value.as_str())
                .or_default()
                .entry(lot.unit)
                .and_modify(|held| {
                    if (lot.acquired_at, lot.lot) < (held.acquired_at, held.lot) {
                        *held = lot;
                    }
                })
                .or_insert(lot);
        }
    }

    let need = u64::from(assets.max(1));
    let mut peak: u64 = 0;
    let mut best: Option<(Slot, &str, Vec<Evidence>)> = None;
    for (value, units) in &by_value {
        peak = peak.max(units.len() as u64);
        if (units.len() as u64) < need {
            continue;
        }
        let mut reached: Vec<Slot> = units.values().map(|lot| lot.acquired_at).collect();
        reached.sort_unstable();
        let slot = reached[(need - 1) as usize];
        if best.as_ref().is_none_or(|(found, _, _)| slot < *found) {
            let evidence = units
                .values()
                .take(need as usize)
                .map(|lot| evidence_of(lot))
                .collect();
            best = Some((slot, value, evidence));
        }
    }

    if let Some((since, _, evidence)) = best {
        return (
            Progress::Earned {
                since: EarnedSince::Exact(since),
            },
            evidence,
        );
    }
    if any_unknown(traits, &units_ever(lots)) {
        return (Progress::Unavailable(Gap::Traits), Vec::new());
    }
    (
        Progress::Pending {
            have: peak,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// Whether one unit carries `category`/`value`.
///
/// `category = "*"` matches across categories and `value = "*"` (or `None`) matches any
/// value in the category — the two wildcards are independent, as the catalogue states.
fn matches_value(traits: &TraitMap, unit: UnitId, category: &str, value: Option<&str>) -> bool {
    traits.get(&unit).is_some_and(|rows| {
        rows.iter().any(|(found_category, found_value)| {
            (category == "*" || found_category == category)
                && value.is_none_or(|want| want == "*" || found_value == want)
        })
    })
}

// ============================================================================
// Provenance — what the subject did at the primary mint
// ============================================================================

/// Coherent answers need the whole history: a rank is only right if every earlier mint was
/// seen, and "never bought" is only true if the purchase was not before the window.
fn provenance_gap(at: &Assessment) -> Option<Progress> {
    (at.holder_coverage != Coverage::Complete).then_some(Progress::Unavailable(Gap::HolderCoverage))
}

/// The subject's own primary mints, oldest first.
fn mint_lots(lots: &[Holding]) -> Vec<&Holding> {
    let mut minted: Vec<&Holding> = lots
        .iter()
        .filter(|lot| lot.acquired_by == AcquiredBy::Mint)
        .collect();
    minted.sort_by_key(|lot| (lot.acquired_at, lot.lot));
    minted
}

/// A mint inside `seconds` of the collection's first, measured on the asset's own record.
fn mint_within(
    mints: &MintView,
    lots: &[Holding],
    at: &Assessment,
    seconds: u64,
) -> (Progress, Vec<Evidence>) {
    if let Some(gap) = provenance_gap(at) {
        return (gap, Vec::new());
    }
    let Some(summary) = mints.summary else {
        return (Progress::Unavailable(Gap::MintFacts), Vec::new());
    };
    let minted = mint_lots(lots);
    for lot in &minted {
        if let Some(fact) = mints.facts.get(&lot.unit)
            && fact.unix.saturating_sub(summary.first_unix) <= seconds
        {
            return (
                Progress::Earned {
                    since: EarnedSince::Exact(lot.acquired_at),
                },
                vec![evidence_of(lot)],
            );
        }
    }
    if minted.is_empty() {
        (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        )
    } else {
        // The subject minted, and their mint height is fixed: this cannot become true.
        (Progress::Missed, Vec::new())
    }
}

/// A mint whose collection rank satisfies `accept`.
fn mint_rank(
    mints: &MintView,
    lots: &[Holding],
    at: &Assessment,
    accept: impl Fn(u32) -> bool,
) -> (Progress, Vec<Evidence>) {
    if let Some(gap) = provenance_gap(at) {
        return (gap, Vec::new());
    }
    if mints.summary.is_none() {
        return (Progress::Unavailable(Gap::MintFacts), Vec::new());
    }
    let minted = mint_lots(lots);
    for lot in &minted {
        if let Some(fact) = mints.facts.get(&lot.unit)
            && accept(fact.rank)
        {
            return (
                Progress::Earned {
                    since: EarnedSince::Exact(lot.acquired_at),
                },
                vec![evidence_of(lot)],
            );
        }
    }
    if minted.is_empty() {
        (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        )
    } else {
        (Progress::Missed, Vec::new())
    }
}

/// A mint that followed a quiet period of at least `gap` seconds.
fn mint_after_gap(
    mints: &MintView,
    lots: &[Holding],
    at: &Assessment,
    gap: u64,
) -> (Progress, Vec<Evidence>) {
    if let Some(gap) = provenance_gap(at) {
        return (gap, Vec::new());
    }
    if mints.summary.is_none() {
        return (Progress::Unavailable(Gap::MintFacts), Vec::new());
    }
    let minted = mint_lots(lots);
    for lot in &minted {
        if let Some(fact) = mints.facts.get(&lot.unit)
            && let Some(previous) = fact.previous_unix
            && fact.unix.saturating_sub(previous) >= gap
        {
            return (
                Progress::Earned {
                    since: EarnedSince::Exact(lot.acquired_at),
                },
                vec![evidence_of(lot)],
            );
        }
    }
    if minted.is_empty() {
        (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        )
    } else {
        (Progress::Missed, Vec::new())
    }
}

/// How many assets the subject minted at primary.
fn minted_count(lots: &[Holding], at: &Assessment, need: u64) -> (Progress, Vec<Evidence>) {
    if let Some(gap) = provenance_gap(at) {
        return (gap, Vec::new());
    }
    let need = need.max(1);
    let minted = mint_lots(lots);
    if minted.len() as u64 >= need {
        let proving = minted[(need - 1) as usize];
        return (
            Progress::Earned {
                since: EarnedSince::Exact(proving.acquired_at),
            },
            minted
                .iter()
                .take(need as usize)
                .map(|lot| evidence_of(lot))
                .collect(),
        );
    }
    (
        Progress::Pending {
            have: minted.len() as u64,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// How many of the subject's own mints are still held.
fn minted_and_still_held(
    lots: &[Holding],
    at: &Assessment,
    need: u64,
) -> (Progress, Vec<Evidence>) {
    if let Some(gap) = provenance_gap(at) {
        return (gap, Vec::new());
    }
    let need = need.max(1);
    let held: Vec<&Holding> = mint_lots(lots)
        .into_iter()
        .filter(|lot| lot.is_open())
        .collect();
    if held.len() as u64 >= need {
        let proving = held[(need - 1) as usize];
        return (
            Progress::Earned {
                since: EarnedSince::Exact(proving.acquired_at),
            },
            held.iter()
                .take(need as usize)
                .map(|lot| evidence_of(lot))
                .collect(),
        );
    }
    (
        Progress::Pending {
            have: held.len() as u64,
            need,
            eta_unix: None,
        },
        Vec::new(),
    )
}

/// Every asset the subject minted is still theirs.
///
/// One-way: a mint lot that has been sold is a disposal the history cannot take back, so the
/// answer is `Missed` from then on. A subject who never minted is `Pending`, because the fold
/// cannot know the mint has closed.
fn all_mints_retained(lots: &[Holding], at: &Assessment) -> (Progress, Vec<Evidence>) {
    if let Some(gap) = provenance_gap(at) {
        return (gap, Vec::new());
    }
    let minted = mint_lots(lots);
    let Some(first) = minted.first() else {
        return (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        );
    };
    if minted.iter().any(|lot| lot.is_closed()) {
        return (Progress::Missed, Vec::new());
    }
    (
        Progress::Earned {
            since: EarnedSince::Exact(first.acquired_at),
        },
        vec![evidence_of(first)],
    )
}

/// Every asset the subject held arrived at the primary mint.
///
/// A gift and a purchase are both an arrival from another party, and the fold does not
/// distinguish them, so this is "nothing I held came from anyone else".
fn never_bought(lots: &[Holding], at: &Assessment) -> (Progress, Vec<Evidence>) {
    if let Some(gap) = provenance_gap(at) {
        return (gap, Vec::new());
    }
    if lots
        .iter()
        .any(|lot| lot.acquired_by == AcquiredBy::Transfer)
    {
        return (Progress::Missed, Vec::new());
    }
    let minted = mint_lots(lots);
    let Some(first) = minted.first() else {
        return (
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            },
            Vec::new(),
        );
    };
    (
        Progress::Earned {
            since: EarnedSince::Exact(first.acquired_at),
        },
        vec![evidence_of(first)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold::{apply, rebuild};
    use crate::memory::MemoryStore;
    use crate::state::AcquiredBy;
    use chain_events::Coverage;

    fn definition(id: &'static str, predicate: Predicate) -> Definition<'static> {
        Definition { id, predicate }
    }

    fn find<'a>(outcomes: &'a [Outcome<'_>], id: &str) -> &'a Outcome<'a> {
        outcomes
            .iter()
            .find(|outcome| outcome.id == id)
            .unwrap_or_else(|| panic!("no outcome {id}"))
    }

    /// Unix seconds the in-memory fixtures are stamped from: `MemoryStore` writes
    /// `occurred_at` as `FIXTURE_ORIGIN + height`, so one synthetic slot is one second and
    /// every hand-computed duration in these tests stays what it was.
    const FIXTURE_ORIGIN: u64 = 1_700_000_000;

    /// A complete reading at `now`, on the fixtures' clock.
    fn at(now: Slot) -> Assessment {
        Assessment::complete(now, FIXTURE_ORIGIN + now)
    }

    #[test]
    fn a_subject_that_never_disposed_is_earned_at_its_first_acquisition() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[definition("never-sold", Predicate::NeverDisposed)],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "never-sold").progress,
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );
        assert_eq!(store.subjects().unwrap().len(), 1);
    }

    #[test]
    fn a_subject_that_sold_is_missed_and_not_pending() {
        let mut store = MemoryStore::default();
        let (_, subject) = crate::memory::fixtures::alice_mints_and_sells(&mut store);
        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[definition("never-sold", Predicate::NeverDisposed)],
        )
        .unwrap();
        assert_eq!(find(&outcomes, "never-sold").progress, Progress::Missed);
    }

    #[test]
    fn tenure_reports_the_number_reached_and_the_time_it_will_be_earned() {
        // One unit from slot 100. At the reading it has 900 seconds; a 1-day threshold
        // needs 86,400, so 85,500 more seconds → 1,700,086,500 unix.
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let outcomes = evaluate(
            &store,
            subject,
            &at(1_000),
            &[definition(
                "tenure",
                Predicate::TotalTenureAtLeast { days: 1 },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "tenure").progress,
            Progress::Pending {
                have: 900,
                need: 86_400,
                eta_unix: Some(1_700_086_500),
            }
        );
    }

    #[test]
    fn tenure_accrues_faster_with_more_open_units() {
        // Two units from slot 100: 1,800 seconds at the reading, and the 84,600-second
        // headroom is covered by two lots at once → 42,300 more seconds.
        let (store, subject) = crate::memory::fixtures::alice_mints_two();
        let outcomes = evaluate(
            &store,
            subject,
            &at(1_000),
            &[definition(
                "tenure",
                Predicate::TotalTenureAtLeast { days: 1 },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "tenure").progress,
            Progress::Pending {
                have: 1_800,
                need: 86_400,
                eta_unix: Some(1_700_043_300),
            }
        );
    }

    #[test]
    fn tenure_earned_by_a_closed_lot_says_as_at_and_cites_the_acquisition() {
        let mut store = MemoryStore::default();
        let (_, subject) = crate::memory::fixtures::alice_mints_and_sells(&mut store);
        // Held 100 → 200: 100 seconds. Ask for nothing and it is already earned.
        let outcomes = evaluate(
            &store,
            subject,
            &at(9_000),
            &[definition(
                "tenure",
                Predicate::TotalTenureAtLeast { days: 0 },
            )],
        )
        .unwrap();
        let outcome = find(&outcomes, "tenure");
        assert_eq!(
            outcome.progress,
            Progress::Earned {
                since: EarnedSince::AsAt {
                    slot: 9_000,
                    unix: 1_700_009_000,
                }
            }
        );
        assert!(matches!(outcome.evidence[0], Evidence::Lot { .. }));
    }

    #[test]
    fn a_span_is_continuous_and_a_tenure_is_cumulative() {
        // Held 60 days, sold, reacquired at the same slot, held another 60: 120 asset-days
        // of tenure but no 100-day span.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        let sixty = 60 * SECONDS_PER_DAY;
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        apply(
            &mut store,
            &transfer(&asset, "addr1alice", "addr1bob", 100 + sixty),
        )
        .unwrap();
        apply(
            &mut store,
            &transfer(&asset, "addr1bob", "addr1alice", 100 + sixty),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let now = 100 + 2 * sixty;
        let outcomes = evaluate(
            &store,
            alice,
            &at(now),
            &[
                definition("span", Predicate::AnySpanAtLeast { days: 100 }),
                definition("tenure", Predicate::TotalTenureAtLeast { days: 100 }),
            ],
        )
        .unwrap();
        assert!(matches!(
            find(&outcomes, "tenure").progress,
            Progress::Earned { .. }
        ));
        assert_eq!(
            find(&outcomes, "span").progress,
            Progress::Pending {
                have: sixty,
                need: 100 * SECONDS_PER_DAY,
                eta_unix: Some(FIXTURE_ORIGIN + 100 + sixty + 100 * SECONDS_PER_DAY),
            }
        );
    }

    #[test]
    fn a_span_pending_reports_the_date_it_will_be_earned() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let ten = 10 * SECONDS_PER_DAY;
        let now = 100 + ten;
        let outcomes = evaluate(
            &store,
            subject,
            &at(now),
            &[definition("span", Predicate::AnySpanAtLeast { days: 100 })],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "span").progress,
            Progress::Pending {
                have: ten,
                need: 100 * SECONDS_PER_DAY,
                eta_unix: Some(FIXTURE_ORIGIN + 100 + 100 * SECONDS_PER_DAY),
            }
        );
    }

    #[test]
    fn a_span_is_earned_from_a_closed_lot() {
        // The interval ends at the sale, so the span is fixed and still counts.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        let hundred = 100 * SECONDS_PER_DAY;
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        apply(
            &mut store,
            &transfer(&asset, "addr1alice", "addr1bob", 100 + hundred),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(100 + 2 * hundred),
            &[definition("span", Predicate::AnySpanAtLeast { days: 100 })],
        )
        .unwrap();
        assert!(matches!(
            find(&outcomes, "span").progress,
            Progress::Earned { .. }
        ));
    }

    #[test]
    fn span_at_least_counts_distinct_units_not_lots() {
        let (store, subject) = crate::memory::fixtures::alice_mints_two();
        let now = 100 + 100 * SECONDS_PER_DAY;
        let outcomes = evaluate(
            &store,
            subject,
            &at(now),
            &[
                definition(
                    "two",
                    Predicate::SpanAtLeast {
                        days: 100,
                        assets: 2,
                    },
                ),
                definition(
                    "three",
                    Predicate::SpanAtLeast {
                        days: 100,
                        assets: 3,
                    },
                ),
            ],
        )
        .unwrap();
        assert!(matches!(
            find(&outcomes, "two").progress,
            Progress::Earned { .. }
        ));
        assert_eq!(
            find(&outcomes, "three").progress,
            Progress::Pending {
                have: 2,
                need: 3,
                eta_unix: None,
            }
        );
    }

    #[test]
    fn span_at_least_reports_when_the_next_asset_will_qualify() {
        use crate::memory::fixtures::{account, mint, perp};
        let mut store = MemoryStore::default();
        for (index, height) in [100, 200, 300].into_iter().enumerate() {
            let asset = perp(&format!("506572704{index}"));
            apply(&mut store, &mint(&asset, "addr1alice", height)).unwrap();
        }
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(100 + 10 * SECONDS_PER_DAY),
            &[definition(
                "three",
                Predicate::SpanAtLeast {
                    days: 100,
                    assets: 3,
                },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "three").progress,
            Progress::Pending {
                have: 0,
                need: 3,
                eta_unix: Some(FIXTURE_ORIGIN + 300 + 100 * SECONDS_PER_DAY),
            }
        );
    }

    #[test]
    fn holds_never_moved_is_an_open_mint_lot() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[definition("untouched", Predicate::HoldsNeverMoved)],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "untouched").progress,
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );
    }

    #[test]
    fn holds_never_moved_is_not_a_bought_or_reacquired_asset() {
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 200)).unwrap();
        let bob = store.subject(&account("addr1bob")).unwrap();
        let outcomes = evaluate(
            &store,
            bob,
            &at(5_000),
            &[definition("untouched", Predicate::HoldsNeverMoved)],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "untouched").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );

        apply(&mut store, &transfer(&asset, "addr1bob", "addr1alice", 300)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition("untouched", Predicate::HoldsNeverMoved)],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "untouched").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn a_reacquisition_is_an_asset_that_left_and_came_back() {
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 200)).unwrap();
        apply(&mut store, &transfer(&asset, "addr1bob", "addr1alice", 300)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "came-back",
                Predicate::ReacquiredSameAsset { times: 1 },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "came-back").progress,
            Progress::Earned {
                since: EarnedSince::Exact(300)
            }
        );

        // Bob bought and held; buying is not reacquiring.
        let bob = store.subject(&account("addr1bob")).unwrap();
        let outcomes = evaluate(
            &store,
            bob,
            &at(5_000),
            &[definition(
                "came-back",
                Predicate::ReacquiredSameAsset { times: 1 },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "came-back").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn reacquisitions_are_counted_per_event() {
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        for height in [200, 300, 400, 500] {
            let (from, to) = if height % 200 == 0 {
                ("addr1alice", "addr1bob")
            } else {
                ("addr1bob", "addr1alice")
            };
            apply(&mut store, &transfer(&asset, from, to, height)).unwrap();
        }
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "came-back",
                Predicate::ReacquiredSameAsset { times: 2 },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "came-back").progress,
            Progress::Earned {
                since: EarnedSince::Exact(500)
            }
        );
    }

    #[test]
    fn a_return_after_an_idle_spell_needs_the_gap() {
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        let ninety = 90 * SECONDS_PER_DAY;
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 200)).unwrap();
        apply(
            &mut store,
            &transfer(&asset, "addr1bob", "addr1alice", 200 + ninety),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(300 + ninety),
            &[definition(
                "returned",
                Predicate::ReturnedAfterIdle { days: 90 },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "returned").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200 + ninety)
            }
        );
    }

    #[test]
    fn a_return_within_the_idle_threshold_is_pending() {
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        let ninety = 90 * SECONDS_PER_DAY;
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 200)).unwrap();
        apply(
            &mut store,
            &transfer(&asset, "addr1bob", "addr1alice", 200 + ninety - 1),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(200 + ninety),
            &[definition(
                "returned",
                Predicate::ReturnedAfterIdle { days: 90 },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "returned").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn an_open_holding_does_not_create_an_idle_gap() {
        // Alice still held her other Perp while the first was away, so she held something
        // throughout and never returned from nothing.
        use crate::memory::fixtures::{perp, transfer};
        let (mut store, alice) = crate::memory::fixtures::alice_mints_two();
        let asset = perp("5065727031");
        apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 200)).unwrap();
        apply(
            &mut store,
            &transfer(&asset, "addr1bob", "addr1alice", 200 + 90 * SECONDS_PER_DAY),
        )
        .unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(300 + 90 * SECONDS_PER_DAY),
            &[definition(
                "returned",
                Predicate::ReturnedAfterIdle { days: 90 },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "returned").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn the_lot_only_predicates_report_a_coverage_gap() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let at = Assessment {
            now: 5_000,
            now_unix: FIXTURE_ORIGIN + 5_000,
            holder_coverage: Coverage::Partial,
        };
        let outcomes = evaluate(
            &store,
            subject,
            &at,
            &[
                definition("span", Predicate::AnySpanAtLeast { days: 100 }),
                definition(
                    "portfolio",
                    Predicate::SpanAtLeast {
                        days: 100,
                        assets: 5,
                    },
                ),
                definition("untouched", Predicate::HoldsNeverMoved),
                definition("came-back", Predicate::ReacquiredSameAsset { times: 1 }),
                definition("returned", Predicate::ReturnedAfterIdle { days: 90 }),
            ],
        )
        .unwrap();
        for outcome in &outcomes {
            assert_eq!(
                outcome.progress,
                Progress::Unavailable(Gap::HolderCoverage),
                "{}",
                outcome.id
            );
        }
    }

    #[test]
    fn a_partial_coverage_makes_a_span_unavailable_rather_than_short() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let at = Assessment {
            now: 1_000,
            now_unix: FIXTURE_ORIGIN + 1_000,
            holder_coverage: Coverage::Partial,
        };
        let outcomes = evaluate(
            &store,
            subject,
            &at,
            &[
                definition("tenure", Predicate::TotalTenureAtLeast { days: 1 }),
                definition("never-sold", Predicate::NeverDisposed),
                definition("from-mint", Predicate::HeldSinceMint),
            ],
        )
        .unwrap();
        for id in ["tenure", "never-sold", "from-mint"] {
            assert_eq!(
                find(&outcomes, id).progress,
                Progress::Unavailable(Gap::HolderCoverage)
            );
        }
    }

    #[test]
    fn a_round_trip_counts_only_what_was_bought_and_sold() {
        // Alice minted at 100, sold to Bob at 200, and Bob sold back at 300.
        // - Bob bought and sold: one completed round trip.
        // - Alice's mint lot is not a round trip, and her re-purchase at 300 has not
        //   left again, so she has none. Buying something back is not a round trip.
        let mut store = MemoryStore::default();
        let (asset, alice, bob) = crate::memory::fixtures::round_trip(&mut store);

        let outcomes = evaluate(
            &store,
            bob,
            &at(9_000),
            &[definition(
                "round-trip",
                Predicate::RoundTripsAtLeast { n: 1 },
            )],
        )
        .unwrap();
        assert!(matches!(
            find(&outcomes, "round-trip").progress,
            Progress::Earned { .. }
        ));

        for need in [1, 2] {
            let outcomes = evaluate(
                &store,
                alice,
                &at(9_000),
                &[definition(
                    "round-trip",
                    Predicate::RoundTripsAtLeast { n: need },
                )],
            )
            .unwrap();
            assert_eq!(
                find(&outcomes, "round-trip").progress,
                Progress::Pending {
                    have: 0,
                    need: u64::from(need),
                    eta_unix: None,
                }
            );
        }
        assert_eq!(
            asset.as_ref().map(|asset| asset.token_id()),
            Some(Some("5065727031"))
        );

        // Alice sells it on again: now she has completed one.
        let sale = crate::memory::fixtures::transfer(
            asset.as_ref().unwrap(),
            "addr1alice",
            "addr1carol",
            400,
        );
        crate::fold::apply(&mut store, &sale).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(9_000),
            &[definition(
                "round-trip",
                Predicate::RoundTripsAtLeast { n: 1 },
            )],
        )
        .unwrap();
        assert!(matches!(
            find(&outcomes, "round-trip").progress,
            Progress::Earned { .. }
        ));
    }

    #[test]
    fn a_market_purchase_is_a_fill_where_the_subject_is_the_buyer() {
        use crate::memory::fixtures::{account, filled};
        use chain_events::{PartyRole, Side};
        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &filled(
                None,
                Side::Ask,
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                100,
            ),
        )
        .unwrap();
        let definitions = [definition("first-trade", Predicate::MarketPurchase)];

        let bob = store.subject(&account("addr1bob")).unwrap();
        let outcomes = evaluate(&store, bob, &at(5_000), &definitions).unwrap();
        assert_eq!(
            find(&outcomes, "first-trade").progress,
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );

        // The seller was the other side of the same trade.
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(&store, alice, &at(5_000), &definitions).unwrap();
        assert_eq!(
            find(&outcomes, "first-trade").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn a_store_without_market_facts_reports_the_gap_rather_than_missed() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[definition("first-trade", Predicate::MarketPurchase)],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "first-trade").progress,
            Progress::Unavailable(Gap::MarketFacts)
        );
    }

    #[test]
    fn listed_count_counts_distinct_orders_not_relistings() {
        use crate::memory::fixtures::{account, placed};
        use chain_events::{PartyRole, Side};
        let mut store = MemoryStore::default();
        for (order, height) in [("o1", 100), ("o2", 110), ("o1", 120)] {
            apply(
                &mut store,
                &placed(
                    order,
                    Side::Ask,
                    &[("addr1alice", PartyRole::Seller)],
                    height,
                ),
            )
            .unwrap();
        }
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[
                definition("two", Predicate::ListedCount { at_least: 2 }),
                definition("three", Predicate::ListedCount { at_least: 3 }),
            ],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "two").progress,
            Progress::Earned {
                since: EarnedSince::Exact(110)
            }
        );
        assert_eq!(
            find(&outcomes, "three").progress,
            Progress::Pending {
                have: 2,
                need: 3,
                eta_unix: None
            }
        );
    }

    #[test]
    fn an_offer_created_is_an_item_bid_not_a_collection_bid() {
        use crate::memory::fixtures::{account, placed};
        use chain_events::{OfferScope, PartyRole, Side};
        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &placed(
                "collection",
                Side::Bid(OfferScope::Collection),
                &[("addr1alice", PartyRole::Buyer)],
                100,
            ),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition("haggler", Predicate::OfferCreated)],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "haggler").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );

        apply(
            &mut store,
            &placed(
                "item",
                Side::Bid(OfferScope::Item),
                &[("addr1alice", PartyRole::Buyer)],
                200,
            ),
        )
        .unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition("haggler", Predicate::OfferCreated)],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "haggler").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
    }

    #[test]
    fn a_collection_offer_fill_is_a_different_achievement_from_an_item_offer_fill() {
        use crate::memory::fixtures::{account, filled};
        use chain_events::{OfferScope, PartyRole, Side};
        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &filled(
                None,
                Side::Bid(OfferScope::Collection),
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                100,
            ),
        )
        .unwrap();
        let bob = store.subject(&account("addr1bob")).unwrap();
        let outcomes = evaluate(
            &store,
            bob,
            &at(5_000),
            &[
                definition("dumpster-diver", Predicate::BoughtViaCollectionOffer),
                definition("played-for-and-got", Predicate::OfferAcceptedAsBuyer),
            ],
        )
        .unwrap();
        assert!(matches!(
            find(&outcomes, "dumpster-diver").progress,
            Progress::Earned { .. }
        ));
        assert_eq!(
            find(&outcomes, "played-for-and-got").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );

        apply(
            &mut store,
            &filled(
                None,
                Side::Bid(OfferScope::Item),
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                200,
            ),
        )
        .unwrap();
        let outcomes = evaluate(
            &store,
            bob,
            &at(5_000),
            &[definition(
                "played-for-and-got",
                Predicate::OfferAcceptedAsBuyer,
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "played-for-and-got").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
    }

    #[test]
    fn a_listing_age_is_measured_from_the_order_the_fill_names() {
        use crate::memory::fixtures::{account, filled, placed};
        use chain_events::{PartyRole, Side};
        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &placed("o1", Side::Ask, &[("addr1alice", PartyRole::Seller)], 100),
        )
        .unwrap();
        apply(
            &mut store,
            &filled(
                Some("o1"),
                Side::Ask,
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                700,
            ),
        )
        .unwrap();
        let bob = store.subject(&account("addr1bob")).unwrap();
        let outcomes = evaluate(
            &store,
            bob,
            &at(5_000),
            &[
                definition("sniper", Predicate::BoughtEscrowWithin { seconds: 600 }),
                definition("stale", Predicate::BoughtListingOlderThan { days: 30 }),
            ],
        )
        .unwrap();
        // 700 − 100 = 600 seconds, exactly the window.
        assert!(matches!(
            find(&outcomes, "sniper").progress,
            Progress::Earned { .. }
        ));
        assert_eq!(
            find(&outcomes, "stale").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn an_unkeyed_fill_reports_the_age_gap_rather_than_a_miss() {
        // The Perps archive names no order on any fill, so an age predicate must say the
        // source cannot answer rather than call the purchase fast or stale.
        use crate::memory::fixtures::{account, filled};
        use chain_events::{PartyRole, Side};
        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &filled(
                None,
                Side::Ask,
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                100,
            ),
        )
        .unwrap();
        let bob = store.subject(&account("addr1bob")).unwrap();
        let outcomes = evaluate(
            &store,
            bob,
            &at(5_000),
            &[
                definition("sniper", Predicate::BoughtEscrowWithin { seconds: 600 }),
                definition("stale", Predicate::BoughtListingOlderThan { days: 30 }),
            ],
        )
        .unwrap();
        for id in ["sniper", "stale"] {
            assert_eq!(
                find(&outcomes, id).progress,
                Progress::Unavailable(Gap::OrderAge),
                "{id}"
            );
        }
    }

    #[test]
    fn a_sale_is_judged_against_the_floor_its_own_fill_consumed() {
        // One ask at 1 ADA is both the floor and the order that settles. A floor looked up
        // after the fill would find an empty book and report the gap, so this pins that the
        // fold captured the floor before the ask left.
        use crate::memory::fixtures::{account, filled_at, placed_at};
        use chain_events::{PartyRole, Price, Side};

        let mut store = MemoryStore::default();
        let one = Price::native(1_000_000, 6);
        apply(
            &mut store,
            &placed_at(
                "o1",
                Side::Ask,
                one.clone(),
                &[("addr1alice", PartyRole::Seller)],
                100,
            ),
        )
        .unwrap();
        apply(
            &mut store,
            &filled_at(
                Some("o1"),
                Side::Ask,
                Some(one.clone()),
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                200,
            ),
        )
        .unwrap();

        let alice = store.subject(&account("addr1alice")).unwrap();
        let history = store
            .market_history(alice)
            .unwrap()
            .expect("the store saw market facts");
        assert_eq!(history.fills[0].floor, Some(one));

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[
                definition("dumper", Predicate::SoldAtOrBelowFloor),
                definition("top-of-book", Predicate::SoldAboveFloor),
            ],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "dumper").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
        assert_eq!(
            find(&outcomes, "top-of-book").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn a_sale_above_the_standing_floor_is_top_of_book() {
        use crate::memory::fixtures::{account, filled_at, placed_at};
        use chain_events::{PartyRole, Price, Side};

        let mut store = MemoryStore::default();
        let one = Price::native(1_000_000, 6);
        let two = Price::native(2_000_000, 6);
        apply(
            &mut store,
            &placed_at(
                "o1",
                Side::Ask,
                one,
                &[("addr1alice", PartyRole::Seller)],
                100,
            ),
        )
        .unwrap();
        apply(
            &mut store,
            &placed_at(
                "o2",
                Side::Ask,
                two.clone(),
                &[("addr1bob", PartyRole::Seller)],
                110,
            ),
        )
        .unwrap();
        apply(
            &mut store,
            &filled_at(
                Some("o2"),
                Side::Ask,
                Some(two),
                &[
                    ("addr1bob", PartyRole::Seller),
                    ("addr1carol", PartyRole::Buyer),
                ],
                200,
            ),
        )
        .unwrap();

        let bob = store.subject(&account("addr1bob")).unwrap();
        let outcomes = evaluate(
            &store,
            bob,
            &at(5_000),
            &[
                definition("top-of-book", Predicate::SoldAboveFloor),
                definition("dumper", Predicate::SoldAtOrBelowFloor),
            ],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "top-of-book").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
        assert_eq!(
            find(&outcomes, "dumper").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn a_sale_above_a_rising_floor_is_into_strength() {
        use crate::memory::fixtures::{account, filled_at, placed_at, updated};
        use chain_events::{PartyRole, Price, Side};

        let mut store = MemoryStore::default();
        let one = Price::native(1_000_000, 6);
        let two = Price::native(2_000_000, 6);
        let three = Price::native(3_000_000, 6);
        let four = Price::native(4_000_000, 6);

        // Re-pricing the 1 ADA ask over the 2 ADA one lifts the floor, so the last move
        // before the sale is up.
        apply(
            &mut store,
            &placed_at(
                "o1",
                Side::Ask,
                one,
                &[("addr1alice", PartyRole::Seller)],
                100,
            ),
        )
        .unwrap();
        apply(
            &mut store,
            &placed_at(
                "o2",
                Side::Ask,
                two,
                &[("addr1bob", PartyRole::Seller)],
                110,
            ),
        )
        .unwrap();
        apply(&mut store, &updated("o1", three, 120)).unwrap();
        apply(
            &mut store,
            &placed_at(
                "o3",
                Side::Ask,
                four.clone(),
                &[("addr1carol", PartyRole::Seller)],
                130,
            ),
        )
        .unwrap();
        apply(
            &mut store,
            &filled_at(
                Some("o3"),
                Side::Ask,
                Some(four),
                &[
                    ("addr1carol", PartyRole::Seller),
                    ("addr1dave", PartyRole::Buyer),
                ],
                200,
            ),
        )
        .unwrap();

        let carol = store.subject(&account("addr1carol")).unwrap();
        let outcomes = evaluate(
            &store,
            carol,
            &at(5_000),
            &[
                definition("into-strength", Predicate::SoldIntoStrength),
                definition("top-of-book", Predicate::SoldAboveFloor),
            ],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "into-strength").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
        assert!(matches!(
            find(&outcomes, "top-of-book").progress,
            Progress::Earned { .. }
        ));
    }

    #[test]
    fn an_acquisition_while_the_floor_is_down_is_a_dip_buy() {
        use crate::memory::fixtures::{account, mint, perp, placed_at};
        use chain_events::{PartyRole, Price, Side};

        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &placed_at(
                "o1",
                Side::Ask,
                Price::native(4_000_000, 6),
                &[("addr1alice", PartyRole::Seller)],
                100,
            ),
        )
        .unwrap();
        apply(
            &mut store,
            &placed_at(
                "o2",
                Side::Ask,
                Price::native(2_000_000, 6),
                &[("addr1bob", PartyRole::Seller)],
                200,
            ),
        )
        .unwrap();
        apply(&mut store, &mint(&perp("5065727031"), "addr1carol", 300)).unwrap();

        let carol = store.subject(&account("addr1carol")).unwrap();
        let outcomes = evaluate(
            &store,
            carol,
            &at(5_000),
            &[
                definition("dip", Predicate::AcquiredDuringDrawdown { fraction: 0.4 }),
                definition(
                    "deeper",
                    Predicate::AcquiredDuringDrawdown { fraction: 0.6 },
                ),
            ],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "dip").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
        assert_eq!(
            find(&outcomes, "deeper").progress,
            Progress::Pending {
                have: 5_000,
                need: 6_000,
                eta_unix: None
            }
        );
    }

    #[test]
    fn a_holding_span_that_covers_a_drawdown_is_held_through() {
        use crate::memory::fixtures::{account, mint, perp, placed_at};
        use chain_events::{PartyRole, Price, Side};

        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &placed_at(
                "o1",
                Side::Ask,
                Price::native(4_000_000, 6),
                &[("addr1alice", PartyRole::Seller)],
                100,
            ),
        )
        .unwrap();
        apply(&mut store, &mint(&perp("5065727031"), "addr1carol", 200)).unwrap();
        apply(
            &mut store,
            &placed_at(
                "o2",
                Side::Ask,
                Price::native(2_000_000, 6),
                &[("addr1bob", PartyRole::Seller)],
                300,
            ),
        )
        .unwrap();

        let carol = store.subject(&account("addr1carol")).unwrap();
        let outcomes = evaluate(
            &store,
            carol,
            &at(5_000),
            &[
                definition("held-through", Predicate::DrawdownCovered { fraction: 0.5 }),
                definition("deeper", Predicate::DrawdownCovered { fraction: 0.6 }),
            ],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "held-through").progress,
            Progress::Earned {
                since: EarnedSince::Exact(300)
            }
        );
        assert_eq!(
            find(&outcomes, "deeper").progress,
            Progress::Pending {
                have: 5_000,
                need: 6_000,
                eta_unix: None
            }
        );
    }

    #[test]
    fn a_sale_with_no_floor_to_compare_against_reports_the_gap() {
        use crate::memory::fixtures::{account, filled_at};
        use chain_events::{PartyRole, Price, Side};

        let mut store = MemoryStore::default();
        // The store carries market facts, but no listing ever entered the book, so there is
        // no floor to judge the sale by. That is a gap, not a miss.
        apply(
            &mut store,
            &filled_at(
                None,
                Side::Ask,
                Some(Price::native(1_000_000, 6)),
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                100,
            ),
        )
        .unwrap();

        let alice = store.subject(&account("addr1alice")).unwrap();
        assert!(
            store.market_history(alice).unwrap().is_some(),
            "the store saw the market family"
        );
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[
                definition("dumper", Predicate::SoldAtOrBelowFloor),
                definition("into-strength", Predicate::SoldIntoStrength),
            ],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "dumper").progress,
            Progress::Unavailable(Gap::FloorFacts)
        );
        assert_eq!(
            find(&outcomes, "into-strength").progress,
            Progress::Unavailable(Gap::FloorFacts)
        );
    }

    #[test]
    fn a_sale_into_a_standing_bid_is_judged_against_the_floor() {
        // A bid fill changes no ask, so the fold has no book outcome to capture. The sale is
        // still judged by the floor that stood, which is the store's to answer.
        use crate::memory::fixtures::{account, filled_at, placed_at};
        use chain_events::{OfferScope, PartyRole, Price, Side};

        let mut store = MemoryStore::default();
        let two = Price::native(2_000_000, 6);
        apply(
            &mut store,
            &placed_at(
                "o1",
                Side::Ask,
                two.clone(),
                &[("addr1bob", PartyRole::Seller)],
                100,
            ),
        )
        .unwrap();
        apply(
            &mut store,
            &filled_at(
                None,
                Side::Bid(OfferScope::Collection),
                Some(Price::native(1_000_000, 6)),
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                200,
            ),
        )
        .unwrap();

        let alice = store.subject(&account("addr1alice")).unwrap();
        let history = store
            .market_history(alice)
            .unwrap()
            .expect("the store saw market facts");
        assert_eq!(history.fills[0].floor, Some(two));

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[
                definition("dumper", Predicate::SoldAtOrBelowFloor),
                definition("top-of-book", Predicate::SoldAboveFloor),
            ],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "dumper").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
        assert_eq!(
            find(&outcomes, "top-of-book").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn a_subject_that_moves_one_asset_ten_times_is_a_churner() {
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        // One movement per holder event the subject is a party to, whichever direction it
        // goes: the mint is the first, then nine transfers.
        for step in 0..9u64 {
            let height = 110 + step * 10;
            if step % 2 == 0 {
                apply(
                    &mut store,
                    &transfer(&asset, "addr1alice", "addr1bob", height),
                )
                .unwrap();
            } else {
                apply(
                    &mut store,
                    &transfer(&asset, "addr1bob", "addr1alice", height),
                )
                .unwrap();
            }
            if step == 7 {
                let alice = store.subject(&account("addr1alice")).unwrap();
                let outcomes = evaluate(
                    &store,
                    alice,
                    &at(5_000),
                    &[definition(
                        "churner",
                        Predicate::MovementRatioAtLeast { ratio: 10.0 },
                    )],
                )
                .unwrap();
                assert_eq!(
                    find(&outcomes, "churner").progress,
                    Progress::Pending {
                        have: 9,
                        need: 10,
                        eta_unix: None
                    }
                );
            }
        }

        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "churner",
                Predicate::MovementRatioAtLeast { ratio: 10.0 },
            )],
        )
        .unwrap();
        let outcome = find(&outcomes, "churner");
        assert_eq!(
            outcome.progress,
            Progress::Earned {
                since: EarnedSince::AsAt {
                    slot: 5_000,
                    unix: FIXTURE_ORIGIN + 5_000
                }
            }
        );
        assert_eq!(
            outcome.evidence,
            vec![Evidence::Movements {
                movements: 10,
                units: 1
            }]
        );
    }

    #[test]
    fn a_movement_ratio_needs_a_complete_holder_history() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let at = Assessment {
            now: 5_000,
            now_unix: FIXTURE_ORIGIN + 5_000,
            holder_coverage: Coverage::Partial,
        };
        let outcomes = evaluate(
            &store,
            subject,
            &at,
            &[definition(
                "churner",
                Predicate::MovementRatioAtLeast { ratio: 10.0 },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "churner").progress,
            Progress::Unavailable(Gap::HolderCoverage)
        );
    }

    #[test]
    fn no_dump_is_earned_by_a_seller_who_stayed_above_the_mint() {
        use crate::memory::fixtures::{account, filled_at};
        use chain_events::{PartyRole, Price, Side};

        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &filled_at(
                None,
                Side::Ask,
                Some(Price::native(2_000_000, 6)),
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                100,
            ),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "no-dump",
                Predicate::NoSaleBelowMint {
                    amount: 1_000_000,
                    decimals: 6,
                },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "no-dump").progress,
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );
    }

    #[test]
    fn no_dump_is_missed_by_a_sale_below_the_mint() {
        use crate::memory::fixtures::{account, filled_at};
        use chain_events::{PartyRole, Price, Side};

        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &filled_at(
                None,
                Side::Ask,
                Some(Price::native(500_000, 6)),
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                100,
            ),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "no-dump",
                Predicate::NoSaleBelowMint {
                    amount: 1_000_000,
                    decimals: 6,
                },
            )],
        )
        .unwrap();
        assert_eq!(find(&outcomes, "no-dump").progress, Progress::Missed);
    }

    #[test]
    fn no_dump_is_pending_for_a_holder_who_never_sold() {
        use crate::memory::fixtures::{mint, perp, placed};
        use chain_events::{PartyRole, Side};

        let mut store = MemoryStore::default();
        apply(&mut store, &mint(&perp("5065727031"), "addr1alice", 100)).unwrap();
        // The store has to have seen the market family, or "never sold" is unprovable — a
        // sale could be in the part of the history it was not told about.
        apply(
            &mut store,
            &placed("o1", Side::Ask, &[("addr1bob", PartyRole::Seller)], 110),
        )
        .unwrap();
        let alice = store
            .subject(&crate::memory::fixtures::account("addr1alice"))
            .unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "no-dump",
                Predicate::NoSaleBelowMint {
                    amount: 1_000_000,
                    decimals: 6,
                },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "no-dump").progress,
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None
            }
        );
    }

    #[test]
    fn an_unpriced_sale_cannot_prove_no_dump() {
        use crate::memory::fixtures::{account, filled_at};
        use chain_events::{PartyRole, Side};

        let mut store = MemoryStore::default();
        apply(
            &mut store,
            &filled_at(
                None,
                Side::Ask,
                None,
                &[
                    ("addr1alice", PartyRole::Seller),
                    ("addr1bob", PartyRole::Buyer),
                ],
                100,
            ),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "no-dump",
                Predicate::NoSaleBelowMint {
                    amount: 1_000_000,
                    decimals: 6,
                },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "no-dump").progress,
            Progress::Unavailable(Gap::FillPrice)
        );
    }

    #[test]
    fn a_trait_set_is_earned_across_assets() {
        let (mut store, subject) = crate::memory::fixtures::alice_mints_two();
        let units: Vec<UnitId> = store
            .lots(subject)
            .unwrap()
            .iter()
            .map(|lot| lot.unit)
            .collect();
        store.seed_traits(
            units[0],
            &[("Faction", "Genovault"), ("Hands", "Diamond Hands")],
        );
        store.seed_traits(units[1], &[("Faction", "Astro Schism")]);

        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[
                definition(
                    "faction",
                    Predicate::HasAllTraits {
                        category: "Faction".into(),
                        values: vec!["Genovault".into()],
                    },
                ),
                definition(
                    "three-crowns",
                    Predicate::HasAllTraits {
                        category: "Faction".into(),
                        values: vec!["Genovault".into(), "Astro Schism".into()],
                    },
                ),
                definition(
                    "hands",
                    Predicate::HasAllTraits {
                        category: "Hands".into(),
                        values: vec!["Diamond Hands".into()],
                    },
                ),
            ],
        )
        .unwrap();

        assert!(matches!(
            find(&outcomes, "faction").progress,
            Progress::Earned { .. }
        ));
        // Two factions across two assets IS the set: `tri-faction` is "a Perp from each
        // of the factions", and no Perp carries three factions.
        assert!(matches!(
            find(&outcomes, "three-crowns").progress,
            Progress::Earned { .. }
        ));
        assert!(matches!(
            find(&outcomes, "hands").progress,
            Progress::Earned { .. }
        ));
    }

    /// Three assets, one faction each, minted at 100 / 150 / 200.
    fn three_factions() -> (MemoryStore, SubjectId) {
        use crate::memory::fixtures::{account, mint, perp};
        let mut store = MemoryStore::default();
        let factions = ["Genovault", "Astro Schism", "Kraashers"];
        for (index, faction) in factions.iter().enumerate() {
            let asset = perp(&format!("506572703{index}"));
            apply(
                &mut store,
                &mint(&asset, "addr1alice", 100 + index as u64 * 50),
            )
            .unwrap();
            let unit = store.unit(&asset).unwrap();
            store.seed_traits(unit, &[("Faction", faction)]);
        }
        let alice = store.subject(&account("addr1alice")).unwrap();
        (store, alice)
    }

    #[test]
    fn a_faction_set_is_earned_when_the_last_faction_arrives() {
        let (store, alice) = three_factions();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "tri-faction",
                Predicate::HasAllTraits {
                    category: "Faction".into(),
                    values: vec![
                        "Genovault".into(),
                        "Astro Schism".into(),
                        "Kraashers".into(),
                    ],
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "tri-faction").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
        assert_eq!(find(&outcomes, "tri-faction").evidence.len(), 3);
    }

    #[test]
    fn a_faction_set_needs_all_three_at_once() {
        // Genovault is sold at 150 before Kraashers arrives at 200, so the three are never
        // held together — the ever-held form would have earned it, this one must not.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let genovault = perp("5065727030");
        let astro = perp("5065727031");
        let kraashers = perp("5065727032");
        apply(&mut store, &mint(&genovault, "addr1alice", 100)).unwrap();
        apply(
            &mut store,
            &transfer(&genovault, "addr1alice", "addr1bob", 150),
        )
        .unwrap();
        apply(&mut store, &mint(&astro, "addr1alice", 150)).unwrap();
        apply(&mut store, &mint(&kraashers, "addr1alice", 200)).unwrap();
        for (asset, faction) in [
            (&genovault, "Genovault"),
            (&astro, "Astro Schism"),
            (&kraashers, "Kraashers"),
        ] {
            let unit = store.unit(asset).unwrap();
            store.seed_traits(unit, &[("Faction", faction)]);
        }
        let alice = store.subject(&account("addr1alice")).unwrap();

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "tri-faction",
                Predicate::HasAllTraits {
                    category: "Faction".into(),
                    values: vec![
                        "Genovault".into(),
                        "Astro Schism".into(),
                        "Kraashers".into(),
                    ],
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "tri-faction").progress,
            Progress::Pending {
                have: 2,
                need: 3,
                eta_unix: None,
            }
        );
    }

    #[test]
    fn a_faction_set_survives_selling_one_of_them() {
        use crate::memory::fixtures::{perp, transfer};
        let (mut store, alice) = three_factions();
        let asset = perp("5065727030");
        apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 250)).unwrap();

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "tri-faction",
                Predicate::HasAllTraits {
                    category: "Faction".into(),
                    values: vec![
                        "Genovault".into(),
                        "Astro Schism".into(),
                        "Kraashers".into(),
                    ],
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "tri-faction").progress,
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
    }

    #[test]
    fn a_trait_set_over_assets_with_no_metadata_is_unavailable() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[definition(
                "faction",
                Predicate::HasAllTraits {
                    category: "Faction".into(),
                    values: vec!["Genovault".into()],
                },
            )],
        )
        .unwrap();
        assert_eq!(
            find(&outcomes, "faction").progress,
            Progress::Unavailable(Gap::Traits)
        );
    }

    #[test]
    fn a_wildcard_category_matches_across_categories() {
        let (mut store, subject) = crate::memory::fixtures::alice_mints_one();
        let unit = store.lots(subject).unwrap()[0].unit;
        store.seed_traits(unit, &[("Faction", "Genovault"), ("Hands", "Shard")]);

        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[definition(
                "any",
                Predicate::HasAllTraits {
                    category: "*".into(),
                    values: vec!["Genovault".into(), "Shard".into()],
                },
            )],
        )
        .unwrap();
        assert!(matches!(
            find(&outcomes, "any").progress,
            Progress::Earned { .. }
        ));
    }

    #[test]
    fn evaluating_a_subject_with_no_lots_is_pending_and_not_an_error() {
        let mut store = MemoryStore::default();
        let subject = store
            .subject(&chain_events::AccountRef::new(
                chains::ChainRef::cardano_mainnet(),
                "addr1nobody",
            ))
            .unwrap();
        let outcomes = evaluate(
            &store,
            subject,
            &at(1_000),
            &[
                definition("never-sold", Predicate::NeverDisposed),
                definition("tenure", Predicate::TotalTenureAtLeast { days: 1 }),
            ],
        )
        .unwrap();
        for id in ["never-sold", "tenure"] {
            assert!(matches!(
                find(&outcomes, id).progress,
                Progress::Pending {
                    have: 0,
                    eta_unix: None,
                    ..
                }
            ));
        }
    }

    #[test]
    fn one_evaluation_answers_every_definition_without_a_second_read() {
        // The assertion is structural: `evaluate` takes `&S`, and this store counts reads.
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let definitions: Vec<Definition<'_>> = vec![
            definition("a", Predicate::NeverDisposed),
            definition("b", Predicate::HeldSinceMint),
            definition("c", Predicate::TotalTenureAtLeast { days: 1 }),
            definition("d", Predicate::RoundTripsAtLeast { n: 1 }),
        ];
        let outcomes = evaluate(&store, subject, &at(1_000), &definitions).unwrap();
        assert_eq!(outcomes.len(), 4);
        assert_eq!(store.lot_reads(), 1);
    }

    #[test]
    fn a_rebuild_and_an_incremental_fold_agree_by_construction() {
        // `rebuild` is a loop over `apply`; this pins that it is not a second algorithm.
        let mut batch = MemoryStore::default();
        let mut incremental = MemoryStore::default();
        let events = crate::memory::fixtures::history();
        let report = rebuild(&mut batch, events.clone()).unwrap();
        for event in &events {
            apply(&mut incremental, event).unwrap();
        }
        assert_eq!(report.events, events.len() as u64);
        assert_eq!(report.subjects, incremental.subjects().unwrap().len());
        assert_eq!(
            batch.lots(SubjectId(0)).unwrap(),
            incremental.lots(SubjectId(0)).unwrap()
        );
    }

    #[test]
    fn the_acquired_by_flag_is_what_separates_a_mint_lot_from_a_bought_one() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        assert_eq!(
            store.lots(subject).unwrap()[0].acquired_by,
            AcquiredBy::Mint
        );
    }

    fn has_trait(category: &str, value: &str) -> Predicate {
        Predicate::HasTrait {
            category: category.into(),
            value: value.into(),
        }
    }

    #[test]
    fn a_trait_is_earned_at_the_slot_the_asset_arrived() {
        let (mut store, subject) = crate::memory::fixtures::alice_mints_one();
        let unit = store.lots(subject).unwrap()[0].unit;
        store.seed_traits(unit, &[("Hands", "Diamond Hands")]);

        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[definition(
                "diamond-hands",
                has_trait("Hands", "Diamond Hands"),
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "diamond-hands").progress,
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );
    }

    #[test]
    fn a_trait_survives_selling_the_asset() {
        // "Held a Perp wearing X" is a fact about the past; selling it does not un-earn it.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 200)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let unit = store.unit(&asset).unwrap();
        store.seed_traits(unit, &[("Hands", "Diamond Hands")]);

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "diamond-hands",
                has_trait("Hands", "Diamond Hands"),
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "diamond-hands").progress,
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );
    }

    #[test]
    fn a_star_value_matches_any_value_in_the_category() {
        let (mut store, subject) = crate::memory::fixtures::alice_mints_one();
        let unit = store.lots(subject).unwrap()[0].unit;
        store.seed_traits(unit, &[("Leader", "Some Leader")]);

        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[definition("the-leader", has_trait("Leader", "*"))],
        )
        .unwrap();

        assert!(matches!(
            find(&outcomes, "the-leader").progress,
            Progress::Earned { .. }
        ));
    }

    #[test]
    fn a_trait_with_no_metadata_is_unavailable() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let outcomes = evaluate(
            &store,
            subject,
            &at(5_000),
            &[definition(
                "diamond-hands",
                has_trait("Hands", "Diamond Hands"),
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "diamond-hands").progress,
            Progress::Unavailable(Gap::Traits)
        );
    }

    /// Two assets Alice minted at different heights, with the trait rows seeded by unit.
    fn two_minted(
        first: (&str, &str),
        second: (&str, &str),
        second_height: u64,
    ) -> (MemoryStore, SubjectId, Vec<UnitId>) {
        use crate::memory::fixtures::{account, mint, perp};
        let mut store = MemoryStore::default();
        let one = perp("5065727031");
        let two = perp("5065727032");
        apply(&mut store, &mint(&one, "addr1alice", 100)).unwrap();
        apply(&mut store, &mint(&two, "addr1alice", second_height)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let units = vec![store.unit(&one).unwrap(), store.unit(&two).unwrap()];
        store.seed_traits(units[0], &[first]);
        store.seed_traits(units[1], &[second]);
        (store, alice, units)
    }

    #[test]
    fn a_value_set_is_earned_when_the_last_value_arrives() {
        let (store, alice, _) = two_minted(
            ("Clothes", "Elemental Fire Mutation"),
            ("Clothes", "Elemental Ice Mutation"),
            150,
        );
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "elementalist",
                Predicate::HasTraitValues {
                    category: "Clothes".into(),
                    values: vec![
                        "Elemental Fire Mutation".into(),
                        "Elemental Ice Mutation".into(),
                    ],
                    at_least: 2,
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "elementalist").progress,
            Progress::Earned {
                since: EarnedSince::Exact(150)
            }
        );
        assert_eq!(find(&outcomes, "elementalist").evidence.len(), 2);
    }

    #[test]
    fn a_value_set_reports_how_many_were_seen() {
        let (store, alice, _) = two_minted(
            ("Clothes", "Elemental Fire Mutation"),
            ("Faction", "Genovault"),
            150,
        );
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "elementalist",
                Predicate::HasTraitValues {
                    category: "Clothes".into(),
                    values: vec![
                        "Elemental Fire Mutation".into(),
                        "Elemental Ice Mutation".into(),
                    ],
                    at_least: 2,
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "elementalist").progress,
            Progress::Pending {
                have: 1,
                need: 2,
                eta_unix: None,
            }
        );
    }

    #[test]
    fn at_once_is_earned_only_while_the_intervals_overlap() {
        // One [100, 200), two [150, …): they overlap from 150.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let one = perp("5065727031");
        let two = perp("5065727032");
        apply(&mut store, &mint(&one, "addr1alice", 100)).unwrap();
        apply(&mut store, &mint(&two, "addr1alice", 150)).unwrap();
        apply(&mut store, &transfer(&one, "addr1alice", "addr1bob", 200)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        for asset in [&one, &two] {
            let unit = store.unit(asset).unwrap();
            store.seed_traits(unit, &[("Post It", "KEK")]);
        }

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "post-it-overload",
                Predicate::HeldWithTraitAtOnce {
                    category: "Post It".into(),
                    value: None,
                    at_least: 2,
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "post-it-overload").progress,
            Progress::Earned {
                since: EarnedSince::Exact(150)
            }
        );
    }

    #[test]
    fn at_once_does_not_count_a_disposal_at_the_same_slot_as_simultaneous() {
        // One [100, 200) and two [200, …): half-open, so they never overlap.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let one = perp("5065727031");
        let two = perp("5065727032");
        apply(&mut store, &mint(&one, "addr1alice", 100)).unwrap();
        apply(&mut store, &mint(&two, "addr1alice", 200)).unwrap();
        apply(&mut store, &transfer(&one, "addr1alice", "addr1bob", 200)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        for asset in [&one, &two] {
            let unit = store.unit(asset).unwrap();
            store.seed_traits(unit, &[("Post It", "KEK")]);
        }

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "post-it-overload",
                Predicate::HeldWithTraitAtOnce {
                    category: "Post It".into(),
                    value: None,
                    at_least: 2,
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "post-it-overload").progress,
            Progress::Pending {
                have: 1,
                need: 2,
                eta_unix: None,
            }
        );
    }

    #[test]
    fn a_zero_length_lot_neither_counts_nor_breaks_the_sweep() {
        // Alice holds one spanning lot [100, 200) and a second lot of the SAME unit that
        // opens and closes at 200. Ends sort before starts at a shared slot, so without
        // skipping the zero-length interval its end would decrement the spanning lot.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 200)).unwrap();
        apply(&mut store, &transfer(&asset, "addr1bob", "addr1alice", 200)).unwrap();
        apply(
            &mut store,
            &transfer(&asset, "addr1alice", "addr1carol", 200),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let unit = store.unit(&asset).unwrap();
        store.seed_traits(unit, &[("Post It", "KEK")]);

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "post-it-overload",
                Predicate::HeldWithTraitAtOnce {
                    category: "Post It".into(),
                    value: None,
                    at_least: 2,
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "post-it-overload").progress,
            Progress::Pending {
                have: 1,
                need: 2,
                eta_unix: None,
            }
        );
    }

    #[test]
    fn holds_at_least_is_a_peak_and_needs_no_traits() {
        // One [100, …), two [150, …): the peak is two from 150, even after one is sold.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let one = perp("5065727031");
        let two = perp("5065727032");
        apply(&mut store, &mint(&one, "addr1alice", 100)).unwrap();
        apply(&mut store, &mint(&two, "addr1alice", 150)).unwrap();
        apply(&mut store, &transfer(&one, "addr1alice", "addr1bob", 200)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "two-perps",
                Predicate::HoldsAtLeast { at_least: 2 },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "two-perps").progress,
            Progress::Earned {
                since: EarnedSince::Exact(150)
            }
        );
    }

    #[test]
    fn a_holding_only_kept_through_a_listing_reads_listed() {
        // Alice minted two and listed one: she owns two, but keeps one.
        use crate::memory::fixtures::{account, mint, perp, placed};
        use chain_events::{PartyRole, Side};
        let mut store = MemoryStore::default();
        apply(&mut store, &mint(&perp("5065727031"), "addr1alice", 100)).unwrap();
        apply(&mut store, &mint(&perp("5065727032"), "addr1alice", 150)).unwrap();
        // The fixture's placement is for 5065727031.
        apply(
            &mut store,
            &placed(
                "ask-1",
                Side::Ask,
                &[("addr1alice", PartyRole::Seller)],
                200,
            ),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[
                definition("two-perps", Predicate::HoldsAtLeast { at_least: 2 }),
                definition("one-perp", Predicate::HoldsAtLeast { at_least: 1 }),
            ],
        )
        .unwrap();
        assert_eq!(find(&outcomes, "two-perps").standing, Standing::Listed);
        assert_eq!(find(&outcomes, "one-perp").standing, Standing::Now);
    }

    #[test]
    fn a_holding_reads_now_or_past_and_a_record_has_no_tense() {
        // Alice minted two and sold one: she held two once and holds one now.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let one = perp("5065727031");
        let two = perp("5065727032");
        apply(&mut store, &mint(&one, "addr1alice", 100)).unwrap();
        apply(&mut store, &mint(&two, "addr1alice", 150)).unwrap();
        apply(&mut store, &transfer(&one, "addr1alice", "addr1bob", 200)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[
                definition("two-perps", Predicate::HoldsAtLeast { at_least: 2 }),
                definition("one-perp", Predicate::HoldsAtLeast { at_least: 1 }),
                definition("minted", Predicate::MintedCount { at_least: 1 }),
                definition("three-perps", Predicate::HoldsAtLeast { at_least: 3 }),
            ],
        )
        .unwrap();

        assert_eq!(find(&outcomes, "two-perps").standing, Standing::Past);
        assert_eq!(find(&outcomes, "one-perp").standing, Standing::Now);
        assert_eq!(find(&outcomes, "minted").standing, Standing::Record);
        assert_eq!(
            find(&outcomes, "three-perps").standing,
            Standing::Record,
            "unearned has no tense either"
        );
    }

    #[test]
    fn holds_at_least_does_not_count_a_sale_and_buy_at_the_same_slot() {
        // One [100, 200) and two [200, …): half-open, so the peak stays at one.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let one = perp("5065727031");
        let two = perp("5065727032");
        apply(&mut store, &mint(&one, "addr1alice", 100)).unwrap();
        apply(&mut store, &mint(&two, "addr1alice", 200)).unwrap();
        apply(&mut store, &transfer(&one, "addr1alice", "addr1bob", 200)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "two-perps",
                Predicate::HoldsAtLeast { at_least: 2 },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "two-perps").progress,
            Progress::Pending {
                have: 1,
                need: 2,
                eta_unix: None,
            }
        );
    }

    #[test]
    fn a_shared_value_counts_assets_ever_held_not_only_at_once() {
        // One [100, 200) and two [300, …), both wearing Red: never simultaneous, still a pair.
        use crate::memory::fixtures::{account, mint, perp, transfer};
        let mut store = MemoryStore::default();
        let one = perp("5065727031");
        let two = perp("5065727032");
        apply(&mut store, &mint(&one, "addr1alice", 100)).unwrap();
        apply(&mut store, &mint(&two, "addr1alice", 300)).unwrap();
        apply(&mut store, &transfer(&one, "addr1alice", "addr1bob", 200)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        for asset in [&one, &two] {
            let unit = store.unit(asset).unwrap();
            store.seed_traits(unit, &[("Clothes", "Red")]);
        }

        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "matched",
                Predicate::SameValueAcrossAssets {
                    category: "Clothes".into(),
                    assets: 2,
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "matched").progress,
            Progress::Earned {
                since: EarnedSince::Exact(300)
            }
        );
    }

    #[test]
    fn a_shared_value_reports_the_best_value_count() {
        let (store, alice, _) = two_minted(("Clothes", "Red"), ("Clothes", "Blue"), 150);
        let outcomes = evaluate(
            &store,
            alice,
            &at(5_000),
            &[definition(
                "matched",
                Predicate::SameValueAcrossAssets {
                    category: "Clothes".into(),
                    assets: 2,
                },
            )],
        )
        .unwrap();

        assert_eq!(
            find(&outcomes, "matched").progress,
            Progress::Pending {
                have: 1,
                need: 2,
                eta_unix: None,
            }
        );
    }

    /// Alice mints first at 100, Bob second at 200.
    fn two_minters() -> (MemoryStore, SubjectId, SubjectId) {
        use crate::memory::fixtures::{account, mint, perp};
        let mut store = MemoryStore::default();
        apply(&mut store, &mint(&perp("5065727030"), "addr1alice", 100)).unwrap();
        apply(&mut store, &mint(&perp("5065727031"), "addr1bob", 200)).unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let bob = store.subject(&account("addr1bob")).unwrap();
        (store, alice, bob)
    }

    fn progress_of(store: &MemoryStore, subject: SubjectId, predicate: Predicate) -> Progress {
        let outcomes = evaluate(store, subject, &at(5_000), &[definition("p", predicate)]).unwrap();
        find(&outcomes, "p").progress
    }

    #[test]
    fn mint_facts_rank_in_fold_order() {
        let (store, alice, bob) = two_minters();
        let alice_unit = store.lots(alice).unwrap()[0].unit;
        let bob_unit = store.lots(bob).unwrap()[0].unit;

        assert_eq!(
            store.mint_of(alice_unit).unwrap().unwrap(),
            MintFact {
                slot: 100,
                unix: 1_700_000_100,
                rank: 1,
                previous_slot: None,
                previous_unix: None,
            }
        );
        assert_eq!(
            store.mint_of(bob_unit).unwrap().unwrap(),
            MintFact {
                slot: 200,
                unix: 1_700_000_200,
                rank: 2,
                previous_slot: Some(100),
                previous_unix: Some(1_700_000_100),
            }
        );
        assert_eq!(
            store.mint_summary().unwrap().unwrap(),
            MintSummary {
                first_slot: 100,
                first_unix: 1_700_000_100,
                last_slot: 200,
                last_unix: 1_700_000_200,
                count: 2,
            }
        );
    }

    #[test]
    fn a_mint_inside_the_window_is_earned_and_a_late_one_is_missed() {
        let (store, alice, bob) = two_minters();
        // The window is 50 s: Alice minted 0 s after the first mint, Bob 100 s after it.
        assert_eq!(
            progress_of(&store, alice, Predicate::MintWithin { seconds: 50 }),
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );
        assert_eq!(
            progress_of(&store, bob, Predicate::MintWithin { seconds: 50 }),
            Progress::Missed
        );
    }

    #[test]
    fn a_rank_is_read_from_the_collection_not_the_holder() {
        let (store, alice, bob) = two_minters();
        assert_eq!(
            progress_of(&store, alice, Predicate::MintRankAtMost { n: 1 }),
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );
        assert_eq!(
            progress_of(&store, bob, Predicate::MintRankAtMost { n: 1 }),
            Progress::Missed
        );
        assert_eq!(
            progress_of(&store, bob, Predicate::MintRankAtLeast { n: 2 }),
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
    }

    #[test]
    fn a_rank_fraction_divides_by_the_whole_mint() {
        let (store, alice, bob) = two_minters();
        assert_eq!(
            progress_of(
                &store,
                bob,
                Predicate::MintRankAfterFraction { fraction: 0.9 }
            ),
            Progress::Earned {
                since: EarnedSince::Exact(200)
            }
        );
        assert_eq!(
            progress_of(
                &store,
                alice,
                Predicate::MintRankAfterFraction { fraction: 0.9 }
            ),
            Progress::Missed
        );
    }

    #[test]
    fn a_mint_after_a_week_of_quiet_is_earned_only_by_the_later_mint() {
        use crate::memory::fixtures::{account, mint, perp};
        let mut store = MemoryStore::default();
        apply(&mut store, &mint(&perp("5065727030"), "addr1alice", 100)).unwrap();
        apply(
            &mut store,
            &mint(&perp("5065727031"), "addr1bob", 100 + 7 * 86_400 + 1),
        )
        .unwrap();
        let alice = store.subject(&account("addr1alice")).unwrap();
        let bob = store.subject(&account("addr1bob")).unwrap();

        assert_eq!(
            progress_of(&store, bob, Predicate::MintAfterGap { days: 7 }),
            Progress::Earned {
                since: EarnedSince::Exact(100 + 7 * 86_400 + 1)
            }
        );
        // The first mint has no predecessor, so it cannot be "after a gap".
        assert_eq!(
            progress_of(&store, alice, Predicate::MintAfterGap { days: 7 }),
            Progress::Missed
        );
    }

    #[test]
    fn minted_count_counts_only_the_subjects_own_mints() {
        let (mut store, alice) = crate::memory::fixtures::alice_mints_two();
        assert_eq!(
            progress_of(&store, alice, Predicate::MintedCount { at_least: 2 }),
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );
        assert_eq!(
            progress_of(&store, alice, Predicate::MintedCount { at_least: 3 }),
            Progress::Pending {
                have: 2,
                need: 3,
                eta_unix: None,
            }
        );

        // Bob receives one and mints none. A purchase is not a mint.
        let asset = crate::memory::fixtures::perp("5065727031");
        apply(
            &mut store,
            &crate::memory::fixtures::transfer(&asset, "addr1alice", "addr1bob", 300),
        )
        .unwrap();
        let bob = store
            .subject(&crate::memory::fixtures::account("addr1bob"))
            .unwrap();
        assert_eq!(
            progress_of(&store, bob, Predicate::MintedCount { at_least: 1 }),
            Progress::Pending {
                have: 0,
                need: 1,
                eta_unix: None,
            }
        );
    }

    #[test]
    fn minted_and_still_held_needs_the_asset_in_hand() {
        let (mut store, alice) = crate::memory::fixtures::alice_mints_two();
        assert_eq!(
            progress_of(&store, alice, Predicate::MintedAndStillHeld { at_least: 2 }),
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );

        let asset = crate::memory::fixtures::perp("5065727031");
        apply(
            &mut store,
            &crate::memory::fixtures::transfer(&asset, "addr1alice", "addr1bob", 300),
        )
        .unwrap();
        assert_eq!(
            progress_of(&store, alice, Predicate::MintedAndStillHeld { at_least: 2 }),
            Progress::Pending {
                have: 1,
                need: 2,
                eta_unix: None,
            }
        );
    }

    #[test]
    fn all_mints_retained_is_missed_once_one_is_sold() {
        let (mut store, alice) = crate::memory::fixtures::alice_mints_two();
        assert_eq!(
            progress_of(&store, alice, Predicate::AllMintsRetained),
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );

        let asset = crate::memory::fixtures::perp("5065727031");
        apply(
            &mut store,
            &crate::memory::fixtures::transfer(&asset, "addr1alice", "addr1bob", 300),
        )
        .unwrap();
        assert_eq!(
            progress_of(&store, alice, Predicate::AllMintsRetained),
            Progress::Missed
        );
    }

    #[test]
    fn never_bought_is_missed_by_an_arrival_from_another_party() {
        let (mut store, alice) = crate::memory::fixtures::alice_mints_two();
        assert_eq!(
            progress_of(&store, alice, Predicate::NeverBought),
            Progress::Earned {
                since: EarnedSince::Exact(100)
            }
        );

        let asset = crate::memory::fixtures::perp("5065727031");
        apply(
            &mut store,
            &crate::memory::fixtures::transfer(&asset, "addr1alice", "addr1bob", 300),
        )
        .unwrap();
        let bob = store
            .subject(&crate::memory::fixtures::account("addr1bob"))
            .unwrap();
        assert_eq!(
            progress_of(&store, bob, Predicate::NeverBought),
            Progress::Missed
        );
    }
}
