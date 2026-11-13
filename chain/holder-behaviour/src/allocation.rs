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
use crate::evaluate::Standing;

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

/// Which points table an achievement scores in, and so which phase its points earn.
///
/// Settled 2026-11-11 for Perps, and general to any campaign that separates the two: what cannot
/// be bought before a snapshot — the mint, and time held — earns the guarantee; what a holder can
/// still chase — collecting, trading, the factions — earns first come.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Track {
    /// Where a holder came from and how long they stayed. Points climb [`Allocation::bonus`],
    /// which adds guaranteed slots. The default, because a score had only this meaning before
    /// tracks existed.
    #[default]
    Bloodline,
    /// What a holder did with what they hold. Points climb [`Allocation::clout`], which adds
    /// first-come slots.
    Clout,
}

impl Track {
    pub const ALL: [Self; 2] = [Self::Bloodline, Self::Clout];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bloodline => "bloodline",
            Self::Clout => "clout",
        }
    }
}

/// One value per [`Track`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Tracked<T> {
    pub bloodline: T,
    pub clout: T,
}

impl<T> Tracked<T> {
    pub fn get(&self, track: Track) -> &T {
        match track {
            Track::Bloodline => &self.bloodline,
            Track::Clout => &self.clout,
        }
    }

    pub fn get_mut(&mut self, track: Track) -> &mut T {
        match track {
            Track::Bloodline => &mut self.bloodline,
            Track::Clout => &mut self.clout,
        }
    }

    /// The same function applied to each track's value.
    pub fn map<U>(self, mut f: impl FnMut(Track, T) -> U) -> Tracked<U> {
        Tracked {
            bloodline: f(Track::Bloodline, self.bloodline),
            clout: f(Track::Clout, self.clout),
        }
    }
}

/// How a campaign's mint runs: its phases in order, and when the snapshot is read. Published with
/// the policy so a page draws the timeline and its dates from the same file the rule comes from,
/// never from its own copy.
///
/// The "count in full by" date is not stored: it is [`Launch::snapshot_at`] less the seasoning's
/// mature days, so it cannot drift from the rule that ages a holding.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Launch {
    /// Unix seconds the holdings are read for the allocation.
    pub snapshot_at: u64,
    /// The phases, in the order they open.
    pub phases: Vec<LaunchPhase>,
    /// What the public pays once the last phase ends, in US cents. `None`: no public sale stated.
    #[serde(default)]
    pub public_price_cents: Option<u32>,
    /// The first-come mints aimed at OG holders, out of the first-come phase. A target the
    /// points tables are tuned to, not a cap.
    #[serde(default)]
    pub og_fcfs_target: Option<u32>,
}

/// One phase of a [`Launch`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LaunchPhase {
    /// What the page calls it: `GTD · OG Perps`.
    pub name: String,
    /// Which allowance it honours.
    pub kind: PhaseKind,
    /// Mints in the phase.
    pub mints: u32,
    /// Its price per mint in US cents. `None` or `0`: free.
    #[serde(default)]
    pub price_cents: Option<u32>,
    /// Whose phase it is, when it is not this campaign's holders (`partner communities`).
    #[serde(default)]
    pub audience: Option<String>,
}

/// Which allowance a [`LaunchPhase`] honours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhaseKind {
    Gtd,
    Fcfs,
}

impl Launch {
    /// The last moment a unit can arrive and still count in full at the snapshot.
    pub fn mature_by(&self, seasoning: Option<Seasoning>) -> u64 {
        seasoning.map_or(self.snapshot_at, |s| {
            self.snapshot_at
                .saturating_sub(u64::from(s.mature_days) * crate::SECONDS_PER_DAY)
        })
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
    /// Past the holding ladder's last rung, `mints` more for every `every` units: a whale's
    /// allocation keeps climbing at a published, steady rate rather than a table long enough to
    /// reach it. `None`: the last rung is the top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holding_tail: Option<Tail>,
    /// Mints the score adds on a guaranteed allocation: every rung reached adds its own. Read
    /// against the [`Track::Bloodline`] points when the catalogue has tracks.
    #[serde(default, skip_serializing_if = "Rungs::is_empty")]
    pub bonus: Rungs,
    /// First-come mints the [`Track::Clout`] points add, every rung reached adding its own, for
    /// any gated identity. Empty: clout earns nothing, the rule before tracks.
    #[serde(default, skip_serializing_if = "Rungs::is_empty")]
    pub clout: Rungs,
    /// Past the bonus ladder's last rung, `mints` more for every `every` points: a rate
    /// ("one per thousand") is one rung and this. `None`: the last rung is the top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bonus_tail: Option<Tail>,
    /// The same, past the clout ladder's last rung.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clout_tail: Option<Tail>,
    /// The bonus ladder pays first come, for any gated identity, as clout does: both points
    /// tables are a chase, and the guarantee is what is held. Off, the rule before this field:
    /// the bonus adds to a guaranteed allocation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub bonus_fcfs: bool,
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
    /// What share of its points a **holding** keeps once it is no longer held: a faction head
    /// sold is remembered, not scored as if it were still in the wallet.
    ///
    /// Setting it also switches on the present tense for holdings: one held now counts in full,
    /// however common it is and however many others the holder has, because what a holder keeps
    /// is the collection's signal and should never be crowded out. Records (a mint, a span, a
    /// sale) are untouched. `None`, the default: every holding scores as if still held, the
    /// rule every manifest written before this field was scored by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub past_share: Option<f64>,
    /// Units that must be held now for the guaranteed phase. Below it, a gated subject is first
    /// come however high it scores; a promotion still outranks it. Zero: no minimum.
    #[serde(default)]
    pub gtd_min_held: u32,
    /// How the holding ladder weighs a unit by how long it has been held. `None`: every unit
    /// held now counts in full, the rule before this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seasoning: Option<Seasoning>,
    /// Guaranteed slots for holding the collection's token. `None`: the token earns no slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenRule>,
}

impl Allocation {
    /// A points table's ladder and the steady climb past it.
    pub fn track_ladder(&self, track: Track) -> (&Rungs, Option<Tail>) {
        match track {
            Track::Bloodline => (&self.bonus, self.bonus_tail),
            Track::Clout => (&self.clout, self.clout_tail),
        }
    }

    /// The phase a points table pays in: clout is always first come, the bloodline guaranteed
    /// unless [`Allocation::bonus_fcfs`].
    pub fn track_phase(&self, track: Track) -> PhaseKind {
        match (track, self.bonus_fcfs) {
            (Track::Bloodline, false) => PhaseKind::Gtd,
            _ => PhaseKind::Fcfs,
        }
    }

    /// The mints `points` reach in a table: every rung reached, and the tail past the last.
    pub fn track_mints(&self, track: Track, points: u32) -> u32 {
        let (rungs, tail) = self.track_ladder(track);
        let past = match (tail, rungs.as_slice().last()) {
            (Some(tail), Some(last)) if tail.every > 0 && points > last.at_least => {
                (points - last.at_least) / tail.every * tail.mints
            }
            _ => 0,
        };
        rungs.reached(points) + past
    }

    /// The next step above `points` in a table: where it is, and what it adds. `None` at the top
    /// of a ladder with no tail, and for a table with no ladder.
    pub fn track_next(&self, track: Track, points: u32) -> Option<Rung> {
        let (rungs, tail) = self.track_ladder(track);
        if let Some(rung) = rungs.as_slice().iter().find(|r| r.at_least > points) {
            return Some(*rung);
        }
        let (tail, last) = (tail?, rungs.as_slice().last()?);
        if tail.every == 0 {
            return None;
        }
        let steps = (points - last.at_least) / tail.every + 1;
        Some(Rung {
            at_least: last.at_least + steps * tail.every,
            mints: tail.mints,
        })
    }

    /// The step `points` last reached in a table: the foot of the bar to [`Self::track_next`].
    /// Zero below the first.
    pub fn track_below(&self, track: Track, points: u32) -> u32 {
        let (rungs, tail) = self.track_ladder(track);
        let Some(last) = rungs.as_slice().last() else {
            return 0;
        };
        match tail {
            Some(tail) if tail.every > 0 && points >= last.at_least => {
                last.at_least + (points - last.at_least) / tail.every * tail.every
            }
            _ => rungs.highest(points).map_or(0, |rung| rung.at_least),
        }
    }
}

/// How far through the [`Seasoning`] a unit held now is.
///
/// This is the middle road between a frozen snapshot and a live count. Buying ahead of a mint
/// still moves a holder up the ladder, so the market is not shut, but a unit bought in the last
/// days before the snapshot is worth a fraction of one already held: a run on the floor the night
/// before buys less than it would under a plain count, and a unit moved into a new wallet starts
/// again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Maturity {
    /// Held less than [`Seasoning::maturing_days`], or arrived since the snapshot was read.
    New,
    /// Held at least [`Seasoning::maturing_days`] and less than [`Seasoning::mature_days`].
    Maturing,
    /// Held at least [`Seasoning::mature_days`]: counts in full.
    Mature,
}

/// When a unit held moves from [`Maturity::New`] to [`Maturity::Maturing`] to
/// [`Maturity::Mature`], and what each stage counts for on the holding ladder.
///
/// `maturing_days == mature_days` is a schedule with no middle stage: new until it is mature.
/// The catalogue may still write the older `{ days, share }`, which is exactly that.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(from = "SeasoningFile")]
pub struct Seasoning {
    pub maturing_days: u32,
    pub mature_days: u32,
    pub new_share: f64,
    pub maturing_share: f64,
}

