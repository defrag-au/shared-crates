//! The allocation policy: what a subject's earned set scores under a campaign's thresholds,
//! and which phase that puts it in.
//!
//! # Why this is in the crate and not in the tool that models a mint
//!
//! A campaign asks two different programs the same question. The modelling tool sweeps the
//! thresholds over a whole bundle and prints how many people land in each phase; the site
//! answers one identity and shows it a number. If each carried its own copy of "core ≥ 3 is
//! guaranteed", the first threshold that moved would make them disagree, and the disagreement
//! would be a wrong phase for real people rather than a test failure. So the rule lives here,
//! once, and both call it.
//!
//! # What a phase is *not* decided by
//!
//! Only what a bundle can prove: the earned set, its rarity, and the lots. The two inputs a
//! bundle cannot answer — whether the holder meets the campaign's gate *now*, and whether a
//! live status promotes it — are passed in by the caller, because they are statuses and a
//! status is asked, not folded. [`Counts`] is therefore the junction: a [`Score`] from the
//! snapshot, and two booleans from a read taken at the claim.

use std::collections::{BTreeMap, BTreeSet};

use crate::bundle::{Bundle, Lot, Rarity, SubjectView};

/// The part an achievement plays in a score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Never filtered: every one of these counts.
    Core,
    /// Counts only when it is rare enough, and only the rarest few.
    Flavour,
}

impl Role {
    /// The catalogue's spelling, or `None` for a role this build does not know.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "core" => Some(Self::Core),
            "flavour" => Some(Self::Flavour),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Flavour => "flavour",
        }
    }
}

/// A gated allocation policy, as a campaign writes it.
///
/// `Copy` because every field is swept: the modelling tool recomputes the whole population for
/// each threshold combination, and a policy that had to be borrowed through that would be noise.
/// `Serialize` because the published manifest carries it: a consumer that reads the bundle must
/// not be configured with its own copy of the thresholds.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Allocation {
    /// Core achievements for the guaranteed phase.
    pub gtd_core: u32,
    /// Core achievements for the first-come phase.
    pub fcfs_core: u32,
    /// Flavour achievements that also reach the first-come phase. Zero disables this path.
    pub fcfs_flavour: u32,
    /// Mints per identity, at most.
    pub per_identity: u32,
    /// A flavour achievement held by more than this share does not count.
    pub rarity_floor: f64,
    /// How many of a holder's rarest flavour achievements count.
    pub count_rarest: u32,
}

/// What an earned set scores, before any holding is read.
///
/// Deliberately not the holdings: an identity unions several wallets' earned sets, so the
/// `peak`/`current` of any one row is not the answer. Callers that want those read them from
/// the row they care about with [`held_at`] and [`peak_concurrent`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Score {
    /// The core achievements earned, ascending. Every one counts.
    pub core: Vec<u16>,
    /// The flavour achievements that cleared the floor, rarest first, already capped.
    pub credits: Vec<u16>,
}

impl Score {
    /// The phase inputs, once the caller has read what only a status can answer, and what
    /// holdings it wants to state.
    pub fn counts(self, peak: u32, current: u32, enabled: bool, promoted: bool) -> Counts {
        Counts {
            core: self.core.len() as u32,
            credits: self.credits,
            peak,
            current,
            enabled,
            promoted,
        }
    }
}

/// Everything a phase decision reads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Counts {
    pub core: u32,
    pub credits: Vec<u16>,
    pub peak: u32,
    pub current: u32,
    /// Whether the campaign's gate holds **now**. A subject without it has no profile at all,
    /// whatever it earned before it sold.
    pub enabled: bool,
    /// Whether a live status puts it in the guaranteed phase regardless of its score.
    pub promoted: bool,
}

/// The phase a subject is allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Gtd,
    Fcfs,
    Ungated,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gtd => "gtd",
            Self::Fcfs => "fcfs",
            Self::Ungated => "ungated",
        }
    }
}

/// Which phase this subject is in.
///
/// The order is the campaign's, and each step is deliberate: no gate means no profile at all,
/// a promotion outranks any score, and the flavour path is disabled rather than widened when
/// `fcfs_flavour` is zero.
pub fn phase(subject: &Counts, policy: Allocation) -> Phase {
    if !subject.enabled {
        return Phase::Ungated;
    }
    if subject.promoted || subject.core >= policy.gtd_core {
        Phase::Gtd
    } else if subject.core >= policy.fcfs_core
        || (policy.fcfs_flavour > 0 && subject.credits.len() as u32 >= policy.fcfs_flavour)
    {
        Phase::Fcfs
    } else {
        Phase::Ungated
    }
}

