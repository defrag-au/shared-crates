//! Applying facts: one event in, the lots it changed out.
//!
//! # The contract
//!
//! [`apply`] is **ordered and unconditional**: it applies the event it is given, in the
//! order it is given, and does not consult a cursor. Applying the same event twice
//! doubles it. Idempotency belongs to whoever owns the stream — a Durable Object's
//! `last_emission_id`, an `event-merge` dedupe, a replay that reads a file once — because
//! only that caller can say what "already seen" means for its source. A fold that
//! remembered event ids would grow without bound and still be wrong about a re-org.
//!
//! # What a movement means
//!
//! A [`HolderEvent`] is a party list with signed deltas, and a party list can decline to
//! name a sender (a fee split, a batched fill). The fold does not need a sender: a
//! positive delta opens a lot for that party and a negative delta closes one, so a split
//! credits the recipients and debits the seller exactly as the ledger states, with the
//! ambiguity carried in the shape rather than guessed away.
//!
//! # Anomalies, not errors
//!
//! A disposal with no open lot to close means the stream started after the acquisition,
//! or arrived out of order. Neither is a reason to abandon a 34,000-event replay, and
//! neither is a number worth inventing: it becomes an [`Anomaly`] the report counts. A
//! non-zero count is a failure the caller can see, which is the point.

use chain_events::{
    ChainEvent, EventBody, Family, HolderEvent, HolderKind, MarketEvent, OfferScope, PartyRole,
    Side,
};

use crate::state::{
    AcquiredBy, Ask, BookChange, BookOutcome, MarketSide, NewFill, NewLot, Slot, Store, SubjectId,
    UnitId,
};

/// Something the fold could not place.
///
/// Carries the CAIP-19 asset and the bech32 account rather than an interned id, because
/// this is read by a human debugging a stream and an id means nothing outside the store
/// that issued it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anomaly {
    /// Units left a party the fold never saw acquire them.
    ///
    /// `short` is how many could not be closed — not how much the event moved. On a
    /// complete stream this is zero; a non-zero count means the walk began mid-history or
    /// the events arrived out of order.
    DisposedWithoutAcquisition {
        /// The party whose balance went down.
        account: String,
        /// The asset, as CAIP-19.
        asset: String,
        /// How many units had no lot to come out of.
        short: i64,
    },
    /// A movement with no chain coordinate, so it cannot be placed in time.
    NoCoordinate {
        /// The asset, as CAIP-19.
        asset: String,
        /// The party whose balance changed.
        account: String,
    },
    /// A holder fact that names a collection and no asset.
    ///
    /// Not a shape the translators emit, and not one this fold can score: a holding is
    /// a fact about one unit.
    CollectionScoped,
}

/// What one event changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// Every subject whose lots changed, deduplicated, in first-seen party order.
    ///
    /// Empty means the event told the fold nothing it stores — a market fact, an asset
    /// metadata change, or a holder event it had to refuse. The caller uses this to
    /// decide whose standings to re-evaluate.
    pub subjects: Vec<SubjectId>,
    /// Anything that could not be placed. Empty on a complete, ordered stream.
    pub anomalies: Vec<Anomaly>,
}

impl Applied {
    fn touch(&mut self, subject: SubjectId) {
        if !self.subjects.contains(&subject) {
            self.subjects.push(subject);
        }
    }
}

/// Applies one event.
///
/// See the module docs for the ordering and idempotency contract. Nothing is applied for
/// a market, asset or admin fact: those are stored by someone whose job that is, and this
/// fold only records what an achievement reads.
pub fn apply<S: Store>(store: &mut S, event: &ChainEvent) -> Result<Applied, S::Error> {
    let mut applied = Applied::default();
    match &event.body {
        EventBody::Holder(holder) => apply_holder(store, event, holder, &mut applied)?,
        EventBody::Market(market) => apply_market(store, event, market, &mut applied)?,
        EventBody::Asset(_) | EventBody::Admin(_) => {}
    }
    Ok(applied)
}

