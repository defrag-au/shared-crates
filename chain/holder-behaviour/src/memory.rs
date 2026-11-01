//! An in-memory [`Store`], for tests and for a replay that does not need to persist.
//!
//! Not a cache and not a production backend: it exists so the fold can be exercised
//! without a database, and so the replay tool can answer "what would this stream earn"
//! in one pass. A Durable Object implements the same trait over SQLite.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};

use chain_events::{AccountRef, Currency, Price};
use chains::TokenRef;

use crate::state::{
    BookChange, BookOutcome, Fill, FloorPoint, FloorSeries, Holding, LotId, MarketHistory,
    MarketSide, MintFact, MintSummary, NewFill, NewLot, Placement, Slot, Store, SubjectId, UnitId,
};

/// A live ask, kept in the shape the floor and a fill's age read it.
#[derive(Debug, Clone)]
struct BookAsk {
    asset: TokenRef,
    price: Price,
    seller: Option<AccountRef>,
    unix: u64,
}

/// A [`Store`] that keeps everything in hash maps.
#[derive(Debug, Default)]
pub struct MemoryStore {
    accounts: HashMap<AccountRef, SubjectId>,
    subjects_by_id: Vec<AccountRef>,
    tokens: HashMap<TokenRef, UnitId>,
    units: Vec<TokenRef>,
    lots: BTreeMap<LotId, Holding>,
    by_subject: HashMap<SubjectId, Vec<LotId>>,
    by_subject_unit: HashMap<(SubjectId, UnitId), Vec<LotId>>,
    traits: HashMap<UnitId, Vec<(String, String)>>,
    /// The collection's mint records, keyed by unit.
    mints: HashMap<UnitId, MintFact>,
    mint_summary: Option<MintSummary>,
    /// The live ask book, keyed the same way.
    asks: HashMap<String, BookAsk>,
    /// The scale the floor is expressed at, once a native ask has set it.
    floor_decimals: Option<u8>,
    /// The collection's floor, one entry per change.
    floor: Vec<FloorPoint>,
    /// What each subject did on a marketplace.
    market: HashMap<SubjectId, MarketHistory>,
    /// How many holder movements each subject was a party to, for the ratio predicates.
    movements: HashMap<SubjectId, u64>,
    /// Whether the stream carried the market family at all.
    saw_market: bool,
    next_lot: u32,
    /// How many times `open_lots_of` has been asked. Used by the test that pins "one lot
    /// read per subject per evaluation" rather than trusting the comment.
    lot_reads: Cell<usize>,
}

impl MemoryStore {
    /// Records trait values for an asset, as a metadata pass would.
    pub fn seed_traits(&mut self, unit: UnitId, traits: &[(&str, &str)]) {
        self.traits.insert(
            unit,
            traits
                .iter()
                .map(|(category, value)| ((*category).to_string(), (*value).to_string()))
                .collect(),
        );
    }

    /// How many lot reads the store has served.
    pub fn lot_reads(&self) -> usize {
        self.lot_reads.get()
    }
}

impl Store for MemoryStore {
    type Error = std::convert::Infallible;

    fn subject(&mut self, account: &AccountRef) -> Result<SubjectId, Self::Error> {
        if let Some(existing) = self.accounts.get(account) {
            return Ok(*existing);
        }
        let id = SubjectId(self.accounts.len() as u32);
        self.accounts.insert(account.clone(), id);
        self.subjects_by_id.push(account.clone());
        Ok(id)
    }

    fn unit(&mut self, token: &TokenRef) -> Result<UnitId, Self::Error> {
        if let Some(existing) = self.tokens.get(token) {
            return Ok(*existing);
        }
        let id = UnitId(self.units.len() as u32);
        self.tokens.insert(token.clone(), id);
        self.units.push(token.clone());
        Ok(id)
    }

    fn token_of(&self, unit: UnitId) -> Result<Option<TokenRef>, Self::Error> {
        Ok(self.units.get(unit.0 as usize).cloned())
    }

    fn account_of(&self, subject: SubjectId) -> Result<Option<AccountRef>, Self::Error> {
        Ok(self.subjects_by_id.get(subject.0 as usize).cloned())
    }

    fn open_lot(&mut self, lot: NewLot) -> Result<Holding, Self::Error> {
        let id = LotId(self.next_lot);
        self.next_lot += 1;
        let holding = Holding {
            lot: id,
            subject: lot.subject,
            unit: lot.unit,
            quantity: lot.quantity,
            open: lot.quantity,
            acquired_at: lot.acquired_at,
            acquired_unix: lot.acquired_unix,
            acquired_by: lot.acquired_by,
            acquired_tx: lot.acquired_tx,
            disposed_at: None,
            disposed_unix: None,
            disposed_tx: None,
        };
        self.lots.insert(id, holding.clone());
        self.by_subject.entry(lot.subject).or_default().push(id);
        self.by_subject_unit
            .entry((lot.subject, lot.unit))
            .or_default()
            .push(id);
        Ok(holding)
    }

