//! Listings, sales, offers and the order lifecycle.
//!
//! # One entity, six transitions
//!
//! A marketplace fact is always about an **order** — a listing or an offer — and the
//! sources disagree about almost everything except its lifecycle. Model the lifecycle and
//! both feeds land on it; model the messages and the vocabulary becomes their union.
//!
//! An order is a **standing set of terms**, not a trade: an ask is an intent to sell an
//! asset at a price, a bid an intent to buy one. `Placed` and `Reopened` make one live,
//! `Updated` changes what it asks, and `Filled`, `Cancelled` and `Invalidated` end it. A
//! sale is therefore a *transition of an order*, not an entity of its own — which is why
//! it names one, and why a withdrawal does too.
//!
//! An earlier draft did the latter: a struct with a `kind` field and six nullable fields
//! beside it. It could represent a cancellation that carried a settlement price and no
//! parties, because nothing in the type said which fields belonged to which fact. The
//! transitions below cannot: each carries exactly what its own fact states, and a
//! combination that no source can produce is not constructible.
//!
//! The pair to watch is `Placed`/`Updated`. A first listing and a re-price are different
//! facts — one creates an order, the other changes what it asks — and a fold that has to
//! infer the difference from a repeated order id is a fold that will eventually get it
//! wrong. A sale is a `Filled`, not a `Closed` with a reason: a fill carries both parties
//! and what it settled for, which a withdrawal does not.
//!
//! # Where a field is absent, the source is
//!
//! An order's identity is **required** on the four transitions that are *about* an order —
//! `Placed`, `Updated`, `Reopened`, `Invalidated`. A fact that cannot name the order it
//! changed cannot be applied to one, so an unnamed placement is not a weak fact but an
//! unusable one; the adapter refuses it rather than storing a row no reader can use.
//!
//! It is optional on the two transitions that are standalone observations of an order
//! ending, where the source genuinely may not know which order it was — and there the
//! absence is [`Order::Unknown`], not `None`. A named state forces a consumer to decide
//! what to do with a sale it cannot key, where a `None` invites `unwrap_or_default()` and an
//! empty join key that silently matches nothing:
//!
//! - `Filled.order` — a sale settled without an order hash (observed on a real OpenSea
//!   capture), or a Cardano sale whose listing predates the walk's floor.
//! - `Cancelled.order` — OpenSea names it; a Cardano delist spends the listing and the
//!   wire reports the spent output's index beside the *spending* transaction's hash.
//!
//! The other absences are facts one feed cannot state:
//!
//! - `Filled.price` — an in-kind settlement has a consideration this vocabulary cannot
//!   price. `None` is a stated absence; a zero would read as a free sale.
//! - `Cancelled.side` — OpenSea's `item_cancelled` does not say whether it withdrew a
//!   listing or a bid.
//!
//! See `cnft.dev-workers/docs/design/CHAIN_EVENTS.md` for the two sources this
//! reconciles and the places they cannot be reconciled.

use chains::TokenRef;
use serde::{Deserialize, Serialize};

use crate::event::AccountRef;

