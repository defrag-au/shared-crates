//! What a fold remembers, and the operations a backend has to provide.
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
//!
//! # Market facts are placements and fills
//!
//! A marketplace fact is stored only where a predicate reads it: a [`Placement`] (who listed
//! or bid, and when) and a [`Fill`] (which side was taken, which side the subject was on,
//! and the order's age where both ends were seen). Everything else about an order's
//! lifecycle — a re-price, a reopening, a withdrawal — changes state no predicate reads, so
//! the fold ignores it.

use chain_events::{AccountRef, PartyRole, Price, TxRef};
use chains::TokenRef;
use serde::{Deserialize, Serialize};

/// Chain height: a Cardano slot, an EVM block.
///
/// Used for **order and citation** — a rank, an interval's start, the evidence an earned
/// event points at. It is not a clock: one Cardano slot happens to be one second, but an EVM
/// block is not, so every duration is measured on unix seconds instead (see
/// [`Holding::tenure_at`] and [`crate::Assessment::now_unix`]). That is why a lot carries
/// both axes rather than converting one into the other.
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
    /// Chain height it arrived at — ordering, ranks and evidence.
    pub acquired_at: Slot,
    /// Unix seconds it arrived at — the clock durations are measured on.
    ///
    /// A chain that does not keep time in its height (EVM block numbers) is the reason this
    /// is separate: a rank sorts by `acquired_at`, a tenure counts `acquired_unix`.
    pub acquired_unix: u64,
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
    /// Unix seconds the lot arrived at — the clock tenure is measured on.
    pub acquired_unix: u64,
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
    /// Unix seconds the lot finished leaving, beside [`Holding::disposed_at`].
    pub disposed_unix: Option<u64>,
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

    /// Seconds this lot has contributed to its subject's tenure, as at `now_unix`.
    ///
    /// The **length of the interval**, not quantity-weighted. One NFT held for a year and
    /// one position of 1,500,000 fungible units held for a year are both one lot-year —
    /// scaling by quantity would turn a tenure predicate into a balance predicate, which
    /// is the wealth proxy the design doc rules out. Summing across lots then means what
    /// "total tenure" should: two assets held a year is two asset-years.
    ///
    /// Measured on unix seconds, never on the height: a rank sorts by `acquired_at`, but a
    /// duration cannot, because an EVM block number is not a second.
    ///
    /// A disposal later than `now_unix` — which only a source disagreeing with the caller's
    /// clock can state — contributes nothing rather than wrapping.
    pub fn tenure_at(&self, now_unix: u64) -> u64 {
        let end = self.disposed_unix.unwrap_or(now_unix);
        end.saturating_sub(self.acquired_unix)
    }
}

/// The operations a backend provides.
///
/// Small on purpose: each one is a shape SQLite answers with a point lookup, so the
/// Cloudflare Durable Object and the in-memory test store differ only in storage. No
/// method returns an iterator, because a backend that streams and a fold that scans
/// would have to agree on an order, and this trait does not impose one.
/// The primary-mint record for one unit.
///
/// A fact about the **asset**, not the holder: every subject who ever held it sees the same
/// record. `rank` is the fold's order of mint events, which is chain order for an ordered
/// stream — so a rebuild and an incremental fold agree by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintFact {
    /// Chain height the unit was minted at — what a rank orders on.
    pub slot: Slot,
    /// Unix seconds it was minted at — what a window or a gap measures.
    pub unix: u64,
    /// 1-based position in the collection's mint order.
    pub rank: u32,
    /// The previous mint's height, or `None` when this was the collection's first.
    pub previous_slot: Option<Slot>,
    /// The previous mint's unix seconds, beside [`MintFact::previous_slot`].
    pub previous_unix: Option<u64>,
}

/// The collection's mint extent, as far as the fold has seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintSummary {
    /// The first mint's height.
    pub first_slot: Slot,
    /// The first mint's unix seconds — the origin `mint_within` measures from.
    pub first_unix: u64,
    /// The latest mint's height.
    pub last_slot: Slot,
    /// The latest mint's unix seconds.
    pub last_unix: u64,
    /// How many units have been minted.
    pub count: u32,
}

/// Which side of the market an order is on, reduced to what a predicate reads.
///
/// The wire model keeps a bid's [`OfferScope`](chain_events::OfferScope) inside its side;
/// a predicate asks about one of the four shapes by name, so the store holds the flat form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarketSide {
    /// A listing: an intent to sell one asset.
    Ask,
    /// A bid scoped to one asset.
    ItemBid,
    /// A bid scoped to a trait.
    TraitBid,
    /// A bid scoped to the whole collection.
    CollectionBid,
}

/// A listing or offer, as the fold first saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The marketplace's order identity.
    pub order: String,
    /// What the order asks or offers.
    pub side: MarketSide,
    /// Chain height it was placed at, when the source names one.
    pub at: Option<Slot>,
    /// Unix seconds it was placed at.
    pub unix: u64,
}