    fn save_lot(&mut self, lot: &Holding) -> Result<(), Self::Error> {
        self.lots.insert(lot.lot, lot.clone());
        Ok(())
    }

    fn open_lots_of(&self, subject: SubjectId, unit: UnitId) -> Result<Vec<Holding>, Self::Error> {
        self.lot_reads.set(self.lot_reads.get() + 1);
        let mut lots: Vec<Holding> = self
            .by_subject_unit
            .get(&(subject, unit))
            .into_iter()
            .flatten()
            .filter_map(|id| self.lots.get(id))
            .filter(|lot| lot.is_open())
            .cloned()
            .collect();
        // Oldest first is the closing order the fold depends on, so the store sorts
        // rather than returning insertion order.
        lots.sort_by_key(|lot| (lot.acquired_at, lot.lot));
        Ok(lots)
    }

    fn lots(&self, subject: SubjectId) -> Result<Vec<Holding>, Self::Error> {
        self.lot_reads.set(self.lot_reads.get() + 1);
        let mut lots: Vec<Holding> = self
            .by_subject
            .get(&subject)
            .into_iter()
            .flatten()
            .filter_map(|id| self.lots.get(id))
            .cloned()
            .collect();
        lots.sort_by_key(|lot| lot.lot);
        Ok(lots)
    }

    fn traits_of(&self, unit: UnitId) -> Result<Vec<(String, String)>, Self::Error> {
        Ok(self.traits.get(&unit).cloned().unwrap_or_default())
    }

    fn record_mint(
        &mut self,
        unit: UnitId,
        slot: Slot,
        unix: u64,
    ) -> Result<MintFact, Self::Error> {
        if let Some(existing) = self.mints.get(&unit) {
            return Ok(*existing);
        }
        let fact = MintFact {
            slot,
            unix,
            rank: self.mint_summary.map_or(1, |summary| summary.count + 1),
            previous_slot: self.mint_summary.map(|summary| summary.last_slot),
            previous_unix: self.mint_summary.map(|summary| summary.last_unix),
        };
        self.mints.insert(unit, fact);
        self.mint_summary = Some(match self.mint_summary {
            Some(summary) => MintSummary {
                last_slot: slot,
                last_unix: unix,
                count: summary.count + 1,
                ..summary
            },
            None => MintSummary {
                first_slot: slot,
                first_unix: unix,
                last_slot: slot,
                last_unix: unix,
                count: 1,
            },
        });
        Ok(fact)
    }

    fn mint_of(&self, unit: UnitId) -> Result<Option<MintFact>, Self::Error> {
        Ok(self.mints.get(&unit).copied())
    }

    fn mint_summary(&self) -> Result<Option<MintSummary>, Self::Error> {
        Ok(self.mint_summary)
    }

    fn observe_market(&mut self) -> Result<(), Self::Error> {
        self.saw_market = true;
        Ok(())
    }

    fn apply_book(
        &mut self,
        change: BookChange<'_>,
        at: Option<Slot>,
        unix: u64,
    ) -> Result<BookOutcome, Self::Error> {
        // Both are read before the change: a fill removes the ask it settled against, so the
        // floor afterwards is not the floor the sale was made against.
        let (floor, rising) = self.floor_now();
        let order_age = match change {
            BookChange::Placed { order, ask } => {
                self.asks.entry(order.to_string()).or_insert(BookAsk {
                    asset: ask.asset,
                    price: ask.price,
                    seller: ask.seller,
                    unix,
                });
                None
            }
            BookChange::Repriced { order, price } => {
                if let Some(ask) = self.asks.get_mut(order) {
                    ask.price = price;
                }
                None
            }
            BookChange::Closed { order } => {
                self.asks.remove(order);
                None
            }
            BookChange::Filled {
                order,
                asset,
                seller,
                settled,
            } => {
                let key = order
                    .filter(|order| self.asks.contains_key(*order))
                    .map(str::to_string)
                    .or_else(|| self.match_ask(asset, seller, settled));
                key.and_then(|key| self.asks.remove(&key))
                    .map(|ask| unix.saturating_sub(ask.unix))
            }
        };
        self.refresh_floor(at, unix);
        Ok(BookOutcome {
            order_age,
            floor,
            rising,
        })
    }