/// Why a subject's row could not be scored.
///
/// Every variant is a bundle that disagrees with itself or with the catalogue it was built
/// against. None of them is a zero: a subject with an unreadable row is not a subject with no
/// achievements, and reporting it as one would understate a real holder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreError {
    /// The subject earned an achievement the bundle has no rarity row for.
    NoRarity { key: String, achievement: u16 },
    /// The bundle marks the achievement unmeasured, so its share is unknown.
    Unmeasured { key: String, achievement: u16 },
    /// The catalogue names a role that is neither core nor flavour.
    UnknownRole { achievement: u16, role: String },
    /// The bundle's achievement index is past the end of the catalogue given.
    NotInCatalogue { achievement: u16 },
}

impl std::fmt::Display for ScoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoRarity { key, achievement } => write!(
                formatter,
                "bundle has no rarity row for achievement #{achievement}, which {key} earned"
            ),
            Self::Unmeasured { key, achievement } => write!(
                formatter,
                "{key} earned #{achievement}, which the bundle marks unmeasured"
            ),
            Self::UnknownRole { achievement, role } => write!(
                formatter,
                "achievement #{achievement} has role {role:?}, which is neither core nor flavour"
            ),
            Self::NotInCatalogue { achievement } => write!(
                formatter,
                "bundle achievement #{achievement} is not in this catalogue"
            ),
        }
    }
}

impl std::error::Error for ScoreError {}

/// Score one subject's bundle row against a policy.
pub fn score<'a>(
    bundle: &Bundle<'_>,
    subject: &SubjectView<'_>,
    role_of: impl Fn(u16) -> Option<&'a str>,
    policy: Allocation,
) -> Result<Score, ScoreError> {
    score_earned(
        bundle,
        &subject.key_str(),
        subject.earned().map(|earned| earned.achievement),
        role_of,
        policy,
    )
}

/// Score an earned set against a policy.
///
/// `earned` is an iterator rather than a slice because an identity's score is the union of its
/// wallets' sets: the caller concatenates the rows and this deduplicates, so one achievement
/// earned twice — by two wallets, or by one asset that came back — counts once. `key` names the
/// subject in an error message, and for a union it is the identity.
///
/// The result carries the indexes and not only how many: a surface has to be able to name what
/// lifted a holder, and a count cannot.
pub fn score_earned<'a>(
    bundle: &Bundle<'_>,
    key: &str,
    earned: impl IntoIterator<Item = u16>,
    role_of: impl Fn(u16) -> Option<&'a str>,
    policy: Allocation,
) -> Result<Score, ScoreError> {
    let mut core: Vec<u16> = Vec::new();
    let mut flavour: Vec<(f64, u16)> = Vec::new();

    for index in earned.into_iter().collect::<BTreeSet<u16>>() {
        match role_of(index) {
            Some(text) if Role::parse(text) == Some(Role::Core) => core.push(index),
            Some(text) if Role::parse(text) == Some(Role::Flavour) => {
                let rarity: Rarity = bundle.rarity(index).ok_or_else(|| ScoreError::NoRarity {
                    key: key.to_string(),
                    achievement: index,
                })?;
                if rarity.population == 0 {
                    return Err(ScoreError::Unmeasured {
                        key: key.to_string(),
                        achievement: index,
                    });
                }
                let share = f64::from(rarity.holders_with) / f64::from(rarity.population);
                if share <= policy.rarity_floor {
                    flavour.push((share, index));
                }
            }
            Some(text) => {
                return Err(ScoreError::UnknownRole {
                    achievement: index,
                    role: text.to_string(),
                });
            }
            None => return Err(ScoreError::NotInCatalogue { achievement: index }),
        }
    }

    flavour.sort_by(|left, right| left.0.total_cmp(&right.0));
    flavour.truncate(policy.count_rarest as usize);

    Ok(Score {
        core,
        credits: flavour.into_iter().map(|(_, index)| index).collect(),
    })
}

/// Units held at one instant, from the lots' half-open intervals.
///
/// A lot is open at `slot` when it was acquired at or before it and has not been disposed of
/// by then — the same boundary [`peak_concurrent`] uses, so a sale and a purchase at one slot
/// are never both true.
pub fn held_at(lots: &[Lot], slot: u64) -> u32 {
    lots.iter()
        .filter(|lot| lot.acquired_at <= slot && lot.disposed_at.is_none_or(|end| end > slot))
        .count() as u32
}

