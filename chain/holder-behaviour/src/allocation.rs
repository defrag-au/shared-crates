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

use crate::bundle::{Bundle, Lot, SubjectView};

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
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Allocation {
    /// The score that reaches the guaranteed phase. Zero: every gated subject is guaranteed.
    pub gtd_score: u32,
    /// The score that reaches the first-come phase.
    pub fcfs_score: u32,
    /// Rare achievements that also reach the first-come phase, whatever the score. Zero disables
    /// this path rather than widening it.
    pub fcfs_rarest: u32,
    /// Mints per identity when there is no holding ladder: the flat allocation.
    pub per_identity: u32,
    /// A flavour achievement held by more than this share does not count.
    pub rarity_floor: f64,
    /// How many of a holder's rarest flavour achievements count.
    pub count_rarest: u32,
    /// Mints by units held now: the highest rung reached is the base. Empty: `per_identity`.
    ///
    /// This is what lets a whale's allocation scale without a single score deciding it: the
    /// holding is a status read at the claim, and the ladder is published, so a holder can see
    /// exactly which rung their count reaches.
    #[serde(default, skip_serializing_if = "Rungs::is_empty")]
    pub holding: Rungs,
    /// Mints the score adds on a guaranteed allocation: every rung reached adds its own.
    #[serde(default, skip_serializing_if = "Rungs::is_empty")]
    pub bonus: Rungs,
    /// Mints a promoting status (a Leader) adds on a guaranteed allocation.
    #[serde(default)]
    pub promote_bonus: u32,
    /// Mints a partner holding adds on a guaranteed allocation: the crossover.
    #[serde(default)]
    pub partner_bonus: u32,
    /// First-come mints for an identity that holds none now but once earned the campaign's
    /// lapsed marker. Zero disables the path.
    #[serde(default)]
    pub lapsed_mints: u32,
    /// First-come mints for an identity whose only standing is a partner holding. Zero disables.
    #[serde(default)]
    pub partner_mints: u32,
    /// The most any identity takes, whatever adds up. Zero: no ceiling beyond the rungs.
    #[serde(default)]
    pub max_per_identity: u32,
}

/// The most rungs a ladder may have. A ladder is a published table a holder reads, so it is
/// short by nature; a fixed capacity keeps [`Allocation`] `Copy` for the modelling sweeps.
pub const RUNGS_MAX: usize = 8;

/// One rung: reach `at_least` (units, or score) and it is worth `mints`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Rung {
    pub at_least: u32,
    pub mints: u32,
}

/// A ladder of rungs, ascending by `at_least`. Reads and writes as a plain list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "Vec<Rung>", into = "Vec<Rung>")]
pub struct Rungs {
    len: u8,
    items: [Rung; RUNGS_MAX],
}

impl Rungs {
    /// A ladder from rungs strictly ascending by `at_least`, at most [`RUNGS_MAX`] of them.
    pub fn new(rungs: &[Rung]) -> Result<Self, String> {
        if rungs.len() > RUNGS_MAX {
            return Err(format!(
                "a ladder has at most {RUNGS_MAX} rungs, not {}",
                rungs.len()
            ));
        }
        if let Some(pair) = rungs
            .windows(2)
            .find(|pair| pair[1].at_least <= pair[0].at_least)
        {
            return Err(format!(
                "rungs must ascend: {} does not follow {}",
                pair[1].at_least, pair[0].at_least
            ));
        }
        let mut items = [Rung::default(); RUNGS_MAX];
        items[..rungs.len()].copy_from_slice(rungs);
        Ok(Self {
            len: rungs.len() as u8,
            items,
        })
    }

