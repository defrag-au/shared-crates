//! An in-memory [`Store`], for tests and for a replay that does not need to persist.
//!
//! Not a cache and not a production backend: it exists so the fold can be exercised
//! without a database, and so the replay tool can answer "what would this stream earn"
//! in one pass. A Durable Object implements the same trait over SQLite.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};

use chain_events::AccountRef;
use chains::TokenRef;

use crate::state::{Holding, LotId, NewLot, Store, SubjectId, UnitId};

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
            acquired_by: lot.acquired_by,
            acquired_tx: lot.acquired_tx,
            disposed_at: None,
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

    fn subjects(&self) -> Result<Vec<SubjectId>, Self::Error> {
        Ok((0..self.subjects_by_id.len() as u32)
            .map(SubjectId)
            .collect())
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
        HolderKind, PartyDelta, TxRef,
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