/// [`Seasoning`] as a catalogue or a manifest writes it: the stage names, or the single-step
/// `{ days, share }` written before there was a middle stage.
#[derive(serde::Deserialize)]
struct SeasoningFile {
    #[serde(alias = "days")]
    mature_days: u32,
    #[serde(alias = "share")]
    new_share: f64,
    maturing_days: Option<u32>,
    maturing_share: Option<f64>,
}

impl From<SeasoningFile> for Seasoning {
    fn from(file: SeasoningFile) -> Self {
        Self {
            maturing_days: file
                .maturing_days
                .unwrap_or(file.mature_days)
                .min(file.mature_days),
            mature_days: file.mature_days,
            new_share: file.new_share,
            maturing_share: file.maturing_share.unwrap_or(file.new_share),
        }
    }
}

impl Seasoning {
    /// One step: new until `days`, then mature.
    pub fn single(days: u32, share: f64) -> Self {
        Self {
            maturing_days: days,
            mature_days: days,
            new_share: share,
            maturing_share: share,
        }
    }

    /// The stage of a unit held `held_secs`.
    pub fn maturity(&self, held_secs: u64) -> Maturity {
        let days = held_secs / crate::SECONDS_PER_DAY;
        if days >= u64::from(self.mature_days) {
            Maturity::Mature
        } else if days >= u64::from(self.maturing_days) {
            Maturity::Maturing
        } else {
            Maturity::New
        }
    }

    /// What a unit at `maturity` counts for on the ladder.
    pub fn share(&self, maturity: Maturity) -> f64 {
        match maturity {
            Maturity::New => self.new_share.clamp(0.0, 1.0),
            Maturity::Maturing => self.maturing_share.clamp(0.0, 1.0),
            Maturity::Mature => 1.0,
        }
    }

    /// A fungible balance weighed by how long it has been held: `now` units, of which
    /// `held_maturing` were held throughout the last [`Self::maturing_days`] and `held_mature`
    /// throughout the last [`Self::mature_days`]. Each stage counts at its share, one floor over
    /// the partial stages together, as the holding ladder counts Perps.
    ///
    /// Settled 2026-11-11 for $PERP: the same schedule as a Perp, so units moved into a fresh
    /// wallet, or staked from one, the week before a snapshot are new. The held-throughout figures
    /// are a wallet's lowest balance across each window (`token-ledger held-through`); they are
    /// clamped here so a reading out of order never counts more than is held.
    pub fn weigh_held(&self, now: u64, held_maturing: u64, held_mature: u64) -> u64 {
        let mature = held_mature.min(now);
        let maturing = held_maturing.clamp(mature, now) - mature;
        let new = now - mature - maturing;
        let partial = maturing as f64 * self.share(Maturity::Maturing)
            + new as f64 * self.share(Maturity::New);
        mature.saturating_add(partial.floor() as u64)
    }
}

/// Units held now, by [`Maturity`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Matured {
    pub new: u32,
    pub maturing: u32,
    pub mature: u32,
}

impl Matured {
    pub fn at(&self, maturity: Maturity) -> u32 {
        match maturity {
            Maturity::New => self.new,
            Maturity::Maturing => self.maturing,
            Maturity::Mature => self.mature,
        }
    }

    fn add(&mut self, maturity: Maturity) {
        match maturity {
            Maturity::New => self.new += 1,
            Maturity::Maturing => self.maturing += 1,
            Maturity::Mature => self.mature += 1,
        }
    }

    pub fn total(&self) -> u32 {
        self.new + self.maturing + self.mature
    }

    /// This reading held against a live count: never more than `current` in all, the oldest
    /// believed first, and whatever the live count holds beyond it is [`Maturity::New`] — a unit
    /// bought since the snapshot was read is not in its lots.
    pub fn within(&self, current: u32) -> Self {
        let mature = self.mature.min(current);
        let maturing = self.maturing.min(current - mature);
        Self {
            new: current - mature - maturing,
            maturing,
            mature,
        }
    }

    /// Two wallets' readings, as one identity's.
    pub fn plus(&self, other: &Self) -> Self {
        Self {
            new: self.new + other.new,
            maturing: self.maturing + other.maturing,
            mature: self.mature + other.mature,
        }
    }
}

/// A steady climb past a ladder's last rung: `mints` for every `every` units beyond it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tail {
    pub every: u32,
    pub mints: u32,
}

/// What the holding ladder gives `units`: the highest rung reached, and the tail past the last.
pub fn holding_mints(policy: &Allocation, units: u32) -> u32 {
    let rung = policy.holding.highest(units).map_or(0, |rung| rung.mints);
    let tail = match (policy.holding_tail, policy.holding.as_slice().last()) {
        (Some(tail), Some(last)) if tail.every > 0 && units > last.at_least => {
            (units - last.at_least) / tail.every * tail.mints
        }
        _ => 0,
    };
    rung + tail
}

/// The most rungs a ladder may have. A ladder is a published table a holder reads, so it is
/// short by nature; a fixed capacity keeps [`Allocation`] `Copy` for the modelling sweeps.
///
/// 24 since 2026-11-11: the Perps holding ladder climbs one per five Perps to fifty, then one per
/// ten to 150 — twenty rungs — before its tail takes over.
pub const RUNGS_MAX: usize = 24;

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
    /// Flavour holdings held now, which count in full outside the floor and the cap. Empty
    /// unless the policy sets [`Allocation::past_share`].
    pub held: Vec<u16>,
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
            clout: 0,
            rarest: self.credits.len() as u32,
            peak,
            current,
            enabled,
            promoted,
            lapsed: false,
            partner: false,
            matured: Matured::default(),
            granted: Grant::default(),
            token: 0,
        }
    }
}

impl Tracked<Score> {
    /// The phase inputs from both tracks: the bloodline's points as the score, the clout's
    /// alongside, and the rare-credit count summed — it is one holder's rarity either way.
    pub fn counts(self, peak: u32, current: u32, enabled: bool, promoted: bool) -> Counts {
        let clout = self.clout.points;
        let rarest = self.clout.credits.len() as u32;
        let mut counts = self.bloodline.counts(peak, current, enabled, promoted);
        counts.clout = clout;
        counts.rarest += rarest;
        counts
    }
}

impl Counts {
    /// The [`Track::Clout`] points, which climb [`Allocation::clout`].
    pub fn clout(mut self, points: u32) -> Self {
        self.clout = points;
        self
    }

    /// The token held now, across the identity's wallets.
    pub fn token(mut self, units: u64) -> Self {
        self.token = units;
        self
    }

    /// The slots the earned set grants outright, read by [`grants`].
    pub fn granted(mut self, granted: Grant) -> Self {
        self.granted = granted;
        self
    }

    /// The units held now by maturity, read from the lots by [`maturity_at`]. Held against
    /// `current` when the ladder reads it (see [`Matured::within`]), so a live count lower than
    /// the snapshot's is the one believed and a unit bought since it is new.
    pub fn matured(mut self, matured: Matured) -> Self {
        self.matured = matured;
        self
    }

    /// The units held now by maturity, as the ladder reads them against the live count.
    pub fn maturity(&self) -> Matured {
        self.matured.within(self.current)
    }

    /// The units the holding ladder reads: every unit held now at its maturity's share. A holder
    /// is never below the first rung: one living unit, however new, counts as one.
    pub fn ladder_units(&self, policy: Allocation) -> u32 {
        match policy.seasoning {
            None => self.current,
            Some(seasoning) => {
                let held = self.maturity();
                // One floor over the partial stages together: 2 maturing at 75% and 2 new at 50%
                // is 2.5, so 2.
                let partial = [Maturity::New, Maturity::Maturing]
                    .into_iter()
                    .map(|stage| f64::from(held.at(stage)) * seasoning.share(stage))
                    .sum::<f64>();
                let units = held.mature + partial.floor() as u32;
                units.max(u32::from(self.current > 0))
            }
        }
    }

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
    /// The points of every counted achievement — the [`Track::Bloodline`] ones when the catalogue
    /// has tracks.
    pub score: u32,
    /// The points of every counted [`Track::Clout`] achievement. See [`Counts::clout`].
    pub clout: u32,
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
    /// Units held now by maturity, as the snapshot's lots read them. See [`Counts::matured`].
    pub matured: Matured,
    /// What the earned set grants outright, from [`grants`]. See [`Counts::granted`].
    pub granted: Grant,
    /// The token held now, in its smallest unit, across the identity's wallets. See
    /// [`Allocation::token`].
    pub token: u64,
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

/// What an achievement grants outright: slots on top of whatever the ladder and the score give.
///
/// A Leader's guaranteed slot is the worked example. A grant is not points: it does not move a
/// holder toward a threshold, it is the slot itself, which is what lets a rare act be worth one
/// Perpling exactly rather than "enough points to maybe reach a rung".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Grant {
    #[serde(default)]
    pub gtd: u32,
    #[serde(default)]
    pub fcfs: u32,
    /// Earning it promotes: guaranteed whatever the score, and exempt from
    /// [`Allocation::gtd_min_held`]. A Leader held is the case: the canon's power tier is always
    /// guaranteed, and reading it from the snapshot means a page that cannot ask a live status
    /// still knows.
    #[serde(default, skip_serializing_if = "is_false")]
    pub promotes: bool,
    /// The grant is per piece held: each Leader held is its own five slots. Off, it is once per
    /// identity however many pieces earn it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub per_piece: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Grant {
    pub fn is_empty(&self) -> bool {
        self.gtd == 0 && self.fcfs == 0 && !self.promotes
    }
}

