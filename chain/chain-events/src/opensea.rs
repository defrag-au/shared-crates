//! OpenSea's messages, translated into this vocabulary.
//!
//! Enabled by the `opensea` feature. Every type the stream delivers maps to exactly
//! one family, and a type this crate has never seen is **refused by name** rather than
//! silently dropped.
//!
//! # What a message is allowed to assert
//!
//! A marketplace message is an interpretation, so this module keeps each one to what it
//! actually states:
//!
//! - `item_transferred` is a movement. `item_sold` is a **sale**, and it is also a
//!   movement when `taker` is present — the two rows get the same [`EventId`] as the
//!   separate transfer message, so the merge is an upsert rather than a double count.
//! - `item_cancelled` is a [`Cancelled`](MarketEvent::Cancelled) with no [`Side`] — the
//!   message does not say whether a listing or a bid was withdrawn, and the order hash is
//!   the join that would tell you, which belongs in a projection rather than here.
//! - `order_invalidate` is [`Invalidated`](MarketEvent::Invalidated), not a cancellation.
//!   The message says the order stopped being valid and no more; an expiry read as a
//!   withdrawal would remove an order that is still there.
//! - `order_revalidate` is [`Reopened`](MarketEvent::Reopened): the order is live again at
//!   the terms it already had, which the message does not repeat.
//! - A metadata update carries the whole document and no before-state, so the event
//!   records the [`ImageMetadataV1`] fingerprint it produced and no `before_hash`.
//!
//! # What a message cannot give
//!
//! There is no block. OpenSea reports a transaction hash and a wall-clock timestamp, so
//! [`ChainEvent::coord`] is `None` — see [`ChainEvent`] on why that is not zero. And the
//! feed has no replay, so `coverage` stays [`Coverage::Partial`] for everything here.

use std::fmt;

use chains::{AssetRefError, AssetType, ChainRef, TokenRef};
use opensea_stream::{
    Account as StreamAccount, CollectionOffer, EventStamp, EventType, ItemCancelled, ItemListed,
    ItemMetadataUpdated, ItemReceivedBid, ItemRef, ItemSold, ItemTransferred, OrderInvalidate,
    OrderRevalidate, PaymentToken, StreamEvent, TraitOffer, TransactionRef,
};

use crate::asset::{AssetEvent, AssetKind, HashAlgo, ImageMetadataV1};
use crate::event::{AccountRef, ChainEvent, EventBody, EventDraft, EventSource, TxRef};
use crate::holder::HolderEvent;
use crate::market::{
    MarketEvent, MarketParty, OfferScope, Order, OrderRef, PartyRole, Price, Side,
};

/// One message, translated.
///
/// More than one event only for `item_sold`, which is both a market fact and — when it
/// names a taker — the movement that market fact implies.
pub fn from_opensea(
    event: &StreamEvent,
    stamp: &EventStamp,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    match event {
        StreamEvent::ItemTransferred(movement) => transferred(movement, collection, recorded_at),
        StreamEvent::ItemSold(sale) => sold(sale, collection, recorded_at),
        StreamEvent::ItemMetadataUpdated(update) => {
            metadata(update, stamp, collection, recorded_at)
        }
        StreamEvent::ItemCancelled(order) => cancelled(order, collection, recorded_at),
        StreamEvent::ItemListed(payload) => listed(payload, collection, recorded_at),
        StreamEvent::ItemReceivedBid(payload) => bid(payload, collection, recorded_at),
        StreamEvent::CollectionOffer(offer) => collection_offer(offer, collection, recorded_at),
        StreamEvent::TraitOffer(offer) => trait_offer(offer, collection, recorded_at),
        StreamEvent::OrderInvalidate(order) => invalidated(order, collection, recorded_at),
        StreamEvent::OrderRevalidate(order) => revalidated(order, collection, recorded_at),
        StreamEvent::Unmodelled(event) => Err(TranslateError::Unmodelled(event.event_type.clone())),
    }
}

