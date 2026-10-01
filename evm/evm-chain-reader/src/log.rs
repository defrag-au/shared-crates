//! Logs: how a contract's history is asked for and what comes back.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::block::BlockTag;
use crate::hex::{self, HexError, Quantity};

/// A `0x`-prefixed, even-length hex blob — calldata, or a log's `data`.
///
/// Validated on the way in so a response that is not hex is a decode error at the
/// boundary rather than a surprise inside a decoder. `"0x"` is the empty blob, and
/// is legitimate: it is what a node returns for a call the contract does not
/// implement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HexData(String);

impl HexData {
    /// Validates and keeps a hex blob.
    pub fn new(raw: impl Into<String>) -> Result<Self, HexError> {
        let raw = raw.into();
        hex::decode(&raw)?;
        Ok(Self(raw.to_ascii_lowercase()))
    }

    /// Encodes bytes as a hex blob.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(hex::to_hex(bytes))
    }

    /// The blob as it appears on the wire.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The bytes, decoded.
    pub fn decode(&self) -> Result<Vec<u8>, HexError> {
        hex::decode(&self.0)
    }

    /// Whether there are no bytes — `"0x"`.
    pub fn is_empty(&self) -> bool {
        self.0.len() <= 2
    }
}

impl fmt::Display for HexData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for HexData {
    type Error = HexError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(raw)
    }
}

impl From<HexData> for String {
    fn from(data: HexData) -> Self {
        data.0
    }
}

/// One of a log's indexed words — a 32-byte hash, or an address padded to one.
///
/// Normalised to 64 digits by left-padding, so a topic from a response and the same
/// topic as a constant compare equal. Empty is refused: a missing word is not a
/// word.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LogTopic(String);

impl LogTopic {
    /// Reads a topic word, `0x`-prefixed and at most 64 digits.
    pub fn parse(raw: &str) -> Result<Self, HexError> {
        let digits = raw
            .strip_prefix("0x")
            .or_else(|| raw.strip_prefix("0X"))
            .unwrap_or(raw);

        if digits.is_empty() {
            return Err(HexError::Empty);
        }
        if digits.len() > 64 {
            return Err(HexError::TooLong(digits.len()));
        }

        let padded = format!("{digits:0>64}");
        let bytes = hex::decode(&format!("0x{padded}"))?;
        Ok(Self(format!("0x{}", hex::encode(&bytes))))
    }

    /// The word, lower-case, 64 digits.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LogTopic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for LogTopic {
    type Error = HexError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(&raw)
    }
}

impl From<LogTopic> for String {
    fn from(topic: LogTopic) -> Self {
        topic.0
    }
}

/// What `eth_getLogs` is asked for.
///
/// `topics` is positional: index 0 matches `topics[0]` of a log, and a `None` in a
/// position is the wildcard. That is what lets a scan ask for "a `Transfer` into
/// this address" without asking for every `Transfer` the contract ever emitted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFilter {
    /// One contract, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    /// The first block to include. A node's own default is usually `latest`, which
    /// is rarely what a reconcile wants — set it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_block: Option<BlockTag>,
    /// The last block to include.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_block: Option<BlockTag>,
    /// The positional topic filter. Empty means every log.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<Option<LogTopic>>,
}

impl LogFilter {
    /// Every log of a contract across a block range.
    pub fn contract(address: Address, from: BlockTag, to: BlockTag) -> Self {
        Self {
            address: Some(address),
            from_block: Some(from),
            to_block: Some(to),
            topics: Vec::new(),
        }
    }

    /// Narrows the positional topic filter.
    ///
    /// `None` is a wildcard position, so `[Some(transfer), None, Some(to)]` is
    /// "any transfer whose second indexed argument is `to`".
    pub fn topics(mut self, topics: Vec<Option<LogTopic>>) -> Self {
        self.topics = topics;
        self
    }

    /// The range, when the filter sets one.
    pub fn range(&self) -> Option<(BlockTag, BlockTag)> {
        Some((self.from_block?, self.to_block?))
    }
}

/// One log, as `eth_getLogs` returns it.
///
/// Field names are the wire's camelCase, so this is also what a capture of a log
/// query reads and writes. `removed` is a reorg marker: a node that has already
/// answered once can answer again with `removed: true` for the same log, meaning it
/// is no longer in the canonical chain. A reconcile that ignores it will keep an
/// ownership row that the chain has undone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    /// The contract that emitted it.
    pub address: Address,
    /// Its indexed words, `topics[0]` first.
    pub topics: Vec<LogTopic>,
    /// The non-indexed part.
    pub data: HexData,
    /// The block it is in.
    #[serde(default)]
    pub block_number: Option<Quantity>,
    /// The block hash — the field to compare when checking a reorg.
    #[serde(default)]
    pub block_hash: Option<HexData>,
    /// The transaction that emitted it.
    #[serde(default)]
    pub transaction_hash: Option<HexData>,
    /// The transaction's position in the block.
    #[serde(default)]
    pub transaction_index: Option<Quantity>,
    /// The log's position in the block.
    #[serde(default)]
    pub log_index: Option<Quantity>,
    /// Whether a reorg has removed it.
    #[serde(default)]
    pub removed: Option<bool>,
}

