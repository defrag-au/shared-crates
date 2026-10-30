//! The ten OpenSea messages, translated, against the committed frames.
//!
//! The fixtures live beside the protocol crate that captured them, and are read across
//! from here on purpose: one corpus, so a frame cannot be edited in one place and diverge
//! from the decoder tested in the other.

#![cfg(feature = "opensea")]

use std::str::FromStr;

use chains::{AssetType, ChainRef, TokenRef};
use opensea_stream::{EventStamp, StreamEvent};

use chain_events::{
    AccountRef, AssetKind, ChainEvent, Coverage, Currency, EventBody, EventId, EventSource,
    HolderEvent, MarketEvent, OfferScope, Order, PartyDelta, PartyRole, Side, TranslateError,
    TxRef, from_opensea,
};

const ITEM_TRANSFERRED: &str =
    include_str!("../../../evm/opensea-stream/tests/fixtures/item_transferred.json");
const ITEM_SOLD: &str = include_str!("../../../evm/opensea-stream/tests/fixtures/item_sold.json");
const ITEM_METADATA_UPDATED: &str =
    include_str!("../../../evm/opensea-stream/tests/fixtures/item_metadata_updated.json");
const ITEM_LISTED: &str =
    include_str!("../../../evm/opensea-stream/tests/fixtures/item_listed.json");
const ITEM_CANCELLED: &str =
    include_str!("../../../evm/opensea-stream/tests/fixtures/item_cancelled.json");
const ITEM_RECEIVED_BID: &str =
    include_str!("../../../evm/opensea-stream/tests/fixtures/item_received_bid.json");
const COLLECTION_OFFER: &str =
    include_str!("../../../evm/opensea-stream/tests/fixtures/collection_offer.json");
const TRAIT_OFFER: &str =
    include_str!("../../../evm/opensea-stream/tests/fixtures/trait_offer.json");
const ORDER_INVALIDATE: &str =
    include_str!("../../../evm/opensea-stream/tests/fixtures/order_invalidate.json");
const ORDER_REVALIDATE: &str =
    include_str!("../../../evm/opensea-stream/tests/fixtures/order_revalidate.json");

const RECORDED_AT: u64 = 1_800_000_000;

fn collection(chain: &str, contract: &str) -> AssetType {
    AssetType::new(ChainRef::from_str(chain).unwrap(), "erc721", contract).unwrap()
}

fn translate(raw: &str, collection: &AssetType) -> Vec<ChainEvent> {
    let stamp = EventStamp::from_wire(raw).unwrap().unwrap();
    let event = StreamEvent::from_wire(raw).unwrap().unwrap();
    from_opensea(&event, &stamp, collection, RECORDED_AT).unwrap()
}

fn translate_err(raw: &str, collection: &AssetType) -> TranslateError {
    let stamp = EventStamp::from_wire(raw).unwrap().unwrap();
    let event = StreamEvent::from_wire(raw).unwrap().unwrap();
    from_opensea(&event, &stamp, collection, RECORDED_AT).unwrap_err()
}

#[test]
fn a_transfer_becomes_one_movement_and_agrees_with_the_streams_own_clock() {
    let events = translate(
        ITEM_TRANSFERRED,
        &collection("eip155:8453", "0x827922686190790b37229fd06084350e74485b72"),
    );
    assert_eq!(events.len(), 1);

    let event = &events[0];
    assert_eq!(event.source, EventSource::OpenseaStream);
    assert_eq!(event.coverage, Coverage::Partial);
    // No block, and `None` rather than a zero that would sort as genesis.
    assert!(event.coord.is_none());
    assert_eq!(
        event.asset.as_ref().and_then(|asset| asset.token_id()),
        Some("76817133")
    );

    // The payload's own `transaction.timestamp` is 1790472889. The adapter parses
    // `event_timestamp` independently, so agreement is a real cross-check of the parser
    // rather than of two reads of the same field.
    assert_eq!(event.occurred_at, 1_790_472_889);

    let EventBody::Holder(movement) = &event.body else {
        panic!("expected a movement");
    };
    let chain = ChainRef::from_str("eip155:8453").unwrap();
    assert_eq!(
        movement.parties,
        vec![
            PartyDelta {
                account: AccountRef::new(chain, "0x6399ed6725cc163d019aa64ff55b22149d7179a8"),
                delta: -1,
            },
            PartyDelta {
                account: AccountRef::new(chain, "0x61040e143a77f165ba44543af4a079f2c809d14b"),
                delta: 1,
            },
        ]
    );
}