/// Why a message could not become a fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslateError {
    /// A type this crate does not model. Refused by name rather than dropped.
    Unmodelled(EventType),
    /// The message names a chain nobody has paired with an EIP-155 id.
    UnknownChain(String),
    /// The message is for a different chain than the collection being watched.
    ChainMismatch {
        /// The chain the watch set names.
        watched: ChainRef,
        /// The chain the message names.
        payload: ChainRef,
    },
    /// The message is for a different contract than the collection being watched.
    CollectionMismatch {
        /// The contract the watch set names.
        watched: String,
        /// The contract in the message's `nft_id`.
        payload: String,
    },
    /// An `nft_id` that is not `{chain}/{contract}/{token}`.
    MalformedNftId(String),
    /// A message that is *about an order* arrived without an order id.
    ///
    /// A placement, a reopening or an invalidation cannot be applied to an order nobody
    /// can name, so this is refused rather than stored as a row no reader can use. A
    /// settlement or a withdrawal is a standalone fact and carries `None` instead — see
    /// [`MarketEvent`].
    MissingOrderId(EventType),
    /// A timestamp that is not the RFC 3339 the stream sends.
    BadTimestamp {
        /// Which field it came from.
        field: &'static str,
        /// What it held.
        value: String,
    },
    /// The token identity could not be built.
    Asset(AssetRefError),
}

impl fmt::Display for TranslateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unmodelled(event_type) => {
                write!(formatter, "no translation for event type {event_type:?}")
            }
            Self::UnknownChain(slug) => {
                write!(formatter, "OpenSea chain slug {slug:?} has no known id")
            }
            Self::ChainMismatch { watched, payload } => write!(
                formatter,
                "message is for {payload} but the watch is for {watched}"
            ),
            Self::CollectionMismatch { watched, payload } => write!(
                formatter,
                "message is for contract {payload} but the watch is for {watched}"
            ),
            Self::MalformedNftId(nft_id) => {
                write!(
                    formatter,
                    "{nft_id:?} is not {{chain}}/{{contract}}/{{token}}"
                )
            }
            Self::MissingOrderId(event_type) => write!(
                formatter,
                "{event_type:?} names no order, and the fact is about one"
            ),
            Self::BadTimestamp { field, value } => {
                write!(formatter, "{field} is not RFC 3339: {value:?}")
            }
            Self::Asset(error) => write!(formatter, "asset identity: {error}"),
        }
    }
}

impl std::error::Error for TranslateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Asset(error) => Some(error),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// The ten messages
// ---------------------------------------------------------------------------

fn transferred(
    movement: &ItemTransferred,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &movement.chain)?;
    let asset = asset_ref(collection, &movement.item.nft_id)?;
    let body = HolderEvent::transfer(
        account(chain, &movement.from_account),
        account(chain, &movement.to_account),
        u64::from(movement.quantity),
    );
    let discriminator = body.discriminator();
    let occurred_at = parse_timestamp("event_timestamp", &movement.event_timestamp)?;

    let event = ChainEvent::draft(
        chain,
        collection.clone(),
        EventBody::Holder(body),
        occurred_at,
        EventSource::OpenseaStream,
    )
    .asset(asset)
    .tx_maybe(tx(&movement.transaction))
    .build(&discriminator, recorded_at);

    Ok(vec![event])
}

