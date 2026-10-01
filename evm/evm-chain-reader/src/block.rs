//! Which block a read is against.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::hex::{self, HexError};

/// The block a call or a log query is evaluated against.
///
/// A number or one of the three tags a node understands. `pending` is deliberately
/// absent from what [`BlockTag::Latest`] means: a pending block is not yet mined,
/// so a read against it can see writes that a reorg then undoes, which is the
/// opposite of what a reconcile wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlockTag {
    /// A specific height.
    Number(u64),
    /// The head of the chain.
    Latest,
    /// Genesis.
    Earliest,
}

impl BlockTag {
    /// The `eth_getLogs` / `eth_call` wire value.
    pub fn as_wire(self) -> String {
        match self {
            Self::Number(height) => hex::quantity_string(height),
            Self::Latest => "latest".to_owned(),
            Self::Earliest => "earliest".to_owned(),
        }
    }

    /// The height, when the tag names one.
    pub fn height(self) -> Option<u64> {
        match self {
            Self::Number(height) => Some(height),
            Self::Latest | Self::Earliest => None,
        }
    }
}

impl fmt::Display for BlockTag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_wire())
    }
}

impl From<u64> for BlockTag {
    fn from(height: u64) -> Self {
        Self::Number(height)
    }
}

impl Serialize for BlockTag {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_wire())
    }
}

impl<'de> Deserialize<'de> for BlockTag {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BlockTagVisitor;

        impl Visitor<'_> for BlockTagVisitor {
            type Value = BlockTag;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a block number as a hex quantity, or \"latest\"/\"earliest\"")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<BlockTag, E> {
                match value {
                    "latest" => Ok(BlockTag::Latest),
                    "earliest" => Ok(BlockTag::Earliest),
                    other => hex::quantity(other)
                        .map(BlockTag::Number)
                        .map_err(|error: HexError| de::Error::custom(error.to_string())),
                }
            }
        }

        deserializer.deserialize_str(BlockTagVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_writes_as_hex_or_as_a_name() {
        assert_eq!(BlockTag::Number(4663).as_wire(), "0x1237");
        assert_eq!(BlockTag::Latest.as_wire(), "latest");
        assert_eq!(BlockTag::Earliest.as_wire(), "earliest");
    }

    #[test]
    fn a_tag_round_trips_through_its_wire_form() {
        for tag in [
            BlockTag::Number(0),
            BlockTag::Number(19_000_000),
            BlockTag::Latest,
        ] {
            let json = serde_json::to_string(&tag).unwrap();
            assert_eq!(serde_json::from_str::<BlockTag>(&json).unwrap(), tag);
        }
    }

    #[test]
    fn a_tag_reads_from_a_bare_json_string() {
        assert_eq!(
            serde_json::from_str::<BlockTag>("\"0x1237\"").unwrap(),
            BlockTag::Number(4663)
        );
        assert_eq!(
            serde_json::from_str::<BlockTag>("\"latest\"").unwrap(),
            BlockTag::Latest
        );
    }
}
