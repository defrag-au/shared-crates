//! The events a consumer acts on, and the envelope they arrive in.
//!
//! **All ten types the stream delivers are modelled**, each against a captured frame. That
//! matters more than it sounds: the point of this crate is that a consumer never sees an
//! untyped body, and a payload written from documentation rather than from the socket is
//! how a decoder ends up agreeing with the docs and failing on the wire. Both
//! `collection_offer` and `order_revalidate` carry `"item": {}`, and a bid carries
//! `created_date` where a listing carries `listing_date` — none of which the guide says.
//!
//! [`StreamEvent::Unmodelled`] remains for an event type this crate has never seen, so a
//! type OpenSea adds later is carried and logged rather than dropped or fatal.

use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize};

use crate::event_type::EventType;
use crate::frame::{Frame, FrameEvent, FrameHeader};
use crate::topic::Topic;

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

/// A contract address as OpenSea writes it.
///
/// The same shape as [`Account`], kept separate because a contract is not a wallet and
/// a consumer that treated one as the other would be matching a maker against a
/// collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractRef {
    /// The address.
    pub address: String,
}

/// A trait constraint an offer is made against.
///
/// Deliberately not [`Trait`]: a trait *on an item* is a `value`, and the criterion an
/// offer names is a `trait_name` — the stream uses different keys for the same idea in
/// the two places, and one type would have to pretend otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraitCriterion {
    /// The trait's category.
    pub trait_type: String,
    /// The value the offer is constrained to.
    pub trait_name: String,
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

/// A listing or bid was cancelled.
///
/// The message does not say which, so the name is the marketplace's: `item_cancelled`
/// fires for both. The shape is a listing's — a `maker`, a price and an order — and a
/// cancelled bid arrives in the same shape with the same fields, which is why the
/// fields a bid may omit are optional here rather than absent.
///
/// Unlike [`OrderInvalidate`], this one carries the price (`base_price`) and the
/// listing's own dates, so a cancellation can be priced without the earlier listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCancelled {
    /// The chain, by OpenSea's slug.
    pub chain: String,
    /// Which collection.
    pub collection: CollectionRef,
    /// When it happened, RFC 3339.
    pub event_timestamp: String,
    /// What it was listed at, in the payment token's smallest unit.
    #[serde(with = "wasm_safe_serde::u128_required")]
    pub base_price: u128,
    /// When the order was placed, RFC 3339.
    #[serde(default)]
    pub listing_date: Option<String>,
    /// When the order would have lapsed, RFC 3339. Absent on an order with no end.
    #[serde(default)]
    pub expiration_date: Option<String>,
    /// Whether the order was private.
    #[serde(default)]
    pub is_private: bool,
    /// What was listed.
    pub item: ItemRef,
    /// `null` on a public listing.
    #[serde(default)]
    pub listing_type: Option<String>,
    /// The seller, or the bidder when this is a bid.
    pub maker: Account,
    /// The order that ended. Empty is possible, as on a sale.
    #[serde(default)]
    pub order_hash: String,
    /// What it was denominated in.
    pub payment_token: PaymentToken,
    /// The settlement contract, or the zero address when there was none.
    #[serde(default)]
    pub protocol_address: String,
    /// How many.
    pub quantity: u32,
    /// The counterparty, when there was one.
    #[serde(default)]
    pub taker: Option<Account>,
    /// The transaction that cancelled it, when there was one.
    #[serde(default)]
    pub transaction: Option<TransactionRef>,
}

/// An asset was put up for sale.
///
/// The counterpart of [`ItemCancelled`]: both carry a price and the listing's dates, which
/// is what lets a cancellation be priced without having kept the listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemListed {
    /// The chain, by OpenSea's slug.
    pub chain: String,
    /// Which collection.
    pub collection: CollectionRef,
    /// When it happened, RFC 3339.
    pub event_timestamp: String,
    /// When the order was placed, RFC 3339.
    #[serde(default)]
    pub listing_date: Option<String>,
    /// When the order lapses, RFC 3339.
    #[serde(default)]
    pub expiration_date: Option<String>,
    /// Whether the order is private.
    #[serde(default)]
    pub is_private: bool,
    /// What was listed.
    pub item: ItemRef,
    /// `null` on a public listing.
    #[serde(default)]
    pub listing_type: Option<String>,
    /// The seller.
    pub maker: Account,
    /// The order. Empty is possible, as on a sale.
    #[serde(default)]
    pub order_hash: String,
    /// What it is denominated in.
    pub payment_token: PaymentToken,
    /// The asking price, in the payment token's smallest unit.
    #[serde(with = "wasm_safe_serde::u128_required")]
    pub base_price: u128,
    /// The settlement contract.
    #[serde(default)]
    pub protocol_address: String,
    /// How many.
    pub quantity: u32,
    /// The counterparty, when there is one.
    #[serde(default)]
    pub taker: Option<Account>,
}