/// What a subject is allocated, in both phases at once: a holder can be guaranteed some and
/// first come for more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Slots {
    pub gtd: u32,
    pub fcfs: u32,
}

impl Slots {
    pub fn total(&self) -> u32 {
        self.gtd + self.fcfs
    }
}

/// The grants an earned set carries: once per achievement however many wallets earned it, or
/// once per piece held for a [`Grant::per_piece`] grant, the pieces summed across wallets (two
/// wallets hold different Perps).
///
/// A holding grants only while it is held: a Leader sold or listed is not a Leader's slot. A
/// record grants for good, because it cannot be undone. `grant_of` is the catalogue's; `None`
/// grants nothing. Each earned row is `(achievement, standing, pieces held)`.
pub fn grants(
    earned: impl IntoIterator<Item = (u16, Standing, u16)>,
    grant_of: impl Fn(u16) -> Option<Grant>,
) -> Grant {
    let mut by_index: BTreeMap<u16, (Standing, u32)> = BTreeMap::new();
    for (index, standing, held) in earned {
        let entry = by_index.entry(index).or_insert((standing, 0));
        entry.0 = entry.0.union(standing);
        if standing == Standing::Now {
            entry.1 += u32::from(held.max(1));
        }
    }
    by_index
        .into_iter()
        .filter(|(_, (standing, _))| !standing.is_lapsed_holding())
        .filter_map(|(index, (_, pieces))| grant_of(index).map(|grant| (grant, pieces)))
        .fold(Grant::default(), |sum, (grant, pieces)| {
            let times = match grant.per_piece {
                true => pieces.max(1),
                false => 1,
            };
            Grant {
                gtd: sum.gtd + grant.gtd * times,
                fcfs: sum.fcfs + grant.fcfs * times,
                promotes: sum.promotes || grant.promotes,
                per_piece: false,
            }
        })
}

/// The most steps a token ladder may have. Fixed so [`TokenRule`] stays `Copy` for the sweeps.
pub const TOKEN_STEPS_MAX: usize = 12;

/// One step of a token ladder: hold `at_least` units and the balance is worth `mints`. Units are
/// the token's smallest, so `u64`: a token with decimals passes `u32` long before it is a whale's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct TokenStep {
    pub at_least: u64,
    pub mints: u32,
}

/// Past a token ladder's last step, `mints` more for every `every` units beyond it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TokenTail {
    pub every: u64,
    pub mints: u32,
}

/// What holding a token is worth, in guaranteed slots: the highest step a balance reaches, and the
/// tail past the last. It needs no Perp — a token holder is a member of the collection's economy in
/// their own right — and no stake, because the token is the stake.
///
/// Settled 2026-11-11 for $PERP: 3M → 1, 10M → 2, 20M → 3, 40M → 4, 60M → 5, then one more for
/// every 20M. The older flat `{ per, gtd }` reads as one step and a tail of the same pace, which is
/// exactly what it meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "TokenRuleFile", into = "TokenRuleFile")]
pub struct TokenRule {
    len: u8,
    steps: [TokenStep; TOKEN_STEPS_MAX],
    pub tail: Option<TokenTail>,
    /// How much of a unit provided as liquidity counts, in percent. A pool share's token amount
    /// is exposure rather than a holding — it moves with the price, and the provider also holds
    /// the other side of the pair — so a campaign may count it at a discount. `100` when unset,
    /// so a catalogue written before this counts LP as it always did.
    pub lp_percent: u8,
}

/// [`TokenRule`] as a catalogue or manifest writes it: a `ladder` and its `tail`, or the flat
/// `{ per, gtd }` written before there was a ladder.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct TokenRuleFile {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ladder: Vec<TokenStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tail: Option<TokenTail>,
    #[serde(default, skip_serializing)]
    per: Option<u64>,
    #[serde(default, skip_serializing)]
    gtd: Option<u32>,
    #[serde(default = "full_weight")]
    lp_percent: u8,
}

fn full_weight() -> u8 {
    100
}

impl TryFrom<TokenRuleFile> for TokenRule {
    type Error = String;

    fn try_from(file: TokenRuleFile) -> Result<Self, String> {
        let (ladder, tail) = match (file.per, file.gtd) {
            (Some(per), gtd) if file.ladder.is_empty() => {
                if per == 0 {
                    return Err("token.per must be at least 1".to_string());
                }
                let mints = gtd.unwrap_or(1);
                (
                    vec![TokenStep {
                        at_least: per,
                        mints,
                    }],
                    Some(TokenTail { every: per, mints }),
                )
            }
            (None, None) => (file.ladder, file.tail),
            _ => {
                return Err(
                    "token: write a `ladder`, or the flat `per`/`gtd`, not both".to_string()
                );
            }
        };
        Self::new(&ladder, tail, file.lp_percent)
    }
}

impl From<TokenRule> for TokenRuleFile {
    fn from(rule: TokenRule) -> Self {
        Self {
            ladder: rule.steps().to_vec(),
            tail: rule.tail,
            per: None,
            gtd: None,
            lp_percent: rule.lp_percent,
        }
    }
}

impl TokenRule {
    /// A ladder strictly ascending by `at_least`, at most [`TOKEN_STEPS_MAX`] steps, and a tail
    /// with a non-zero pace.
    pub fn new(
        steps: &[TokenStep],
        tail: Option<TokenTail>,
        lp_percent: u8,
    ) -> Result<Self, String> {
        if steps.is_empty() {
            return Err("token: the ladder needs at least one step".to_string());
        }
        if steps.len() > TOKEN_STEPS_MAX {
            return Err(format!(
                "token: a ladder has at most {TOKEN_STEPS_MAX} steps, not {}",
                steps.len()
            ));
        }
        if let Some(pair) = steps
            .windows(2)
            .find(|pair| pair[1].at_least <= pair[0].at_least)
        {
            return Err(format!(
                "token: steps must ascend: {} does not follow {}",
                pair[1].at_least, pair[0].at_least
            ));
        }
        if tail.is_some_and(|tail| tail.every == 0) {
            return Err("token: tail.every must be at least 1".to_string());
        }
        let mut items = [TokenStep::default(); TOKEN_STEPS_MAX];
        items[..steps.len()].copy_from_slice(steps);
        Ok(Self {
            len: steps.len() as u8,
            steps: items,
            tail,
            lp_percent,
        })
    }

    pub fn steps(&self) -> &[TokenStep] {
        &self.steps[..usize::from(self.len)]
    }

    /// The units that count: everything held, in a wallet or staked or locked, plus the pooled
    /// units at [`Self::lp_percent`].
    pub fn weigh(&self, held: u64, pooled: u64) -> u64 {
        let counted = u128::from(pooled) * u128::from(self.lp_percent.min(100)) / 100;
        held.saturating_add(u64::try_from(counted).unwrap_or(u64::MAX))
    }

    /// The slots a balance is worth: the highest step reached, and the tail past the last.
    pub fn slots(&self, units: u64) -> u32 {
        let step = self
            .steps()
            .iter()
            .rev()
            .find(|step| units >= step.at_least)
            .map_or(0, |step| step.mints);
        let tail = match (self.tail, self.steps().last()) {
            (Some(tail), Some(last)) if units > last.at_least => {
                u32::try_from((units - last.at_least) / tail.every)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(tail.mints)
            }
            _ => 0,
        };
        step.saturating_add(tail)
    }
}

/// Which phase the ladder and the score put a subject in, before any grant: the old single
/// phase, now only the question of where the holding rung lands.
fn qualifies(subject: &Counts, policy: Allocation) -> Phase {
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
    // A guarantee is for a holder with a stake in the collection now, not one coasting on a
    // history: below the minimum the best a score reaches is first come.
    let holds_enough = subject.current >= policy.gtd_min_held;
    if subject.promoted
        || subject.granted.promotes
        || (holds_enough && subject.score >= policy.gtd_score)
    {
        Phase::Gtd
    } else if !holds_enough
        || subject.score >= policy.fcfs_score
        || (policy.fcfs_rarest > 0 && subject.rarest >= policy.fcfs_rarest)
    {
        Phase::Fcfs
    } else {
        Phase::Ungated
    }
}

/// The headline phase: guaranteed if any slot is, first come if only those are, and ungated —
/// the public phase — when the rule allocates nothing. Read from [`slots`], so a phase and its
/// count can never disagree.
pub fn phase(subject: &Counts, policy: Allocation) -> Phase {
    let slots = slots(subject, policy);
    if slots.gtd > 0 {
        Phase::Gtd
    } else if slots.fcfs > 0 {
        Phase::Fcfs
    } else {
        Phase::Ungated
    }
}

/// How many mints this subject is allocated in all: [`slots`] summed.
pub fn mints(subject: &Counts, policy: Allocation) -> u32 {
    slots(subject, policy).total()
}

impl Slots {
    /// The better of two allocations for the holder: more guaranteed first, then more in all.
    pub fn better(self, other: Self) -> Self {
        if (other.gtd, other.total()) > (self.gtd, self.total()) {
            other
        } else {
            self
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            gtd: self.gtd + other.gtd,
            fcfs: self.fcfs + other.fcfs,
        }
    }
}

