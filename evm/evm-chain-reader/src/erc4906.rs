//! ERC-4906: the optional signal that a token's metadata has changed.
//!
//! The design note's open question is whether the Robinhood-chain collections
//! implement this, and it is not answerable from the OpenSea stream — the stream
//! reports that metadata *did* change, not why or whether a contract said so.
//! [`EvmRpcClient::supports_erc4906`] is the check, and it decides which of two
//! shapes a metadata reconcile takes:
//!
//! - **Supports it** — `MetadataUpdate` is a replayable log, so a gap is closed by
//!   scanning a block range, exactly as `Transfer` is.
//! - **Does not** — the contract never says, so metadata changes are a
//!   marketplace-stream trigger with a periodic sweep behind it, because the chain
//!   has no event to replay.
//!
//! That is what makes this a fork in the reconcile design rather than a detail.
//!
//! Both topics below are verified against a signature database, not recalled.

use crate::abi::{self, AbiError};
use crate::address::Address;
use crate::block::BlockTag;
use crate::hex;
use crate::log::{HexData, Log, LogFilter, LogTopic};
use crate::rpc::{EvmRpcClient, RpcError};
use crate::uint256::U256;

/// `keccak256("MetadataUpdate(uint256)")` — one token's metadata changed.
pub const METADATA_UPDATE_TOPIC: &str =
    "0xf8e1a15aba9398e019f0b49df1a4fde98ee17ae345cb5f6b5e2c27f5033e8ce7";

/// `keccak256("BatchMetadataUpdate(uint256,uint256)")` — an inclusive range of
/// tokens changed, `from` and `to` in the data.
pub const BATCH_METADATA_UPDATE_TOPIC: &str =
    "0x6bd5c950a8d8df17f772f5af37cb3655737899cbf903264b9795592da439661c";

/// ERC-4906's ERC-165 interface id — what `supportsInterface` is asked.
pub const INTERFACE_ID: [u8; 4] = [0x49, 0x06, 0x49, 0x06];

/// `supportsInterface(bytes4)`.
const SUPPORTS_INTERFACE: &str = "0x01ffc9a7";

/// What a metadata-update event said changed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MetadataUpdate {
    /// One token.
    One(U256),
    /// An inclusive range of token ids.
    Range { from: U256, to: U256 },
}

impl MetadataUpdate {
    /// Every token id the update names, when the range is small enough to expand.
    ///
    /// A range is expressed as two 256-bit ends, and expanding it is the caller's
    /// decision because a contract is free to emit a range wider than any
    /// collection: `None` means "too wide to enumerate", which is a fact worth
    /// stating rather than a bound this crate picks silently.
    pub fn expand(&self, limit: u128) -> Option<Vec<U256>> {
        match self {
            Self::One(token) => Some(vec![*token]),
            Self::Range { from, to } => {
                let start = from.to_u64()?;
                let end = to.to_u64()?;
                let width = u128::from(end.checked_sub(start)?) + 1;
                if width > limit {
                    return None;
                }
                Some((start..=end).map(U256::from_u64).collect())
            }
        }
    }
}

/// A metadata update as a log reported it, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MetadataEvent {
    /// What changed.
    pub update: MetadataUpdate,
    /// The transaction, when the node filled it in.
    pub transaction: Option<String>,
    /// The block.
    pub block: Option<u64>,
    /// Its position in the block.
    pub log_index: Option<u64>,
}

impl MetadataEvent {
    fn from_log(log: &Log) -> Result<Self, AbiError> {
        Ok(Self {
            update: decode_metadata_update(log)?,
            transaction: log.transaction().map(str::to_owned),
            block: log.block(),
            log_index: log.log_index.map(|index| index.get()),
        })
    }
}

/// Decodes either ERC-4906 event.
pub fn decode_metadata_update(log: &Log) -> Result<MetadataUpdate, AbiError> {
    let Some(topic) = log.topics.first() else {
        return Err(AbiError::WrongTopics {
            expected: "at least 1",
            got: 0,
        });
    };

    let data = log.data.decode()?;
    match topic.as_str() {
        METADATA_UPDATE_TOPIC => Ok(MetadataUpdate::One(U256::from_word(abi::word_at(
            &data, 0,
        )?))),
        BATCH_METADATA_UPDATE_TOPIC => Ok(MetadataUpdate::Range {
            from: U256::from_word(abi::word_at(&data, 0)?),
            to: U256::from_word(abi::word_at(&data, 1)?),
        }),
        other => Err(AbiError::WrongEvent {
            expected: "MetadataUpdate(uint256) or BatchMetadataUpdate(uint256,uint256)",
            got: other.to_owned(),
        }),
    }
}

/// The `supportsInterface` calldata for one interface id — the id left-padded to a
/// word.
fn interface_calldata(id: [u8; 4]) -> Result<HexData, AbiError> {
    abi::calldata(
        SUPPORTS_INTERFACE,
        &format!("0x{}{}", "0".repeat(56), hex::encode(&id)),
    )
}

