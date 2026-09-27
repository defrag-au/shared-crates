//! The metadata event, decoded from a frame captured off the live socket on
//! 2026-09-27.

use opensea_stream::{EventFilter, EventType, Frame, StreamEvent, Topic};

/// A real `item_metadata_updated` frame, carrying both an image and five traits.
const ITEM_METADATA_UPDATED: &str = include_str!("fixtures/item_metadata_updated.json");

#[test]
fn a_captured_metadata_update_decodes_with_its_image_and_traits() {
    let event = StreamEvent::from_wire(ITEM_METADATA_UPDATED)
        .unwrap()
        .unwrap();
    let StreamEvent::ItemMetadataUpdated(update) = event else {
        panic!("expected a metadata update");
    };

    assert_eq!(
        update.item.nft_id,
        "abstract/0x99bb83ae9bb0c0a6be865cacf67760947f91cb70/29804427"
    );
    assert_eq!(update.item.chain.name, "abstract");
    assert_eq!(update.collection.slug, "objekt-2");
    assert_eq!(
        update.item.metadata.name.as_deref(),
        Some("Summer26 TaeIn 103Z #29804427")
    );
    assert_eq!(
        update.item.metadata.background_color.as_deref(),
        Some("#619AFF")
    );

    let image = update.item.metadata.image_url.as_deref().unwrap();
    assert!(image.ends_with(".webp"), "got {image}");
}

#[test]
fn the_traits_arrive_as_the_complete_set() {
    let event = StreamEvent::from_wire(ITEM_METADATA_UPDATED)
        .unwrap()
        .unwrap();
    let StreamEvent::ItemMetadataUpdated(update) = event else {
        panic!("expected a metadata update");
    };

    let traits: Vec<(&str, &str)> = update
        .item
        .metadata
        .traits
        .iter()
        .map(|entry| (entry.trait_type.as_str(), entry.value.as_str()))
        .collect();

    assert_eq!(
        traits,
        vec![
            ("Artist", "idntt"),
            ("Class", "Basic"),
            ("Member", "TaeIn"),
            ("Season", "Summer26"),
            ("Collection", "103Z"),
        ]
    );
}

#[test]
fn a_metadata_update_carries_no_timestamp_of_its_own() {
    // A fact about the wire rather than about the model, so it is asserted against
    // the captured bytes: this event family has no `event_timestamp`, where a
    // movement does, leaving the envelope's `sent_at` as the only timestamp.
    assert!(ITEM_METADATA_UPDATED.contains("\"sent_at\""));
    assert!(!ITEM_METADATA_UPDATED.contains("event_timestamp"));
}

#[test]
fn the_three_types_an_ownership_ledger_wants_are_what_a_capture_subscribes_to() {
    let frame = Frame::join(
        Topic::collection("madjacket-rh"),
        EventFilter::only([
            EventType::ItemTransferred,
            EventType::ItemSold,
            EventType::ItemMetadataUpdated,
        ]),
        "1",
    );
    assert_eq!(
        frame.to_wire().unwrap(),
        r#"["1","1","collection:madjacket-rh","phx_join",{"event_types":["item_transferred","item_sold","item_metadata_updated"]}]"#
    );
}
