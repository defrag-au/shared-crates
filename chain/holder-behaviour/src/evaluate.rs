//! Reading a subject's lots as achievements, and saying what is missing.
//!
//! # Earned, pending, missed, unavailable
//!
//! Four answers, and the difference between the last three is the whole point:
//!
//! - [`Progress::Earned`] — the predicate is true, and the outcome cites what proves it.
//! - [`Progress::Pending`] — not yet true, and the subject is on a route to it. For a
//!   tenure predicate this carries the number reached, the number needed and **the slot
//!   at which the currently open lots get there**, which is free: the interval is
//!   already stored, so no clock and no nightly job is needed to keep a near-miss fresh.
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

use chain_events::{Coverage, TxRef};

use std::collections::{BTreeMap, BTreeSet};

use crate::predicate::Predicate;
use crate::state::{Holding, LotId, Slot, Store, SubjectId, UnitId};

/// One subject's trait rows, read once per evaluation.
///
/// Keyed by unit so a predicate reads the map instead of the store. A unit present with an
/// empty `Vec` is a unit whose metadata was never recorded — "no rows", which every trait
/// predicate reports as `Unavailable` rather than as a miss.
type TraitMap = BTreeMap<UnitId, Vec<(String, String)>>;

/// Chain seconds in a day.
///
/// Cardano keeps one slot per second, verified against the archive (a 25,198,962-slot
/// span of Perps history is exactly 25,198,962 unix seconds). A chain that does not keep
/// time this way must convert before it states a predicate in days; the predicate
/// vocabulary has no other unit because a block count is not a duration.
pub const SECONDS_PER_DAY: u64 = 86_400;

/// What the caller believes about the store it is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assessment {
    /// The chain height to read the open intervals at. A predicate that grows with time
    /// is answered as at this slot, which is what makes a pending tenure reportable.
    pub now: Slot,
    /// How much of the holder family this store's source can account for.
    ///
    /// [`Coverage::Partial`] makes every span and `never_*` predicate `Unavailable`
    /// rather than short: a span truncated by a missing movement is not a near miss.
    pub holder_coverage: Coverage,
}

impl Assessment {
    /// An assessment at `now` over a source that has proven completeness.
    pub const fn complete(now: Slot) -> Self {
        Self {
            now,
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
        /// The slot the currently open lots reach `need`, when they ever do.
        ///
        /// `None` means nothing in hand will change the answer: the subject holds none,
        /// or holds nothing that accrues toward this predicate.
        eta_slot: Option<Slot>,
    },
    /// A one-way predicate the subject has already failed.
    Missed,
    /// The source cannot answer, and the reason is named.
    Unavailable(Gap),
}

/// When an earned predicate became true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarnedSince {
    /// The slot the fact itself happened.
    Exact(Slot),
    /// True as at this reading; the crossing slot was not reconstructed.
    AsAt(Slot),
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
    let mut outcomes = Vec::with_capacity(definitions.len());
    for definition in definitions {
        let (progress, evidence) = match &definition.predicate {
            Predicate::NeverDisposed => never_disposed(&lots, at),
            Predicate::HeldSinceMint => held_since_mint(&lots, at),
            Predicate::TotalTenureAtLeast { days } => {
                total_tenure_at_least(&lots, at, u64::from(*days) * SECONDS_PER_DAY)
            }
            Predicate::RoundTripsAtLeast { n } => round_trips_at_least(&lots, at, u64::from(*n)),
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
            } => held_with_trait_at_once(
                &traits,
                &lots,
                at,
                category,
                value.as_deref(),
                *at_least,
            ),
            Predicate::SameValueAcrossAssets { category, assets } => {
                same_value_across_assets(&traits, &lots, at, category, *assets)
            }
        };
        outcomes.push(Outcome {
            id: definition.id,
            progress,
            evidence,
        });
    }
    Ok(outcomes)
}

/// Reads every unit's trait rows once.
fn read_traits<S: Store>(store: &S, lots: &[Holding]) -> Result<TraitMap, S::Error> {
    let mut traits = TraitMap::new();
    for unit in units_ever(lots) {
        traits.insert(unit, store.traits_of(unit)?);
    }
    Ok(traits)
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
                eta_slot: None,
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
                eta_slot: None,
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
    let have: u64 = lots.iter().map(|lot| lot.tenure_at(at.now)).sum();
    if have >= need {
        // The threshold was crossed somewhere between the last event and `now`, and the
        // lots alone do not say which disposal did it. Cite the earliest acquisition: it
        // is the transaction that started the clock.
        let earliest = lots.iter().min_by_key(|lot| lot.acquired_at);
        let evidence = earliest.map(evidence_of).into_iter().collect();
        return (
            Progress::Earned {
                since: EarnedSince::AsAt(at.now),
            },
            evidence,
        );
    }

    // One open lot accrues one second per second however many units it holds, because
    // tenure is the length of an interval and not its size.
    let open_lots = lots.iter().filter(|lot| lot.is_open()).count() as u64;
    let eta_slot = (open_lots > 0).then(|| {
        let remaining = need - have;
        at.now.saturating_add(remaining.div_ceil(open_lots))
    });
    (
        Progress::Pending {
            have,
            need,
            eta_slot,
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
                since: EarnedSince::AsAt(at.now),
            },
            evidence,
        );
    }
    // A round trip needs a purchase and a sale; neither is on a schedule, so there is no
    // slot to promise.
    (
        Progress::Pending {
            have,
            need,
            eta_slot: None,
        },
        Vec::new(),
    )
}