/// The most distinct units held at one instant, from the lots' half-open intervals.
///
/// Ends sort before starts at a shared slot, so a sale and a purchase are never simultaneous;
/// a unit held through two overlapping lots is one unit.
pub fn peak_concurrent(lots: &[Lot], cursor: u64) -> u32 {
    #[derive(Clone, Copy)]
    struct Mark {
        slot: u64,
        start: bool,
        unit: u32,
    }

    let mut marks: Vec<Mark> = Vec::new();
    for lot in lots {
        let end = lot.disposed_at.unwrap_or(cursor);
        if end <= lot.acquired_at {
            continue;
        }
        marks.push(Mark {
            slot: lot.acquired_at,
            start: true,
            unit: lot.unit,
        });
        marks.push(Mark {
            slot: end,
            start: false,
            unit: lot.unit,
        });
    }
    marks.sort_unstable_by(|left, right| {
        left.slot
            .cmp(&right.slot)
            .then_with(|| left.start.cmp(&right.start))
    });

    let mut open: BTreeMap<u32, u32> = BTreeMap::new();
    let mut peak = 0u32;
    for mark in marks {
        if mark.start {
            *open.entry(mark.unit).or_default() += 1;
        } else if let Some(count) = open.get_mut(&mark.unit) {
            *count -= 1;
            if *count == 0 {
                open.remove(&mark.unit);
            }
        }
        peak = peak.max(open.len() as u32);
    }
    peak
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{Earned, Snapshot, SubjectRecord, write};
    use crate::memory::fixtures;
    use chain_events::Coverage;

    const CURSOR: u64 = 201_753_328;

    fn policy() -> Allocation {
        Allocation {
            gtd_core: 3,
            fcfs_core: 1,
            fcfs_flavour: 2,
            per_identity: 7,
            rarity_floor: 0.02,
            count_rarest: 3,
        }
    }

    fn counts(core: u32, credits: usize, enabled: bool, promoted: bool) -> Counts {
        Counts {
            core,
            credits: vec![0; credits],
            peak: 0,
            current: 0,
            enabled,
            promoted,
        }
    }

    #[test]
    fn a_subject_without_the_gate_has_no_phase_whatever_it_earned() {
        let drowned = counts(9, 5, false, false);
        assert_eq!(phase(&drowned, policy()), Phase::Ungated);
    }

    #[test]
    fn a_promotion_outranks_the_score() {
        let promoted = counts(0, 0, true, true);
        assert_eq!(phase(&promoted, policy()), Phase::Gtd);
    }

    #[test]
    fn the_guaranteed_threshold_is_inclusive() {
        assert_eq!(phase(&counts(3, 0, true, false), policy()), Phase::Gtd);
        assert_eq!(phase(&counts(2, 0, true, false), policy()), Phase::Fcfs);
    }

    #[test]
    fn enough_rare_flavour_reaches_the_first_come_phase_without_the_core_count() {
        let rarest = counts(0, 2, true, false);
        assert_eq!(phase(&rarest, policy()), Phase::Fcfs);
        let one_short = counts(0, 1, true, false);
        assert_eq!(phase(&one_short, policy()), Phase::Ungated);
    }

    #[test]
    fn a_zero_flavour_threshold_disables_that_path_rather_than_widening_it() {
        let only_core = Allocation {
            fcfs_flavour: 0,
            ..policy()
        };
        let rarest = counts(0, 5, true, false);
        assert_eq!(phase(&rarest, only_core), Phase::Ungated);
        assert_eq!(phase(&counts(1, 0, true, false), only_core), Phase::Fcfs);
    }

    /// A bundle with one subject earning `earned`, and a rarity row per achievement supplied
    /// by `rarity`.
    fn bundle_with(earned: &[u16], rarity: Vec<Rarity>) -> Vec<u8> {
        write(&Snapshot {
            built_at: 1_793_427_000,
            collection: fixtures::perps().to_string(),
            coverage: Coverage::Complete,
            holder_cursor: CURSOR,
            market_cursor: CURSOR,
            catalogue_digest: 0xdead_beef,
            achievement_count: rarity.len() as u16,
            rarity,
            subjects: vec![SubjectRecord {
                key: "aa".repeat(28),
                earned: earned
                    .iter()
                    .map(|achievement| Earned {
                        achievement: *achievement,
                        valid_from: CURSOR - 1_000,
                    })
                    .collect(),
                lots: vec![Lot {
                    unit: 7,
                    quantity: 1,
                    open: 1,
                    acquired_at: CURSOR - 2_000,
                    acquired_unix: CURSOR - 2_000,
                    disposed_at: None,
                    disposed_unix: None,
                    from_mint: true,
                }],
            }],
        })
    }

    fn crowded() -> Rarity {
        Rarity {
            holders_with: 500,
            population: 1_000,
        }
    }

    fn scarce() -> Rarity {
        Rarity {
            holders_with: 5,
            population: 1_000,
        }
    }

    #[test]
    fn a_core_achievement_counts_however_common_it_is() {
        let bytes = bundle_with(&[0], vec![crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let score = score(
            &bundle,
            &subject,
            |index| (index == 0).then_some("core"),
            policy(),
        )
        .unwrap();
        assert_eq!(score.core, vec![0], "the index, so a surface can name it");
        assert!(score.credits.is_empty());
    }

    #[test]
    fn a_flavour_achievement_above_the_floor_is_dropped() {
        let bytes = bundle_with(&[0], vec![crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let score = score(&bundle, &subject, |_| Some("flavour"), policy()).unwrap();
        assert!(score.core.is_empty());
        assert!(
            score.credits.is_empty(),
            "half the population holding it is not a credit at a 2% floor"
        );
    }

    #[test]
    fn the_rarest_flavour_are_kept_and_capped() {
        // Three flavour achievements: two scarce, one crowded. `count_rarest` is 2.
        let bytes = bundle_with(&[0, 1, 2], vec![scarce(), crowded(), scarce()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let capped = Allocation {
            count_rarest: 2,
            ..policy()
        };
        let score = score(&bundle, &subject, |_| Some("flavour"), capped).unwrap();
        assert_eq!(
            score.credits,
            vec![0, 2],
            "the two scarce ones, in index order"
        );
    }

    #[test]
    fn an_unmeasured_achievement_is_an_error_and_not_an_absence() {
        let bytes = bundle_with(
            &[0],
            vec![Rarity {
                holders_with: 0,
                population: 0,
            }],
        );
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let error = score(&bundle, &subject, |_| Some("flavour"), policy()).unwrap_err();
        assert!(matches!(error, ScoreError::Unmeasured { .. }));
    }

    #[test]
    fn an_achievement_the_catalogue_does_not_name_is_an_error() {
        let bytes = bundle_with(&[0], vec![scarce()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let error = score(&bundle, &subject, |_| None, policy()).unwrap_err();
        assert!(matches!(error, ScoreError::NotInCatalogue { .. }));
    }

    #[test]
    fn a_role_that_is_neither_is_an_error_rather_than_a_silent_flavour() {
        let bytes = bundle_with(&[0], vec![scarce()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let error = score(&bundle, &subject, |_| Some("bonus"), policy()).unwrap_err();
        assert_eq!(
            error,
            ScoreError::UnknownRole {
                achievement: 0,
                role: "bonus".into()
            }
        );
    }

    /// Two lots that overlapped: one still open, one sold long enough ago to be gone.
    fn lots() -> Vec<Lot> {
        vec![
            Lot {
                unit: 7,
                quantity: 1,
                open: 1,
                acquired_at: CURSOR - 9_000,
                acquired_unix: CURSOR - 9_000,
                disposed_at: None,
                disposed_unix: None,
                from_mint: true,
            },
            Lot {
                unit: 9,
                quantity: 1,
                open: 0,
                acquired_at: CURSOR - 8_500,
                acquired_unix: CURSOR - 8_500,
                disposed_at: Some(CURSOR - 6_000),
                disposed_unix: Some(CURSOR - 6_000),
                from_mint: false,
            },
        ]
    }

    #[test]
    fn a_lot_still_open_is_held_now_and_a_closed_one_is_only_its_peak() {
        let lots = lots();
        assert_eq!(held_at(&lots, CURSOR), 1, "only the open lot is still held");
        assert_eq!(
            peak_concurrent(&lots, CURSOR),
            2,
            "both were held at some point before the cursor"
        );
        assert_eq!(
            held_at(&lots, CURSOR - 7_000),
            2,
            "between the second acquisition and the sale, both were held"
        );
    }

    #[test]
    fn an_achievement_earned_by_two_wallets_counts_once() {
        // The union of two rows: both earned #0, one also earned #1.
        let bytes = bundle_with(&[0, 1], vec![crowded(), crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let union = score_earned(
            &bundle,
            "0xidentity",
            [0u16, 1, 0],
            |_| Some("core"),
            policy(),
        )
        .unwrap();
        assert_eq!(
            union.core,
            vec![0, 1],
            "three entries, two distinct achievements"
        );
    }
}