fn sold(
    sale: &ItemSold,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &sale.chain)?;
    let asset = asset_ref(collection, &sale.item.nft_id)?;
    let occurred_at = parse_timestamp("event_timestamp", &sale.event_timestamp)?;

    let market = MarketEvent::Filled {
        order: order_of(&sale.order_hash),
        // A sale takes a listing: the ask is what was accepted.
        side: Side::Ask,
        price: Some(price(sale.sale_price, &sale.payment_token)),
        parties: {
            let mut parties = vec![MarketParty {
                role: PartyRole::Seller,
                account: account(chain, &sale.maker),
            }];
            if let Some(taker) = &sale.taker {
                parties.push(MarketParty {
                    role: PartyRole::Buyer,
                    account: account(chain, taker),
                });
            }
            parties
        },
    };
    let market_discriminator = market.discriminator();

    let mut events = vec![
        ChainEvent::draft(
            chain,
            collection.clone(),
            EventBody::Market(market),
            occurred_at,
            EventSource::OpenseaStream,
        )
        .asset(asset.clone())
        .tx_maybe(tx(&sale.transaction))
        .build(&market_discriminator, recorded_at),
    ];

    // A sale names both sides, so it states the movement too. The parties are the same
    // ones the separate `item_transferred` message carries, which is what makes this
    // the same event id rather than a second row.
    if let Some(taker) = &sale.taker {
        let body = HolderEvent::transfer(
            account(chain, &sale.maker),
            account(chain, taker),
            u64::from(sale.quantity),
        );
        let discriminator = body.discriminator();
        events.push(
            ChainEvent::draft(
                chain,
                collection.clone(),
                EventBody::Holder(body),
                occurred_at,
                EventSource::OpenseaStream,
            )
            .asset(asset)
            .tx_maybe(tx(&sale.transaction))
            .build(&discriminator, recorded_at),
        );
    }

    Ok(events)
}

fn metadata(
    update: &ItemMetadataUpdated,
    stamp: &EventStamp,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &update.item.chain.name)?;
    let asset = asset_ref(collection, &update.item.nft_id)?;

    // This event family has no `event_timestamp` of its own: the envelope's `sent_at`
    // is the only time there is, which is what `EventStamp` exists for.
    let occurred_at = parse_timestamp("sent_at", &stamp.sent_at)?;

    let traits: Vec<(&str, &str)> = update
        .item
        .metadata
        .traits
        .iter()
        .map(|trait_| (trait_.trait_type.as_str(), trait_.value.as_str()))
        .collect();
    let fingerprint = ImageMetadataV1 {
        image_url: update.item.metadata.image_url.as_deref(),
        animation_url: update.item.metadata.animation_url.as_deref(),
        metadata_url: update.item.metadata.metadata_url.as_deref(),
        background_color: update.item.metadata.background_color.as_deref(),
        traits: &traits,
    }
    .fingerprint();

    let body = AssetEvent::new(AssetKind::Metadata, HashAlgo::ImageMetadataV1, fingerprint);
    let discriminator = body.discriminator();

    Ok(vec![
        ChainEvent::draft(
            chain,
            collection.clone(),
            EventBody::Asset(body),
            occurred_at,
            EventSource::OpenseaStream,
        )
        .asset(asset)
        .build(&discriminator, recorded_at),
    ])
}

fn cancelled(
    order: &ItemCancelled,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &order.chain)?;
    let asset = asset_ref(collection, &order.item.nft_id)?;
    let occurred_at = parse_timestamp("event_timestamp", &order.event_timestamp)?;

    // The message does not say whether it withdrew a listing or a bid, and a later
    // `Invalidated` is a different fact, so neither is promoted into the other.
    let market = MarketEvent::Cancelled {
        order: order_of(&order.order_hash),
        side: None,
        price: Some(price(order.base_price, &order.payment_token)),
        parties: Vec::new(),
    };
    let discriminator = market.discriminator();

    Ok(vec![
        ChainEvent::draft(
            chain,
            collection.clone(),
            EventBody::Market(market),
            occurred_at,
            EventSource::OpenseaStream,
        )
        .asset(asset)
        .tx_maybe(tx(&order.transaction))
        .build(&discriminator, recorded_at),
    ])
}