    fn record_movement(&mut self, subject: SubjectId) -> Result<(), Self::Error> {
        *self.movements.entry(subject).or_default() += 1;
        Ok(())
    }

    fn movements_of(&self, subject: SubjectId) -> Result<Option<u64>, Self::Error> {
        Ok(Some(
            self.movements.get(&subject).copied().unwrap_or_default(),
        ))
    }

    fn record_placement(
        &mut self,
        subject: SubjectId,
        order: &str,
        side: MarketSide,
        at: Option<Slot>,
        unix: u64,
    ) -> Result<(), Self::Error> {
        let history = self.market.entry(subject).or_default();
        // A relist reuses the order identity; it is the same listing, not a second one.
        if !history
            .placements
            .iter()
            .any(|placement| placement.order == order)
        {
            history.placements.push(Placement {
                order: order.to_string(),
                side,
                at,
                unix,
            });
        }
        Ok(())
    }

    fn record_fill(&mut self, subject: SubjectId, fill: NewFill) -> Result<(), Self::Error> {
        // A fill that did not change the ask book — a bid taken, or an ask whose seller the
        // event did not name — arrives without a floor, because the fold captures it from the
        // book change. The sale is still judged against the floor that stood, so the store
        // fills it in; only a book with no native ask leaves it `None`.
        let (floor, floor_rising) = match fill.floor {
            Some(floor) => (Some(floor), fill.floor_rising),
            None => self.floor_now(),
        };
        self.market.entry(subject).or_default().fills.push(Fill {
            order: fill.order,
            side: fill.side,
            role: fill.role,
            price: fill.price,
            at: fill.at,
            unix: fill.unix,
            order_age: fill.order_age,
            floor,
            floor_rising,
        });
        Ok(())
    }

    fn market_history(&self, subject: SubjectId) -> Result<Option<MarketHistory>, Self::Error> {
        if !self.saw_market {
            return Ok(None);
        }
        Ok(Some(self.market.get(&subject).cloned().unwrap_or_default()))
    }

    fn floor_series(&self) -> Result<Option<FloorSeries>, Self::Error> {
        if !self.saw_market {
            return Ok(None);
        }
        Ok(Some(FloorSeries {
            decimals: self.floor_decimals.unwrap_or(0),
            points: self.floor.clone(),
        }))
    }

    fn subjects(&self) -> Result<Vec<SubjectId>, Self::Error> {
        Ok((0..self.subjects_by_id.len() as u32)
            .map(SubjectId)
            .collect())
    }
}

impl MemoryStore {
    /// The floor and its direction as the book stood before the next change.
    ///
    /// Read from the tail of the series rather than recomputed, so it agrees with what
    /// [`Store::floor_series`] would answer for that instant. `None` for the floor means the
    /// book held no native ask; `None` for the direction means there is no earlier floor to
    /// compare with, or the book emptied in between — a refill from nothing is not a rise.
    fn floor_now(&self) -> (Option<Price>, Option<bool>) {
        let Some(decimals) = self.floor_decimals else {
            return (None, None);
        };
        let current = self.floor.last().and_then(|point| point.floor);
        let previous = self
            .floor
            .len()
            .checked_sub(2)
            .and_then(|index| self.floor[index].floor);
        let floor = current.map(|amount| Price::native(amount, decimals));
        let rising = match (current, previous) {
            (Some(current), Some(previous)) => Some(current > previous),
            _ => None,
        };
        (floor, rising)
    }

    /// The live ask a fill consumed, matched on the asset and seller it names.
    ///
    /// A settled price narrows it first; the oldest ask breaks a tie, so a seller with two
    /// live listings on one asset is consumed in the order they were placed.
    fn match_ask(
        &self,
        asset: Option<&TokenRef>,
        seller: &AccountRef,
        settled: Option<&Price>,
    ) -> Option<String> {
        let asset = asset?;
        let candidates: Vec<(&String, &BookAsk)> = self
            .asks
            .iter()
            .filter(|(_, ask)| &ask.asset == asset && ask.seller.as_ref() == Some(seller))
            .collect();
        if let Some(settled) = settled
            && let Some((order, _)) = candidates.iter().find(|(_, ask)| {
                ask.price.amount == settled.amount && ask.price.decimals == settled.decimals
            })
        {
            return Some((*order).clone());
        }
        candidates
            .into_iter()
            .min_by_key(|(order, ask)| (ask.unix, order.as_str()))
            .map(|(order, _)| order.clone())
    }

