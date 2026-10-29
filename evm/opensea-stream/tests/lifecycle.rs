//! The order lifecycle: a cancellation and an invalidation, from real frames.
//!
//! These are the two events that let a market ledger stop believing a listing exists.
//! They are also the two that showed why the vocabulary needs an `order_invalidated`
//! distinct from `delisted`: `item_cancelled` carries a price and a maker, and
//! `order_invalidate` carries neither — not even which side of the book the order was.

use opensea_stream::{EventType, StreamEvent};

/// A real `item_cancelled` frame, off the live socket on 2026-09-27.
const ITEM_CANCELLED: &str = include_str!("fixtures/item_cancelled.json");

/// A real `order_invalidate` frame, off the live socket on 2026-09-27.
const ORDER_INVALIDATE: &str = include_str!("fixtures/order_invalidate.json");

#[test]
fn a_cancellation_decodes_with_its_price_and_order() {
    let event = StreamEvent::from_wire(ITEM_CANCELLED).unwrap().unwrap();
    let StreamEvent::ItemCancelled(cancelled) = event else {
        panic!("expected a cancellation");
    };

    assert_eq!(cancelled.chain, "robinhood");
    assert_eq!(cancelled.collection.slug, "quotrons404");
    // The price is a decimal string on the wire and a `u128` here; the whole point of
    // modelling it is that a delist can be priced without the earlier listing.
    assert_eq!(cancelled.base_price, 1_470_000_000_000_000_000);
    assert_eq!(cancelled.payment_token.symbol, "WETH");
    assert_eq!(cancelled.payment_token.decimals, 18);
    assert_eq!(
        cancelled.maker.address,
        "0x738dec8877eedf8c4a0f23e03fa1df4657ef6998"
    );
    assert_eq!(
        cancelled.order_hash,
        "0x4aeaa2b25dfed843afefb267c2de26cca7a99dd9fdd1779a460f58e51f146cba"
    );
}

#[test]
fn a_cancellation_carries_the_order_it_ended_and_when_it_would_have_lapsed() {
    let event = StreamEvent::from_wire(ITEM_CANCELLED).unwrap().unwrap();
    let StreamEvent::ItemCancelled(cancelled) = event else {
        panic!("expected a cancellation");
    };

    assert_eq!(cancelled.quantity, 1);
    assert_eq!(
        cancelled.listing_date.as_deref(),
        Some("2026-09-27T01:07:35.000000Z")
    );
    assert_eq!(
        cancelled.expiration_date.as_deref(),
        Some("2026-09-27T01:07:35.000000Z")
    );
    // A public listing, and it never settled — which is what makes this a cancellation
    // rather than the sale that is its own message.
    assert!(cancelled.taker.is_none());
    assert!(cancelled.transaction.is_none());
}

#[test]
fn an_invalidation_names_the_order_but_not_its_side() {
    let event = StreamEvent::from_wire(ORDER_INVALIDATE).unwrap().unwrap();
    let StreamEvent::OrderInvalidate(invalidated) = event else {
        panic!("expected an invalidation");
    };

    assert_eq!(invalidated.chain, "robinhood");
    assert_eq!(invalidated.collection.slug, "button-presser");
    assert_eq!(
        invalidated.order_hash,
        "0x31e1f1f738927715f09d294f8c90b8320c7e5bdf5728a9660053219f7d31e2cc"
    );
    assert_eq!(
        invalidated.item.as_ref().map(|item| item.nft_id.as_str()),
        Some("robinhood/0xe5143de9d3ccbc31ffb4e7fc66d8320e0e2693d2/3147")
    );
    // There is no price, no maker and no listing type on the payload. That absence is
    // the reason `MarketKind::OrderInvalidated` exists rather than a guess at whether
    // this was a listing or a bid.
}

#[test]
fn both_lifecycle_types_are_recognised_by_their_own_names() {
    let cancelled = StreamEvent::from_wire(ITEM_CANCELLED).unwrap().unwrap();
    let invalidated = StreamEvent::from_wire(ORDER_INVALIDATE).unwrap().unwrap();

    assert!(matches!(cancelled, StreamEvent::ItemCancelled(_)));
    assert!(matches!(invalidated, StreamEvent::OrderInvalidate(_)));
    assert_eq!(EventType::ItemCancelled.as_wire(), "item_cancelled");
    assert_eq!(EventType::OrderInvalidate.as_wire(), "order_invalidate");
}
