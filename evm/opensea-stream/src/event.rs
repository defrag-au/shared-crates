//! The events a consumer acts on, and the envelope they arrive in.
//!
//! Three events are modelled, because they are the ones an ownership ledger and
//! the trait and image caches beside it want: [`ItemTransferred`], [`ItemSold`]
//! and [`ItemMetadataUpdated`]. The other seven are carried as
//! [`StreamEvent::Unmodelled`] rather than dropped, so a consumer can log or
//! forward them and a new event type is never fatal.

use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

use crate::event_type::EventType;
use crate::frame::{Frame, FrameEvent, FrameHeader};

/// The outer object every stream event arrives in.
///
/// The frame's event position and this `event_type` field carry the same value:
/// the position is what routes a frame, and this field is what a body parser keys
/// on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<B> {
    /// Which event this is.
    pub event_type: EventType,
    /// OpenSea's own marker, carried opaque. It is `2` on the marketplace events
    /// and a millisecond epoch on the movement events, so it is not a schema
    /// version and nothing should branch on it.
    pub version: u64,
    /// When OpenSea sent it, RFC 3339.
    pub sent_at: String,
    /// The event's body.
    pub payload: B,
}

/// The collection an event belongs to — the identity the stream itself routes on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionRef {
    /// OpenSea's slug for the collection.
    pub slug: String,
}

/// A wallet address as OpenSea writes it: lowercase hex, no chain prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// The address.
    pub address: String,
}

/// The asset an event is about.
///
/// `metadata` is deliberately not modelled. A movement is a change of ownership;
/// the name, image and traits are collection state, which a consumer that has
/// synced the collection already holds. Carrying them here would be a second,
/// lossier copy — and they are 73% of the chain's stream traffic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRef {
    /// `{chain}/{contract}/{tokenId}` — the composite id OpenSea uses.
    pub nft_id: String,
    /// The marketplace page for the token.
    pub permalink: String,
}

/// The on-chain transaction behind an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionRef {
    /// The transaction hash.
    pub hash: String,
    /// Unix seconds. A string on the wire, read here as a number.
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub timestamp: u64,
}

/// The token a sale settled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentToken {
    /// The token contract.
    pub address: String,
    /// The scale [`ItemSold::sale_price`] is expressed in — 18 for WETH.
    pub decimals: u8,
    /// The token's name.
    pub name: String,
    /// The token's ticker.
    pub symbol: String,
    /// The token's price in ETH, as a decimal string (`"0.9942781770581453"`).
    ///
    /// Kept as a string: it is a fraction, and parsing it to a float is the wrong
    /// place to lose precision.
    pub eth_price: String,
    /// The token's price in USD, as a decimal string.
    pub usd_price: String,
}

/// An asset moved between two accounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemTransferred {
    /// The chain, by OpenSea's slug (`base`, `robinhood`, …) — not CAIP-2.
    pub chain: String,
    /// Which collection.
    pub collection: CollectionRef,
    /// When it happened, RFC 3339. The stream's own docs name this as the field
    /// to order by, since delivery is not ordered.
    pub event_timestamp: String,
    /// Where it came from.
    pub from_account: Account,
    /// Where it went.
    pub to_account: Account,
    /// What moved.
    pub item: ItemRef,
    /// How many.
    pub quantity: u32,
    /// The transaction that moved it, when there was one.
    pub transaction: Option<TransactionRef>,
}

/// An asset sold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSold {
    /// The chain, by OpenSea's slug.
    pub chain: String,
    /// Which collection.
    pub collection: CollectionRef,
    /// When it happened, RFC 3339.
    pub event_timestamp: String,
    /// When the sale closed, RFC 3339.
    pub closing_date: String,
    /// Whether the sale was private.
    pub is_private: bool,
    /// What sold.
    pub item: ItemRef,
    /// `null` on a public sale.
    pub listing_type: Option<String>,
    /// The seller.
    pub maker: Account,
    /// The sale's order hash — but **not reliably one**. Observed as an empty
    /// string on a sale that settled with a zero `protocol_address`, so treat it
    /// as an identifier that may be absent rather than as a hash to parse.
    pub order_hash: String,
    /// What it settled in.
    pub payment_token: PaymentToken,
    /// The settlement contract, or the zero address when there was none.
    pub protocol_address: String,
    /// How many.
    pub quantity: u32,
    /// In the payment token's smallest unit, read from a string.
    ///
    /// Pair it with [`PaymentToken::decimals`] to make it mean something: at 18
    /// decimals `227000000000000` is 0.000227 WETH. It is a `u128` rather than a
    /// 256-bit type because wei-scale values pass `u64` at ~18.4 ether and no
    /// real price approaches `u128`'s 3.4e38.
    #[serde(with = "wasm_safe_serde::u128_required")]
    pub sale_price: u128,
    /// The buyer.
    pub taker: Option<Account>,
    /// The transaction that settled it, when there was one.
    pub transaction: Option<TransactionRef>,
}

