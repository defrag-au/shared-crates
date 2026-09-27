//! The OpenSea Stream API's wire protocol, as pure types.
//!
//! OpenSea's real-time feed is a Phoenix (Elixir) WebSocket at
//! `wss://stream-api.opensea.io/socket/websocket?vsn=2.0.0`. This crate is the
//! half of consuming it that needs no socket: the frame vocabulary, the topic and
//! event-type vocabulary, and the lifecycle messages. A consumer brings the
//! transport.
//!
//! # The frame is a positional array
//!
//! Every frame is a five-element JSON **array**, not an object:
//!
//! ```text
//! [join_ref, ref, topic, event, payload]
//! ```
//!
//! `event` is one of four protocol messages or an event type — see [`FrameEvent`].
//! A reply echoes the `ref` of the request that caused it; an event frame carries
//! its subscription's `join_ref` and a null `ref`. [`FrameHeader`] reads those
//! routing fields and skips the body, which is the cheap way to decide what a
//! frame is before paying to parse it.
//!
//! # Two filters, both chosen at subscription time
//!
//! OpenSea narrows what a socket receives on two axes, and both live in the join
//! ([`Frame::join`]):
//!
//! - **The topic** — which collection, or the wildcard. Measured: `collection:*`
//!   ran ~4,100 frames/s while six named topics ran 2.7/s.
//! - **The event types** — [`EventFilter`], the payload's `event_types`.
//!   Measured on one collection: excluding `item_received_bid` took it from 478
//!   frames per 20 s to 4, while the type kept was not reduced.
//!
//! So a watch set is a map from collection to event types, not a flat list. The
//! second axis is what makes a busy collection affordable — in one wildcard
//! sample, 73% of Robinhood Chain's stream traffic was `item_metadata_updated`,
//! which most consumers never act on, and a single collection (`up-position-nft`)
//! produced ~600 frames/s on its own.
//!
//! # Two ways to subscribe to nothing, both silent
//!
//! A wrong slug and a filter that matches nothing fail identically: the join
//! replies `ok` and no frame ever arrives, which is indistinguishable from a quiet
//! collection.
//!
//! - **The slug.** [`Topic::Collection`]'s slug is not the slug `/collections/{slug}`
//!   returns. Measured, same socket, same 120 s window:
//!
//!   | topic | join | frames |
//!   | --- | --- | --- |
//!   | `collection:stonkbrokers` | ok | 0 |
//!   | `collection:stonkbrokers-434284142` | ok | 41 |
//!
//!   The suffixed form is what `/chain/{chain}/contract/{address}` returns; the
//!   human slug is what `/collections/{slug}` returns.
//! - **The filter.** An `event_types` list naming a type the collection never
//!   emits delivers nothing, and the type name is not validated — a synthetic name
//!   joined `ok` and produced 0 frames.
//!
//! `ok` says the socket accepted the request. It never says the request will
//! produce anything.
//!
//! # Subscriptions are live, and a leave is scoped
//!
//! Joins and leaves are honoured on an open socket, and a leave affects only its
//! own topic: dropping one of four subscriptions stopped that topic after ~250 ms
//! of in-flight drain while the other three kept arriving. Delivery is best-effort,
//! arrives out of order, and **has no replay** — so the seam any subscription
//! change opens is a reconcile trigger, as is the reconnect a deploy forces.
//!
//! OpenSea's own raw-connection example joins `collection:` — an empty slug —
//! which errors. Only `collection:*` is the wildcard, and [`Topic`] rejects the
//! empty form for that reason.
//!
//! # Capturing events
//!
//! The pieces a capture needs are the join filter and [`StreamEvent::from_wire`]:
//!
//! ```text
//! subscribe:  collection:<slug>   {"event_types":["item_transferred","item_sold"]}
//!
//! for each frame the socket delivers:
//!     match StreamEvent::from_wire(&raw)? {
//!         Some(StreamEvent::ItemTransferred(movement)) => { /* ownership delta */ }
//!         Some(StreamEvent::ItemSold(sale))            => { /* ownership delta, priced */ }
//!         Some(StreamEvent::Unmodelled(event))         => { /* log, do not drop */ }
//!         None                                         => { /* not an event */ }
//!     }
//! ```
//!
//! What this crate does not carry is the socket. The transport is a separate,
//! runtime-specific concern — a Worker's WebSocket and a native client are
//! different bindings of the same frames.

mod endpoint;
mod event;
mod event_type;
mod filter;
mod frame;
mod strings;
mod topic;

pub use endpoint::endpoint_url;
pub use event::{
    Account, CollectionRef, EventEnvelope, ItemRef, ItemSold, ItemTransferred, PaymentToken,
    StreamEvent, TransactionRef, UnmodelledEvent,
};
pub use event_type::EventType;
pub use filter::EventFilter;
pub use frame::{
    EmptyPayload, FRAME_LEN, Frame, FrameEvent, FrameHeader, Reply, ReplyResponse, ReplyStatus,
};
pub use topic::{Topic, TopicParseError};