/// What an identity is allocated: each of its linked wallets on its own, summed — so linking a
/// wallet for convenience never costs a holder anything.
///
/// Settled 2026-11-11. Scoring an identity as one merged holder punished linking: the holding
/// ladder, the points tables and the token ladder all slow with size, so seven wallets on seven
/// anchors read 158 guaranteed and the same seven on one anchor 103. Instead:
///
/// - **A wallet holding at least [`Allocation::gtd_min_held`]** is scored alone, exactly as a
///   subject is: its own ladder, its own points, its own grants and token.
/// - **The wallets under it** may combine: `pool` is them as one holder (units summed, earned sets
///   unioned, grants summed), so two wallets of three reach the stake together. The identity takes
///   the better of the pool and those wallets scored apart ([`Slots::better`]), so a small wallet
///   whose own record already earns first come never loses it by being pooled.
/// - **The token** is read wallet by wallet, outside the pool: its ladder slows with size too.
/// - **And never worse than one merged holder** (`merged`: every wallet's units summed and earned
///   sets unioned): two wallets of nine are one rung each apart and three merged, 4M and 2.5M of
///   the token are one slot apart and two merged, and a gate held in one wallet with the record
///   earned in another only counts merged. The Perps side and the token side each take the better.
///
/// `pool` is `None` when fewer than two wallets are under the stake. The modelling tool scores
/// every wallet alone, so its totals are this rule for any identity whose wallets each hold the
/// stake, and an upper bound on everything except pooling.
pub fn identity_slots(
    wallets: &[Counts],
    pool: Option<&Counts>,
    merged: Option<&Counts>,
    policy: Allocation,
) -> Slots {
    identity_sources(wallets, pool, merged, policy).total()
}

/// [`identity_slots`], source by source: the same choices, so the breakdown a page shows adds up to
/// the identity's count. See [`Sources`].
pub fn identity_sources(
    wallets: &[Counts],
    pool: Option<&Counts>,
    merged: Option<&Counts>,
    policy: Allocation,
) -> Sources {
    // A flat policy allocates per IDENTITY by definition (`per_identity`): summing its wallets
    // would hand out one allocation per wallet. Only a holding ladder is per wallet.
    if policy.holding.is_empty()
        && let Some(merged) = merged
    {
        return sources(merged, policy);
    }
    let token =
        |subject: &Counts| -> u32 { policy.token.map_or(0, |rule| rule.slots(subject.token)) };
    let without_token = |subject: &Counts| -> Sources {
        sources(
            &Counts {
                token: 0,
                ..subject.clone()
            },
            policy,
        )
    };
    let (big, small): (Vec<&Counts>, Vec<&Counts>) = wallets
        .iter()
        .partition(|wallet| wallet.current >= policy.gtd_min_held);
    let mut total = big.iter().fold(Sources::default(), |sum, wallet| {
        sum.plus(without_token(wallet))
    });
    let apart = small.iter().fold(Sources::default(), |sum, wallet| {
        sum.plus(without_token(wallet))
    });
    total = total.plus(match pool {
        Some(pool) if small.len() > 1 => apart.better(without_token(pool)),
        _ => apart,
    });
    let mut token_gtd = wallets.iter().map(token).sum::<u32>();
    if let Some(merged) = merged {
        // Merged wins a tie: its sources read against the identity's own score and clout, which
        // is what a page shows beside them.
        total = without_token(merged).better(total);
        token_gtd = token_gtd.max(token(merged));
    }
    total.token.gtd += token_gtd;
    total
}

/// What this subject is allocated in each phase.
///
/// One rule for the worker answering a holder and the tool modelling a mint:
///
/// - **Gated, over the guarantee's score (or promoted):** the holding ladder's rung (or
///   `per_identity` without a ladder), every score rung reached, and the leader and partner
///   bonuses, all guaranteed.
/// - **Gated, under it:** the holding rung, first come.
/// - **Gated, either way:** every grant on top, in the phase it names. A Leader held is a
///   guaranteed slot whatever the score. Under `gtd_min_held`, a guaranteed grant is first come
///   instead, unless a grant promotes.
/// - **Not gated:** the lapsed or partner door, first come, whichever is larger. No grants: a
///   subject without the gate has no profile.
///
/// `max_per_identity` caps the total when it is set, trimming first come before guaranteed.
///
/// The total of [`sources`]: the one rule, read whole.
pub fn slots(subject: &Counts, policy: Allocation) -> Slots {
    sources(subject, policy).total()
}

/// Where an allocation's slots come from, each source's guaranteed and first-come share. What a
/// page says ("20 from Perps, 14 from Leaders, 3 from bloodline …") is this, never a re-derivation,
/// so it adds up to [`slots`] by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Sources {
    /// The holding ladder's rung (or `per_identity` under a flat policy).
    pub holding: Slots,
    /// The score's rungs — the bloodline points', under points tables.
    pub bloodline: Slots,
    /// The clout points' rungs.
    pub clout: Slots,
    /// Slots achievements grant outright (a Leader's, per piece).
    pub grants: Slots,
    /// The token's ladder.
    pub token: Slots,
    /// A live promotion's or a partner holding's bonus, and the lapsed or partner door.
    pub doors: Slots,
    /// What `max_per_identity` took back, trimmed from first come before guaranteed.
    pub trimmed: Slots,
}

impl Sources {
    /// Every source summed, less what the ceiling trimmed: the allocation.
    pub fn total(&self) -> Slots {
        let sum = [
            self.holding,
            self.bloodline,
            self.clout,
            self.grants,
            self.token,
            self.doors,
        ]
        .into_iter()
        .fold(Slots::default(), Slots::plus);
        Slots {
            gtd: sum.gtd - self.trimmed.gtd,
            fcfs: sum.fcfs - self.trimmed.fcfs,
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            holding: self.holding.plus(other.holding),
            bloodline: self.bloodline.plus(other.bloodline),
            clout: self.clout.plus(other.clout),
            grants: self.grants.plus(other.grants),
            token: self.token.plus(other.token),
            doors: self.doors.plus(other.doors),
            trimmed: self.trimmed.plus(other.trimmed),
        }
    }

    /// The better for the holder of two readings, as [`Slots::better`] compares their totals.
    fn better(self, other: Self) -> Self {
        if self.total().better(other.total()) == self.total() {
            self
        } else {
            other
        }
    }
}

/// [`slots`], source by source. See [`Sources`].
pub fn sources(subject: &Counts, policy: Allocation) -> Sources {
    let mut out = Sources::default();
    if !subject.enabled {
        if qualifies(subject, policy) == Phase::Fcfs {
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
            out.doors.fcfs = lapsed.max(partner);
        }
    } else {
        let base = match policy.holding.is_empty() {
            true => policy.per_identity,
            false => holding_mints(&policy, subject.ladder_units(policy)),
        };
        // The bloodline's mints: guaranteed with the guarantee, unless the policy says both
        // tables are a chase, when they are first come for any gated identity as clout's are.
        let bloodline = policy.track_mints(Track::Bloodline, subject.score);
        if policy.bonus_fcfs {
            out.bloodline.fcfs = bloodline;
        }
        match qualifies(subject, policy) {
            Phase::Gtd => {
                out.holding.gtd = base;
                if !policy.bonus_fcfs {
                    out.bloodline.gtd = bloodline;
                }
                out.doors.gtd = if subject.promoted {
                    policy.promote_bonus
                } else {
                    0
                } + if subject.partner {
                    policy.partner_bonus
                } else {
                    0
                };
            }
            // Under the minimum held, the bonus the score reaches is first come instead, as a
            // guaranteed grant is: the record still earns, the stake decides the phase.
            Phase::Fcfs if subject.current < policy.gtd_min_held => {
                out.holding.fcfs = base;
                out.bloodline.fcfs = bloodline;
            }
            Phase::Fcfs => out.holding.fcfs = base,
            Phase::Ungated => {}
        }
        // Clout is first come whatever the guarantee: any gated identity climbs it.
        out.clout.fcfs = policy.track_mints(Track::Clout, subject.clout);
        // A guaranteed grant needs the same stake the ladder does: under the minimum held now,
        // it is a first-come slot instead. A promotion is exempt, as it is everywhere.
        let promoted = subject.promoted || subject.granted.promotes;
        match promoted || subject.current >= policy.gtd_min_held {
            true => out.grants.gtd = subject.granted.gtd,
            false => out.grants.fcfs = subject.granted.gtd,
        }
        out.grants.fcfs += subject.granted.fcfs;
    }
    // The token is its own door: guaranteed with or without a Perp, and outside the stake.
    if let Some(rule) = policy.token {
        out.token.gtd = rule.slots(subject.token);
    }
    let total = out.total();
    if policy.max_per_identity > 0 && total.total() > policy.max_per_identity {
        let over = total.total() - policy.max_per_identity;
        let from_fcfs = over.min(total.fcfs);
        out.trimmed = Slots {
            gtd: over - from_fcfs,
            fcfs: from_fcfs,
        };
    }
    out
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
    /// A flavour holding held now: counts in full, however common, outside the cap.
    Held,
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
        matches!(self, Self::Core | Self::Credit | Self::Held)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Credit => "rarest",
            Self::Held => "held",
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
    /// Whether it is a record or a holding, and if a holding, whether it is still held.
    pub standing: Standing,
    /// What it added to the score: `points` in full, `points` at the policy's past share for a
    /// holding no longer held, or zero when it did not count.
    pub worth: u32,
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
        subject
            .earned()
            .map(|earned| (earned.achievement, earned.standing)),
        role_of,
        points_of,
        policy,
    )
}