/// One settled trade, from one party's point of view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fill {
    /// The order that was taken, when the source named it.
    pub order: Option<String>,
    /// Which side was taken.
    pub side: MarketSide,
    /// Which side of the trade this party was on.
    pub role: PartyRole,
    /// What it settled for, when the source priced it.
    pub price: Option<Price>,
    /// Chain height it settled at, when the source names one.
    pub at: Option<Slot>,
    /// Unix seconds it settled at.
    pub unix: u64,
    /// How long the order had stood when it was taken.
    ///
    /// `None` when the source did not name the order on the fill and the book could not
    /// match one, or when the placement predates the walk. A predicate that needs an age
    /// reports the gap rather than treating an unkeyed fill as a fast or a stale one.
    pub order_age: Option<u64>,
    /// The floor **just before** this trade, when the book knew one.
    ///
    /// Captured at the trade rather than looked up later, because an ask fill consumes the
    /// ask it settled against: the post-fill floor can be higher than the one the sale was
    /// actually made against.
    pub floor: Option<Price>,
    /// Whether the floor's last move before this trade was up.
    pub floor_rising: Option<bool>,
}

/// A fill before the store has resolved the order it consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFill {
    /// The order, when the source named one.
    pub order: Option<String>,
    /// Which side was taken.
    pub side: MarketSide,
    /// Which side of the trade the subject was on.
    pub role: PartyRole,
    /// What it settled for, when the source priced it.
    pub price: Option<Price>,
    /// Chain height it settled at, when the source names one.
    pub at: Option<Slot>,
    /// Unix seconds it settled at.
    pub unix: u64,
    /// How long the consumed order had stood, when the store matched one.
    pub order_age: Option<u64>,
    /// The floor just before the trade, when the fold could capture one from a book change.
    ///
    /// `None` is "the book did not change for this trade" (or did not exist), and a store is
    /// expected to fill in the standing floor in that case — see [`Store::record_fill`].
    pub floor: Option<Price>,
    /// Whether the floor's last move before the trade was up.
    pub floor_rising: Option<bool>,
}

/// What a book change said about the trade that caused it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookOutcome {
    /// The age of the ask a fill consumed, when the book matched one.
    pub order_age: Option<u64>,
    /// The floor **just before** the change, in one price.
    ///
    /// A fill consumes the ask it settled against, so the floor after the change is not the
    /// floor the sale was judged by. Carrying it on the fill makes the comparison exact.
    pub floor: Option<Price>,
    /// Whether the floor's last move before the change was up.
    pub rising: Option<bool>,
}

/// A live listing, as the collection's floor reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// Which asset is for sale.
    pub asset: TokenRef,
    /// What it asks.
    pub price: Price,
    /// Who listed it, when the source names them.
    pub seller: Option<AccountRef>,
}

/// A change to the collection's live asks.
///
/// Only listings enter the book. A bid is a fact about a buyer, not about the floor, so the
/// fold records it in the subject's history and leaves the book alone.
#[derive(Debug, Clone)]
pub enum BookChange<'a> {
    /// An ask became live at the price it states.
    Placed {
        /// The marketplace's order identity.
        order: &'a str,
        /// The listing.
        ask: Ask,
    },
    /// A live ask's price changed.
    Repriced {
        /// The order being repriced.
        order: &'a str,
        /// What it asks now.
        price: Price,
    },
    /// An ask was withdrawn or invalidated.
    Closed {
        /// The order that ended.
        order: &'a str,
    },
    /// An ask was taken.
    ///
    /// A fill can decline to name its order, so the book is also matched on the asset and
    /// the seller the fill names, preferring the settled price when several asks fit.
    Filled {
        /// The order, when the source named one.
        order: Option<&'a str>,
        /// The asset that changed hands, when the event names it.
        asset: Option<&'a TokenRef>,
        /// The seller the fill names.
        seller: &'a AccountRef,
        /// What it settled for, when the source priced it.
        settled: Option<&'a Price>,
    },
}

/// One observation of the collection's floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloorPoint {
    /// Chain height the floor changed at, when the source gives one.
    pub at: Option<Slot>,
    /// Unix seconds it changed at.
    pub unix: u64,
    /// The lowest live ask in the series' denomination, or `None` when the book was empty.
    pub floor: Option<u128>,
}

/// One drawdown a holding span covered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drawdown {
    /// How far the floor fell from its peak in the span, in `0.0..=1.0`.
    pub fraction: f64,
    /// The trough's chain height, when the source gives one.
    pub at: Option<Slot>,
    /// Unix seconds of the trough.
    pub unix: u64,
}