/// Applies one marketplace fact.
///
/// Placements and fills are recorded for their parties, and **asks** also enter the book the
/// collection's floor is read from. A fill can decline to name its order, so the book is
/// matched on the asset and seller the fill names; that match is what gives an unkeyed fill
/// an age. A re-price, a reopening, a withdrawal and an invalidation change the book but no
/// subject's facts.
fn apply_market<S: Store>(
    store: &mut S,
    event: &ChainEvent,
    market: &MarketEvent,
    applied: &mut Applied,
) -> Result<(), S::Error> {
    store.observe_market()?;
    let at = event.coord.map(|coord| coord.height);
    let unix = event.occurred_at;
    match market {
        MarketEvent::Placed {
            order,
            side,
            price,
            parties,
        } => {
            let side = market_side(side);
            if side == MarketSide::Ask
                && let Some(asset) = event.asset.as_ref()
            {
                let seller = parties
                    .iter()
                    .find(|party| party.role == PartyRole::Seller)
                    .map(|party| party.account.clone());
                store.apply_book(
                    BookChange::Placed {
                        order: &order.id,
                        ask: Ask {
                            asset: asset.clone(),
                            price: price.clone(),
                            seller,
                        },
                    },
                    at,
                    unix,
                )?;
            }
            for party in parties {
                let subject = store.subject(&party.account)?;
                store.record_placement(subject, &order.id, side, at, unix)?;
                applied.touch(subject);
            }
        }
        MarketEvent::Updated { order, price } => {
            store.apply_book(
                BookChange::Repriced {
                    order: &order.id,
                    price: price.clone(),
                },
                at,
                unix,
            )?;
        }
        MarketEvent::Reopened { .. } => {
            // A reopening restates neither the asset nor the price, so the book cannot
            // re-enter the order. Nothing in the Perps archive does this.
        }
        MarketEvent::Cancelled { order, .. } => {
            if let Some(order) = order.named() {
                store.apply_book(BookChange::Closed { order: &order.id }, at, unix)?;
            }
        }
        MarketEvent::Invalidated { order } => {
            store.apply_book(BookChange::Closed { order: &order.id }, at, unix)?;
        }
        MarketEvent::Filled {
            order,
            side,
            price,
            parties,
        } => {
            let side = market_side(side);
            let book = if side == MarketSide::Ask {
                match parties.iter().find(|party| party.role == PartyRole::Seller) {
                    Some(seller) => store.apply_book(
                        BookChange::Filled {
                            order: order.named().map(|order| order.id.as_str()),
                            asset: event.asset.as_ref(),
                            seller: &seller.account,
                            settled: price.as_ref(),
                        },
                        at,
                        unix,
                    )?,
                    // A fill with no seller named did not touch the book, so no part of it
                    // can be judged against the pre-fill floor.
                    None => BookOutcome::default(),
                }
            } else {
                BookOutcome::default()
            };
            let order = order.named().map(|order| order.id.clone());
            for party in parties {
                let subject = store.subject(&party.account)?;
                store.record_fill(
                    subject,
                    NewFill {
                        order: order.clone(),
                        side,
                        role: party.role,
                        price: price.clone(),
                        at,
                        unix,
                        order_age: book.order_age,
                        floor: book.floor.clone(),
                        floor_rising: book.rising,
                    },
                )?;
                applied.touch(subject);
            }
        }
    }
    Ok(())
}

fn market_side(side: &Side) -> MarketSide {
    match side {
        Side::Ask => MarketSide::Ask,
        Side::Bid(OfferScope::Item) => MarketSide::ItemBid,
        Side::Bid(OfferScope::Trait) => MarketSide::TraitBid,
        Side::Bid(OfferScope::Collection) => MarketSide::CollectionBid,
    }
}