    pub fn as_slice(&self) -> &[Rung] {
        &self.items[..usize::from(self.len)]
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The highest rung `value` reaches.
    pub fn highest(&self, value: u32) -> Option<Rung> {
        self.as_slice()
            .iter()
            .rev()
            .find(|rung| value >= rung.at_least)
            .copied()
    }

    /// Every rung `value` reaches, summed: a bonus ladder.
    pub fn reached(&self, value: u32) -> u32 {
        self.as_slice()
            .iter()
            .filter(|rung| value >= rung.at_least)
            .map(|rung| rung.mints)
            .sum()
    }
}

impl TryFrom<Vec<Rung>> for Rungs {
    type Error = String;

    fn try_from(rungs: Vec<Rung>) -> Result<Self, Self::Error> {
        Self::new(&rungs)
    }
}

impl From<Rungs> for Vec<Rung> {
    fn from(rungs: Rungs) -> Self {
        rungs.as_slice().to_vec()
    }
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
    /// What the counted set is worth. Summed from the catalogue's own weights, never counted
    /// from how many achievements there are: an achievement is worth what curation said it is,
    /// so a hard one can lift a holder the way three easy ones cannot.
    pub points: u32,
}

impl Score {
    /// The phase inputs, once the caller has read what only a status can answer, and what
    /// holdings it wants to state.
    pub fn counts(self, peak: u32, current: u32, enabled: bool, promoted: bool) -> Counts {
        Counts {
            score: self.points,
            rarest: self.credits.len() as u32,
            peak,
            current,
            enabled,
            promoted,
            lapsed: false,
            partner: false,
        }
    }
}

impl Counts {
    /// Whether the subject once earned the campaign's lapsed marker: a first-come path for one
    /// that holds none now. Only read when the gate is shut.
    pub fn lapsed(mut self, lapsed: bool) -> Self {
        self.lapsed = lapsed;
        self
    }

    /// Whether a partner holding was read for the subject.
    pub fn partner(mut self, partner: bool) -> Self {
        self.partner = partner;
        self
    }
}

/// Everything a phase decision reads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Counts {
    /// The points of every counted achievement.
    pub score: u32,
    /// How many rare achievements counted, which is the alternate first-come path.
    pub rarest: u32,
    pub peak: u32,
    pub current: u32,
    /// Whether the campaign's gate holds **now**. A subject without it has no profile at all,
    /// whatever it earned before it sold.
    pub enabled: bool,
    /// Whether a live status puts it in the guaranteed phase regardless of its score.
    pub promoted: bool,
    /// Whether it once earned the lapsed marker. See [`Counts::lapsed`].
    pub lapsed: bool,
    /// Whether a partner holding was read. See [`Counts::partner`].
    pub partner: bool,
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
/// a promotion outranks any score, and the rare path is disabled rather than widened when
/// `fcfs_rarest` is zero.
pub fn phase(subject: &Counts, policy: Allocation) -> Phase {
    if !subject.enabled {
        // No gate, no guarantee. Two first-come doors stay open when the campaign opens them:
        // an identity that once earned the lapsed marker, and one whose standing is a partner.
        let lapsed = policy.lapsed_mints > 0 && subject.lapsed;
        let partner = policy.partner_mints > 0 && subject.partner;
        return match lapsed || partner {
            true => Phase::Fcfs,
            false => Phase::Ungated,
        };
    }
    if subject.promoted || subject.score >= policy.gtd_score {
        Phase::Gtd
    } else if subject.score >= policy.fcfs_score
        || (policy.fcfs_rarest > 0 && subject.rarest >= policy.fcfs_rarest)
    {
        Phase::Fcfs
    } else {
        Phase::Ungated
    }
}

/// How many mints this subject's phase allocates it.
///
/// One rule for the worker answering a holder and the tool modelling a mint:
///
/// - **Gated and guaranteed:** the holding ladder's rung (or `per_identity` without a ladder),
///   plus every score rung reached, plus the leader and partner bonuses.
/// - **Gated, first come:** the holding rung alone.
/// - **Not gated, first come:** the lapsed or partner mints, whichever is larger.
/// - **Ungated:** none — the public phase is not this rule's to allocate.
///
/// `max_per_identity` caps the total when it is set.
pub fn mints(subject: &Counts, policy: Allocation) -> u32 {
    let phase = phase(subject, policy);
    let total = match (phase, subject.enabled) {
        (Phase::Ungated, _) => 0,
        (Phase::Fcfs, false) => {
            let lapsed = if subject.lapsed {
                policy.lapsed_mints
            } else {
                0
            };
            let partner = if subject.partner {
                policy.partner_mints
            } else {
                0
            };
            lapsed.max(partner)
        }
        (_, true) => {
            let base = match policy.holding.is_empty() {
                true => policy.per_identity,
                false => policy
                    .holding
                    .highest(subject.current)
                    .map_or(0, |rung| rung.mints),
            };
            match phase {
                Phase::Gtd => {
                    base + policy.bonus.reached(subject.score)
                        + if subject.promoted {
                            policy.promote_bonus
                        } else {
                            0
                        }
                        + if subject.partner {
                            policy.partner_bonus
                        } else {
                            0
                        }
                }
                _ => base,
            }
        }
        (Phase::Gtd, false) => 0,
    };
    match policy.max_per_identity {
        0 => total,
        cap => total.min(cap),
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
    /// The catalogue gives the achievement no points, so the score it contributes is unknown.
    Unweighted { achievement: u16 },
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
            Self::Unweighted { achievement } => write!(
                formatter,
                "achievement #{achievement} carries no points in this catalogue, so it cannot score"
            ),
        }
    }
}