/// The collection's native-coin floor over time.
///
/// A floor is the lowest **live ask**, so the series changes only when the book does. An
/// observation with `floor: None` is a book that had no live native ask at that instant —
/// distinct from no observation, which means the walk had not started.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FloorSeries {
    /// The scale `floor` amounts are expressed at.
    pub decimals: u8,
    /// Ascending by `unix`, one entry per change of floor.
    pub points: Vec<FloorPoint>,
}

impl FloorSeries {
    /// The floor as at `unix`: the last observation at or before it.
    pub fn at(&self, unix: u64) -> Option<FloorPoint> {
        let index = self.points.partition_point(|point| point.unix <= unix);
        index.checked_sub(1).map(|index| self.points[index])
    }

    /// The highest floor at or before `unix`.
    pub fn peak_through(&self, unix: u64) -> Option<u128> {
        self.points
            .iter()
            .take_while(|point| point.unix <= unix)
            .filter_map(|point| point.floor)
            .max()
    }

    /// Whether the floor's last move was up.
    ///
    /// `None` when there is no earlier floor to compare with, or when the book emptied in
    /// between — a refill from nothing is not a rise.
    pub fn rising_at(&self, unix: u64) -> Option<bool> {
        let index = self.points.partition_point(|point| point.unix <= unix);
        let current = self.points.get(index.checked_sub(1)?)?.floor?;
        let previous = self.points.get(index.checked_sub(2)?)?.floor?;
        Some(current > previous)
    }

    /// The deepest fall from a peak reached **inside** `[from, to]`.
    ///
    /// The floor at `from` seeds the peak, so a span that began at the top and fell counts;
    /// a peak that predates the span does not. `None` when the floor at `from` is unknown.
    pub fn max_drawdown(&self, from: u64, to: u64) -> Option<Drawdown> {
        let mut peak = self.at(from)?.floor?;
        let mut worst = Drawdown {
            fraction: 0.0,
            at: None,
            unix: from,
        };
        for point in self
            .points
            .iter()
            .filter(|point| point.unix > from && point.unix <= to)
        {
            let Some(floor) = point.floor else {
                continue;
            };
            if floor > peak {
                peak = floor;
                continue;
            }
            let fraction = 1.0 - floor as f64 / peak as f64;
            if fraction > worst.fraction {
                worst = Drawdown {
                    fraction,
                    at: point.at,
                    unix: point.unix,
                };
            }
        }
        Some(worst)
    }
}

/// Everything a subject did on a marketplace, as far as the store saw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarketHistory {
    /// Distinct orders the subject placed, oldest first.
    pub placements: Vec<Placement>,
    /// Trades the subject was a party to, oldest first.
    pub fills: Vec<Fill>,
}