fn listed(
    listed: &ItemListed,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &listed.chain)?;
    let asset = asset_ref(collection, &listed.item.nft_id)?;
    let occurred_at = parse_timestamp("event_timestamp", &listed.event_timestamp)?;

    let market = MarketEvent::Placed {
        order: required_order(&listed.order_hash, EventType::ItemListed)?,
        side: Side::Ask,
        price: price(listed.base_price, &listed.payment_token),
        parties: vec![MarketParty {
            role: PartyRole::Seller,
            account: account(chain, &listed.maker),
        }],
    };
    let discriminator = market.discriminator();

    Ok(vec![
        ChainEvent::draft(
            chain,
            collection.clone(),
            EventBody::Market(market),
            occurred_at,
            EventSource::OpenseaStream,
        )
        .asset(asset)
        .build(&discriminator, recorded_at),
    ])
}

fn bid(
    bid: &ItemReceivedBid,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &bid.chain)?;
    let asset = asset_ref(collection, &bid.item.nft_id)?;
    let occurred_at = parse_timestamp("event_timestamp", &bid.event_timestamp)?;

    let market = MarketEvent::Placed {
        order: required_order(&bid.order_hash, EventType::ItemReceivedBid)?,
        side: Side::Bid(OfferScope::Item),
        price: price(bid.base_price, &bid.payment_token),
        // A bid's maker is the bidder, so it is the buying side of the eventual trade.
        parties: vec![MarketParty {
            role: PartyRole::Buyer,
            account: account(chain, &bid.maker),
        }],
    };
    let discriminator = market.discriminator();

    Ok(vec![
        ChainEvent::draft(
            chain,
            collection.clone(),
            EventBody::Market(market),
            occurred_at,
            EventSource::OpenseaStream,
        )
        .asset(asset)
        .build(&discriminator, recorded_at),
    ])
}

fn collection_offer(
    offer: &CollectionOffer,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &offer.chain)?;
    let occurred_at = parse_timestamp("event_timestamp", &offer.event_timestamp)?;

    let market = MarketEvent::Placed {
        order: required_order(&offer.order_hash, EventType::CollectionOffer)?,
        side: Side::Bid(OfferScope::Collection),
        price: price(offer.base_price, &offer.payment_token),
        parties: vec![MarketParty {
            role: PartyRole::Buyer,
            account: account(chain, &offer.maker),
        }],
    };
    let discriminator = market.discriminator();

    // Collection-scoped: the frame's `item` is `{}`, so there is no asset to name.
    Ok(vec![
        ChainEvent::draft(
            chain,
            collection.clone(),
            EventBody::Market(market),
            occurred_at,
            EventSource::OpenseaStream,
        )
        .build(&discriminator, recorded_at),
    ])
}

fn trait_offer(
    offer: &TraitOffer,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &offer.chain)?;
    let occurred_at = parse_timestamp("event_timestamp", &offer.event_timestamp)?;

    let market = MarketEvent::Placed {
        order: required_order(&offer.order_hash, EventType::TraitOffer)?,
        side: Side::Bid(OfferScope::Trait),
        price: price(offer.base_price, &offer.payment_token),
        parties: vec![MarketParty {
            role: PartyRole::Buyer,
            account: account(chain, &offer.maker),
        }],
    };
    let discriminator = market.discriminator();

    Ok(vec![
        ChainEvent::draft(
            chain,
            collection.clone(),
            EventBody::Market(market),
            occurred_at,
            EventSource::OpenseaStream,
        )
        .build(&discriminator, recorded_at),
    ])
}

fn invalidated(
    order: &OrderInvalidate,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &order.chain)?;
    let occurred_at = parse_timestamp("event_timestamp", &order.event_timestamp)?;

    let market = MarketEvent::Invalidated {
        order: required_order(&order.order_hash, EventType::OrderInvalidate)?,
    };
    let discriminator = market.discriminator();

    Ok(vec![
        ChainEvent::draft(
            chain,
            collection.clone(),
            EventBody::Market(market),
            occurred_at,
            EventSource::OpenseaStream,
        )
        .maybe_asset(optional_asset(collection, &order.item)?)
        .build(&discriminator, recorded_at),
    ])
}

