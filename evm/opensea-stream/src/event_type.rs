//! The event types the stream delivers.

use serde::de::Deserializer;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::strings::MapStrVisitor;

/// What a streamed event is about, and one half of what a subscription can be
/// narrowed to.
///
/// [`EventType::Unknown`] carries a type this crate does not name. That arm is
/// the point: OpenSea adds event types, and a reader that cannot represent a new
/// one stops delivering rather than ignoring it.
///
/// Every wire value below has been seen on the live socket — all ten appear in a
/// single 20 s wildcard sample — so none of them is aspirational.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventType {
    /// `item_sold`
    ItemSold,
    /// `item_listed`
    ItemListed,
    /// `item_transferred`
    ItemTransferred,
    /// `item_metadata_updated`
    ItemMetadataUpdated,
    /// `item_cancelled`
    ItemCancelled,
    /// `item_received_bid` — an offer on one item.
    ItemReceivedBid,
    /// `collection_offer`
    CollectionOffer,
    /// `trait_offer`
    TraitOffer,
    /// `order_invalidate`
    OrderInvalidate,
    /// `order_revalidate`
    OrderRevalidate,
    /// An event type this crate does not name, carried verbatim.
    Unknown(String),
}

impl EventType {
    /// The wire value.
    pub fn as_wire(&self) -> &str {
        match self {
            Self::ItemSold => "item_sold",
            Self::ItemListed => "item_listed",
            Self::ItemTransferred => "item_transferred",
            Self::ItemMetadataUpdated => "item_metadata_updated",
            Self::ItemCancelled => "item_cancelled",
            Self::ItemReceivedBid => "item_received_bid",
            Self::CollectionOffer => "collection_offer",
            Self::TraitOffer => "trait_offer",
            Self::OrderInvalidate => "order_invalidate",
            Self::OrderRevalidate => "order_revalidate",
            Self::Unknown(value) => value,
        }
    }

    /// The inverse of [`EventType::as_wire`], total by construction.
    pub(crate) fn from_wire(value: &str) -> Self {
        match value {
            "item_sold" => Self::ItemSold,
            "item_listed" => Self::ItemListed,
            "item_transferred" => Self::ItemTransferred,
            "item_metadata_updated" => Self::ItemMetadataUpdated,
            "item_cancelled" => Self::ItemCancelled,
            "item_received_bid" => Self::ItemReceivedBid,
            "collection_offer" => Self::CollectionOffer,
            "trait_offer" => Self::TraitOffer,
            "order_invalidate" => Self::OrderInvalidate,
            "order_revalidate" => Self::OrderRevalidate,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_wire())
    }
}

impl Serialize for EventType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for EventType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(MapStrVisitor(EventType::from_wire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_type_round_trips_through_its_wire_value() {
        for event_type in [
            EventType::ItemSold,
            EventType::ItemListed,
            EventType::ItemTransferred,
            EventType::ItemMetadataUpdated,
            EventType::ItemCancelled,
            EventType::ItemReceivedBid,
            EventType::CollectionOffer,
            EventType::TraitOffer,
            EventType::OrderInvalidate,
            EventType::OrderRevalidate,
        ] {
            assert_eq!(EventType::from_wire(event_type.as_wire()), event_type);
        }
    }

    #[test]
    fn an_unrecognised_type_is_kept_rather_than_dropped() {
        assert_eq!(
            EventType::from_wire("item_something_new"),
            EventType::Unknown("item_something_new".to_owned())
        );
    }
}
