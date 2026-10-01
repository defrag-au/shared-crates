//! The subscription vocabulary — what a socket can be joined to.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

/// What a per-collection topic carries in front of its slug.
const COLLECTION_PREFIX: &str = "collection:";

/// The slug position that means "every collection" rather than one.
const WILDCARD_SLUG: &str = "*";

/// The socket's own heartbeat topic.
const SYSTEM_TOPIC: &str = "phoenix";

/// A subscription topic.
///
/// # The slug trap
///
/// [`Topic::Collection`]'s slug is not the slug the REST API's
/// `/collections/{slug}` endpoint returns, and a wrong slug is silent: the join
/// replies `ok` and then nothing ever arrives. The suffixed form that works is
/// what `/chain/{chain}/contract/{address}` returns. See the crate docs for the
/// measurement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Topic {
    /// One collection: `collection:<slug>`.
    Collection(String),
    /// Every collection on every chain: `collection:*`.
    ///
    /// Discovery only. Measured at ~4,100 frames/s, ~85% of it metadata and
    /// cancellation churn, so it is not a steady-state subscription.
    AllCollections,
    /// `phoenix` — the socket's own heartbeat topic, not a collection.
    System,
}

impl Topic {
    /// One collection's events.
    pub fn collection(slug: impl Into<String>) -> Self {
        Self::Collection(slug.into())
    }

    /// The topic as it appears in a frame.
    pub fn as_wire(&self) -> String {
        match self {
            Self::Collection(slug) => format!("{COLLECTION_PREFIX}{slug}"),
            Self::AllCollections => format!("{COLLECTION_PREFIX}{WILDCARD_SLUG}"),
            Self::System => SYSTEM_TOPIC.to_owned(),
        }
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_wire())
    }
}

impl FromStr for Topic {
    type Err = TopicParseError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        if raw == SYSTEM_TOPIC {
            return Ok(Self::System);
        }

        if let Some(slug) = raw.strip_prefix(COLLECTION_PREFIX) {
            if slug == WILDCARD_SLUG {
                return Ok(Self::AllCollections);
            }
            // An empty slug is rejected rather than accepted as a collection
            // with no name: it is what OpenSea's own raw example joins, and it
            // answers `error`. Accepting it here would turn their bug into ours.
            if !slug.is_empty() {
                return Ok(Self::Collection(slug.to_owned()));
            }
        }

        Err(TopicParseError {
            topic: raw.to_owned(),
        })
    }
}

impl Serialize for Topic {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_wire())
    }
}

impl<'de> Deserialize<'de> for Topic {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TopicVisitor;

        impl<'de> Visitor<'de> for TopicVisitor {
            type Value = Topic;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a stream topic string")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Topic, E> {
                value.parse().map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(TopicVisitor)
    }
}

/// A string that is not a topic this crate recognises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicParseError {
    topic: String,
}

impl fmt::Display for TopicParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let topic = &self.topic;
        write!(
            formatter,
            "not a stream topic: {topic:?} (expected \"collection:<slug>\", \"collection:*\" or \"phoenix\")"
        )
    }
}

impl std::error::Error for TopicParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_collection_topic_round_trips_through_its_wire_form() {
        let topic: Topic = "collection:stonkbrokers-434284142".parse().unwrap();
        assert_eq!(
            topic,
            Topic::Collection("stonkbrokers-434284142".to_owned())
        );
        assert_eq!(topic.as_wire(), "collection:stonkbrokers-434284142");
    }

    #[test]
    fn the_wildcard_is_its_own_variant() {
        assert_eq!(
            "collection:*".parse::<Topic>().unwrap(),
            Topic::AllCollections
        );
    }

    #[test]
    fn the_system_topic_is_not_a_collection() {
        assert_eq!("phoenix".parse::<Topic>().unwrap(), Topic::System);
    }

    #[test]
    fn the_documented_empty_slug_is_rejected() {
        // OpenSea's raw-connection example joins `collection:`. It answers
        // `error`; only `collection:*` is the wildcard.
        let error = "collection:".parse::<Topic>().unwrap_err();
        assert_eq!(error.topic, "collection:");
    }

    #[test]
    fn anything_else_is_rejected() {
        assert!("".parse::<Topic>().is_err());
        assert!("collections:stonkbrokers".parse::<Topic>().is_err());
        assert!("stonkbrokers-434284142".parse::<Topic>().is_err());
    }
}