impl Log {
    /// The block height, if the node filled it in.
    pub fn block(&self) -> Option<u64> {
        self.block_number.map(Quantity::get)
    }

    /// Whether a reorg has removed this log.
    pub fn is_removed(&self) -> bool {
        self.removed.unwrap_or(false)
    }

    /// The transaction hash, lower-case hex, if there is one.
    pub fn transaction(&self) -> Option<&str> {
        self.transaction_hash.as_ref().map(HexData::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MADJACKET: &str = "0x7980aa64093853cb78c927e05b88fed96e945f81";
    const TRANSFER: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

    fn address() -> Address {
        Address::parse(MADJACKET).unwrap()
    }

    #[test]
    fn a_filter_writes_as_the_rpc_expects() {
        let filter = LogFilter::contract(address(), BlockTag::Number(100), BlockTag::Latest);
        assert_eq!(
            serde_json::to_string(&filter).unwrap(),
            format!(r#"{{"address":"{MADJACKET}","fromBlock":"0x64","toBlock":"latest"}}"#)
        );
    }

    #[test]
    fn a_wildcard_topic_position_writes_as_null() {
        let recipient =
            LogTopic::parse(&format!("0x{}{}", "0".repeat(24), &MADJACKET[2..])).unwrap();
        let filter = LogFilter::contract(address(), BlockTag::Number(1), BlockTag::Number(2))
            .topics(vec![
                Some(LogTopic::parse(TRANSFER).unwrap()),
                None,
                Some(recipient),
            ]);

        let json = serde_json::to_string(&filter).unwrap();
        assert!(json.contains(r#""topics":["0xddf252ad"#), "got {json}");
        assert!(json.contains(",null,"), "got {json}");
    }

    #[test]
    fn a_filter_without_a_topic_narrows_to_nothing() {
        let filter = LogFilter::contract(address(), BlockTag::Number(1), BlockTag::Number(2));
        assert!(!serde_json::to_string(&filter).unwrap().contains("topics"));
    }

    #[test]
    fn a_hex_blob_keeps_its_bytes_and_rejects_non_hex() {
        assert_eq!(
            HexData::new("0x00ff").unwrap().decode().unwrap(),
            vec![0, 255]
        );
        assert!(HexData::new("0xzz").is_err());
        assert!(HexData::new("00ff").is_err());
        assert!(HexData::from_bytes(&[]).is_empty());
    }

    #[test]
    fn a_topic_is_padded_to_a_full_word() {
        let topic = LogTopic::parse("0xab").unwrap();
        assert_eq!(topic.as_str(), format!("0x{}ab", "0".repeat(62)));
    }

    #[test]
    fn an_empty_topic_is_refused() {
        assert!(LogTopic::parse("0x").is_err());
    }

    #[test]
    fn a_log_reads_from_a_node_response() {
        let raw = format!(
            r#"{{
                "address": "{MADJACKET}",
                "topics": [
                    "{TRANSFER}",
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "0x00000000000000000000000061040e143a77f165ba44543af4a079f2c809d14b"
                ],
                "data": "0x00000000000000000000000000000000000000000000000000000000000004d2",
                "blockNumber": "0x1237",
                "blockHash": "0x1111111111111111111111111111111111111111111111111111111111111111",
                "transactionHash": "0x2222222222222222222222222222222222222222222222222222222222222222",
                "transactionIndex": "0x1",
                "logIndex": "0x0",
                "removed": false
            }}"#
        );

        let log: Log = serde_json::from_str(&raw).unwrap();
        assert_eq!(log.block(), Some(4663));
        assert_eq!(log.topics.len(), 3);
        assert!(!log.is_removed());
        assert_eq!(
            log.transaction().unwrap(),
            "0x2222222222222222222222222222222222222222222222222222222222222222"
        );
    }

    #[test]
    fn a_log_without_the_optional_block_fields_still_reads() {
        let raw = format!(r#"{{"address":"{MADJACKET}","topics":[],"data":"0x"}}"#);
        let log: Log = serde_json::from_str(&raw).unwrap();
        assert_eq!(log.block(), None);
        assert!(!log.is_removed());
    }
}
