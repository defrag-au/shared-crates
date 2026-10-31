//! What a fold remembers, and the ten operations a backend has to provide.
//!
//! # Lots, not balances
//!
//! A holding is stored as an **acquisition** — a lot — rather than a balance, because
//! every question this crate answers is a question about a span of time. A balance would
//! have to be paired with a separate "since when" column that silently resets when a
//! second unit arrives, and the two predicates that matter most (`total_tenure_at_least`,
//! `held_since_mint`) are exactly the ones a reset would corrupt.
//!
//! A lot carries `quantity` and `open` so one row describes both a whole NFT and part of a
//! fungible position, but the **duration** a lot contributes is the length of its
//! interval and never its size. That is what keeps a tenure predicate from quietly
//! becoming a balance predicate: one position of 1,500,000 units held for a year is one
//! lot-year, the same as one NFT held for a year.
//!
//! # Facts, not rollups
//!
//! Deliberately no per-subject counter table. Everything an evaluator needs is a function
//! of the subject's lots, so a rollup would be a second answer to the same question and
//! the two would eventually disagree — and a rollup that drifts is invisible until an
//! allocation is wrong. `apply` reads the open lots of **one unit** (one or two rows) and
//! writes them back, so per-event cost does not depend on how much the subject holds.
//!
//! # One store, one collection
//!
//! Interning is per store and a store is scoped to one collection. That is what makes
//! `collections = ["perps"]` in a catalogue definition mean something: a predicate reading
//! `total_tenure_at_least` is asking about Perps specifically, and a store that had mixed
//! the token and the NFTs could not answer it.

use chain_events::{AccountRef, TxRef};
use chains::TokenRef;
use serde::{Deserialize, Serialize};

/// Chain height: a Cardano slot, an EVM block.
///
/// Seconds on Cardano (one slot is one second, verified against the archive), so a
/// duration in ticks is a duration in seconds there. Never assumed for another chain —
/// a predicate converts through [`crate::SECONDS_PER_DAY`] only where the chain is
/// known to keep time, which is why durations are stated in days on the predicate and
/// the runtime supplies the conversion.
pub type Slot = u64;

/// An interned subject within one collection's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SubjectId(pub u32);

/// An interned asset within one collection's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnitId(pub u32);

/// A store-assigned lot identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LotId(pub u32);

/// How a lot arrived. The distinction `round_trips_at_least` and `held_since_mint` read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquiredBy {
    /// Created in this event — the primary mint.
    Mint,
    /// Received from another party: a sale, or a wallet-to-wallet transfer. The fold
    /// cannot tell those apart without a market fact, and does not guess.
    Transfer,
}

/// A lot to be stored. The store assigns its [`LotId`] and hands back the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewLot {
    /// Who now holds it.
    pub subject: SubjectId,
    /// Which asset.
    pub unit: UnitId,
    /// How many units this acquisition covers. One for an NFT, any positive number
    /// for a fungible.
    pub quantity: i64,
    /// Chain height it arrived at.
    pub acquired_at: Slot,
    /// How it arrived.
    pub acquired_by: AcquiredBy,
    /// The transaction that delivered it, when the source named one.
    pub acquired_tx: Option<TxRef>,
}

/// One acquisition of one asset by one subject, and how much of it is left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holding {
    /// The store-assigned identity.
    pub lot: LotId,
    /// Who holds it.
    pub subject: SubjectId,
    /// Which asset.
    pub unit: UnitId,
    /// How many units were acquired.
    pub quantity: i64,
    /// How many are still held. `0` is fully disposed.
    pub open: i64,
    /// Chain height the lot arrived at — the start of its interval.
    pub acquired_at: Slot,
    /// How it arrived.
    pub acquired_by: AcquiredBy,
    /// The transaction that delivered it.
    pub acquired_tx: Option<TxRef>,
    /// When the lot finished leaving — the end of its interval.
    ///
    /// `None` while any of it is held. A partially trimmed lot stays open, and its
    /// interval keeps running: selling some of a position does not restart the clock on
    /// the rest.
    pub disposed_at: Option<Slot>,
    /// The transaction that took the last units out — what an earned event cites.
    pub disposed_tx: Option<TxRef>,
}

impl Holding {
    /// Whether any of it is still held.
    pub fn is_open(&self) -> bool {
        self.open > 0
    }

    /// Whether any of it has left.
    pub fn is_disposed(&self) -> bool {
        self.open < self.quantity
    }

    /// Whether the whole acquisition has left — a completed round trip, when it
    /// arrived from another party.
    pub fn is_closed(&self) -> bool {
        self.disposed_at.is_some()
    }

    /// Seconds this lot has contributed to its subject's tenure, as at `now`.
    ///
    /// The **length of the interval**, not quantity-weighted. One NFT held for a year and
    /// one position of 1,500,000 fungible units held for a year are both one lot-year —
    /// scaling by quantity would turn a tenure predicate into a balance predicate, which
    /// is the wealth proxy the design doc rules out. Summing across lots then means what
    /// "total tenure" should: two assets held a year is two asset-years.
    ///
    /// A disposal later than `now` — which only a source disagreeing with the caller's
    /// clock can produce — contributes nothing rather than wrapping.
    pub fn tenure_at(&self, now: Slot) -> u64 {
        self.disposed_at
            .unwrap_or(now)
            .saturating_sub(self.acquired_at)
    }
}