/// One marketplace fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarketEvent {
    /// An order became live.
    Placed {
        /// The order. Required: a placement that cannot name its order cannot be
        /// applied to one, and an adapter that has no identity for it refuses the fact
        /// rather than storing a row no reader can use.
        order: OrderRef,
        /// Which side made it.
        side: Side,
        /// What it asks, or offers.
        price: Price,
        /// Who made it — the seller of an ask, the bidder of a bid. Absent when the
        /// source does not name one; a Cardano listing update carries no payouts to
        /// derive a seller from.
        parties: Vec<MarketParty>,
    },
    /// An order's terms changed. The order is the same one, at a new price.
    ///
    /// Only Cardano states this: OpenSea reports a relist as a new `item_listed`, which
    /// arrives here as a second `Placed` against the same order id.
    Updated {
        /// The order being repriced. A re-price cannot be about an unnamed order —
        /// there would be nothing to apply it to.
        order: OrderRef,
        /// What it asks now.
        price: Price,
    },
    /// A closed order became live again, at the terms it already had.
    Reopened {
        /// The order that became valid again. Required — reopening an unnamed order
        /// says nothing.
        order: OrderRef,
    },
    /// An order was taken — the sale.
    Filled {
        /// The order that was consumed, where the source names it.
        ///
        /// [`Order::Unknown`] when it does not — never an absent field, because a sale
        /// always consumes an order and only the *feed* can be silent about which.
        order: Order,
        /// Which side was taken: an ask is a sale, a bid is an acceptance.
        side: Side,
        /// What it settled for, or `None` for an in-kind settlement this vocabulary
        /// cannot price.
        price: Option<Price>,
        /// Both sides, where the source names them.
        parties: Vec<MarketParty>,
    },
    /// An order was withdrawn.
    Cancelled {
        /// The order, where the source names it.
        order: Order,
        /// Which side was withdrawn. OpenSea's `item_cancelled` does not say.
        side: Option<Side>,
        /// What it stood at when it was withdrawn, where the source says.
        ///
        /// Kept because a withdrawal is the last statement a source makes about an
        /// order, and OpenSea's message carries the price. A Cardano delist carries
        /// none — it spends a listing whose datum the walk may not have decoded.
        price: Option<Price>,
        /// Whoever withdrew it, where the source names them.
        parties: Vec<MarketParty>,
    },
    /// An order stopped being valid, and the source does not say whether it was
    /// withdrawn, filled or expired.
    ///
    /// This is its own transition rather than a cancellation with a caveat: an
    /// invalidation is not a withdrawal, and a fold that treats it as one removes an
    /// order that may simply have expired.
    Invalidated {
        /// The order that stopped being valid. Required: the message always names it,
        /// and an invalidation of an unnamed order tells a reader nothing.
        order: OrderRef,
    },
}

impl MarketEvent {
    /// The transition, as the string the store keys on.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Placed { .. } => "placed",
            Self::Updated { .. } => "updated",
            Self::Reopened { .. } => "reopened",
            Self::Filled { .. } => "filled",
            Self::Cancelled { .. } => "cancelled",
            Self::Invalidated { .. } => "invalidated",
        }
    }

    /// The order this fact is about, where the source names one.
    pub fn order(&self) -> Option<&OrderRef> {
        match self {
            Self::Placed { order, .. }
            | Self::Updated { order, .. }
            | Self::Reopened { order }
            | Self::Invalidated { order } => Some(order),
            Self::Filled { order, .. } | Self::Cancelled { order, .. } => order.named(),
        }
    }

    /// A stable string for [`EventId::derive`](crate::EventId::derive).
    ///
    /// An order id identifies an order and its whole lifecycle, so it is the
    /// discriminator whenever there is one. Without it, the parties are what makes two
    /// otherwise identical facts different. The kind is already hashed by the caller, so
    /// it is not repeated here.
    pub fn discriminator(&self) -> String {
        match self {
            Self::Placed { order, .. }
            | Self::Updated { order, .. }
            | Self::Reopened { order }
            | Self::Invalidated { order } => order.id.clone(),
            Self::Filled { order, parties, .. } | Self::Cancelled { order, parties, .. } => {
                match order.named() {
                    Some(order) => order.id.clone(),
                    None => parties_discriminator(parties),
                }
            }
        }
    }
}

/// Which side of the market an order is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// An owner offering the asset.
    Ask,
    /// A buyer offering to pay, scoped to what they will accept.
    ///
    /// The scope lives here rather than beside the event because an ask has no scope to
    /// have: a listing names one asset, always. Flattening the two into one field made
    /// "a collection-wide listing" representable, and nothing can produce one.
    Bid(OfferScope),
}

impl Side {
    /// The scope a bid is against, or `None` for an ask.
    pub const fn scope(self) -> Option<OfferScope> {
        match self {
            Self::Ask => None,
            Self::Bid(scope) => Some(scope),
        }
    }
}