impl std::error::Error for ScoreError {}

/// What happened to one earned achievement, in the order a reader would ask.
///
/// This is the scoring rule's own vocabulary: a debug surface renders these rather than deciding
/// again what counted, so an answer and its explanation cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A core achievement. Every one counts.
    Core,
    /// A rare enough flavour achievement that counted.
    Credit,
    /// Flavour the rarity floor drops: held by too much of the population to tell holders apart.
    Crowded,
    /// Flavour rare enough to count, but past `count_rarest`.
    Capped,
    /// The bundle has no rarity row for it, so its share is unknown.
    NoRarity,
    /// The bundle marks it unmeasured: the fold never answered it.
    Unmeasured,
    /// The catalogue names a role that is neither core nor flavour.
    UnknownRole,
    /// The bundle's index is past the end of the catalogue.
    NotInCatalogue,
    /// It would count, but the catalogue gives it no points.
    Unweighted,
}

impl Outcome {
    /// Whether this achievement is part of the score.
    pub fn counted(self) -> bool {
        matches!(self, Self::Core | Self::Credit)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Credit => "rarest",
            Self::Crowded => "crowded",
            Self::Capped => "capped",
            Self::NoRarity => "no-rarity",
            Self::Unmeasured => "unmeasured",
            Self::UnknownRole => "unknown-role",
            Self::NotInCatalogue => "not-in-catalogue",
            Self::Unweighted => "unweighted",
        }
    }
}

/// One earned achievement, what the catalogue says it is worth, and what scoring made of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceRow {
    pub achievement: u16,
    /// The catalogue's weight. `None` when the catalogue did not give one, or does not carry the
    /// achievement at all. Present on a capped row too: it is what would have counted.
    pub points: Option<u32>,
    pub outcome: Outcome,
}

/// Every earned achievement with its outcome, and the score when one could be formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trace {
    /// One row per earned achievement, in index order.
    pub rows: Vec<TraceRow>,
    /// The score, or the first refusal. A refused trace still carries every row, which is the
    /// point: the rows say which achievement the catalogue or the bundle failed to answer for.
    pub score: Result<Score, ScoreError>,
}

/// Score one subject's bundle row against a policy.
pub fn score<'a>(
    bundle: &Bundle<'_>,
    subject: &SubjectView<'_>,
    role_of: impl Fn(u16) -> Option<&'a str>,
    points_of: impl Fn(u16) -> Option<u32>,
    policy: Allocation,
) -> Result<Score, ScoreError> {
    score_earned(
        bundle,
        &subject.key_str(),
        subject.earned().map(|earned| earned.achievement),
        role_of,
        points_of,
        policy,
    )
}