/// An offer was made on one asset.
///
/// A bid, in the marketplace's vocabulary. It carries `created_date` rather than the
/// `listing_date` a listing carries, and neither `is_private` nor `listing_type` —
/// differences that are only visible in a captured frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemReceivedBid {
    /// The chain, by OpenSea's slug.
    pub chain: String,
    /// Which collection.
    pub collection: CollectionRef,
    /// When it happened, RFC 3339.
    pub event_timestamp: String,
    /// When the offer was made, RFC 3339.
    #[serde(default)]
    pub created_date: Option<String>,
    /// When the offer lapses, RFC 3339.
    #[serde(default)]
    pub expiration_date: Option<String>,
    /// What the offer is for.
    pub item: ItemRef,
    /// The bidder.
    pub maker: Account,
    /// The order. Empty is possible, as on a sale.
    #[serde(default)]
    pub order_hash: String,
    /// What the offer is denominated in.
    pub payment_token: PaymentToken,
    /// The offer, in the payment token's smallest unit.
    #[serde(with = "wasm_safe_serde::u128_required")]
    pub base_price: u128,
    /// The settlement contract.
    #[serde(default)]
    pub protocol_address: String,
    /// How many.
    pub quantity: u32,
    /// The counterparty, when there is one.
    #[serde(default)]
    pub taker: Option<Account>,
}

/// An offer against any asset in a collection.
///
/// Collection-scoped, so there is no item: the captured frame's `item` is `{}`, which is
/// why this type has no `item` field at all rather than an optional one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionOffer {
    /// The chain, by OpenSea's slug.
    pub chain: String,
    /// Which collection.
    pub collection: CollectionRef,
    /// When it happened, RFC 3339.
    pub event_timestamp: String,
    /// When the offer was made, RFC 3339.
    #[serde(default)]
    pub created_date: Option<String>,
    /// When the offer lapses, RFC 3339.
    #[serde(default)]
    pub expiration_date: Option<String>,
    /// The contract the offer is constrained to.
    #[serde(default)]
    pub asset_contract_criteria: Option<ContractRef>,
    /// The collection the offer is constrained to.
    #[serde(default)]
    pub collection_criteria: Option<CollectionRef>,
    /// The bidder.
    pub maker: Account,
    /// The order. Empty is possible, as on a sale.
    #[serde(default)]
    pub order_hash: String,
    /// What the offer is denominated in.
    pub payment_token: PaymentToken,
    /// The offer, in the payment token's smallest unit.
    #[serde(with = "wasm_safe_serde::u128_required")]
    pub base_price: u128,
    /// The settlement contract.
    #[serde(default)]
    pub protocol_address: String,
    /// How many.
    pub quantity: u32,
    /// The counterparty, when there is one.
    #[serde(default)]
    pub taker: Option<Account>,
}