    /// Recomputes the floor and appends an observation when it changed.
    ///
    /// The series is denominated in the first native ask's scale. An ask in another
    /// currency is not part of the native floor, and is kept in the book only so a fill can
    /// still be matched to it.
    fn refresh_floor(&mut self, at: Option<Slot>, unix: u64) {
        if self.floor_decimals.is_none() {
            self.floor_decimals = self
                .asks
                .values()
                .find(|ask| matches!(ask.price.currency, Currency::Native))
                .map(|ask| ask.price.decimals);
        }
        let floor = self.floor_decimals.and_then(|decimals| {
            self.asks
                .values()
                .filter(|ask| {
                    matches!(ask.price.currency, Currency::Native) && ask.price.decimals == decimals
                })
                .map(|ask| ask.price.amount)
                .min()
        });
        if self.floor.last().is_none_or(|last| last.floor != floor) {
            self.floor.push(FloorPoint { at, unix, floor });
        }
    }
}

/// Small, explicit histories the tests share.
///
/// Built through the same `ChainEvent` shape a producer emits — not through a shortcut —
/// so a change to the envelope breaks these the same way it breaks a replay.
#[cfg(test)]
pub(crate) mod fixtures {
    use chain_events::{
        AccountRef, ChainCoord, ChainEvent, Coverage, EventBody, EventSource, HolderEvent,
        HolderKind, MarketEvent, MarketParty, Order, OrderRef, PartyDelta, PartyRole, Price, Side,
        TxRef,
    };
    use chains::{AssetType, ChainRef, TokenRef};
    use std::str::FromStr;

    use super::MemoryStore;
    use crate::state::{Store, SubjectId};

    /// The chain every fixture is on.
    pub(crate) fn cardano() -> ChainRef {
        ChainRef::from_str("cardano:mainnet").unwrap()
    }

    /// The Perps collection, as CAIP-19 without a token half.
    pub(crate) fn perps() -> AssetType {
        AssetType::new(cardano(), "cnt", &"e6ba9c0f".repeat(8)).unwrap()
    }

    /// One asset of the collection. Names are hex, as the chain carries them.
    pub(crate) fn perp(name: &str) -> TokenRef {
        TokenRef::new(perps(), Some(name)).unwrap()
    }

    pub(crate) fn account(address: &str) -> AccountRef {
        AccountRef::new(cardano(), address)
    }

    /// A holder event, with the coordinates a ledger source would state.
    pub(crate) fn event(
        asset: &TokenRef,
        kind: HolderKind,
        parties: &[(&str, i64)],
        height: u64,
    ) -> ChainEvent {
        let parties: Vec<PartyDelta> = parties
            .iter()
            .map(|(address, delta)| PartyDelta {
                account: account(address),
                delta: *delta,
            })
            .collect();
        ChainEvent::draft(
            cardano(),
            perps(),
            EventBody::Holder(HolderEvent { kind, parties }),
            1_700_000_000 + height,
            EventSource::PolicyArchive,
        )
        .asset(asset.clone())
        .at(ChainCoord::new(height))
        .tx(TxRef::new("ab".repeat(32), 0))
        .coverage(Coverage::Complete)
        .build("fixture", 1_700_000_000)
    }

    pub(crate) fn mint(asset: &TokenRef, to: &str, height: u64) -> ChainEvent {
        event(asset, HolderKind::Mint, &[(to, 1)], height)
    }

    pub(crate) fn transfer(asset: &TokenRef, from: &str, to: &str, height: u64) -> ChainEvent {
        event(asset, HolderKind::Transfer, &[(from, -1), (to, 1)], height)
    }

    /// A marketplace placement, with the price and parties a real listing carries.
    pub(crate) fn placed(
        order: &str,
        side: Side,
        parties: &[(&str, PartyRole)],
        height: u64,
    ) -> ChainEvent {
        placed_at(order, side, Price::native(1_000_000, 6), parties, height)
    }

    /// A marketplace placement at `price`.
    pub(crate) fn placed_at(
        order: &str,
        side: Side,
        price: Price,
        parties: &[(&str, PartyRole)],
        height: u64,
    ) -> ChainEvent {
        let parties: Vec<MarketParty> = parties
            .iter()
            .map(|(address, role)| MarketParty {
                role: *role,
                account: account(address),
            })
            .collect();
        ChainEvent::draft(
            cardano(),
            perps(),
            EventBody::Market(MarketEvent::Placed {
                order: OrderRef::new(order),
                side,
                price,
                parties,
            }),
            1_700_000_000 + height,
            EventSource::MarketplaceDecode,
        )
        .asset(perp("5065727031"))
        .at(ChainCoord::new(height))
        .coverage(Coverage::Complete)
        .build("fixture", 1_700_000_000)
    }