impl EvmRpcClient {
    /// `supportsInterface(bytes4)` — ERC-165.
    ///
    /// A contract that does not implement ERC-165 at all reverts rather than
    /// answering `false`, which arrives as [`RpcError::Rpc`]. So "no answer" and
    /// "answered no" are different outcomes here, deliberately: the first means
    /// there is nothing to ask, the second means the contract was asked and said no.
    pub async fn supports_interface(
        &self,
        contract: &Address,
        id: [u8; 4],
        block: BlockTag,
    ) -> Result<bool, RpcError> {
        let data = interface_calldata(id)?;
        let result = self.eth_call(contract, &data, block).await?;
        Ok(abi::boolean(&abi::single_word(&result)?))
    }

    /// Whether a contract implements ERC-4906.
    pub async fn supports_erc4906(
        &self,
        contract: &Address,
        block: BlockTag,
    ) -> Result<bool, RpcError> {
        self.supports_interface(contract, INTERFACE_ID, block).await
    }

    /// Every ERC-4906 event of a contract over a block range, in block order.
    ///
    /// Two queries, because the two event types have different `topics[0]` and the
    /// filter's positional form has no "either of these" in this crate — the fix
    /// for which is a topic that holds a set, which nothing has needed yet.
    pub async fn metadata_updates(
        &self,
        contract: &Address,
        from: BlockTag,
        to: BlockTag,
    ) -> Result<Vec<MetadataEvent>, RpcError> {
        let mut events = Vec::new();
        for topic in [METADATA_UPDATE_TOPIC, BATCH_METADATA_UPDATE_TOPIC] {
            let filter = LogFilter::contract(contract.clone(), from, to)
                .topics(vec![Some(LogTopic::parse(topic)?)]);
            for log in self.get_logs(&filter).await? {
                if log.is_removed() {
                    continue;
                }
                events.push(MetadataEvent::from_log(&log)?);
            }
        }

        // The two queries are independent, so the merge has to restore order.
        // A log with no block is put last rather than first: `None` is a node that
        // omitted the field, not a block zero.
        events.sort_by_key(|event| {
            (
                event.block.unwrap_or(u64::MAX),
                event.log_index.unwrap_or(u64::MAX),
            )
        });
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::Quantity;

    const MADJACKET: &str = "0x7980aa64093853cb78c927e05b88fed96e945f81";

    fn log_with(topic: &str, data: &str) -> Log {
        Log {
            address: Address::parse(MADJACKET).unwrap(),
            topics: vec![LogTopic::parse(topic).unwrap()],
            data: HexData::new(data).unwrap(),
            block_number: Some(Quantity::from(17_000_000)),
            block_hash: None,
            transaction_hash: None,
            transaction_index: None,
            log_index: Some(Quantity::from(1)),
            removed: None,
        }
    }

    #[test]
    fn a_single_token_update_decodes() {
        let log = log_with(METADATA_UPDATE_TOPIC, &format!("0x{:0>64}", "4d2"));
        assert_eq!(
            decode_metadata_update(&log).unwrap(),
            MetadataUpdate::One(U256::from_decimal("1234").unwrap())
        );
    }

    #[test]
    fn a_batch_update_decodes_its_range() {
        let log = log_with(
            BATCH_METADATA_UPDATE_TOPIC,
            &format!("0x{:0>64}{:0>64}", "1", "a"),
        );
        assert_eq!(
            decode_metadata_update(&log).unwrap(),
            MetadataUpdate::Range {
                from: U256::from_decimal("1").unwrap(),
                to: U256::from_decimal("10").unwrap(),
            }
        );
    }

    #[test]
    fn a_log_of_another_event_is_refused() {
        let log = log_with(
            "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
            &format!("0x{:0>64}", "1"),
        );
        assert!(matches!(
            decode_metadata_update(&log).unwrap_err(),
            AbiError::WrongEvent { .. }
        ));
    }

    #[test]
    fn a_range_expands_only_when_the_caller_says_it_is_small_enough() {
        let update = MetadataUpdate::Range {
            from: U256::from_decimal("1").unwrap(),
            to: U256::from_decimal("3").unwrap(),
        };
        let expanded = update.expand(10).unwrap();
        assert_eq!(
            expanded.iter().map(U256::to_decimal).collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );
        assert!(update.expand(2).is_none());
    }

    #[test]
    fn a_single_update_always_expands() {
        let update = MetadataUpdate::One(U256::from_decimal("1234").unwrap());
        assert_eq!(update.expand(1).unwrap().len(), 1);
    }

    #[test]
    fn the_interface_id_is_the_one_the_standard_names() {
        // `0x49064906`, which is the value the design note and the EIP both write.
        assert_eq!(hex::encode(&INTERFACE_ID), "49064906");
    }

    #[test]
    fn the_supports_interface_call_is_the_selector_and_a_padded_id() {
        let data = interface_calldata(INTERFACE_ID).unwrap();
        assert_eq!(
            data.as_str(),
            format!("0x01ffc9a7{}{}", "0".repeat(56), "49064906")
        );
    }
}