/// An offer against any asset carrying a trait.
///
/// The same payload as [`CollectionOffer`] plus the constraint. `trait_criteria` and
/// `trait_criteria_list` carry the same value in the captured frame; both are kept
/// because both are on the wire and which one a future frame populates is not something
/// to assume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraitOffer {
    /// The chain, by OpenSea's slug.
    pub chain: String,
    /// Which collection.
    pub collection: CollectionRef,
    /// When it happened, RFC 3339.
    pub event_timestamp: String,
    /// When the offer was made, RFC 3339.
    #[serde(default)]
    pub created_date: Option<String>,
    /// When the offer lapses, RFC 3339.
    #[serde(default)]
    pub expiration_date: Option<String>,
    /// The contract the offer is constrained to.
    #[serde(default)]
    pub asset_contract_criteria: Option<ContractRef>,
    /// The collection the offer is constrained to.
    #[serde(default)]
    pub collection_criteria: Option<CollectionRef>,
    /// The trader's own trait constraint, when there is one.
    #[serde(default)]
    pub trait_criteria: Option<TraitCriterion>,
    /// The same constraints as a list.
    #[serde(default)]
    pub trait_criteria_list: Vec<TraitCriterion>,
    /// The bidder.
    pub maker: Account,
    /// The order. Empty is possible, as on a sale.
    #[serde(default)]
    pub order_hash: String,
    /// What the offer is denominated in.
    pub payment_token: PaymentToken,
    /// The offer, in the payment token's smallest unit.
    #[serde(with = "wasm_safe_serde::u128_required")]
    pub base_price: u128,
    /// The settlement contract.
    #[serde(default)]
    pub protocol_address: String,
    /// How many.
    pub quantity: u32,
    /// The counterparty, when there is one.
    #[serde(default)]
    pub taker: Option<Account>,
}

/// An order stopped being valid.
///
/// This is the message that cannot be turned into "a listing was delisted" or "an
/// offer was withdrawn": it carries an order hash and an item and nothing that says
/// which of the two the order was. A consumer records it as an invalidation and joins
/// the hash against the earlier listing or bid; the join is the only way to know, and
/// a missed earlier message means never.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderInvalidate {
    /// The chain, by OpenSea's slug.
    pub chain: String,
    /// Which collection.
    pub collection: CollectionRef,
    /// When it happened, RFC 3339.
    pub event_timestamp: String,
    /// The item the order was for, when the frame names one.
    ///
    /// Optional because the pair of lifecycle messages is one shape: `order_revalidate`
    /// reports `"item": {}` where `order_invalidate` names an item, and which of the two
    /// carries one is not something a consumer should have to know.
    #[serde(default, deserialize_with = "empty_object_is_none")]
    pub item: Option<ItemRef>,
    /// The order that stopped being valid.
    pub order_hash: String,
    /// The settlement contract.
    #[serde(default)]
    pub protocol_address: String,
}

/// An order that had become invalid is valid again.
///
/// The inverse of [`OrderInvalidate`] and the same shape, with the same absence: no side,
/// no price, no maker. Captured frames carry `"item": {}` here even when the invalidate
/// that preceded it named an item, so `item` is optional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRevalidate {
    /// The chain, by OpenSea's slug.
    pub chain: String,
    /// Which collection.
    pub collection: CollectionRef,
    /// When it happened, RFC 3339.
    pub event_timestamp: String,
    /// The item the order was for, when the frame names one.
    #[serde(default, deserialize_with = "empty_object_is_none")]
    pub item: Option<ItemRef>,
    /// The order that became valid again.
    pub order_hash: String,
    /// The settlement contract.
    #[serde(default)]
    pub protocol_address: String,
}

/// Reads an `item` that the stream may report as `{}`.
///
/// `Option<ItemRef>` alone does not do it: serde takes only `null` for `None`, and this
/// field says "not about one asset" with an **empty object**. A half-populated item is
/// an error rather than a silent `None`, so a schema change cannot pass as an absence.
fn empty_object_is_none<'de, D>(deserializer: D) -> Result<Option<ItemRef>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct ItemFields {
        #[serde(default)]
        nft_id: Option<String>,
        #[serde(default)]
        permalink: Option<String>,
    }

    let fields = ItemFields::deserialize(deserializer)?;
    match (fields.nft_id, fields.permalink) {
        (Some(nft_id), Some(permalink)) => Ok(Some(ItemRef { nft_id, permalink })),
        (None, None) => Ok(None),
        _ => Err(serde::de::Error::custom(
            "an item carrying only one of nft_id and permalink",
        )),
    }
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

/// The envelope's own fields, without the body.
///
/// [`StreamEvent`] carries a modelled body and nothing else, which leaves a reader
/// without the envelope's `sent_at`. For a movement that does not matter — the body
/// has `event_timestamp`, which is the field the stream's docs say to order by. For
/// an `item_metadata_updated` it does: that event family has no timestamp of its
/// own, so the envelope's `sent_at` is the only one there is, and a checkpoint that
/// could not see it could not record that a metadata update had been processed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventStamp {
    /// The subscription the frame belongs to — which collection, or the wildcard.
    /// This is what a checkpoint is keyed by.
    pub topic: Topic,
    /// Which event this was.
    pub event_type: EventType,
    /// The envelope's opaque marker.
    pub version: u64,
    /// When OpenSea sent it, RFC 3339.
    pub sent_at: String,
}