fn has_all_traits<S: Store>(
    store: &S,
    lots: &[Holding],
    at: &Assessment,
    category: &str,
    values: &[String],
) -> Result<(Progress, Vec<Evidence>), S::Error> {
    let mut units: Vec<UnitId> = lots
        .iter()
        .filter(|lot| lot.is_open())
        .map(|lot| lot.unit)
        .collect();
    units.sort_unstable();
    units.dedup();

    if units.is_empty() {
        return Ok((
            Progress::Pending {
                have: 0,
                need: values.len() as u64,
                eta_slot: None,
            },
            Vec::new(),
        ));
    }

    let mut any_metadata = false;
    for unit in units {
        let traits = store.traits_of(unit)?;
        if traits.is_empty() {
            continue;
        }
        any_metadata = true;
        let matched: Vec<String> = values
            .iter()
            .filter(|value| {
                traits.iter().any(|(found_category, found_value)| {
                    (category == "*" || found_category == category) && found_value == *value
                })
            })
            .cloned()
            .collect();
        if matched.len() == values.len() {
            return Ok((
                Progress::Earned {
                    since: EarnedSince::AsAt(at.now),
                },
                vec![Evidence::Traits {
                    unit,
                    values: matched,
                }],
            ));
        }
    }

    if !any_metadata {
        // Every held asset is missing metadata: the answer is unknown, not negative.
        return Ok((Progress::Unavailable(Gap::Traits), Vec::new()));
    }
    Ok((
        Progress::Pending {
            have: 0,
            need: values.len() as u64,
            eta_slot: None,
        },
        Vec::new(),
    ))
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

    #[test]
    fn a_subject_that_never_disposed_is_earned_at_its_first_acquisition() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let outcomes = evaluate(
            &store,
            subject,
            &Assessment::complete(5_000),
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
            &Assessment::complete(5_000),
            &[definition("never-sold", Predicate::NeverDisposed)],
        )
        .unwrap();
        assert_eq!(find(&outcomes, "never-sold").progress, Progress::Missed);
    }

    #[test]
    fn tenure_reports_the_number_reached_and_the_slot_it_will_be_earned() {
        // One unit from slot 100. At slot 1,000 it has 900 seconds; a 1-day threshold
        // needs 86,400, so 85,500 more seconds → earned at 86,500.
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let outcomes = evaluate(
            &store,
            subject,
            &Assessment::complete(1_000),
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
                eta_slot: Some(86_500),
            }
        );
    }

    #[test]
    fn tenure_accrues_faster_with_more_open_units() {
        // Two units from slot 100: 1,800 seconds at 1,000, and 43,200 more seconds of
        // headroom are covered at half the rate in slots.
        let (store, subject) = crate::memory::fixtures::alice_mints_two();
        let outcomes = evaluate(
            &store,
            subject,
            &Assessment::complete(1_000),
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
                eta_slot: Some(43_300),
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
            &Assessment::complete(9_000),
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
                since: EarnedSince::AsAt(9_000)
            }
        );
        assert!(matches!(outcome.evidence[0], Evidence::Lot { .. }));
    }

    #[test]
    fn a_partial_coverage_makes_a_span_unavailable_rather_than_short() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let at = Assessment {
            now: 1_000,
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
            &Assessment::complete(9_000),
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
                &Assessment::complete(9_000),
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
                    eta_slot: None,
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
            &Assessment::complete(9_000),
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
    fn a_trait_set_is_earned_when_one_asset_carries_every_value() {
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
            &Assessment::complete(5_000),
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
        // Two factions across two assets is not one asset carrying both.
        assert_eq!(
            find(&outcomes, "three-crowns").progress,
            Progress::Pending {
                have: 0,
                need: 2,
                eta_slot: None,
            }
        );
        assert!(matches!(
            find(&outcomes, "hands").progress,
            Progress::Earned { .. }
        ));
    }

    #[test]
    fn a_trait_set_over_assets_with_no_metadata_is_unavailable() {
        let (store, subject) = crate::memory::fixtures::alice_mints_one();
        let outcomes = evaluate(
            &store,
            subject,
            &Assessment::complete(5_000),
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
            &Assessment::complete(5_000),
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
            &Assessment::complete(1_000),
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
                    eta_slot: None,
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
        let outcomes =
            evaluate(&store, subject, &Assessment::complete(1_000), &definitions).unwrap();
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
}