fn apply_holder<S: Store>(
    store: &mut S,
    event: &ChainEvent,
    holder: &HolderEvent,
    applied: &mut Applied,
) -> Result<(), S::Error> {
    let Some(asset) = event.asset.as_ref() else {
        applied.anomalies.push(Anomaly::CollectionScoped);
        return Ok(());
    };
    let Some(slot) = event.coord.map(|coord| coord.height) else {
        // Without a height there is no interval, and a lot at height zero would sort as
        // genesis. One anomaly per party, so the report names who was affected.
        for party in holder.parties.iter().filter(|party| party.delta != 0) {
            applied.anomalies.push(Anomaly::NoCoordinate {
                asset: asset.to_string(),
                account: party.account.address.clone(),
            });
        }
        return Ok(());
    };

    let unit = store.unit(asset)?;
    if matches!(holder.kind, HolderKind::Mint) {
        // The collection's mint order is a fact about the unit, not the holder, and the
        // store assigns the rank so an incremental fold and a rebuild agree.
        store.record_mint(unit, slot, event.occurred_at)?;
    }
    let acquired_by = match holder.kind {
        HolderKind::Mint => AcquiredBy::Mint,
        // A transfer with no source (`SourceBelowFloor`) is still an arrival from
        // somewhere; it is not a mint, and `round_trips_at_least` relies on that.
        HolderKind::Transfer | HolderKind::Burn => AcquiredBy::Transfer,
    };

    for party in &holder.parties {
        if party.delta == 0 {
            continue;
        }
        let subject = store.subject(&party.account)?;
        applied.touch(subject);
        // One movement per party per event, which is the count `movement_ratio_at_least`
        // reads. A zero delta is not a movement for that party and is skipped above.
        store.record_movement(subject)?;

        if party.delta > 0 {
            // One lot per party, not one per unit: an NFT's delta is 1, so the shape is
            // the same, and a fungible's balance stays one row instead of a million.
            store.open_lot(NewLot {
                subject,
                unit,
                quantity: party.delta,
                acquired_at: slot,
                acquired_unix: event.occurred_at,
                acquired_by,
                acquired_tx: event.tx.clone(),
            })?;
        } else {
            let short = close(
                store,
                subject,
                unit,
                -party.delta,
                slot,
                event.occurred_at,
                event.tx.clone(),
            )?;
            if short > 0 {
                applied.anomalies.push(Anomaly::DisposedWithoutAcquisition {
                    account: party.account.address.clone(),
                    asset: asset.to_string(),
                    short,
                });
            }
        }
    }
    Ok(())
}

/// Closes up to `wanted` units from the subject's oldest open lots of `unit`.
///
/// Returns how many units had no lot to come out of. Oldest-first is the convention both
/// a holder and a tax lot understands, and it is the only order that keeps a lot's
/// `acquired_at` earlier than its disposal across repeated trims.
fn close<S: Store>(
    store: &mut S,
    subject: SubjectId,
    unit: UnitId,
    wanted: i64,
    slot: Slot,
    unix: u64,
    tx: Option<chain_events::TxRef>,
) -> Result<i64, S::Error> {
    let mut remaining = wanted;
    for mut lot in store.open_lots_of(subject, unit)? {
        if remaining == 0 {
            break;
        }
        let taking = remaining.min(lot.open);
        lot.open -= taking;
        if lot.open == 0 {
            lot.disposed_at = Some(slot);
            lot.disposed_unix = Some(unix);
            lot.disposed_tx = tx.clone();
        }
        store.save_lot(&lot)?;
        remaining -= taking;
    }
    Ok(remaining)
}

/// What a replay did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// How many events were read.
    pub events: u64,
    /// How many events changed a lot or a market fact. The rest were facts this fold does
    /// not store.
    pub changed: u64,
    /// Events by family, so a report can say what it ignored rather than implying it
    /// saw only holder facts.
    pub holder: u64,
    /// Market facts read and not stored (no predicate in this crate reads them yet).
    pub market: u64,
    /// Asset facts read and not stored.
    pub asset: u64,
    /// Admin facts read and not stored.
    pub admin: u64,
    /// How many subjects the store ended up holding.
    pub subjects: usize,
    /// Everything that could not be placed. Empty on a complete, ordered stream.
    pub anomalies: Vec<Anomaly>,
}