impl EventStamp {
    /// Reads the routing fields and the envelope, skipping the body.
    ///
    /// `Ok(None)` for a frame that is not an event, which is what a reader sees
    /// constantly and ignores.
    pub fn from_wire(raw: &str) -> Result<Option<Self>, serde_json::Error> {
        let header = FrameHeader::from_wire(raw)?;
        let FrameEvent::Event(event_type) = header.event else {
            return Ok(None);
        };

        let frame: Frame<EventEnvelope<IgnoredAny>> = Frame::from_wire(raw)?;
        Ok(Some(Self {
            topic: header.topic,
            event_type,
            version: frame.payload.version,
            sent_at: frame.payload.sent_at,
        }))
    }
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
    /// A listing or bid ended without being taken.
    ItemCancelled(Box<ItemCancelled>),
    /// An asset was put up for sale.
    ItemListed(Box<ItemListed>),
    /// An offer was made on one asset.
    ItemReceivedBid(Box<ItemReceivedBid>),
    /// An offer was made against a whole collection.
    CollectionOffer(Box<CollectionOffer>),
    /// An offer was made against a trait.
    TraitOffer(Box<TraitOffer>),
    /// An order stopped being valid, with no statement of which kind it was.
    OrderInvalidate(Box<OrderInvalidate>),
    /// An order that had been invalid became valid again.
    OrderRevalidate(Box<OrderRevalidate>),
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
            EventType::ItemCancelled => {
                let frame: Frame<EventEnvelope<ItemCancelled>> = Frame::from_wire(raw)?;
                Ok(Some(Self::ItemCancelled(Box::new(frame.payload.payload))))
            }
            EventType::OrderInvalidate => {
                let frame: Frame<EventEnvelope<OrderInvalidate>> = Frame::from_wire(raw)?;
                Ok(Some(Self::OrderInvalidate(Box::new(frame.payload.payload))))
            }
            EventType::ItemListed => {
                let frame: Frame<EventEnvelope<ItemListed>> = Frame::from_wire(raw)?;
                Ok(Some(Self::ItemListed(Box::new(frame.payload.payload))))
            }
            EventType::ItemReceivedBid => {
                let frame: Frame<EventEnvelope<ItemReceivedBid>> = Frame::from_wire(raw)?;
                Ok(Some(Self::ItemReceivedBid(Box::new(frame.payload.payload))))
            }
            EventType::CollectionOffer => {
                let frame: Frame<EventEnvelope<CollectionOffer>> = Frame::from_wire(raw)?;
                Ok(Some(Self::CollectionOffer(Box::new(frame.payload.payload))))
            }
            EventType::TraitOffer => {
                let frame: Frame<EventEnvelope<TraitOffer>> = Frame::from_wire(raw)?;
                Ok(Some(Self::TraitOffer(Box::new(frame.payload.payload))))
            }
            EventType::OrderRevalidate => {
                let frame: Frame<EventEnvelope<OrderRevalidate>> = Frame::from_wire(raw)?;
                Ok(Some(Self::OrderRevalidate(Box::new(frame.payload.payload))))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A real `item_metadata_updated` frame, off the live socket on 2026-09-27.
    const METADATA: &str = include_str!("../tests/fixtures/item_metadata_updated.json");

    #[test]
    fn a_stamp_carries_the_envelope_of_an_event_the_body_leaves_out() {
        let stamp = EventStamp::from_wire(METADATA).unwrap().unwrap();
        assert_eq!(stamp.event_type, super::EventType::ItemMetadataUpdated);
        assert_eq!(stamp.topic, Topic::AllCollections);
        assert_eq!(stamp.sent_at, "2026-09-27T04:26:32.072000Z");
        assert_eq!(stamp.version, 1790483192072);
    }

    #[test]
    fn a_protocol_frame_has_no_stamp() {
        let raw = r#"["1","1","collection:x","phx_reply",{"status":"ok","response":{}}]"#;
        assert_eq!(EventStamp::from_wire(raw).unwrap(), None);
    }
}