    /// A re-price of a live order.
    pub(crate) fn updated(order: &str, price: Price, height: u64) -> ChainEvent {
        ChainEvent::draft(
            cardano(),
            perps(),
            EventBody::Market(MarketEvent::Updated {
                order: OrderRef::new(order),
                price,
            }),
            1_700_000_000 + height,
            EventSource::MarketplaceDecode,
        )
        .asset(perp("5065727031"))
        .at(ChainCoord::new(height))
        .coverage(Coverage::Complete)
        .build("fixture", 1_700_000_000)
    }

    /// A settled trade. `order` names the order, or `None` for a source that did not.
    pub(crate) fn filled(
        order: Option<&str>,
        side: Side,
        parties: &[(&str, PartyRole)],
        height: u64,
    ) -> ChainEvent {
        filled_at(
            order,
            side,
            Some(Price::native(2_000_000, 6)),
            parties,
            height,
        )
    }

    /// A settled trade at `price`, or unpriced when `price` is `None`.
    pub(crate) fn filled_at(
        order: Option<&str>,
        side: Side,
        price: Option<Price>,
        parties: &[(&str, PartyRole)],
        height: u64,
    ) -> ChainEvent {
        let parties: Vec<MarketParty> = parties
            .iter()
            .map(|(address, role)| MarketParty {
                role: *role,
                account: account(address),
            })
            .collect();
        ChainEvent::draft(
            cardano(),
            perps(),
            EventBody::Market(MarketEvent::Filled {
                order: match order {
                    Some(id) => Order::Named(OrderRef::new(id)),
                    None => Order::Unknown,
                },
                side,
                price,
                parties,
            }),
            1_700_000_000 + height,
            EventSource::MarketplaceDecode,
        )
        .asset(perp("5065727031"))
        .at(ChainCoord::new(height))
        .coverage(Coverage::Complete)
        .build("fixture", 1_700_000_000)
    }

    /// Alice mints one asset at slot 100 and keeps it.
    pub(crate) fn alice_mints_one() -> (MemoryStore, SubjectId) {
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        store.apply(&mint(&asset, "addr1alice", 100));
        let subject = store.subject(&account("addr1alice")).unwrap();
        (store, subject)
    }

    /// Alice mints two different assets at slot 100 and keeps both.
    pub(crate) fn alice_mints_two() -> (MemoryStore, SubjectId) {
        let mut store = MemoryStore::default();
        store.apply(&mint(&perp("5065727031"), "addr1alice", 100));
        store.apply(&mint(&perp("5065727032"), "addr1alice", 100));
        let subject = store.subject(&account("addr1alice")).unwrap();
        (store, subject)
    }

    /// Alice mints at 100 and sells to Bob at 200.
    pub(crate) fn alice_mints_and_sells(store: &mut MemoryStore) -> (TokenRef, SubjectId) {
        let asset = perp("5065727031");
        store.apply(&mint(&asset, "addr1alice", 100));
        store.apply(&transfer(&asset, "addr1alice", "addr1bob", 200));
        let alice = store.subject(&account("addr1alice")).unwrap();
        (asset, alice)
    }

    /// Alice mints at 100, sells to Bob at 200, and buys it back at 300.
    pub(crate) fn round_trip(store: &mut MemoryStore) -> (Option<TokenRef>, SubjectId, SubjectId) {
        let asset = perp("5065727031");
        store.apply(&mint(&asset, "addr1alice", 100));
        store.apply(&transfer(&asset, "addr1alice", "addr1bob", 200));
        store.apply(&transfer(&asset, "addr1bob", "addr1alice", 300));
        let alice = store.subject(&account("addr1alice")).unwrap();
        let bob = store.subject(&account("addr1bob")).unwrap();
        (Some(asset), alice, bob)
    }

    /// A four-event history over two assets and two parties.
    pub(crate) fn history() -> Vec<ChainEvent> {
        let one = perp("5065727031");
        let two = perp("5065727032");
        vec![
            mint(&one, "addr1alice", 100),
            mint(&two, "addr1alice", 110),
            transfer(&two, "addr1alice", "addr1bob", 200),
            transfer(&one, "addr1alice", "addr1bob", 300),
        ]
    }

    /// A small extension so the fixture helpers read like the events they build.
    trait ApplyFixture {
        fn apply(&mut self, event: &ChainEvent);
    }

    impl ApplyFixture for MemoryStore {
        fn apply(&mut self, event: &ChainEvent) {
            crate::fold::apply(self, event).expect("the in-memory store cannot fail");
        }
    }
}