/// What a fold remembers, and the operations a backend has to provide.
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

    /// Records a primary mint of `unit` at `slot`/`unix`, assigning its rank.
    ///
    /// The store assigns the rank because it owns the collection's mint order: a caller
    /// that computed it would need a second read and could disagree with a concurrent
    /// fold. Recording the same unit twice keeps the first record — a mint is a fact about
    /// an asset, not a counter to increment.
    fn record_mint(&mut self, unit: UnitId, slot: Slot, unix: u64)
    -> Result<MintFact, Self::Error>;

    /// The mint record for a unit, when this store saw the collection mint.
    fn mint_of(&self, unit: UnitId) -> Result<Option<MintFact>, Self::Error>;

    /// The collection's mint extent, or `None` when it minted into this store not at all.
    fn mint_summary(&self) -> Result<Option<MintSummary>, Self::Error>;

    /// Notes that the source carried the market family at all.
    ///
    /// Market history and a collection nobody traded look identical from the outside, so a
    /// store that was built from a holder-only stream must be able to say "I was not told",
    /// not "there was nothing".
    fn observe_market(&mut self) -> Result<(), Self::Error>;

    /// Applies a change to the collection's live asks.
    ///
    /// Returns what the change said about the trade that caused it: the age of the ask a
    /// fill consumed, which is what lets an unkeyed fill still answer
    /// `bought_escrow_within`, and the floor and its direction **before** the change, which
    /// is what a sale has to be judged against.
    fn apply_book(
        &mut self,
        change: BookChange<'_>,
        at: Option<Slot>,
        unix: u64,
    ) -> Result<BookOutcome, Self::Error>;

    /// Records that `subject` placed `order`. Idempotent per `(subject, order)`.
    fn record_placement(
        &mut self,
        subject: SubjectId,
        order: &str,
        side: MarketSide,
        at: Option<Slot>,
        unix: u64,
    ) -> Result<(), Self::Error>;

    /// Records that `subject` was a party to one holder movement.
    ///
    /// A counter, not a log: `movement_ratio_at_least` is the only reader, and the movement
    /// itself is already cited by the lot it opened or closed. Applying the same event twice
    /// counts it twice, the same contract every other `record_*` here has.
    fn record_movement(&mut self, subject: SubjectId) -> Result<(), Self::Error>;

    /// How many holder movements the subject has been a party to.
    ///
    /// `None` means this store does not carry movement counts — a bundle row does not — so a
    /// ratio predicate reports a gap rather than a zero.
    fn movements_of(&self, subject: SubjectId) -> Result<Option<u64>, Self::Error>;

    /// Records a settled trade for one party.
    ///
    /// A fill that changed the ask book carries the floor captured **before** that change,
    /// because the change may have consumed the very ask the sale was made against. A fill
    /// that changed nothing — a bid taken, or an ask whose seller the event did not name —
    /// arrives with no floor, and the store answers with the floor that stood, which it owns.
    /// Only a book with no native ask may leave it `None`.
    fn record_fill(&mut self, subject: SubjectId, fill: NewFill) -> Result<(), Self::Error>;

    /// The subject's market history.
    ///
    /// `None` means **this store carries no market source**, which every market predicate
    /// reports as `Unavailable` rather than as "no activity".
    fn market_history(&self, subject: SubjectId) -> Result<Option<MarketHistory>, Self::Error>;

    /// The collection's floor over time.
    ///
    /// `None` means this store was not built from a marketplace source. A `Some` series with
    /// no points means the source was seen but no listing ever entered the book.
    fn floor_series(&self) -> Result<Option<FloorSeries>, Self::Error>;

    /// Every subject the store has seen, in no particular order.
    ///
    /// This is the eligible population: every stake that ever held the collection, plus any
    /// that only ever traded it (a bidder who never acquired). Rarity is measured over it, so
    /// it is never just the current holders.
    fn subjects(&self) -> Result<Vec<SubjectId>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lot(quantity: i64, open: i64, acquired_at: Slot, disposed_at: Option<Slot>) -> Holding {
        // The unit tests exercise interval arithmetic, so the synthetic clock is the same
        // coordinate on both axes: one tick is one second.
        Holding {
            lot: LotId(1),
            subject: SubjectId(1),
            unit: UnitId(1),
            quantity,
            open,
            acquired_at,
            acquired_unix: acquired_at,
            acquired_by: AcquiredBy::Mint,
            acquired_tx: None,
            disposed_at,
            disposed_unix: disposed_at,
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

    fn series(points: &[(u64, Option<u128>)]) -> FloorSeries {
        FloorSeries {
            decimals: 6,
            points: points
                .iter()
                .map(|(unix, floor)| FloorPoint {
                    at: None,
                    unix: *unix,
                    floor: *floor,
                })
                .collect(),
        }
    }

    #[test]
    fn a_floor_is_the_last_observation_at_or_before_the_reading() {
        let floor = series(&[(100, Some(50)), (200, Some(40)), (300, None)]);
        assert_eq!(floor.at(50), None);
        assert_eq!(floor.at(150).unwrap().floor, Some(50));
        assert_eq!(floor.at(200).unwrap().floor, Some(40));
        assert_eq!(floor.at(400).unwrap().floor, None);
        assert_eq!(floor.peak_through(250), Some(50));
        assert_eq!(floor.peak_through(50), None);
    }

    #[test]
    fn a_drawdown_is_measured_from_a_peak_inside_the_span() {
        // 100 → 40 → 100. A span that begins at the top covers the fall; one that begins
        // in the trough does not.
        let floor = series(&[(0, Some(100)), (10, Some(40)), (20, Some(100))]);
        let covered = floor.max_drawdown(0, 15).expect("the floor is known at 0");
        assert!((covered.fraction - 0.6).abs() < 1e-9);
        assert_eq!(covered.unix, 10);

        let after = floor
            .max_drawdown(10, 25)
            .expect("the floor is known at 10");
        assert_eq!(after.fraction, 0.0);

        // The floor at 50 is the last observation, so the span is flat, not unknown.
        assert_eq!(floor.max_drawdown(50, 60).unwrap().fraction, 0.0);
        // Before the first observation there is no floor to measure from.
        let empty = series(&[(100, Some(100))]);
        assert!(empty.max_drawdown(50, 60).is_none());
    }

    #[test]
    fn a_floor_that_empties_is_not_a_rise() {
        let floor = series(&[(0, Some(100)), (10, None), (20, Some(90))]);
        assert_eq!(floor.rising_at(5), None); // no earlier floor to compare with
        assert_eq!(floor.rising_at(15), None); // the book emptied
        assert_eq!(floor.rising_at(25), None); // a refill from nothing is not a rise

        let rising = series(&[(0, Some(100)), (10, Some(120))]);
        assert_eq!(rising.rising_at(15), Some(true));
        let falling = series(&[(0, Some(120)), (10, Some(100))]);
        assert_eq!(falling.rising_at(15), Some(false));
    }
}
