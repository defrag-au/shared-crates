//! The frame codec, against frames captured from the live socket on 2026-09-27.
//!
//! The fixtures are bytes, not a model of bytes: they are what came off
//! `wss://stream-api.opensea.io` while subscribed to `collection:*`, so a change
//! in this crate that stops reading real frames fails here.

use opensea_stream::{
    EmptyPayload, EventFilter, EventType, Frame, FrameEvent, FrameHeader, Reply, ReplyStatus, Topic,
};

/// A real `order_invalidate` frame.
const ORDER_INVALIDATE: &str = include_str!("fixtures/order_invalidate.json");

/// A real `item_cancelled` frame.
const ITEM_CANCELLED: &str = include_str!("fixtures/item_cancelled.json");

#[test]
fn a_join_is_the_documented_five_element_array() {
    let frame = Frame::join(
        Topic::collection("stonkbrokers-434284142"),
        EventFilter::All,
        "1",
    );
    assert_eq!(
        frame.to_wire().unwrap(),
        r#"["1","1","collection:stonkbrokers-434284142","phx_join",{}]"#
    );
}

#[test]
fn a_filtered_join_carries_its_event_types_in_the_payload() {
    let frame = Frame::join(
        Topic::collection("stonkbrokers-434284142"),
        EventFilter::only([EventType::ItemSold, EventType::ItemCancelled]),
        "1",
    );
    assert_eq!(
        frame.to_wire().unwrap(),
        r#"["1","1","collection:stonkbrokers-434284142","phx_join",{"event_types":["item_sold","item_cancelled"]}]"#
    );
}

#[test]
fn a_join_names_itself_as_its_own_join_ref() {
    let frame = Frame::join(Topic::collection("x"), EventFilter::All, "7");
    assert_eq!(frame.join_ref.as_deref(), Some("7"));
    assert_eq!(frame.reference.as_deref(), Some("7"));
}

#[test]
fn a_leave_names_the_join_it_ends() {
    let frame = Frame::leave(Topic::collection("x"), "1", "2");
    assert_eq!(
        frame.to_wire().unwrap(),
        r#"["1","2","collection:x","phx_leave",{}]"#
    );
}

#[test]
fn a_heartbeat_rides_the_system_topic_with_no_join_ref() {
    let frame = Frame::heartbeat("2");
    assert_eq!(frame.topic, Topic::System);
    assert_eq!(frame.join_ref, None);
    assert_eq!(
        frame.to_wire().unwrap(),
        r#"[null,"2","phoenix","heartbeat",{}]"#
    );
}

#[test]
fn a_filtered_join_round_trips_through_its_wire_form() {
    let original = Frame::join(
        Topic::collection("stonkbrokers-434284142"),
        EventFilter::only([EventType::ItemListed]),
        "1",
    );
    let wire = original.to_wire().unwrap();
    let decoded = Frame::<EventFilter>::from_wire(&wire).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn an_unfiltered_join_round_trips_as_all() {
    let original = Frame::join(Topic::collection("x"), EventFilter::All, "1");
    let wire = original.to_wire().unwrap();
    let decoded = Frame::<EventFilter>::from_wire(&wire).unwrap();
    assert_eq!(decoded.payload, EventFilter::All);
    assert_eq!(decoded, original);
}

#[test]
fn a_leave_round_trips_through_its_wire_form() {
    let original = Frame::leave(Topic::collection("x"), "1", "2");
    let wire = original.to_wire().unwrap();
    let decoded = Frame::<EmptyPayload>::from_wire(&wire).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn a_captured_frame_routes_without_the_reader_parsing_its_body() {
    let header = FrameHeader::from_wire(ORDER_INVALIDATE).unwrap();
    assert_eq!(header.join_ref.as_deref(), Some("1"));
    assert_eq!(header.reference, None);
    assert_eq!(header.topic, Topic::AllCollections);
    assert_eq!(header.event, FrameEvent::Event(EventType::OrderInvalidate));
}

#[test]
fn the_second_captured_frame_routes_to_its_own_event() {
    let header = FrameHeader::from_wire(ITEM_CANCELLED).unwrap();
    assert_eq!(header.topic, Topic::AllCollections);
    assert_eq!(header.event, FrameEvent::Event(EventType::ItemCancelled));
}

#[test]
fn an_unknown_event_type_is_carried_rather_than_rejected() {
    let raw = r#"["1",null,"collection:x","item_something_new",{"a":1}]"#;
    let header = FrameHeader::from_wire(raw).unwrap();
    assert_eq!(
        header.event,
        FrameEvent::Event(EventType::Unknown("item_something_new".to_owned()))
    );
}

#[test]
fn a_phx_reply_parses_its_status() {
    let raw = r#"["1","1","collection:x","phx_reply",{"status":"ok","response":{}}]"#;
    let frame = Frame::<Reply>::from_wire(raw).unwrap();
    assert_eq!(frame.topic, Topic::collection("x"));
    assert_eq!(frame.event, FrameEvent::Reply);
    assert_eq!(frame.payload.status, ReplyStatus::Ok);
    assert_eq!(frame.payload.response.reason, None);
}

#[test]
fn an_error_reply_keeps_its_reason() {
    let raw = r#"["1","2","collection:x","phx_reply",{"status":"error","response":{"reason":"unmatched topic"}}]"#;
    let frame = Frame::<Reply>::from_wire(raw).unwrap();
    assert_eq!(frame.payload.status, ReplyStatus::Error);
    assert_eq!(
        frame.payload.response.reason.as_deref(),
        Some("unmatched topic")
    );
}

#[test]
fn a_frame_missing_its_payload_is_an_error() {
    let raw = r#"["1","1","collection:x","phx_join"]"#;
    assert!(FrameHeader::from_wire(raw).is_err());
}
