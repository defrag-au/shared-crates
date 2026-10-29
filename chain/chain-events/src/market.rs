//! Listings, sales, offers and the order lifecycle.
//!
//! This family is the one the two sources only partly reconcile. Cardano states a
//! listing as a fact (a UTxO with a datum) and sees every one of them; an EVM listing
//! is an off-chain signature until it is filled or cancelled, so EVM marketplace
//! history is never complete. The shape below is the **fact**, and a marketplace
//! message is interpreted into it — which is why [`MarketKind::OrderInvalidated`]
//! exists rather than a guess at which of the two it was.

use chains::TokenRef;
use serde::{Deserialize, Serialize};

use crate::event::AccountRef;

/// One marketplace fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketEvent {
    /// Which fact.
    pub kind: MarketKind,
    /// What the offer was against, for the offer kinds. A sale has none.
    pub scope: Option<OfferScope>,
    /// What it settled for, or was offered at.
    pub price: Option<Price>,
    /// The seller, or the maker of an order.
    pub seller: Option<AccountRef>,
    /// The buyer, or the taker.
    pub buyer: Option<AccountRef>,
    /// The order this fact is about — a hash on EVM, an output reference on Cardano.
    pub order: Option<OrderRef>,
    /// Why something ended, when the source says.
    pub reason: Option<MarketReason>,
}

impl MarketEvent {
    /// An event of `kind` with nothing else known yet.
    pub fn new(kind: MarketKind) -> Self {
        Self {
            kind,
            scope: None,
            price: None,
            seller: None,
            buyer: None,
            order: None,
            reason: None,
        }
    }

    /// A stable string for [`EventId::derive`](crate::EventId::derive).
    ///
    /// An order id identifies a listing, an offer and their lifecycle, so it is the
    /// discriminator whenever there is one. Without an order it falls back to the
    /// parties and the price — the two things that make two otherwise identical facts
    /// different — and only then to the kind alone.
    pub fn discriminator(&self) -> String {
        if let Some(order) = &self.order {
            return order.id.clone();
        }
        let parties = format!(
            "{}:{}",
            self.seller
                .as_ref()
                .map(|seller| seller.address.as_str())
                .unwrap_or(""),
            self.buyer
                .as_ref()
                .map(|buyer| buyer.address.as_str())
                .unwrap_or(""),
        );
        match &self.price {
            Some(price) => format!("{parties}:{}", price.amount),
            None => parties,
        }
    }
}

/// Which marketplace fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketKind {
    /// An asset was put up for sale.
    Listed,
    /// A listing ended without a sale.
    Delisted,
    /// An asset sold.
    Sold,
    /// An offer was made. See [`OfferScope`] for what it was against.
    OfferMade,
    /// An offer was taken.
    OfferAccepted,
    /// An offer was withdrawn or lapsed.
    OfferCancelled,
    /// An order stopped being valid, and the source does not say of which kind.
    OrderInvalidated,
    /// A previously invalidated order became valid again.
    OrderRevalidated,
}

impl MarketKind {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Listed => "listed",
            Self::Delisted => "delisted",
            Self::Sold => "sold",
            Self::OfferMade => "offer_made",
            Self::OfferAccepted => "offer_accepted",
            Self::OfferCancelled => "offer_cancelled",
            Self::OrderInvalidated => "order_invalidated",
            Self::OrderRevalidated => "order_revalidated",
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

/// Why a listing or offer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketReason {
    /// The owner withdrew it.
    Cancelled,
    /// It passed its end time.
    Expired,
    /// It was taken, and the sale is its own row.
    Fulfilled,
}

impl MarketReason {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::Fulfilled => "fulfilled",
        }
    }
}

/// The order a fact is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderRef {
    /// An order hash on EVM; a transaction output reference on Cardano, where the
    /// listing *is* a UTxO and there is no order id to speak of.
    pub id: String,
}

impl OrderRef {
    /// A reference to `id`.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

/// What something settled for.
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

    fn ada() -> TokenRef {
        TokenRef::from_str(
            "cardano:mainnet/cnt:00000000000000000000000000000000000000000000000000000000",
        )
        .unwrap()
    }

    fn seller() -> AccountRef {
        AccountRef::new(
            ChainRef::from_str("cardano:mainnet").unwrap(),
            "addr1vseller",
        )
    }

    #[test]
    fn a_listing_carries_its_price_and_order() {
        let event = MarketEvent {
            kind: MarketKind::Listed,
            scope: None,
            price: Some(Price::native(2_000_000, 6)),
            seller: Some(seller()),
            buyer: None,
            order: Some(OrderRef::new("ab".repeat(32))),
            reason: None,
        };
        assert_eq!(event.kind.as_str(), "listed");
        assert_eq!(event.price.as_ref().unwrap().amount, 2_000_000);
    }

    #[test]
    fn a_price_above_u64_round_trips_as_a_string() {
        // 0.9942781770581453 ETH in wei is well inside u64, so the case that matters
        // is the one that is not: a value a JSON number would lose in a browser.
        let price = Price::asset(
            340_282_366_920_938_463_463_374_607_431_768_211_455,
            ada(),
            18,
        );
        let json = serde_json::to_string(&price).unwrap();
        assert!(json.contains("340282366920938463463374607431768211455"));
        let back: Price = serde_json::from_str(&json).unwrap();
        assert_eq!(back, price);
    }

    #[test]
    fn a_native_currency_is_not_a_null_token() {
        let json = serde_json::to_string(&Currency::Native).unwrap();
        assert_eq!(json, "\"native\"");
        let back: Currency = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Currency::Native);
    }

    #[test]
    fn an_offer_states_its_scope_because_the_sources_use_three_messages() {
        let event = MarketEvent {
            kind: MarketKind::OfferMade,
            scope: Some(OfferScope::Trait),
            ..MarketEvent::new(MarketKind::OfferMade)
        };
        assert_eq!(event.scope.unwrap().as_str(), "trait");
    }

    #[test]
    fn the_discriminator_prefers_the_order_then_falls_back_to_the_parties() {
        let with_order = MarketEvent {
            order: Some(OrderRef::new("0xabc")),
            ..MarketEvent::new(MarketKind::Sold)
        };
        assert_eq!(with_order.discriminator(), "0xabc");

        let without = MarketEvent {
            seller: Some(seller()),
            price: Some(Price::native(7, 6)),
            ..MarketEvent::new(MarketKind::Sold)
        };
        assert_eq!(without.discriminator(), "addr1vseller::7");
    }

    #[test]
    fn an_invalidation_is_its_own_kind_rather_than_a_guess() {
        // The source does not say whether it killed a listing or a bid, so the
        // vocabulary does not either.
        assert_eq!(MarketKind::OrderInvalidated.as_str(), "order_invalidated");
        assert_eq!(MarketKind::OrderRevalidated.as_str(), "order_revalidated");
    }
}