#[test]
fn a_sale_is_a_market_row_and_the_movement_it_implies() {
    let collection = collection("eip155:2020", "0x32950db2a7164ae833121501c797d79e7b79d74c");
    let events = translate(ITEM_SOLD, &collection);
    assert_eq!(events.len(), 2);

    let EventBody::Market(MarketEvent::Filled {
        order: sale_order,
        side,
        price,
        parties,
    }) = &events[0].body
    else {
        panic!("expected a fill");
    };
    // A sale takes a listing.
    assert_eq!(*side, Side::Ask);
    assert_eq!(price.as_ref().unwrap().amount, 227_000_000_000_000);
    assert_eq!(
        parties
            .iter()
            .map(|party| (party.role, party.account.address.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (
                PartyRole::Seller,
                "0x3deaa70597437963c437788dc3fc9950c2ce1681"
            ),
            (
                PartyRole::Buyer,
                "0xd502f0842657e29dfadd6747c9f232f37b3128d7"
            ),
        ]
    );
    // The capture's sale settled with an empty order hash and the zero settlement
    // contract, so the source names no order — a state, not a missing value.
    assert!(matches!(sale_order, Order::Unknown));
    assert!(sale_order.named().is_none());
    assert!(matches!(
        price.as_ref().unwrap().currency,
        Currency::Contract { .. }
    ));

    // The movement the sale implies, built from the same parties and transaction, is
    // the same fact the separate `item_transferred` message would carry — so the same
    // id, and the merge is an upsert rather than a double count.
    let EventBody::Holder(movement) = &events[1].body else {
        panic!("expected the implied movement");
    };
    let chain = ChainRef::from_str("eip155:2020").unwrap();
    let expected = HolderEvent::transfer(
        AccountRef::new(chain, "0x3deaa70597437963c437788dc3fc9950c2ce1681"),
        AccountRef::new(chain, "0xd502f0842657e29dfadd6747c9f232f37b3128d7"),
        1,
    );
    assert_eq!(&expected, movement);

    let asset = TokenRef::new(collection, Some("8566445")).unwrap();
    let tx = TxRef::new(
        "0xa506556307c1fe24cc9ed5af7981b7ea5b35f5350e671cc193003aae39ca9498",
        0,
    );
    let derived = EventId::derive(
        chain,
        &EventBody::Holder(expected.clone()),
        Some(&asset),
        Some(&tx),
        &expected.discriminator(),
    );
    assert_eq!(events[1].id, derived);
}

#[test]
fn a_metadata_update_fingerprints_the_document_under_the_envelope_time() {
    let events = translate(
        ITEM_METADATA_UPDATED,
        &collection("eip155:2741", "0x99bb83ae9bb0c0a6be865cacf67760947f91cb70"),
    );
    assert_eq!(events.len(), 1);

    // This event family has no `event_timestamp`; the envelope's `sent_at` is the only
    // time there is. 2026-09-27T04:26:32Z, which the transfer fixture's own clock
    // cross-checks (01:34:49Z that day is 1790472889).
    assert_eq!(events[0].occurred_at, 1_790_483_192);

    let EventBody::Asset(change) = &events[0].body else {
        panic!("expected an asset change");
    };
    assert_eq!(change.kind, AssetKind::Metadata);
    assert_eq!(change.hash_algo.as_str(), "image_metadata_v1");
    assert_eq!(change.after_hash.len(), 32);
    // The frame carries the whole document and no before-state.
    assert!(change.before_hash.is_none());
}

#[test]
fn a_cancellation_admits_no_side_and_keeps_its_price() {
    let events = translate(
        ITEM_CANCELLED,
        &collection("eip155:4663", "0x027aca2794e44f24950d81227dcd516ffbb49d6e"),
    );
    let EventBody::Market(MarketEvent::Cancelled {
        order: cancelled,
        side,
        price,
        parties,
    }) = &events[0].body
    else {
        panic!("expected a cancellation");
    };

    // The message does not say whether the order was a listing or a bid, so the side is
    // stated as absent rather than guessed.
    assert_eq!(*side, None);
    assert_eq!(price.as_ref().unwrap().amount, 1_470_000_000_000_000_000);
    assert_eq!(
        cancelled.named().map(|order| order.id.as_str()),
        Some("0x4aeaa2b25dfed843afefb267c2de26cca7a99dd9fdd1779a460f58e51f146cba")
    );
    assert!(parties.is_empty());
}

#[test]
fn a_listing_carries_its_ask_and_a_native_price_is_not_a_null_token() {
    let events = translate(
        ITEM_LISTED,
        &collection("eip155:80094", "0x88888888a9361f15aadbaca355a6b2938c6a674e"),
    );
    let EventBody::Market(MarketEvent::Placed {
        side,
        price,
        parties,
        ..
    }) = &events[0].body
    else {
        panic!("expected a listing");
    };

    assert_eq!(*side, Side::Ask);
    assert_eq!(price.amount, 88_956_480_000_000_000_000);
    // BERA arrives with the zero address, which is the stream's way of saying "the
    // chain's own coin".
    assert_eq!(price.currency, Currency::Native);
    assert_eq!(parties.len(), 1);
    assert_eq!(parties[0].role, PartyRole::Seller);
}

#[test]
fn a_bid_is_an_offer_whose_maker_is_the_buyer() {
    let events = translate(
        ITEM_RECEIVED_BID,
        &collection("eip155:1", "0xb8ea78fcacef50d41375e44e6814ebba36bb33c4"),
    );
    let EventBody::Market(MarketEvent::Placed { side, parties, .. }) = &events[0].body else {
        panic!("expected an offer");
    };

    assert_eq!(*side, Side::Bid(OfferScope::Item));
    assert_eq!(parties.len(), 1);
    // A bid's maker is the bidder, so it is the buying side of the eventual trade.
    assert_eq!(parties[0].role, PartyRole::Buyer);
    assert_eq!(
        parties[0].account.address.as_str(),
        "0x0d67de0f32fc67a483336011d2e8fe794fcd630d"
    );
}

#[test]
fn a_collection_offer_is_collection_scoped_and_names_no_asset() {
    let events = translate(
        COLLECTION_OFFER,
        &collection("eip155:1", "0x19875070bb70abbb31e248e48c9b617d9d14029e"),
    );

    assert!(events[0].asset.is_none());
    let EventBody::Market(MarketEvent::Placed { side, price, .. }) = &events[0].body else {
        panic!("expected an offer");
    };
    assert_eq!(*side, Side::Bid(OfferScope::Collection));
    assert_eq!(price.amount, 758_000_000_000_000_000);
}

#[test]
fn a_trait_offer_is_trait_scoped() {
    let events = translate(
        TRAIT_OFFER,
        &collection("eip155:1", "0x036721e5a769cc48b3189efbb9cce4471e8a48b1"),
    );

    assert!(events[0].asset.is_none());
    let EventBody::Market(MarketEvent::Placed { side, .. }) = &events[0].body else {
        panic!("expected an offer");
    };
    assert_eq!(*side, Side::Bid(OfferScope::Trait));
}

#[test]
fn an_invalidation_admits_no_side_and_no_price() {
    let events = translate(
        ORDER_INVALIDATE,
        &collection("eip155:4663", "0xe5143de9d3ccbc31ffb4e7fc66d8320e0e2693d2"),
    );
    let EventBody::Market(event) = &events[0].body else {
        panic!("expected a market fact");
    };

    // The message says an order stopped being valid and no more. That is its own
    // transition rather than a withdrawal with a caveat, so there is no price and no
    // side to carry — the shape cannot state them.
    assert_eq!(event.kind(), "invalidated");
    assert!(matches!(event, MarketEvent::Invalidated { .. }));
}

#[test]
fn a_revalidation_is_collection_scoped_because_the_frame_names_no_item() {
    // The watcher's collection is required by the signature even though this message
    // names no item — only its chain is checked.
    let events = translate(
        ORDER_REVALIDATE,
        &collection("eip155:4663", "0x0000000000000000000000000000000000000000"),
    );

    assert!(events[0].asset.is_none());
    let EventBody::Market(MarketEvent::Reopened { order }) = &events[0].body else {
        panic!("expected a reopened order");
    };
    assert_eq!(
        order.id,
        "0x56e64fad8c81401b8d7bca874d2c53394ab034ad68cd90f7e108592b47da0357"
    );
}

#[test]
fn a_message_for_another_chain_is_refused() {
    let error = translate_err(
        ITEM_TRANSFERRED,
        &collection("eip155:4663", "0x827922686190790b37229fd06084350e74485b72"),
    );
    assert!(matches!(error, TranslateError::ChainMismatch { .. }));
}

#[test]
fn a_message_for_another_contract_is_refused() {
    let error = translate_err(
        ITEM_TRANSFERRED,
        &collection("eip155:8453", "0x00000000000000000000000000000000000000aa"),
    );
    assert!(matches!(error, TranslateError::CollectionMismatch { .. }));
}

#[test]
fn a_type_this_crate_cannot_map_is_refused_by_name() {
    // A synthetic frame, because there is no capture of an event OpenSea has not
    // shipped. The point is that the refusal names the type rather than dropping it.
    let raw = r#"["1",null,"collection:*","item_something_new",{"event_type":"item_something_new","version":2,"sent_at":"2026-09-27T01:07:37.101000Z","payload":{}}]"#;
    let error = translate_err(
        raw,
        &collection("eip155:4663", "0x00000000000000000000000000000000000000aa"),
    );
    assert!(matches!(error, TranslateError::Unmodelled(_)));
}
