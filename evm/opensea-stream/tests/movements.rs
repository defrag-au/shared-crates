//! The two movement events, decoded from frames captured off the live socket on
//! 2026-09-27 — the ones an ownership ledger wants.

use opensea_stream::{EventFilter, EventType, Frame, StreamEvent, Topic};

/// A real `item_transferred` frame.
const ITEM_TRANSFERRED: &str = include_str!("fixtures/item_transferred.json");

/// A real `item_sold` frame.
const ITEM_SOLD: &str = include_str!("fixtures/item_sold.json");

/// A real `item_cancelled` frame — an event this crate does not model.
const ITEM_CANCELLED: &str = include_str!("fixtures/item_cancelled.json");

#[test]
fn a_captured_transfer_decodes_into_a_typed_movement() {
    let event = StreamEvent::from_wire(ITEM_TRANSFERRED).unwrap().unwrap();
    let StreamEvent::ItemTransferred(movement) = event else {
        panic!("expected a transfer");
    };

    assert_eq!(movement.chain, "base");
    assert_eq!(movement.collection.slug, "slipstream-position-nft-v1-3");
    assert_eq!(
        movement.from_account.address,
        "0x6399ed6725cc163d019aa64ff55b22149d7179a8"
    );
    assert_eq!(
        movement.to_account.address,
        "0x61040e143a77f165ba44543af4a079f2c809d14b"
    );
    assert_eq!(
        movement.item.nft_id,
        "base/0x827922686190790b37229fd06084350e74485b72/76817133"
    );
    assert_eq!(movement.quantity, 1);
    assert_eq!(
        movement.transaction.as_ref().unwrap().hash,
        "0x6a637c92eea42feec77522dc98e57a1908dfa9caf41af341274d51a5507f3f1b"
    );
    assert_eq!(movement.transaction.as_ref().unwrap().timestamp, 1790472889);
}

#[test]
fn a_captured_sale_decodes_with_its_price_and_scale() {
    let event = StreamEvent::from_wire(ITEM_SOLD).unwrap().unwrap();
    let StreamEvent::ItemSold(sale) = event else {
        panic!("expected a sale");
    };

    assert_eq!(sale.chain, "ronin");
    assert_eq!(sale.collection.slug, "axie-ronin");
    assert_eq!(sale.sale_price, 227_000_000_000_000);
    assert_eq!(sale.payment_token.decimals, 18);
    assert_eq!(sale.payment_token.symbol, "WETH");
    assert_eq!(
        sale.maker.address,
        "0x3deaa70597437963c437788dc3fc9950c2ce1681"
    );
    assert_eq!(
        sale.taker.as_ref().unwrap().address,
        "0xd502f0842657e29dfadd6747c9f232f37b3128d7"
    );
    assert!(!sale.is_private);
}

#[test]
fn a_sale_that_did_not_settle_through_an_order_carries_no_order_hash() {
    // Both of these are real on the captured frame: an empty order hash and the
    // zero address as the settlement contract. A consumer that assumed a hash
    // would fail on exactly this sale.
    let event = StreamEvent::from_wire(ITEM_SOLD).unwrap().unwrap();
    let StreamEvent::ItemSold(sale) = event else {
        panic!("expected a sale");
    };

    assert_eq!(sale.order_hash, "");
    assert_eq!(
        sale.protocol_address,
        "0x0000000000000000000000000000000000000000"
    );
}

#[test]
fn the_metadata_block_is_tolerated_without_being_carried() {
    // The fixture carries a full `item.metadata` — name, image, traits. The body
    // deliberately does not model it, so this passing is the proof that an
    // unmodelled field does not break the parse.
    assert!(ITEM_TRANSFERRED.contains("\"traits\""));
    assert!(matches!(
        StreamEvent::from_wire(ITEM_TRANSFERRED).unwrap(),
        Some(StreamEvent::ItemTransferred(_))
    ));
}

#[test]
fn an_event_this_crate_does_not_model_is_kept_rather_than_dropped() {
    let event = StreamEvent::from_wire(ITEM_CANCELLED).unwrap().unwrap();
    let StreamEvent::Unmodelled(unmodelled) = event else {
        panic!("expected an unmodelled event");
    };

    assert_eq!(unmodelled.event_type, EventType::ItemCancelled);
    assert_eq!(unmodelled.version, 2);
    assert_eq!(unmodelled.sent_at, "2026-09-27T01:07:37.101000Z");
}

#[test]
fn a_protocol_frame_is_not_an_event() {
    let raw = r#"["1","1","collection:x","phx_reply",{"status":"ok","response":{}}]"#;
    assert_eq!(StreamEvent::from_wire(raw).unwrap(), None);
}

#[test]
fn the_two_movement_types_are_what_a_capture_subscribes_to() {
    let frame = Frame::join(
        Topic::collection("stonkbrokers-434284142"),
        EventFilter::only([EventType::ItemTransferred, EventType::ItemSold]),
        "1",
    );
    assert_eq!(
        frame.to_wire().unwrap(),
        r#"["1","1","collection:stonkbrokers-434284142","phx_join",{"event_types":["item_transferred","item_sold"]}]"#
    );
}