/// Score an earned set against a policy.
///
/// `earned` is an iterator rather than a slice because an identity's score is the union of its
/// wallets' sets: the caller concatenates the rows and this deduplicates, so one achievement
/// earned twice — by two wallets, or by one asset that came back — counts once, and is held now
/// if any wallet holds it now. `key` names the subject in an error message, and for a union it
/// is the identity.
pub fn score_earned<'a>(
    bundle: &Bundle<'_>,
    key: &str,
    earned: impl IntoIterator<Item = (u16, Standing)>,
    role_of: impl Fn(u16) -> Option<&'a str>,
    points_of: impl Fn(u16) -> Option<u32>,
    policy: Allocation,
) -> Result<Score, ScoreError> {
    explain(bundle, key, earned, role_of, points_of, policy).score
}

/// [`explain`] once per [`Track`]: each track's earned achievements scored apart, under the same
/// policy — so the rarity floor and the rarest-few cap apply within a table, never across them.
pub fn explain_tracks<'a>(
    bundle: &Bundle<'_>,
    key: &str,
    earned: impl IntoIterator<Item = (u16, Standing)>,
    role_of: impl Fn(u16) -> Option<&'a str> + Copy,
    points_of: impl Fn(u16) -> Option<u32> + Copy,
    track_of: impl Fn(u16) -> Track,
    policy: Allocation,
) -> Tracked<Trace> {
    let mut split: Tracked<Vec<(u16, Standing)>> = Tracked::default();
    for (index, standing) in earned {
        split.get_mut(track_of(index)).push((index, standing));
    }
    split.map(|_, earned| explain(bundle, key, earned, role_of, points_of, policy))
}

/// [`score_earned`] once per [`Track`]. See [`explain_tracks`].
pub fn score_tracks<'a>(
    bundle: &Bundle<'_>,
    key: &str,
    earned: impl IntoIterator<Item = (u16, Standing)>,
    role_of: impl Fn(u16) -> Option<&'a str> + Copy,
    points_of: impl Fn(u16) -> Option<u32> + Copy,
    track_of: impl Fn(u16) -> Track,
    policy: Allocation,
) -> Result<Tracked<Score>, ScoreError> {
    let traces = explain_tracks(bundle, key, earned, role_of, points_of, track_of, policy);
    Ok(Tracked {
        bloodline: traces.bloodline.score?,
        clout: traces.clout.score?,
    })
}

/// Every earned achievement with what scoring made of it, and the score they add up to.
///
/// The rule lives here and [`score_earned`] is this function's `score`: a surface that explains
/// an answer renders these rows instead of re-deciding which achievements counted, which is the
/// bug a debug view exists to catch rather than to reintroduce.
pub fn explain<'a>(
    bundle: &Bundle<'_>,
    key: &str,
    earned: impl IntoIterator<Item = (u16, Standing)>,
    role_of: impl Fn(u16) -> Option<&'a str>,
    points_of: impl Fn(u16) -> Option<u32>,
    policy: Allocation,
) -> Trace {
    let mut indices: BTreeMap<u16, Standing> = BTreeMap::new();
    for (index, standing) in earned {
        indices
            .entry(index)
            .and_modify(|seen| *seen = seen.union(standing))
            .or_insert(standing);
    }
    let mut rows: Vec<TraceRow> = Vec::with_capacity(indices.len());
    let mut refusal: Option<ScoreError> = None;
    let mut eligible: Vec<(f64, u16)> = Vec::new();
    // Without a past share the present tense is off, and a holding scores as a record.
    let present = policy.past_share.is_some();

    for (index, standing) in indices {
        let outcome = match role_of(index) {
            Some(text) if Role::parse(text) == Some(Role::Core) => Outcome::Core,
            // Held now: the collection's signal, which no floor or cap may crowd out.
            Some(text)
                if Role::parse(text) == Some(Role::Flavour)
                    && present
                    && standing == Standing::Now =>
            {
                Outcome::Held
            }
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
            standing,
            worth: 0,
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
            Some(weight) => {
                row.worth = match (policy.past_share, row.standing) {
                    (Some(share), standing) if standing.is_lapsed_holding() => {
                        past_worth(weight, share)
                    }
                    _ => weight,
                };
                points += row.worth;
            }
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
            held: rows
                .iter()
                .filter(|row| row.outcome == Outcome::Held)
                .map(|row| row.achievement)
                .collect(),
            points,
        }),
    };
    Trace { rows, score }
}

/// What a holding no longer held is worth: its points at `share`, rounded to a whole point and
/// never above the points themselves.
fn past_worth(points: u32, share: f64) -> u32 {
    let share = share.clamp(0.0, 1.0);
    ((f64::from(points) * share).round() as u32).min(points)
}

/// When every unit held now will be [`Maturity::Mature`]: the latest open lot's arrival plus the
/// seasoning's mature days. `None` when nothing held is still maturing at `now_unix`.
///
/// What a page says as "full on 22 Nov". A unit bought since the snapshot the lots were read from
/// is not in them; the caller adds its own reading for those (a purchase made now matures
/// `mature_days` from now).
pub fn matures_at(lots: &[Lot], now_unix: u64, seasoning: Seasoning) -> Option<u64> {
    let mature = u64::from(seasoning.mature_days) * crate::SECONDS_PER_DAY;
    lots.iter()
        .filter(|lot| lot.disposed_at.is_none())
        .map(|lot| lot.acquired_unix + mature)
        .filter(|at| *at > now_unix)
        .max()
}