/// Score an earned set against a policy.
///
/// `earned` is an iterator rather than a slice because an identity's score is the union of its
/// wallets' sets: the caller concatenates the rows and this deduplicates, so one achievement
/// earned twice — by two wallets, or by one asset that came back — counts once. `key` names the
/// subject in an error message, and for a union it is the identity.
pub fn score_earned<'a>(
    bundle: &Bundle<'_>,
    key: &str,
    earned: impl IntoIterator<Item = u16>,
    role_of: impl Fn(u16) -> Option<&'a str>,
    points_of: impl Fn(u16) -> Option<u32>,
    policy: Allocation,
) -> Result<Score, ScoreError> {
    explain(bundle, key, earned, role_of, points_of, policy).score
}

/// Every earned achievement with what scoring made of it, and the score they add up to.
///
/// The rule lives here and [`score_earned`] is this function's `score`: a surface that explains
/// an answer renders these rows instead of re-deciding which achievements counted, which is the
/// bug a debug view exists to catch rather than to reintroduce.
pub fn explain<'a>(
    bundle: &Bundle<'_>,
    key: &str,
    earned: impl IntoIterator<Item = u16>,
    role_of: impl Fn(u16) -> Option<&'a str>,
    points_of: impl Fn(u16) -> Option<u32>,
    policy: Allocation,
) -> Trace {
    let indices: BTreeSet<u16> = earned.into_iter().collect();
    let mut rows: Vec<TraceRow> = Vec::with_capacity(indices.len());
    let mut refusal: Option<ScoreError> = None;
    let mut eligible: Vec<(f64, u16)> = Vec::new();

    for index in indices {
        let outcome = match role_of(index) {
            Some(text) if Role::parse(text) == Some(Role::Core) => Outcome::Core,
            Some(text) if Role::parse(text) == Some(Role::Flavour) => match bundle.rarity(index) {
                None => {
                    refusal.get_or_insert(ScoreError::NoRarity {
                        key: key.to_string(),
                        achievement: index,
                    });
                    Outcome::NoRarity
                }
                Some(rarity) if rarity.population == 0 => {
                    refusal.get_or_insert(ScoreError::Unmeasured {
                        key: key.to_string(),
                        achievement: index,
                    });
                    Outcome::Unmeasured
                }
                Some(rarity) => {
                    let share = f64::from(rarity.holders_with) / f64::from(rarity.population);
                    if share <= policy.rarity_floor {
                        eligible.push((share, index));
                        Outcome::Credit
                    } else {
                        Outcome::Crowded
                    }
                }
            },
            Some(text) => {
                refusal.get_or_insert(ScoreError::UnknownRole {
                    achievement: index,
                    role: text.to_string(),
                });
                Outcome::UnknownRole
            }
            None => {
                refusal.get_or_insert(ScoreError::NotInCatalogue { achievement: index });
                Outcome::NotInCatalogue
            }
        };
        rows.push(TraceRow {
            achievement: index,
            points: points_of(index),
            outcome,
        });
    }

    // The rarest keep their place, and the rest are told so rather than dropped silently.
    eligible.sort_by(|left, right| left.0.total_cmp(&right.0));
    let keep: BTreeSet<u16> = eligible
        .iter()
        .take(policy.count_rarest as usize)
        .map(|(_, index)| *index)
        .collect();
    for row in &mut rows {
        if row.outcome == Outcome::Credit && !keep.contains(&row.achievement) {
            row.outcome = Outcome::Capped;
        }
    }

    // Every achievement that counts must be priced. One the catalogue left unpriced is a refusal
    // rather than a zero: a score that silently omits it understates a holder, and the surface
    // showing that score has no way to tell.
    let mut points = 0;
    for row in &mut rows {
        if !row.outcome.counted() {
            continue;
        }
        match row.points {
            Some(weight) => points += weight,
            None => {
                row.outcome = Outcome::Unweighted;
                refusal.get_or_insert(ScoreError::Unweighted {
                    achievement: row.achievement,
                });
            }
        }
    }

    let score = match refusal {
        Some(error) => Err(error),
        None => Ok(Score {
            core: rows
                .iter()
                .filter(|row| row.outcome == Outcome::Core)
                .map(|row| row.achievement)
                .collect(),
            credits: eligible
                .iter()
                .take(policy.count_rarest as usize)
                .map(|(_, index)| *index)
                .collect(),
            points,
        }),
    };
    Trace { rows, score }
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
    use crate::bundle::Rarity;
    use crate::bundle::{Earned, Snapshot, SubjectRecord, write};
    use crate::memory::fixtures;
    use chain_events::Coverage;

    const CURSOR: u64 = 201_753_328;

    fn policy() -> Allocation {
        Allocation {
            gtd_score: 30,
            fcfs_score: 10,
            fcfs_rarest: 2,
            per_identity: 7,
            rarity_floor: 0.02,
            count_rarest: 3,
            ..Allocation::default()
        }
    }

    /// The weight every achievement carries in the tests that are not about weights.
    fn ten(_: u16) -> Option<u32> {
        Some(10)
    }

    fn counts(score: u32, rarest: u32, enabled: bool, promoted: bool) -> Counts {
        Counts {
            score,
            rarest,
            peak: 0,
            current: 0,
            enabled,
            promoted,
            lapsed: false,
            partner: false,
        }
    }

    /// The ladder policy: every holder guaranteed, mints by holding, score and leaders on top.
    fn ladder() -> Allocation {
        let rung = |at_least, mints| Rung { at_least, mints };
        Allocation {
            gtd_score: 0,
            holding: Rungs::new(&[rung(1, 1), rung(5, 3), rung(25, 6), rung(100, 12)]).unwrap(),
            bonus: Rungs::new(&[rung(5_000, 1), rung(10_000, 1), rung(15_000, 2)]).unwrap(),
            promote_bonus: 1,
            partner_bonus: 1,
            lapsed_mints: 1,
            partner_mints: 1,
            max_per_identity: 16,
            ..policy()
        }
    }

    fn holding(score: u32, current: u32) -> Counts {
        Counts {
            current,
            ..counts(score, 0, true, false)
        }
    }

    #[test]
    fn a_holder_gets_the_rung_their_holding_reaches() {
        assert_eq!(mints(&holding(0, 1), ladder()), 1);
        assert_eq!(mints(&holding(0, 4), ladder()), 1);
        assert_eq!(mints(&holding(0, 5), ladder()), 3);
        assert_eq!(mints(&holding(0, 99), ladder()), 6);
        assert_eq!(mints(&holding(0, 100), ladder()), 12);
        assert_eq!(phase(&holding(0, 1), ladder()), Phase::Gtd);
    }

    #[test]
    fn score_rungs_add_up_and_the_ceiling_holds() {
        assert_eq!(mints(&holding(5_000, 1), ladder()), 2);
        assert_eq!(mints(&holding(15_000, 1), ladder()), 1 + 1 + 1 + 2);
        let whale = Counts {
            promoted: true,
            partner: true,
            ..holding(20_000, 500)
        };
        assert_eq!(
            mints(&whale, ladder()),
            16,
            "12 + 4 + 1 + 1 = 18, held to the ceiling"
        );
    }

    #[test]
    fn a_lapsed_or_partner_identity_is_first_come_without_the_gate() {
        let lapsed = counts(9_000, 0, false, false).lapsed(true);
        assert_eq!(phase(&lapsed, ladder()), Phase::Fcfs);
        assert_eq!(mints(&lapsed, ladder()), 1);
        let partner = counts(0, 0, false, false).partner(true);
        assert_eq!(mints(&partner, ladder()), 1);
        let neither = counts(9_000, 0, false, false);
        assert_eq!(mints(&neither, ladder()), 0);
        assert_eq!(
            phase(&lapsed, policy()),
            Phase::Ungated,
            "a policy without the door keeps it shut"
        );
    }

    #[test]
    fn without_a_ladder_the_flat_allocation_stands() {
        assert_eq!(mints(&counts(30, 0, true, false), policy()), 7);
        assert_eq!(mints(&counts(29, 0, true, false), policy()), 7);
        assert_eq!(mints(&counts(0, 0, true, false), policy()), 0);
    }

    #[test]
    fn rungs_must_ascend_and_read_back_as_a_list() {
        let rung = |at_least, mints| Rung { at_least, mints };
        assert!(Rungs::new(&[rung(5, 1), rung(5, 2)]).is_err());
        let ladder = Rungs::new(&[rung(1, 1), rung(5, 3)]).unwrap();
        let json = serde_json::to_string(&ladder).unwrap();
        assert_eq!(
            json,
            r#"[{"at_least":1,"mints":1},{"at_least":5,"mints":3}]"#
        );
        let back: Rungs = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ladder);
    }

    #[test]
    fn a_manifest_written_before_the_ladder_still_reads() {
        let old = r#"{"gtd_score":5000,"fcfs_score":2000,"fcfs_rarest":3,"per_identity":3,
                      "rarity_floor":0.02,"count_rarest":3}"#;
        let policy: Allocation = serde_json::from_str(old).unwrap();
        assert!(policy.holding.is_empty());
        assert_eq!(policy.lapsed_mints, 0);
    }

    #[test]
    fn a_subject_without_the_gate_has_no_phase_whatever_it_earned() {
        let drowned = counts(90, 5, false, false);
        assert_eq!(phase(&drowned, policy()), Phase::Ungated);
    }

    #[test]
    fn a_promotion_outranks_the_score() {
        let promoted = counts(0, 0, true, true);
        assert_eq!(phase(&promoted, policy()), Phase::Gtd);
    }

    #[test]
    fn the_guaranteed_threshold_is_inclusive() {
        assert_eq!(phase(&counts(30, 0, true, false), policy()), Phase::Gtd);
        assert_eq!(phase(&counts(29, 0, true, false), policy()), Phase::Fcfs);
    }

    #[test]
    fn enough_rare_flavour_reaches_the_first_come_phase_without_the_score() {
        let rarest = counts(0, 2, true, false);
        assert_eq!(phase(&rarest, policy()), Phase::Fcfs);
        let one_short = counts(0, 1, true, false);
        assert_eq!(phase(&one_short, policy()), Phase::Ungated);
    }

    #[test]
    fn a_zero_rare_threshold_disables_that_path_rather_than_widening_it() {
        let only_score = Allocation {
            fcfs_rarest: 0,
            ..policy()
        };
        let rarest = counts(0, 5, true, false);
        assert_eq!(phase(&rarest, only_score), Phase::Ungated);
        assert_eq!(phase(&counts(10, 0, true, false), only_score), Phase::Fcfs);
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
            ten,
            policy(),
        )
        .unwrap();
        assert_eq!(score.core, vec![0], "the index, so a surface can name it");
        assert!(score.credits.is_empty());
        assert_eq!(score.points, 10);
    }

    #[test]
    fn an_achievement_is_worth_its_own_points_rather_than_one_each() {
        // Two core achievements, one common and one scarce: the count is two either way, and the
        // score is the sum of what curation said each is worth.
        let bytes = bundle_with(&[0, 1], vec![crowded(), scarce()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let weighed = |index: u16| Some(if index == 1 { 40 } else { 5 });
        let score = score(&bundle, &subject, |_| Some("core"), weighed, policy()).unwrap();
        assert_eq!(score.core.len(), 2);
        assert_eq!(score.points, 45, "5 + 40, not two of anything");
    }

    #[test]
    fn an_achievement_the_catalogue_leaves_unpriced_is_an_error_rather_than_a_zero() {
        let bytes = bundle_with(&[0], vec![crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let error = score(&bundle, &subject, |_| Some("core"), |_| None, policy()).unwrap_err();
        assert_eq!(error, ScoreError::Unweighted { achievement: 0 });
    }

    #[test]
    fn a_flavour_achievement_above_the_floor_is_dropped() {
        let bytes = bundle_with(&[0], vec![crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let score = score(&bundle, &subject, |_| Some("flavour"), ten, policy()).unwrap();
        assert!(score.core.is_empty());
        assert_eq!(
            score.points, 0,
            "an achievement that does not count is worth nothing"
        );
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
        let score = score(&bundle, &subject, |_| Some("flavour"), ten, capped).unwrap();
        assert_eq!(
            score.credits,
            vec![0, 2],
            "the two scarce ones, in index order"
        );
        assert_eq!(score.points, 20, "only the two that count");
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
        let error = score(&bundle, &subject, |_| Some("flavour"), ten, policy()).unwrap_err();
        assert!(matches!(error, ScoreError::Unmeasured { .. }));
    }

    #[test]
    fn an_achievement_the_catalogue_does_not_name_is_an_error() {
        let bytes = bundle_with(&[0], vec![scarce()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let error = score(&bundle, &subject, |_| None, ten, policy()).unwrap_err();
        assert!(matches!(error, ScoreError::NotInCatalogue { .. }));
    }

    #[test]
    fn a_role_that_is_neither_is_an_error_rather_than_a_silent_flavour() {
        let bytes = bundle_with(&[0], vec![scarce()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let subject = bundle.find(&"aa".repeat(28)).unwrap().unwrap();
        let error = score(&bundle, &subject, |_| Some("bonus"), ten, policy()).unwrap_err();
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
    fn the_trace_says_what_happened_to_every_earned_achievement() {
        // #0 is core; #1 is flavour half the population holds; #2-#4 are scarce, and
        // `count_rarest` keeps two of them.
        let bytes = bundle_with(
            &[0, 1, 2, 3, 4],
            vec![crowded(), crowded(), scarce(), scarce(), scarce()],
        );
        let bundle = Bundle::open(&bytes).unwrap();
        let capped = Allocation {
            count_rarest: 2,
            ..policy()
        };
        let trace = explain(
            &bundle,
            "0xidentity",
            [0u16, 1, 2, 3, 4],
            |index| match index {
                0 => Some("core"),
                _ => Some("flavour"),
            },
            ten,
            capped,
        );
        let outcomes: Vec<Outcome> = trace.rows.iter().map(|row| row.outcome).collect();
        assert_eq!(
            outcomes,
            vec![
                Outcome::Core,
                Outcome::Crowded,
                Outcome::Credit,
                Outcome::Credit,
                Outcome::Capped,
            ],
            "index order, one row each, and the capped one named rather than dropped"
        );
        assert_eq!(
            trace.rows[4].points,
            Some(10),
            "a capped row still says what it would have been worth"
        );
        assert_eq!(
            trace.score.as_ref().map(|score| score.points),
            Ok(30),
            "one core and two credits"
        );
    }

    #[test]
    fn a_refused_trace_still_carries_every_row() {
        let bytes = bundle_with(&[0], vec![crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let trace = explain(
            &bundle,
            "0xidentity",
            [0u16],
            |_| Some("core"),
            |_| None,
            policy(),
        );
        assert_eq!(trace.rows.len(), 1);
        assert_eq!(trace.rows[0].outcome, Outcome::Unweighted);
        assert!(trace.score.is_err(), "and the score is refused, not zero");
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
            ten,
            policy(),
        )
        .unwrap();
        assert_eq!(
            union.core,
            vec![0, 1],
            "three entries, two distinct achievements"
        );
        assert_eq!(union.points, 20, "and two weights, not three");
    }
}