/// The ten operations a backend provides.
///
/// Small on purpose: each one is a shape SQLite answers with a point lookup, so the
/// Cloudflare Durable Object and the in-memory test store differ only in storage. No
/// method returns an iterator, because a backend that streams and a fold that scans
/// would have to agree on an order, and this trait does not impose one.
pub trait Store {
    /// The backend's own failure type. The fold never interprets it — it can only
    /// propagate — so it is the backend's to define.
    type Error: core::fmt::Debug;

    /// Interns an account. Idempotent: the same account always returns the same id.
    fn subject(&mut self, account: &AccountRef) -> Result<SubjectId, Self::Error>;

    /// Interns an asset. Idempotent.
    fn unit(&mut self, token: &TokenRef) -> Result<UnitId, Self::Error>;

    /// The asset an interned unit names, so a caller can print evidence.
    fn token_of(&self, unit: UnitId) -> Result<Option<TokenRef>, Self::Error>;

    /// The account an interned subject names, so a caller can print a standing.
    fn account_of(&self, subject: SubjectId) -> Result<Option<AccountRef>, Self::Error>;

    /// Stores a new lot and returns it with its assigned id.
    fn open_lot(&mut self, lot: NewLot) -> Result<Holding, Self::Error>;

    /// Writes back a lot whose `open`, `disposed_at` or `disposed_tx` changed.
    fn save_lot(&mut self, lot: &Holding) -> Result<(), Self::Error>;

    /// The lots of one unit for one subject that still hold something, oldest first.
    ///
    /// This is the read `apply` makes on every disposal. Oldest-first is the closing
    /// order the fold relies on, so a backend returns them sorted rather than leaving
    /// the fold to sort a list it did not index.
    fn open_lots_of(&self, subject: SubjectId, unit: UnitId) -> Result<Vec<Holding>, Self::Error>;

    /// Every lot of one subject, open and closed, in any order.
    ///
    /// Read once per subject at evaluation time, not per event.
    fn lots(&self, subject: SubjectId) -> Result<Vec<Holding>, Self::Error>;

    /// The trait values recorded for one asset, as `(category, value)` pairs.
    ///
    /// Empty means *no metadata was recorded* — a genuine answer for an asset whose
    /// metadata pass has not run, and distinct from an asset whose every category has a
    /// value. `has_all_traits` treats empty as a gap rather than as a miss.
    fn traits_of(&self, unit: UnitId) -> Result<Vec<(String, String)>, Self::Error>;

    /// Every subject the store has seen, in no particular order.
    ///
    /// This is the eligible population — every stake that ever held the collection, not
    /// the current holders — which is what a rarity denominator is measured over.
    fn subjects(&self) -> Result<Vec<SubjectId>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lot(quantity: i64, open: i64, acquired_at: Slot, disposed_at: Option<Slot>) -> Holding {
        Holding {
            lot: LotId(1),
            subject: SubjectId(1),
            unit: UnitId(1),
            quantity,
            open,
            acquired_at,
            acquired_by: AcquiredBy::Mint,
            acquired_tx: None,
            disposed_at,
            disposed_tx: None,
        }
    }

    #[test]
    fn an_open_lot_accrues_the_length_of_its_interval() {
        // One unit, acquired at 100, still held at 1,000: 900 seconds.
        assert_eq!(lot(1, 1, 100, None).tenure_at(1_000), 900);
    }

    #[test]
    fn a_closed_lot_stops_accruing_at_its_disposal() {
        // The clock no longer matters once nothing is held: the assertion is that a
        // closed lot does not keep growing.
        let closed = lot(1, 0, 100, Some(1_000));
        assert_eq!(closed.tenure_at(1_000), 900);
        assert_eq!(closed.tenure_at(50_000), 900);
    }

    #[test]
    fn a_fungible_position_accrues_its_interval_not_its_size() {
        // 1,500,000 units held 900 seconds is 900 seconds of tenure, not 1.35 billion.
        // Scaling by quantity would make a tenure predicate a balance predicate.
        assert_eq!(lot(1_500_000, 1_500_000, 100, None).tenure_at(1_000), 900);
    }

    #[test]
    fn a_partially_trimmed_lot_keeps_the_same_start() {
        // Selling four of ten at slot 400 does not restart the clock on the six that
        // stay, so the interval still begins at 100.
        let trimmed = lot(10, 6, 100, None);
        assert!(trimmed.is_disposed());
        assert!(trimmed.is_open());
        assert!(!trimmed.is_closed());
        assert_eq!(trimmed.tenure_at(1_000), 900);
    }

    #[test]
    fn a_disposal_after_the_reading_clock_does_not_wrap() {
        // A source that stamps a future height is a source disagreeing with the caller,
        // not a reason to produce a tenure near u64::MAX.
        assert_eq!(lot(1, 1, 1_000, None).tenure_at(500), 0);
    }
}