/// Distinct units held now, each at its [`Maturity`] at `now_unix`: what [`Counts::matured`]
/// takes.
///
/// Measured on the lot's unix arrival, the clock tenure already uses. A unit that left and came
/// back matures from its latest arrival, because the open lot is the one that is held; one with
/// two open lots counts once, at the older.
pub fn maturity_at(lots: &[Lot], now_unix: u64, seasoning: Seasoning) -> Matured {
    let mut arrived: BTreeMap<u32, u64> = BTreeMap::new();
    for lot in lots.iter().filter(|lot| lot.disposed_at.is_none()) {
        arrived
            .entry(lot.unit)
            .and_modify(|at| *at = (*at).min(lot.acquired_unix))
            .or_insert(lot.acquired_unix);
    }
    let mut matured = Matured::default();
    for at in arrived.into_values() {
        matured.add(seasoning.maturity(now_unix.saturating_sub(at)));
    }
    matured
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

    /// Earned rows that are all records: the scoring every test before standings wrote.
    fn records(indices: &[u16]) -> Vec<(u16, Standing)> {
        indices
            .iter()
            .map(|index| (*index, Standing::Record))
            .collect()
    }

    fn counts(score: u32, rarest: u32, enabled: bool, promoted: bool) -> Counts {
        Counts {
            score,
            clout: 0,
            rarest,
            peak: 0,
            current: 0,
            enabled,
            promoted,
            lapsed: false,
            partner: false,
            matured: Matured::default(),
            granted: Grant::default(),
            token: 0,
        }
    }

    /// A reading with `mature` units through the seasoning and `maturing` past its first stage.
    fn matured(mature: u32, maturing: u32) -> Matured {
        Matured {
            new: 0,
            maturing,
            mature,
        }
    }

    fn grant(gtd: u32, fcfs: u32) -> Grant {
        Grant {
            gtd,
            fcfs,
            ..Grant::default()
        }
    }

    #[test]
    fn a_per_piece_grant_counts_every_piece_held_across_wallets() {
        let leader = Grant {
            gtd: 5,
            promotes: true,
            per_piece: true,
            ..Grant::default()
        };
        let grant_of = |index: u16| match index {
            0 => Some(leader),
            1 => Some(grant(1, 0)),
            _ => None,
        };
        // Two Leaders in one wallet and one in another; a once-per-identity grant held twice.
        let granted = grants(
            [
                (0, Standing::Now, 2),
                (0, Standing::Now, 1),
                (1, Standing::Now, 4),
                (1, Standing::Now, 1),
            ],
            grant_of,
        );
        assert_eq!(granted.gtd, 3 * 5 + 1);
        assert!(granted.promotes);
        // A Leader only listed is not held: no slots, and no promotion.
        let listed = grants([(0, Standing::Listed, 0)], grant_of);
        assert_eq!(listed, Grant::default());
    }

    /// The flat rule as the older catalogue wrote it: one slot for every `per`.
    fn flat_token(per: u64, lp_percent: u8) -> TokenRule {
        TokenRule::new(
            &[TokenStep {
                at_least: per,
                mints: 1,
            }],
            Some(TokenTail {
                every: per,
                mints: 1,
            }),
            lp_percent,
        )
        .unwrap()
    }

    /// Settled 2026-11-11 for $PERP: 3M → 1, 10M → 2, 20M → 3, 40M → 4, 60M → 5, then one more for
    /// every 20M.
    #[test]
    fn a_token_ladder_reads_its_steps_then_its_tail() {
        let step = |m: u64, mints| TokenStep {
            at_least: m * 1_000_000,
            mints,
        };
        let rule = TokenRule::new(
            &[
                step(3, 1),
                step(10, 2),
                step(20, 3),
                step(40, 4),
                step(60, 5),
            ],
            Some(TokenTail {
                every: 20_000_000,
                mints: 1,
            }),
            50,
        )
        .unwrap();
        let at = |m: u64| rule.slots(m * 1_000_000);
        assert_eq!(at(2), 0);
        assert_eq!((at(3), at(9)), (1, 1));
        assert_eq!((at(10), at(19)), (2, 2));
        assert_eq!((at(20), at(39)), (3, 3));
        assert_eq!((at(40), at(59)), (4, 4));
        assert_eq!((at(60), at(79)), (5, 5));
        assert_eq!((at(80), at(100), at(115)), (6, 7, 7));
        // It round-trips as the ladder, and the flat form still reads.
        let again: TokenRule =
            serde_json::from_str(&serde_json::to_string(&rule).unwrap()).unwrap();
        assert_eq!(again, rule);
        let flat: TokenRule = serde_json::from_str(r#"{"per":3000000,"gtd":1}"#).unwrap();
        assert_eq!(flat, flat_token(3_000_000, 100));
        assert_eq!(flat.slots(10_000_000), 3, "the flat form keeps its pace");
        // Out of order is refused.
        assert!(TokenRule::new(&[step(10, 2), step(3, 1)], None, 100).is_err());
    }

    #[test]
    fn a_token_balance_is_guaranteed_slots_with_or_without_a_perp() {
        let policy = Allocation {
            token: Some(flat_token(3_000_000, 100)),
            gtd_min_held: 5,
            ..ladder()
        };
        // 10M and no Perp: three guaranteed, the gate shut.
        let token_only = counts(0, 0, false, false).token(10_000_000);
        assert_eq!(slots(&token_only, policy), Slots { gtd: 3, fcfs: 0 });
        assert_eq!(phase(&token_only, policy), Phase::Gtd);
        // 2.9M is not a slot.
        assert_eq!(
            slots(&counts(0, 0, false, false).token(2_999_999), policy).gtd,
            0
        );
        // On top of a holder's own, and outside the stake: two held, under five.
        let holder = holding(0, 2).token(6_000_000);
        assert_eq!(slots(&holder, policy).gtd, 2);
    }

    /// Settled 2026-11-11: LP counts at half. MEASURED the same day, the largest $PERP LP
    /// provider held 50.9M in the pool beside 63.8M held and staked.
    #[test]
    fn pooled_units_count_at_the_rules_share_and_held_units_in_full() {
        let half = flat_token(3_000_000, 50);
        assert_eq!(half.weigh(63_800_000, 50_900_000), 63_800_000 + 25_450_000);
        assert_eq!(half.slots(half.weigh(63_800_000, 50_900_000)), 29);
        // 6M only in a pool is worth 3M: one slot, not two.
        assert_eq!(half.slots(half.weigh(0, 6_000_000)), 1);
        // A catalogue written before the field counts LP in full.
        let old: TokenRule = serde_json::from_str(r#"{"per":3000000,"gtd":1}"#).unwrap();
        assert_eq!(old.lp_percent, 100);
        assert_eq!(old.weigh(1, 6_000_000), 6_000_001);
    }

    #[test]
    fn past_the_last_rung_the_tail_climbs_at_its_rate() {
        let tailed = Allocation {
            holding_tail: Some(Tail {
                every: 25,
                mints: 1,
            }),
            ..ladder()
        };
        // The last rung is 100 → 12; 124 is not a full step past it, 125 is, 450 is fourteen.
        assert_eq!(holding_mints(&tailed, 99), 6);
        assert_eq!(holding_mints(&tailed, 124), 12);
        assert_eq!(holding_mints(&tailed, 125), 13);
        assert_eq!(holding_mints(&tailed, 450), 12 + 14);
        assert_eq!(
            holding_mints(&ladder(), 450),
            12,
            "without a tail the last rung is the top"
        );
    }

    #[test]
    fn under_the_minimum_a_guaranteed_grant_is_first_come_unless_it_promotes() {
        let policy = Allocation {
            gtd_score: 10_000,
            gtd_min_held: 3,
            ..ladder()
        };
        // A minter keeping two: well over the score, but under the stake. The ladder's two units,
        // the score's rungs (5K, 10K and 15K's two) and the minter's grant are all first come.
        let minter = holding(20_000, 2).granted(grant(1, 0));
        assert_eq!(
            slots(&minter, policy),
            Slots {
                gtd: 0,
                fcfs: 1 + (1 + 1 + 2) + 1
            }
        );
        // Three held: the same grant is guaranteed, and so is the rung.
        let kept = holding(20_000, 3).granted(grant(1, 0));
        assert_eq!(slots(&kept, policy).gtd, 1 + (1 + 1 + 2) + 1);
        // A Leader on one Perp promotes: guaranteed, its rung and score rungs too.
        let leader = holding(4_000, 1).granted(Grant {
            gtd: 1,
            promotes: true,
            ..Grant::default()
        });
        assert_eq!(
            slots(&leader, policy),
            Slots {
                gtd: 1 + 1,
                fcfs: 0
            }
        );
    }

    #[test]
    fn a_grant_is_a_slot_in_its_phase_whatever_the_score() {
        let policy = Allocation {
            gtd_score: 10_000,
            ..ladder()
        };
        // Under the guarantee's score, one held: the holding rung is first come, and a Leader's
        // grant is a guaranteed slot beside it.
        let leader = holding(4_000, 1).granted(grant(1, 0));
        assert_eq!(slots(&leader, policy), Slots { gtd: 1, fcfs: 1 });
        assert_eq!(
            phase(&leader, policy),
            Phase::Gtd,
            "any guaranteed slot is the headline"
        );
        // Over it: the rung and the score rungs guaranteed, a crew's first-come grant on top.
        let crew = holding(15_000, 5).granted(grant(0, 1));
        assert_eq!(
            slots(&crew, policy),
            Slots {
                gtd: 3 + (1 + 1 + 2),
                fcfs: 1
            },
            "the 5 rung and every score rung to 15,000"
        );
        // No gate, no profile: a grant does not open a door the gate shut.
        let sold = counts(15_000, 0, false, false).granted(grant(1, 1));
        assert_eq!(slots(&sold, policy), Slots::default());
    }

    #[test]
    fn a_ceiling_trims_first_come_before_guaranteed() {
        let policy = Allocation {
            max_per_identity: 14,
            ..ladder()
        };
        let whale = holding(20_000, 500).granted(grant(1, 3));
        // 12 + 4 + 1 = 17 guaranteed, 3 first come: 20, trimmed to 14 from the first come first.
        assert_eq!(slots(&whale, policy), Slots { gtd: 14, fcfs: 0 });
    }

    #[test]
    fn a_holding_grants_while_held_and_a_record_for_good() {
        let grant_of = |index: u16| match index {
            0 => Some(grant(1, 0)),
            1 => Some(grant(0, 1)),
            2 => Some(grant(1, 0)),
            _ => None,
        };
        let granted = grants(
            [
                (0, Standing::Now, 1),
                (1, Standing::Record, 0),
                (2, Standing::Past, 0),
                (3, Standing::Now, 1),
                // Two wallets, one still holding it: held now, and granted once.
                (0, Standing::Past, 0),
            ],
            grant_of,
        );
        assert_eq!(granted, grant(1, 1));
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

    /// The breakdown a page shows is the rule's own: it adds up to the count, source by source.
    #[test]
    fn sources_add_up_to_the_slots_and_name_each_one() {
        let rung = |at_least, mints| Rung { at_least, mints };
        let policy = Allocation {
            gtd_min_held: 5,
            clout: Rungs::new(&[rung(5_000, 1)]).unwrap(),
            token: Some(flat_token(3_000_000, 100)),
            ..ladder()
        };
        // Six held (the 5 rung's three), 10,000 bloodline (two bonus rungs), a Leader's five,
        // 5,000 clout, 6M of the token.
        let holder = holding(10_000, 6)
            .clout(5_000)
            .granted(grant(5, 0))
            .token(6_000_000);
        let read = sources(&holder, policy);
        assert_eq!(read.holding, Slots { gtd: 3, fcfs: 0 });
        assert_eq!(read.bloodline, Slots { gtd: 2, fcfs: 0 });
        assert_eq!(read.grants, Slots { gtd: 5, fcfs: 0 });
        assert_eq!(read.clout, Slots { gtd: 0, fcfs: 1 });
        assert_eq!(read.token, Slots { gtd: 2, fcfs: 0 });
        assert_eq!(read.total(), slots(&holder, policy));
        // Under the stake, the same record reads first come, and says so by source.
        let small = sources(&holding(10_000, 2), policy);
        assert_eq!(small.bloodline, Slots { gtd: 0, fcfs: 2 });
        // A ceiling's trim is a source too, so the total still agrees.
        let capped = Allocation {
            max_per_identity: 4,
            ..policy
        };
        let read = sources(&holder, capped);
        assert_eq!(read.total().total(), 4);
        assert_eq!(read.total(), slots(&holder, capped));
        // And an identity's breakdown is the same choice its count made.
        let wallets = [holder.clone(), holding(0, 9)];
        assert_eq!(
            identity_sources(&wallets, None, Some(&holding(0, 15)), policy).total(),
            identity_slots(&wallets, None, Some(&holding(0, 15)), policy)
        );
    }

    /// A wallet's maturing Perps are full on the date its last lot reaches eight days.
    #[test]
    fn a_wallet_is_full_when_its_newest_lot_matures() {
        let day = crate::SECONDS_PER_DAY;
        let seasoning = Seasoning {
            maturing_days: 4,
            mature_days: 8,
            new_share: 0.5,
            maturing_share: 0.75,
        };
        let lot = |unit, acquired_days: u64| Lot {
            unit,
            quantity: 1,
            open: 1,
            acquired_at: 0,
            acquired_unix: acquired_days * day,
            disposed_at: None,
            disposed_unix: None,
            from_mint: false,
        };
        let now = 100 * day;
        assert_eq!(
            matures_at(&[lot(1, 10), lot(2, 97), lot(3, 95)], now, seasoning),
            Some(105 * day),
            "the newest, three days in, is full in five"
        );
        assert_eq!(matures_at(&[lot(1, 10)], now, seasoning), None);
    }

    /// Settled 2026-11-11: linking a wallet never costs a holder anything.
    #[test]
    fn an_identity_is_its_wallets_summed_and_small_wallets_may_pool() {
        let policy = Allocation {
            gtd_min_held: 5,
            token: Some(flat_token(3_000_000, 100)),
            ..ladder()
        };
        // Two whales: each alone, summed — never the slower ladder of 200 Perps together.
        let whale = holding(0, 100);
        let alone = slots(&whale, policy);
        assert_eq!(
            identity_slots(
                &[whale.clone(), whale.clone()],
                None,
                Some(&holding(0, 200)),
                policy
            ),
            Slots {
                gtd: alone.gtd * 2,
                fcfs: alone.fcfs * 2
            }
        );
        // Two wallets of three: apart, first come at the 1 rung each; pooled, six held is the 5
        // rung, guaranteed. The pool is better, and taken.
        let small = holding(0, 3);
        let apart = slots(&small, policy);
        assert_eq!(apart, Slots { gtd: 0, fcfs: 1 });
        let pooled = holding(0, 6);
        assert_eq!(
            identity_slots(&[small.clone(), small.clone()], Some(&pooled), None, policy),
            slots(&pooled, policy)
        );
        // A small wallet whose record earns first come keeps it: a pool that reads worse is not
        // taken.
        let recorded = holding(10_000, 2);
        let poor_pool = holding(0, 4);
        assert_eq!(
            identity_slots(
                &[recorded.clone(), holding(0, 2)],
                Some(&poor_pool),
                None,
                policy
            ),
            slots(&recorded, policy).plus(slots(&holding(0, 2), policy))
        );
        // The token is each wallet's own ladder, never the pool's.
        let token_a = holding(0, 1).token(6_000_000);
        let token_b = holding(0, 1).token(3_000_000);
        let token_pool = holding(0, 2).token(9_000_000);
        assert_eq!(
            identity_slots(&[token_a, token_b], Some(&token_pool), None, policy).gtd,
            2 + 1
        );
        // …and never worse than merged: 4M and 2.5M are one slot apart, two together.
        let (a, b) = (
            holding(0, 1).token(4_000_000),
            holding(0, 1).token(2_500_000),
        );
        let merged = holding(0, 2).token(6_500_000);
        assert_eq!(identity_slots(&[a, b], None, Some(&merged), policy).gtd, 2);
        // Ladder remainders likewise: two of nine are a rung each apart, the 5 rung's three
        // together on this ladder.
        let nine = holding(0, 9);
        let eighteen = holding(0, 18);
        assert_eq!(
            identity_slots(&[nine.clone(), nine], None, Some(&eighteen), policy),
            slots(&holding(0, 9), policy)
                .plus(slots(&holding(0, 9), policy))
                .better(slots(&eighteen, policy))
        );
    }

    /// Settled 2026-11-13: both tables are a chase. Each pays first come at a rate, a rung and a
    /// tail, and the guarantee is what is held.
    #[test]
    fn both_tables_climb_first_come_at_a_rate() {
        let rung = |at_least, mints| Rung { at_least, mints };
        let per_thousand = Some(Tail {
            every: 1_000,
            mints: 1,
        });
        let policy = Allocation {
            bonus: Rungs::new(&[rung(1_000, 1)]).unwrap(),
            bonus_tail: per_thousand,
            bonus_fcfs: true,
            clout: Rungs::new(&[rung(1_000, 1)]).unwrap(),
            clout_tail: per_thousand,
            max_per_identity: 0,
            ..ladder()
        };
        // Five held is the 3 rung, guaranteed; 15,500 bloodline is fifteen first come and 2,999
        // clout two more.
        let holder = holding(15_500, 5).clout(2_999);
        assert_eq!(
            slots(&holder, policy),
            Slots {
                gtd: 3,
                fcfs: 15 + 2
            }
        );
        let read = sources(&holder, policy);
        assert_eq!(read.bloodline, Slots { gtd: 0, fcfs: 15 });
        assert_eq!(read.clout, Slots { gtd: 0, fcfs: 2 });
        // The step above and the one last reached, on the rung and on the tail.
        assert_eq!(
            policy.track_next(Track::Bloodline, 15_500),
            Some(rung(16_000, 1))
        );
        assert_eq!(policy.track_below(Track::Bloodline, 15_500), 15_000);
        assert_eq!(policy.track_next(Track::Clout, 0), Some(rung(1_000, 1)));
        assert_eq!(policy.track_below(Track::Clout, 999), 0);
        assert_eq!(policy.track_phase(Track::Bloodline), PhaseKind::Fcfs);
        // Under the stake nothing changes for the points: they were first come already.
        assert_eq!(
            sources(&holding(15_500, 2), policy).bloodline,
            Slots { gtd: 0, fcfs: 15 }
        );
        // Without a tail a ladder still tops out.
        let capped = Allocation {
            bonus_tail: None,
            ..policy
        };
        assert_eq!(capped.track_mints(Track::Bloodline, 15_500), 1);
        assert_eq!(capped.track_next(Track::Bloodline, 15_500), None);
    }

    /// Settled 2026-11-11: bloodline points climb the guarantee, clout points climb first come.
    #[test]
    fn clout_climbs_first_come_and_bloodline_climbs_the_guarantee() {
        let rung = |at_least, mints| Rung { at_least, mints };
        let policy = Allocation {
            clout: Rungs::new(&[rung(5_000, 1), rung(12_000, 1)]).unwrap(),
            ..ladder()
        };
        // Five held is the 3 rung; 10,000 bloodline reaches two bonus rungs; 12,000 clout, two
        // first-come rungs.
        let holder = holding(10_000, 5).clout(12_000);
        assert_eq!(
            slots(&holder, policy),
            Slots {
                gtd: 3 + 2,
                fcfs: 2
            }
        );
        // Clout needs the gate, as every allocation does.
        let shut = Counts {
            enabled: false,
            ..holding(0, 0).clout(50_000)
        };
        assert_eq!(slots(&shut, policy), Slots::default());
        // And it earns first come even under the minimum held.
        let small = Allocation {
            gtd_min_held: 5,
            ..policy
        };
        assert_eq!(slots(&holding(0, 2).clout(5_000), small).fcfs, 1 + 1);
        // Under it, bloodline's rungs are first come too, as a guaranteed grant is.
        assert_eq!(
            slots(&holding(10_000, 2), small),
            Slots {
                gtd: 0,
                fcfs: 1 + 2
            }
        );
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
                        standing: Standing::Record,
                        held: 0,
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
            records(&[0, 1, 2, 3, 4]),
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

    /// The present tense on: a holding no longer held keeps a tenth.
    fn present() -> Allocation {
        Allocation {
            past_share: Some(0.1),
            count_rarest: 1,
            ..policy()
        }
    }

    fn worth_of(trace: &Trace, achievement: u16) -> (Outcome, u32) {
        let row = trace
            .rows
            .iter()
            .find(|row| row.achievement == achievement)
            .unwrap();
        (row.outcome, row.worth)
    }

    #[test]
    fn a_holding_no_longer_held_keeps_only_its_past_share() {
        // #0 a core holding sold, #1 a core holding kept, #2 a core record.
        let bytes = bundle_with(&[0, 1, 2], vec![crowded(), crowded(), crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let trace = explain(
            &bundle,
            "0xidentity",
            [
                (0, Standing::Past),
                (1, Standing::Now),
                (2, Standing::Record),
            ],
            |_| Some("core"),
            |_| Some(5_000),
            present(),
        );
        assert_eq!(
            worth_of(&trace, 0),
            (Outcome::Core, 500),
            "a tenth, once sold"
        );
        assert_eq!(
            worth_of(&trace, 1),
            (Outcome::Core, 5_000),
            "in full, still held"
        );
        assert_eq!(
            worth_of(&trace, 2),
            (Outcome::Core, 5_000),
            "a record has no past tense"
        );
        assert_eq!(trace.score.unwrap().points, 10_500);
    }

    #[test]
    fn a_flavour_holding_held_now_escapes_the_floor_and_the_cap() {
        // Two scarce flavour holdings held now and a crowded one held now: all three count,
        // though `count_rarest` is one and the third is above the floor. A scarce one sold
        // competes for the single rarest slot and keeps a tenth.
        let bytes = bundle_with(&[0, 1, 2, 3], vec![scarce(), scarce(), crowded(), scarce()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let trace = explain(
            &bundle,
            "0xidentity",
            [
                (0, Standing::Now),
                (1, Standing::Now),
                (2, Standing::Now),
                (3, Standing::Past),
            ],
            |_| Some("flavour"),
            |_| Some(3_000),
            present(),
        );
        assert_eq!(worth_of(&trace, 0), (Outcome::Held, 3_000));
        assert_eq!(
            worth_of(&trace, 2),
            (Outcome::Held, 3_000),
            "however common"
        );
        assert_eq!(worth_of(&trace, 3), (Outcome::Credit, 300));
        let score = trace.score.unwrap();
        assert_eq!(score.held, vec![0, 1, 2]);
        assert_eq!(score.credits, vec![3]);
        assert_eq!(score.points, 9_300);
    }

    #[test]
    fn without_a_past_share_every_holding_scores_as_it_always_did() {
        let bytes = bundle_with(&[0, 1], vec![scarce(), crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let trace = explain(
            &bundle,
            "0xidentity",
            [(0, Standing::Past), (1, Standing::Now)],
            |_| Some("flavour"),
            |_| Some(3_000),
            policy(),
        );
        assert_eq!(worth_of(&trace, 0), (Outcome::Credit, 3_000));
        assert_eq!(worth_of(&trace, 1), (Outcome::Crowded, 0));
    }

    #[test]
    fn two_wallets_holding_and_having_held_one_achievement_hold_it_now() {
        let bytes = bundle_with(&[0], vec![crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let trace = explain(
            &bundle,
            "0xidentity",
            [(0, Standing::Past), (0, Standing::Now)],
            |_| Some("core"),
            |_| Some(1_000),
            present(),
        );
        assert_eq!(trace.rows.len(), 1);
        assert_eq!(trace.rows[0].standing, Standing::Now);
        assert_eq!(trace.rows[0].worth, 1_000);
    }

    #[test]
    fn a_past_share_rounds_to_a_point_and_never_exceeds_the_points() {
        assert_eq!(past_worth(2_500, 0.1), 250);
        assert_eq!(past_worth(1_200, 0.25), 300);
        assert_eq!(past_worth(250, 0.1), 25);
        assert_eq!(past_worth(15, 0.1), 2, "1.5 rounds up");
        assert_eq!(past_worth(1_000, 3.0), 1_000, "a share is clamped to one");
        assert_eq!(past_worth(1_000, -1.0), 0, "and to zero");
    }

    #[test]
    fn a_unit_held_less_than_the_seasoning_counts_at_its_share() {
        let seasoned = Allocation {
            seasoning: Some(Seasoning::single(90, 0.5)),
            ..ladder()
        };
        // Ten held: four through the seasoning, six bought since. 4 + 6 × 0.5 = 7 on the ladder.
        let holder = holding(0, 10).matured(matured(4, 0));
        assert_eq!(holder.ladder_units(seasoned), 7);
        assert_eq!(
            mints(&holder, seasoned),
            3,
            "7 reaches the 5 rung, not the 10"
        );
        assert_eq!(
            holder.ladder_units(ladder()),
            10,
            "without seasoning, a plain count"
        );
        // A snapshot that saw more seasoned units than a live read now finds is capped by the read.
        assert_eq!(
            holding(0, 3).matured(matured(5, 0)).ladder_units(seasoned),
            3
        );
        // One fresh unit at a half would round to nothing; a holder is never below the first rung.
        let fresh = holding(0, 1);
        assert_eq!(fresh.ladder_units(seasoned), 1);
        assert_eq!(mints(&fresh, seasoned), 1);
        assert_eq!(
            holding(0, 0).ladder_units(seasoned),
            0,
            "and holding none is none"
        );
    }

    /// Settled 2026-11-11: new at a half, four days at three quarters, eight days in full. Five
    /// swept the week before a snapshot is a guarantee; five swept the night before is not.
    #[test]
    fn a_unit_matures_new_then_maturing_then_mature() {
        let day = crate::SECONDS_PER_DAY;
        let seasoning = Seasoning {
            maturing_days: 4,
            mature_days: 8,
            new_share: 0.5,
            maturing_share: 0.75,
        };
        let policy = Allocation {
            seasoning: Some(seasoning),
            ..ladder()
        };
        assert_eq!(seasoning.maturity(3 * day), Maturity::New);
        assert_eq!(seasoning.maturity(4 * day), Maturity::Maturing);
        assert_eq!(seasoning.maturity(8 * day), Maturity::Mature);
        let swept = |days_ago: u64| -> Vec<Lot> {
            (0..5)
                .map(|unit| Lot {
                    unit,
                    quantity: 1,
                    open: 1,
                    acquired_at: 0,
                    acquired_unix: 100 * day - days_ago * day,
                    disposed_at: None,
                    disposed_unix: None,
                    from_mint: false,
                })
                .collect()
        };
        let units = |days_ago| {
            holding(0, 5)
                .matured(maturity_at(&swept(days_ago), 100 * day, seasoning))
                .ladder_units(policy)
        };
        assert_eq!(units(0), 2, "five new: 2.5, so 2");
        assert_eq!(units(3), 2, "still new at three days");
        assert_eq!(units(4), 3, "five maturing: 3.75, so 3");
        assert_eq!(units(8), 5, "eight days: mature, the first rung");
        // Mixed: two mature, two maturing, one new — 2 + 1.5 + 0.5 = 4.
        let mixed = holding(0, 5).matured(matured(2, 2));
        assert_eq!(
            mixed.maturity(),
            Matured {
                new: 1,
                maturing: 2,
                mature: 2
            }
        );
        assert_eq!(mixed.ladder_units(policy), 4);
        // A reading never exceeds the live count: the oldest are believed first.
        assert_eq!(
            holding(0, 3).matured(matured(2, 5)).maturity(),
            matured(2, 1)
        );
        // A schedule with no middle stage reads a unit as new until it is mature.
        let single = Seasoning::single(8, 0.5);
        assert_eq!(single.maturity(5 * day), Maturity::New);
    }

    /// Settled 2026-11-11: $PERP matures on the Perps schedule. MEASURED the same day, one wallet
    /// held 40.4M with only 4.3M of it there all eight days.
    #[test]
    fn a_token_balance_weighs_by_how_long_it_was_held() {
        let seasoning = Seasoning {
            maturing_days: 4,
            mature_days: 8,
            new_share: 0.5,
            maturing_share: 0.75,
        };
        // Held all along: in full.
        assert_eq!(
            seasoning.weigh_held(60_000_000, 60_000_000, 60_000_000),
            60_000_000
        );
        // 4.3M mature, 36.1M maturing: 4.3M + 27.075M.
        assert_eq!(
            seasoning.weigh_held(40_373_434, 40_373_434, 4_297_995),
            4_297_995 + 27_056_579
        );
        // Split into a fresh wallet the night before: at half.
        assert_eq!(seasoning.weigh_held(3_000_000, 0, 0), 1_500_000);
        // A reading out of order never counts more than is held.
        assert_eq!(seasoning.weigh_held(1_000, 5_000, 9_000), 1_000);
    }

    /// The catalogue's two spellings: the stages by name, and the single step written before
    /// there was a middle stage.
    #[test]
    fn a_seasoning_reads_by_stage_or_as_the_older_single_step() {
        let staged: Seasoning = serde_json::from_str(
            r#"{"maturing_days":4,"mature_days":8,"new_share":0.5,"maturing_share":0.75}"#,
        )
        .unwrap();
        assert_eq!(
            (staged.maturing_days, staged.mature_days),
            (4, 8),
            "by stage"
        );
        let older: Seasoning = serde_json::from_str(r#"{"days":90,"share":0.5}"#).unwrap();
        assert_eq!(older, Seasoning::single(90, 0.5), "the single step");
        // And a reading round-trips through its own serialisation.
        let again: Seasoning =
            serde_json::from_str(&serde_json::to_string(&staged).unwrap()).unwrap();
        assert_eq!(again, staged);
    }

    #[test]
    fn a_seasoned_unit_is_an_open_lot_old_enough_counted_once() {
        let day = crate::SECONDS_PER_DAY;
        let now = 400 * day;
        let lot = |unit, acquired_days: u64, disposed: Option<u64>| Lot {
            unit,
            quantity: 1,
            open: i64::from(disposed.is_none()),
            acquired_at: acquired_days,
            acquired_unix: acquired_days * day,
            disposed_at: disposed,
            disposed_unix: disposed.map(|at| at * day),
            from_mint: false,
        };
        let lots = vec![
            lot(1, 10, None),      // held 390 days
            lot(2, 300, None),     // held 100 days
            lot(3, 350, None),     // held 50 days: fresh
            lot(4, 10, Some(200)), // sold: not held
            lot(5, 310, None),     // held exactly 90 days
        ];
        assert_eq!(
            maturity_at(&lots, now, Seasoning::single(90, 0.5)),
            Matured {
                new: 1,
                maturing: 0,
                mature: 3
            }
        );
        assert_eq!(
            maturity_at(&lots, now, Seasoning::single(0, 0.5)).mature,
            4,
            "zero days: every unit held now"
        );
    }

    #[test]
    fn a_guarantee_needs_the_minimum_held_now_and_falls_to_first_come_below_it() {
        let policy = Allocation {
            gtd_min_held: 2,
            ..ladder()
        };
        assert_eq!(phase(&holding(40_000, 1), policy), Phase::Fcfs);
        assert_eq!(
            slots(&holding(40_000, 1), policy),
            Slots {
                gtd: 0,
                fcfs: 1 + (1 + 1 + 2)
            },
            "under the stake, the holding rung and the score's rungs, all first come"
        );
        assert_eq!(phase(&holding(0, 2), policy), Phase::Gtd);
        let leader = Counts {
            promoted: true,
            ..holding(0, 1)
        };
        assert_eq!(
            phase(&leader, policy),
            Phase::Gtd,
            "a promotion still outranks it"
        );
    }

    #[test]
    fn a_refused_trace_still_carries_every_row() {
        let bytes = bundle_with(&[0], vec![crowded()]);
        let bundle = Bundle::open(&bytes).unwrap();
        let trace = explain(
            &bundle,
            "0xidentity",
            records(&[0]),
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
            records(&[0, 1, 0]),
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