/// What an offer was against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferScope {
    /// One asset.
    Item,
    /// Any asset carrying a trait.
    Trait,
    /// Any asset in the collection.
    Collection,
}

impl OfferScope {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Trait => "trait",
            Self::Collection => "collection",
        }
    }
}

/// One party to a market fact.
///
/// A list rather than named nullable fields, for the reason the `holder` family gives:
/// the party is the primitive, and a party the source does not name is an absent element
/// rather than a null field. It also lets a fill carry both sides in one shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketParty {
    /// Which side of the trade this party is.
    pub role: PartyRole,
    /// Who.
    pub account: AccountRef,
}

/// Which side of a trade a party is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyRole {
    /// The side giving the asset — the seller of an ask, the owner in a fill.
    Seller,
    /// The side giving the consideration — the bidder of a bid, the taker of a fill.
    Buyer,
}

impl PartyRole {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Seller => "seller",
            Self::Buyer => "buyer",
        }
    }
}

/// The order a fact is about, and how firmly the source named it.
///
/// A sale and a withdrawal both happen *to* an order, and a source can report either
/// without saying which one. That absence is a state of its own rather than a missing
/// value: it is the feed's silence, not a claim that no order existed — a fill always
/// consumes one, and no source states otherwise.
///
/// It is a named state rather than `Option` so that a consumer must decide what to do with
/// a fact it cannot key. `None` reads like a value that happens to be missing, and
/// `unwrap_or_default()` on one produces an empty join key that matches nothing and
/// raises nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// The source named the order.
    Named(OrderRef),
    /// The source stated the fact and did not name the order it was about.
    ///
    /// OpenSea sends an empty `order_hash` on real settlements; a Cardano sale whose
    /// listing predates the walk's floor cannot name one either.
    Unknown,
}

impl Order {
    /// The identity, when the source named one.
    pub fn named(&self) -> Option<&OrderRef> {
        match self {
            Self::Named(reference) => Some(reference),
            Self::Unknown => None,
        }
    }
}

/// The order a fact is about.
///
/// An order is a **standing set of terms** — an ask (an intent to sell an asset at a
/// price) or a bid (an intent to buy one, scoped to an item, a trait or the collection).
/// It is not a sale: a sale is the [`MarketEvent::Filled`] transition that ends one, which
/// is why a fill and a withdrawal both *name* an order rather than being orders themselves.
///
/// `Reopened` is the inverse of `Invalidated`: OpenSea's `order_revalidate` follows its
/// `order_invalidate` and puts the same order back on the book at the terms it already had.
///
/// The identity is the marketplace's own — an order hash on EVM, a transaction output
/// reference on Cardano, where the listing *is* a UTxO and there is no order id to speak
/// of.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderRef {
    /// An order hash on EVM; a transaction output reference on Cardano.
    pub id: String,
}

impl OrderRef {
    /// A reference to `id`.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

/// What something settled for, or was offered at.
///
/// `amount` is an integer in the currency's smallest unit and is written as a string:
/// wei-scale values do not survive a JSON number in a browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    /// The amount, in `currency`'s smallest unit.
    #[serde(with = "wasm_safe_serde::u128_required")]
    pub amount: u128,
    /// What it is denominated in.
    pub currency: Currency,
    /// The scale `amount` is expressed at — 18 for WETH, 6 for ADA.
    pub decimals: u8,
}

impl Price {
    /// A price in the chain's own coin.
    pub const fn native(amount: u128, decimals: u8) -> Self {
        Self {
            amount,
            currency: Currency::Native,
            decimals,
        }
    }

    /// A price in an asset this workspace can identify.
    pub const fn asset(amount: u128, currency: TokenRef, decimals: u8) -> Self {
        Self {
            amount,
            currency: Currency::Asset(currency),
            decimals,
        }
    }

    /// A price in a contract this workspace cannot name as a [`TokenRef`].
    pub fn contract(
        amount: u128,
        address: impl Into<String>,
        symbol: impl Into<String>,
        decimals: u8,
    ) -> Self {
        Self {
            amount,
            currency: Currency::Contract {
                address: address.into(),
                symbol: symbol.into(),
            },
            decimals,
        }
    }
}

