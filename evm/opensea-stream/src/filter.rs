//! What a subscription asks for.

use serde::de::Deserializer;
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

use crate::event_type::EventType;

/// The join payload: which event types a subscription wants.
///
/// `All` and an empty `Types` are the same thing on the wire — `{}` — because
/// that is what "no filter" looks like, and this type collapses them on read so
/// there is no second way to spell it.
///
/// # A filter that matches nothing is silent
///
/// OpenSea does not validate type names. A filter naming a type the collection
/// never emits — a typo included — joins `ok` and then delivers nothing, which is
/// indistinguishable from a quiet collection. That is the same failure shape as a
/// wrong slug; see the crate docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventFilter {
    /// Every event type the collection emits.
    All,
    /// Only these, as far as OpenSea honours them.
    Types(Vec<EventType>),
}

impl EventFilter {
    /// Only the given event types.
    pub fn only(event_types: impl IntoIterator<Item = EventType>) -> Self {
        Self::Types(event_types.into_iter().collect())
    }
}

impl Serialize for EventFilter {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::All => serializer.serialize_map(Some(0))?.end(),
            Self::Types(event_types) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("event_types", event_types)?;
                map.end()
            }
        }
    }
}

/// The payload as it arrives, before [`EventFilter`] decides what it means.
#[derive(Deserialize)]
struct EventFilterRepr {
    #[serde(default)]
    event_types: Vec<EventType>,
}

impl From<EventFilterRepr> for EventFilter {
    fn from(repr: EventFilterRepr) -> Self {
        if repr.event_types.is_empty() {
            Self::All
        } else {
            Self::Types(repr.event_types)
        }
    }
}

impl<'de> Deserialize<'de> for EventFilter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(EventFilterRepr::deserialize(deserializer)?.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_serialises_as_the_empty_payload() {
        assert_eq!(serde_json::to_string(&EventFilter::All).unwrap(), "{}");
    }

    #[test]
    fn types_serialise_into_an_event_types_array() {
        let filter = EventFilter::only([EventType::ItemSold]);
        assert_eq!(
            serde_json::to_string(&filter).unwrap(),
            r#"{"event_types":["item_sold"]}"#
        );
    }

    #[test]
    fn the_empty_payload_reads_back_as_all() {
        let filter: EventFilter = serde_json::from_str("{}").unwrap();
        assert_eq!(filter, EventFilter::All);
    }

    #[test]
    fn a_named_type_reads_back_as_types() {
        let filter: EventFilter =
            serde_json::from_str(r#"{"event_types":["item_listed"]}"#).unwrap();
        assert_eq!(filter, EventFilter::only([EventType::ItemListed]));
    }

    #[test]
    fn an_explicitly_empty_array_collapses_to_all() {
        let filter: EventFilter = serde_json::from_str(r#"{"event_types":[]}"#).unwrap();
        assert_eq!(filter, EventFilter::All);
    }
}