fn revalidated(
    order: &OrderRevalidate,
    collection: &AssetType,
    recorded_at: u64,
) -> Result<Vec<ChainEvent>, TranslateError> {
    let chain = check_chain(collection, &order.chain)?;
    let occurred_at = parse_timestamp("event_timestamp", &order.event_timestamp)?;

    let market = MarketEvent::Reopened {
        order: required_order(&order.order_hash, EventType::OrderRevalidate)?,
    };
    let discriminator = market.discriminator();

    Ok(vec![
        ChainEvent::draft(
            chain,
            collection.clone(),
            EventBody::Market(market),
            occurred_at,
            EventSource::OpenseaStream,
        )
        .maybe_asset(optional_asset(collection, &order.item)?)
        .build(&discriminator, recorded_at),
    ])
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

fn account(chain: ChainRef, account: &StreamAccount) -> AccountRef {
    AccountRef::new(chain, account.address.clone())
}

/// The chain the message names, checked against the collection being watched.
fn check_chain(collection: &AssetType, slug: &str) -> Result<ChainRef, TranslateError> {
    let payload = opensea_stream::chain_ref(slug)
        .ok_or_else(|| TranslateError::UnknownChain(slug.to_owned()))?;
    if payload != collection.chain() {
        return Err(TranslateError::ChainMismatch {
            watched: collection.chain(),
            payload,
        });
    }
    Ok(payload)
}

/// The asset a message names, checked against the collection being watched.
fn asset_ref(collection: &AssetType, nft_id: &str) -> Result<TokenRef, TranslateError> {
    let malformed = || TranslateError::MalformedNftId(nft_id.to_owned());
    let mut parts = nft_id.splitn(3, '/');

    let slug = parts.next().ok_or_else(malformed)?;
    let contract = parts.next().ok_or_else(malformed)?;
    let token = parts.next().ok_or_else(malformed)?;

    let payload = opensea_stream::chain_ref(slug)
        .ok_or_else(|| TranslateError::UnknownChain(slug.to_owned()))?;
    if payload != collection.chain() {
        return Err(TranslateError::ChainMismatch {
            watched: collection.chain(),
            payload,
        });
    }
    if !contract.eq_ignore_ascii_case(collection.reference()) {
        return Err(TranslateError::CollectionMismatch {
            watched: collection.reference().to_owned(),
            payload: contract.to_owned(),
        });
    }

    TokenRef::new(collection.clone(), Some(token)).map_err(TranslateError::Asset)
}

fn optional_asset(
    collection: &AssetType,
    item: &Option<ItemRef>,
) -> Result<Option<TokenRef>, TranslateError> {
    item.as_ref()
        .map(|item| asset_ref(collection, &item.nft_id))
        .transpose()
}

fn order_ref(order_hash: &str) -> Option<OrderRef> {
    // An empty hash is observed on real settlements, so it is an absent identifier
    // rather than an order whose id is "".
    (!order_hash.is_empty()).then(|| OrderRef::new(order_hash))
}

/// The order for a fact that is allowed to arrive without one.
///
/// Separate from [`order_ref`] because the absence is a state, not a missing value: a
/// settlement or a withdrawal can state its fact without naming the order.
fn order_of(order_hash: &str) -> Order {
    match order_ref(order_hash) {
        Some(reference) => Order::Named(reference),
        None => Order::Unknown,
    }
}

/// The order a message that is *about* an order must name.
///
/// Separate from [`order_ref`] because the two cases differ: a settlement may legitimately
/// arrive without one, a placement or an invalidation may not.
fn required_order(order_hash: &str, event_type: EventType) -> Result<OrderRef, TranslateError> {
    order_ref(order_hash).ok_or(TranslateError::MissingOrderId(event_type))
}

fn tx(transaction: &Option<TransactionRef>) -> Option<TxRef> {
    transaction
        .as_ref()
        .map(|transaction| TxRef::new(transaction.hash.clone(), 0))
}

fn price(amount: u128, token: &PaymentToken) -> Price {
    if is_zero_address(&token.address) {
        Price::native(amount, token.decimals)
    } else {
        Price::contract(
            amount,
            token.address.clone(),
            token.symbol.clone(),
            token.decimals,
        )
    }
}

fn is_zero_address(address: &str) -> bool {
    let digits = address.strip_prefix("0x").unwrap_or(address);
    !digits.is_empty() && digits.bytes().all(|byte| byte == b'0')
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// The unix seconds of an RFC 3339 timestamp, as the stream writes them.
///
/// `2026-09-27T01:34:49.190000Z`, and nothing else: the feed is a single producer with
/// one format, so a different shape is a change worth failing on rather than a date
/// library's worth of tolerance. Fractional seconds are ignored — an event's second is
/// finer than any ordering these facts need, and `--at`-style precision does not belong
/// in a stored fact.
fn parse_timestamp(field: &'static str, value: &str) -> Result<u64, TranslateError> {
    let bad = || TranslateError::BadTimestamp {
        field,
        value: value.to_owned(),
    };

    let (date, rest) = value.split_once('T').ok_or_else(bad)?;
    let rest = rest.strip_suffix('Z').ok_or_else(bad)?;
    let time = rest.split('.').next().unwrap_or(rest);

    let mut date_parts = date.split('-');
    let year: i64 = date_parts
        .next()
        .ok_or_else(bad)?
        .parse()
        .map_err(|_| bad())?;
    let month: u32 = date_parts
        .next()
        .ok_or_else(bad)?
        .parse()
        .map_err(|_| bad())?;
    let day: u32 = date_parts
        .next()
        .ok_or_else(bad)?
        .parse()
        .map_err(|_| bad())?;
    if date_parts.next().is_some() {
        return Err(bad());
    }

    let mut time_parts = time.split(':');
    let hour: u64 = time_parts
        .next()
        .ok_or_else(bad)?
        .parse()
        .map_err(|_| bad())?;
    let minute: u64 = time_parts
        .next()
        .ok_or_else(bad)?
        .parse()
        .map_err(|_| bad())?;
    let second: u64 = time_parts
        .next()
        .ok_or_else(bad)?
        .parse()
        .map_err(|_| bad())?;
    if time_parts.next().is_some() {
        return Err(bad());
    }

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(bad());
    }

    let days = days_from_civil(year, month, day);
    let days = u64::try_from(days).map_err(|_| bad())?;
    Ok(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
///
/// Howard Hinnant's `days_from_civil`, which is exact for every date this can receive
/// and needs no table. Kept here rather than taking a date crate for one format.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = (i64::from(month) + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

impl EventDraft {
    /// Adds the transaction when the message named one.
    fn tx_maybe(self, tx: Option<TxRef>) -> Self {
        match tx {
            Some(tx) => self.tx(tx),
            None => self,
        }
    }

    /// Adds the asset when the message named one.
    fn maybe_asset(self, asset: Option<TokenRef>) -> Self {
        match asset {
            Some(asset) => self.asset(asset),
            None => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_order_about_fact_must_name_its_order() {
        // A placement, a reopening or an invalidation cannot be applied to an order
        // nobody can name, so an empty hash is refused rather than stored un-keyed.
        assert!(required_order("0xabc", EventType::ItemListed).is_ok());
        assert_eq!(
            required_order("", EventType::ItemListed),
            Err(TranslateError::MissingOrderId(EventType::ItemListed))
        );
    }

    #[test]
    fn a_settlement_may_arrive_without_one() {
        // Observed on a real capture: a sale settled with an empty `order_hash`.
        assert!(order_ref("").is_none());
        assert!(order_ref("0xabc").is_some());
    }
}