/// A stable, order-independent string for a party list.
///
/// Sorted by role then account, so two feeds that name the same two parties in a
/// different order derive the same id.
fn parties_discriminator(parties: &[MarketParty]) -> String {
    let mut parts: Vec<String> = parties
        .iter()
        .map(|party| format!("{}:{}", party.role.as_str(), party.account.address))
        .collect();
    parts.sort();
    parts.join(",")
}

/// The unit a price is denominated in.
///
/// Externally tagged, so it reads as `"native"`, `{"asset": "cardano:…"}` or
/// `{"contract": { … }}` — an enum rather than a nullable token field, because
/// "no token" and "the chain's coin" are the same statement and only one field should
/// be able to say it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Currency {
    /// The chain's own coin: ADA on Cardano, the native gas coin on an EVM chain.
    Native,
    /// An asset `chains` can identify — a Cardano policy and name, or an EVM NFT.
    Asset(TokenRef),
    /// A token whose standard `chains` does not name — an ERC-20 payment token.
    ///
    /// Carried as an address and a ticker rather than forced into a [`TokenRef`]: the
    /// identity parser accepts the NFT namespaces only, so "erc20" is not a namespace
    /// it can build. Inventing one here would fail on the first real price, which is
    /// exactly what an adapter is for finding out.
    Contract {
        /// The token's contract address.
        address: String,
        /// The ticker the marketplace states.
        symbol: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use chains::ChainRef;
    use std::str::FromStr;

    fn chain() -> ChainRef {
        ChainRef::from_str("cardano:mainnet").unwrap()
    }

    fn seller() -> MarketParty {
        MarketParty {
            role: PartyRole::Seller,
            account: AccountRef::new(chain(), "addr1vseller"),
        }
    }

    fn buyer() -> MarketParty {
        MarketParty {
            role: PartyRole::Buyer,
            account: AccountRef::new(chain(), "addr1vbuyer"),
        }
    }

    fn ask() -> Side {
        Side::Ask
    }

    #[test]
    fn a_listing_states_its_side_and_its_price() {
        let event = MarketEvent::Placed {
            order: OrderRef::new("ab:0"),
            side: ask(),
            price: Price::native(2_000_000, 6),
            parties: vec![seller()],
        };
        assert_eq!(event.kind(), "placed");
        assert_eq!(event.discriminator(), "ab:0");
        assert_eq!(event.order().map(|order| order.id.as_str()), Some("ab:0"));
    }

    #[test]
    fn a_re_price_is_not_a_second_listing() {
        // The distinction that a struct of options could not state: `Updated` exists so a
        // consumer counting listings does not have to infer one from a repeated order id.
        let listed = MarketEvent::Placed {
            order: OrderRef::new("ab:0"),
            side: ask(),
            price: Price::native(2_000_000, 6),
            parties: vec![seller()],
        };
        let repriced = MarketEvent::Updated {
            order: OrderRef::new("ab:0"),
            price: Price::native(1_500_000, 6),
        };
        assert_eq!(listed.kind(), "placed");
        assert_eq!(repriced.kind(), "updated");
        assert_eq!(listed.discriminator(), repriced.discriminator());
    }

    #[test]
    fn a_fill_carries_both_sides_and_what_it_settled_for() {
        let event = MarketEvent::Filled {
            order: Order::Named(OrderRef::new("ab:0")),
            side: ask(),
            price: Some(Price::native(1_500_000, 6)),
            parties: vec![seller(), buyer()],
        };
        assert_eq!(event.kind(), "filled");
        assert_eq!(event.discriminator(), "ab:0");
    }

    #[test]
    fn an_anonymous_fill_is_separated_by_its_parties_not_by_an_empty_id() {
        // The order is unknown (a Cardano sale of a listing the walk never saw), so the
        // parties are what makes two such sales different.
        let first = MarketEvent::Filled {
            order: Order::Unknown,
            side: ask(),
            price: None,
            parties: vec![seller(), buyer()],
        };
        let second = MarketEvent::Filled {
            order: Order::Unknown,
            side: ask(),
            price: None,
            parties: vec![seller()],
        };
        assert_ne!(first.discriminator(), second.discriminator());
        assert!(first.discriminator().contains("buyer"));
        // The state is explicit, so nothing can read it as "an order with an empty id".
        assert!(first.order().is_none());
    }

    #[test]
    fn the_party_discriminator_does_not_depend_on_order() {
        let one = MarketEvent::Filled {
            order: Order::Unknown,
            side: ask(),
            price: None,
            parties: vec![seller(), buyer()],
        };
        let two = MarketEvent::Filled {
            order: Order::Unknown,
            side: ask(),
            price: None,
            parties: vec![buyer(), seller()],
        };
        assert_eq!(one.discriminator(), two.discriminator());
    }

    #[test]
    fn a_bid_carries_the_scope_it_will_accept() {
        let item = Side::Bid(OfferScope::Item);
        let collection = Side::Bid(OfferScope::Collection);
        assert_eq!(ask().scope(), None);
        assert_eq!(item.scope(), Some(OfferScope::Item));
        assert_eq!(collection.scope(), Some(OfferScope::Collection));
    }

    #[test]
    fn an_invalidation_is_not_a_withdrawal() {
        // The distinction OpenSea's `order_invalidate` forces: it says an order stopped
        // being valid, and a fold that treated it as a withdrawal would remove an order
        // that had merely expired.
        let invalidated = MarketEvent::Invalidated {
            order: OrderRef::new("cd:1"),
        };
        let cancelled = MarketEvent::Cancelled {
            order: Order::Named(OrderRef::new("cd:1")),
            side: None,
            price: None,
            parties: Vec::new(),
        };
        assert_eq!(invalidated.kind(), "invalidated");
        assert_eq!(cancelled.kind(), "cancelled");
        assert_ne!(invalidated, cancelled);
    }

    #[test]
    fn an_unnamed_order_is_a_state_and_not_a_missing_value() {
        // The difference the explicit variant buys: `None` reads like a value that happens
        // to be missing, and `unwrap_or_default()` on one yields an empty join key that
        // matches nothing and raises nothing.
        let named = Order::Named(OrderRef::new("ab:0"));
        assert_eq!(named.named().map(|order| order.id.as_str()), Some("ab:0"));
        assert!(Order::Unknown.named().is_none());

        let json = serde_json::to_string(&Order::Unknown).unwrap();
        assert_eq!(json, "\"unknown\"");
        let back: Order = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Order::Unknown);
    }

    #[test]
    fn the_family_kind_reaches_the_envelope() {
        let body = crate::EventBody::Market(MarketEvent::Invalidated {
            order: OrderRef::new("ef:2"),
        });
        assert_eq!(body.kind(), "invalidated");
        assert_eq!(body.family(), crate::Family::Market);
    }

    #[test]
    fn a_price_above_u64_round_trips_as_a_string() {
        // 0.9942781770581453 ETH in wei is well inside u64, so the case that matters
        // is the one that is not: a value a JSON number would lose in a browser.
        let price = Price::contract(
            340_282_366_920_938_463_463_374_607_431_768_211_455,
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            "WETH",
            18,
        );
        let json = serde_json::to_string(&price).unwrap();
        assert!(json.contains("340282366920938463463374607431768211455"));
        let back: Price = serde_json::from_str(&json).unwrap();
        assert_eq!(back, price);
    }

    #[test]
    fn the_body_is_tagged_by_its_transition() {
        let body = MarketEvent::Placed {
            order: OrderRef::new("ab:0"),
            side: Side::Bid(OfferScope::Trait),
            price: Price::native(7, 6),
            parties: Vec::new(),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.starts_with(r#"{"kind":"placed""#), "{json}");
        assert!(json.contains(r#""side":{"bid":"trait"}"#), "{json}");
        let back: MarketEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, body);
    }
}