/// A token's metadata as the stream reports it.
///
/// The stream sends the whole current metadata, never a diff — a change is only
/// visible by comparing against what you already held. That is also what makes a
/// trait *removal* representable: `traits` is the complete set, so a value that
/// has gone is simply absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemMetadata {
    /// The token's name.
    #[serde(default)]
    pub name: Option<String>,
    /// Long-form description, often multi-line.
    #[serde(default)]
    pub description: Option<String>,
    /// A hex colour, when the token declares one.
    #[serde(default)]
    pub background_color: Option<String>,
    /// Where the image lives.
    ///
    /// Content-addressed on OpenSea's CDN — the path carries a hash that changes
    /// when the image does, which is what makes a changed image a changed URL.
    #[serde(default)]
    pub image_url: Option<String>,
    /// An animated form, when there is one.
    #[serde(default)]
    pub animation_url: Option<String>,
    /// The token's own metadata document, when OpenSea has one. Frequently null.
    #[serde(default)]
    pub metadata_url: Option<String>,
    /// The complete trait set. Absent and empty mean the same thing.
    #[serde(default)]
    pub traits: Vec<Trait>,
}

/// One trait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trait {
    /// The trait's category, e.g. `Member`.
    pub trait_type: String,
    /// The value.
    ///
    /// A string: 4,142 values observed across the wildcard were all strings, so a
    /// numeric one would fail the parse loudly rather than be silently coerced.
    pub value: String,
}

/// The chain as an item names it: `{ "name": "abstract" }`.
///
/// OpenSea's own slug, not CAIP-2. A movement carries its chain at the payload
/// root instead — the two event families do not share a core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainName {
    /// The chain slug.
    pub name: String,
}

/// One token's metadata, as the stream reports it.
///
/// Note the shape: the payload holds only `item` and `collection`. There is no
/// diff, no `chain` at the root, and — unlike a movement — no `event_timestamp`,
/// so the envelope's `sent_at` is the only timestamp there is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemMetadataUpdated {
    /// The token, with its full current metadata.
    pub item: MetadataItem,
    /// Which collection.
    pub collection: CollectionRef,
}

/// The `item` of a metadata update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataItem {
    /// `{chain}/{contract}/{tokenId}`.
    pub nft_id: String,
    /// The marketplace page.
    pub permalink: String,
    /// The chain, by OpenSea's slug.
    pub chain: ChainName,
    /// The token's current metadata.
    pub metadata: ItemMetadata,
}

/// An event this crate does not model, kept rather than dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmodelledEvent {
    /// The event type, named or not.
    pub event_type: EventType,
    /// The envelope's opaque marker.
    pub version: u64,
    /// When OpenSea sent it, RFC 3339.
    pub sent_at: String,
}

/// What a stream frame turned out to be.
///
/// The payloads are heap-heavy — each is a dozen owned strings — and a capture
/// moves them at hundreds per second, so the two modelled bodies sit behind a
/// pointer and moving a variant costs a pointer copy. Field access through the
/// `Box` is unaffected, since it derefs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    /// The asset moved.
    ItemTransferred(Box<ItemTransferred>),
    /// The asset sold.
    ItemSold(Box<ItemSold>),
    /// The token's metadata, reported in full rather than as a diff.
    ItemMetadataUpdated(Box<ItemMetadataUpdated>),
    /// An event type this crate does not decode.
    Unmodelled(UnmodelledEvent),
}

impl StreamEvent {
    /// Decodes one frame into an event.
    ///
    /// `Ok(None)` means the frame is not an event at all — a join, a leave, a
    /// reply or a heartbeat — which a consumer reading a socket sees constantly
    /// and should ignore.
    ///
    /// The frame is read twice: once for its routing fields, to learn the event
    /// type without parsing a body, and once for the concrete body that type calls
    /// for. That keeps every body a real type — no `Value`, no untagged enum — and
    /// costs one extra parse of a frame that is usually a few hundred bytes.
    pub fn from_wire(raw: &str) -> Result<Option<Self>, serde_json::Error> {
        let header = FrameHeader::from_wire(raw)?;

        let FrameEvent::Event(event_type) = header.event else {
            return Ok(None);
        };

        match event_type {
            EventType::ItemTransferred => {
                let frame: Frame<EventEnvelope<ItemTransferred>> = Frame::from_wire(raw)?;
                Ok(Some(Self::ItemTransferred(Box::new(frame.payload.payload))))
            }
            EventType::ItemSold => {
                let frame: Frame<EventEnvelope<ItemSold>> = Frame::from_wire(raw)?;
                Ok(Some(Self::ItemSold(Box::new(frame.payload.payload))))
            }
            EventType::ItemMetadataUpdated => {
                let frame: Frame<EventEnvelope<ItemMetadataUpdated>> = Frame::from_wire(raw)?;
                Ok(Some(Self::ItemMetadataUpdated(Box::new(
                    frame.payload.payload,
                ))))
            }
            other => {
                let frame: Frame<EventEnvelope<IgnoredAny>> = Frame::from_wire(raw)?;
                Ok(Some(Self::Unmodelled(UnmodelledEvent {
                    event_type: other,
                    version: frame.payload.version,
                    sent_at: frame.payload.sent_at,
                })))
            }
        }
    }
}