/// Folds a whole stream, in order.
///
/// The batch path and the live path run [`apply`] for every event; there is no second
/// implementation here, only a loop and a count. That is deliberate — a rebuild that
/// disagreed with the incremental fold would be a bug nobody could see.
pub fn rebuild<S: Store>(
    store: &mut S,
    events: impl IntoIterator<Item = ChainEvent>,
) -> Result<Report, S::Error> {
    let mut report = Report::default();
    for event in events {
        report.events += 1;
        match event.body.family() {
            Family::Holder => report.holder += 1,
            Family::Market => report.market += 1,
            Family::Asset => report.asset += 1,
            Family::Admin => report.admin += 1,
        }
        let applied = apply(store, &event)?;
        if !applied.subjects.is_empty() {
            report.changed += 1;
        }
        report.anomalies.extend(applied.anomalies);
    }
    report.subjects = store.subjects()?.len();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryStore;
    use crate::memory::fixtures::{account, cardano, event, mint, perp, perps, transfer};
    use chain_events::{EventBody, EventSource, HolderEvent};

    #[test]
    fn a_mint_opens_one_lot_for_the_recipient() {
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        let applied = apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();

        assert_eq!(applied.subjects.len(), 1);
        let lots = store.lots(SubjectId(0)).unwrap();
        assert_eq!(lots.len(), 1);
        assert_eq!(lots[0].quantity, 1);
        assert_eq!(lots[0].open, 1);
        assert_eq!(lots[0].acquired_by, AcquiredBy::Mint);
        assert_eq!(lots[0].acquired_at, 100);
    }

    #[test]
    fn a_transfer_closes_the_seller_and_opens_the_buyer() {
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        apply(&mut store, &mint(&asset, "addr1alice", 100)).unwrap();
        apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 200)).unwrap();

        let alice = store.subject(&account("addr1alice")).unwrap();
        let bob = store.subject(&account("addr1bob")).unwrap();

        let alice_lots = store.lots(alice).unwrap();
        assert_eq!(alice_lots[0].open, 0);
        assert_eq!(alice_lots[0].disposed_at, Some(200));
        // 100 seconds held: the interval arithmetic, not a count.
        assert_eq!(alice_lots[0].tenure_at(1_700_010_000), 100);

        let bob_lots = store.lots(bob).unwrap();
        assert_eq!(bob_lots[0].acquired_by, AcquiredBy::Transfer);
        assert_eq!(bob_lots[0].open, 1);
    }

    #[test]
    fn a_party_list_that_declines_a_sender_still_credits_every_gainer() {
        // A marketplace split: one seller, fee and royalty. `direction` refuses to name
        // a sender; the fold does not need it to be named.
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        apply(&mut store, &mint(&asset, "addr1seller", 100)).unwrap();
        let split = event(
            &asset,
            HolderKind::Transfer,
            &[("addr1seller", -1), ("addr1market", 1)],
            200,
        );
        let applied = apply(&mut store, &split).unwrap();

        assert_eq!(applied.subjects.len(), 2);
        assert!(applied.anomalies.is_empty());
        let seller = store.subject(&account("addr1seller")).unwrap();
        assert_eq!(store.lots(seller).unwrap()[0].open, 0);
    }

    #[test]
    fn a_disposal_with_nothing_to_close_is_an_anomaly_and_not_a_negative_balance() {
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        let applied = apply(&mut store, &transfer(&asset, "addr1alice", "addr1bob", 200)).unwrap();

        assert_eq!(
            applied.anomalies,
            vec![Anomaly::DisposedWithoutAcquisition {
                account: "addr1alice".into(),
                asset: asset.to_string(),
                short: 1,
            }]
        );
        // The buyer still gets the lot: a missing acquisition does not lose the arrival.
        let bob = store.subject(&account("addr1bob")).unwrap();
        assert_eq!(store.lots(bob).unwrap()[0].open, 1);
    }

    #[test]
    fn a_movement_with_no_height_is_stated_rather_than_stored_at_genesis() {
        let mut store = MemoryStore::default();
        let asset = perp("5065727031");
        let event = ChainEvent::draft(
            cardano(),
            perps(),
            EventBody::Holder(HolderEvent::transfer(
                account("addr1alice"),
                account("addr1bob"),
                1,
            )),
            1_700_000_000,
            EventSource::MarketplaceDecode,
        )
        .asset(asset.clone())
        .build("test", 1_700_000_000);

        let applied = apply(&mut store, &event).unwrap();
        assert_eq!(applied.anomalies.len(), 2);
        assert!(matches!(applied.anomalies[0], Anomaly::NoCoordinate { .. }));
        assert_eq!(store.subjects().unwrap().len(), 0);
    }

    #[test]
    fn a_fungible_delta_is_one_lot_with_a_quantity() {
        // The token path: 1,500,000 units must not become 1,500,000 rows.
        let mut store = MemoryStore::default();
        let coin = perp("5045525020434f494e");
        apply(
            &mut store,
            &event(&coin, HolderKind::Mint, &[("addr1alice", 1_500_000)], 100),
        )
        .unwrap();

        let alice = store.subject(&account("addr1alice")).unwrap();
        let lots = store.lots(alice).unwrap();
        assert_eq!(lots.len(), 1);
        assert_eq!(lots[0].quantity, 1_500_000);
        assert_eq!(lots[0].open, 1_500_000);
    }

    #[test]
    fn a_partial_disposal_leaves_the_interval_running() {
        let mut store = MemoryStore::default();
        let coin = perp("5045525020434f494e");
        apply(
            &mut store,
            &event(&coin, HolderKind::Mint, &[("addr1alice", 10)], 100),
        )
        .unwrap();
        apply(
            &mut store,
            &event(
                &coin,
                HolderKind::Transfer,
                &[("addr1alice", -4), ("addr1bob", 4)],
                400,
            ),
        )
        .unwrap();

        let alice = store.subject(&account("addr1alice")).unwrap();
        let lot = &store.lots(alice).unwrap()[0];
        assert_eq!(lot.open, 6);
        assert_eq!(lot.quantity, 10);
        // Still open: a partial trim does not end the interval, and does not restart it.
        assert_eq!(lot.disposed_at, None);
        assert_eq!(lot.tenure_at(1_700_001_000), 900);
    }

    #[test]
    fn a_placement_is_recorded_for_its_party_and_the_order_table() {
        use crate::memory::fixtures::placed;
        use chain_events::{PartyRole, Side};
        let mut store = MemoryStore::default();
        let applied = apply(
            &mut store,
            &placed("o1", Side::Ask, &[("addr1alice", PartyRole::Seller)], 100),
        )
        .unwrap();
        assert_eq!(applied.subjects.len(), 1);
        let alice = store.subject(&account("addr1alice")).unwrap();
        let history = store.market_history(alice).unwrap().expect("market seen");
        assert_eq!(history.placements.len(), 1);
        assert_eq!(history.placements[0].order, "o1");
        assert_eq!(history.placements[0].side, crate::state::MarketSide::Ask);
    }

    #[test]
    fn a_market_event_with_no_asset_is_not_an_anomaly() {
        // A collection bid is collection-scoped by construction; the holder fold's
        // "no asset" refusal does not apply to it.
        use crate::memory::fixtures::placed;
        use chain_events::{OfferScope, PartyRole, Side};
        let mut store = MemoryStore::default();
        let applied = apply(
            &mut store,
            &placed(
                "bid",
                Side::Bid(OfferScope::Collection),
                &[("addr1alice", PartyRole::Buyer)],
                100,
            ),
        )
        .unwrap();
        assert!(applied.anomalies.is_empty());
        assert_eq!(applied.subjects.len(), 1);
    }
}
