//! ERC-721: who owns a token, and how it moved.
//!
//! Two ways to answer "who owns what", and a reconcile wants both:
//!
//! - [`EvmRpcClient::owner_of`] and [`EvmRpcClient::balance_of`] read the contract's
//!   current state. This is truth, and it is the oracle a stream-fed ledger is
//!   checked against.
//! - [`EvmRpcClient::transfers`] replays `Transfer` logs over a block range. This is
//!   history, and it is the same event OpenSea's `item_transferred` reports — which
//!   is what makes it the check for whether the stream is a subset of the chain.
//!
//! The function selectors and the event topic are the standard ones, verified
//! against a signature database rather than recalled: `ownerOf(uint256)` is
//! `0x6352211e`, `balanceOf(address)` is `0x70a08231`, and
//! `Transfer(address,address,uint256)` is `0xddf252ad…3b3ef`.

use crate::abi::{self, AbiError};
use crate::address::Address;
use crate::block::BlockTag;
use crate::log::{Log, LogFilter, LogTopic};
use crate::rpc::{EvmRpcClient, RpcError};
use crate::uint256::U256;

/// `keccak256("Transfer(address,address,uint256)")` — the first indexed word of
/// every ERC-721 movement, mints and burns included.
pub const TRANSFER_TOPIC: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

/// `ownerOf(uint256)`.
const OWNER_OF: &str = "0x6352211e";

/// `balanceOf(address)`.
const BALANCE_OF: &str = "0x70a08231";

/// One asset's movement, decoded from a `Transfer` log.
///
/// A mint is `from` zero and a burn is `to` zero — the same mapping the design note
/// puts in the chain-specific normaliser, which is why the zero address is carried
/// rather than filtered here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Transfer {
    /// The contract that emitted it.
    pub contract: Address,
    /// Where it came from — the zero address on a mint.
    pub from: Address,
    /// Where it went — the zero address on a burn.
    pub to: Address,
    /// Which token.
    pub token_id: U256,
    /// The transaction, when the node filled it in.
    pub transaction: Option<String>,
    /// The block it is in.
    pub block: Option<u64>,
    /// Its position in the block, which with `block` is what orders a replay.
    pub log_index: Option<u64>,
}

impl Transfer {
    /// A mint — the token came from nowhere.
    pub fn is_mint(&self) -> bool {
        self.from.is_zero()
    }

    /// A burn — the token went nowhere.
    pub fn is_burn(&self) -> bool {
        self.to.is_zero()
    }
}

/// Decodes a `Transfer` log.
///
/// Two shapes carry `topics[0] == TRANSFER_TOPIC`, and both are in use:
///
/// - **Three topics, token id in `data`** — EIP-721's own, and the common one.
/// - **Four topics, `data` empty, token id as a third indexed argument** — observed
///   on Robinhood Chain (see `tests/fixtures/transfer_indexed_token_id.json`). Some
///   implementations declare `Transfer(address indexed, address indexed, uint256
///   indexed)`; the topic hash is identical, so nothing downstream can tell them
///   apart by `topics[0]` alone and a reader keyed only on the standard shape either
///   errors or reads a token id out of an empty blob.
///
/// Anything else is refused rather than guessed at: the topic count is the only
/// thing that distinguishes the two, so a third arrangement is a contract this
/// crate has not seen and should not be inferring a token id from.
pub fn decode_transfer(log: &Log) -> Result<Transfer, AbiError> {
    let Some(first) = log.topics.first() else {
        return Err(AbiError::WrongTopics {
            expected: "3 or 4",
            got: 0,
        });
    };

    if first.as_str() != TRANSFER_TOPIC {
        return Err(AbiError::WrongEvent {
            expected: "Transfer(address,address,uint256)",
            got: first.as_str().to_owned(),
        });
    }

    let token_id = match log.topics.len() {
        3 => {
            let data = log.data.decode()?;
            U256::from_word(abi::word_at(&data, 0)?)
        }
        4 => U256::decode_word(log.topics[3].as_str())?,
        other => {
            return Err(AbiError::WrongTopics {
                expected: "3 or 4",
                got: other,
            });
        }
    };

    Ok(Transfer {
        contract: log.address.clone(),
        from: Address::from_topic(log.topics[1].as_str())?,
        to: Address::from_topic(log.topics[2].as_str())?,
        token_id,
        transaction: log.transaction().map(str::to_owned),
        block: log.block(),
        log_index: log.log_index.map(|index| index.get()),
    })
}

impl EvmRpcClient {
    /// `ownerOf(uint256)` — the current owner, read at a block.
    ///
    /// A token that does not exist reverts on a spec-compliant contract, which
    /// arrives as [`RpcError::Rpc`] rather than a zero address.
    pub async fn owner_of(
        &self,
        contract: &Address,
        token: &U256,
        block: BlockTag,
    ) -> Result<Address, RpcError> {
        let data = abi::calldata(OWNER_OF, &token.encode_word())?;
        let result = self.eth_call(contract, &data, block).await?;
        Ok(abi::address(&abi::single_word(&result)?)?)
    }

    /// `balanceOf(address)` — how many of the collection an account holds.
    pub async fn balance_of(
        &self,
        contract: &Address,
        owner: &Address,
        block: BlockTag,
    ) -> Result<U256, RpcError> {
        let data = abi::calldata(BALANCE_OF, &owner.encode_word())?;
        let result = self.eth_call(contract, &data, block).await?;
        Ok(abi::uint(&abi::single_word(&result)?))
    }

    /// Every `Transfer` of a contract over a block range, in the order the node
    /// returned them.
    ///
    /// Logs a reorg has removed are dropped: they are not chain truth, and a
    /// reconcile that applied one would write an owner the chain does not have.
    pub async fn transfers(
        &self,
        contract: &Address,
        from: BlockTag,
        to: BlockTag,
    ) -> Result<Vec<Transfer>, RpcError> {
        let filter = LogFilter::contract(contract.clone(), from, to)
            .topics(vec![Some(LogTopic::parse(TRANSFER_TOPIC)?)]);

        let logs = self.get_logs(&filter).await?;
        let mut transfers = Vec::with_capacity(logs.len());
        for log in logs.iter().filter(|log| !log.is_removed()) {
            transfers.push(decode_transfer(log)?);
        }
        Ok(transfers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::Quantity;
    use crate::log::HexData;

    const MADJACKET: &str = "0x7980aa64093853cb78c927e05b88fed96e945f81";
    const BUYER: &str = "0x61040e143a77f165ba44543af4a079f2c809d14b";

    fn word_of_address(address: &str) -> String {
        format!("0x{}{}", "0".repeat(24), &address[2..])
    }

    fn transfer_log(from: &str, to: &str, token: &str) -> Log {
        Log {
            address: Address::parse(MADJACKET).unwrap(),
            topics: vec![
                LogTopic::parse(TRANSFER_TOPIC).unwrap(),
                LogTopic::parse(&word_of_address(from)).unwrap(),
                LogTopic::parse(&word_of_address(to)).unwrap(),
            ],
            data: HexData::new(format!("0x{token:0>64}")).unwrap(),
            block_number: Some(Quantity::from(17_000_000)),
            block_hash: None,
            transaction_hash: Some(HexData::new(format!("0x{}", "22".repeat(32))).unwrap()),
            transaction_index: None,
            log_index: Some(Quantity::from(3)),
            removed: None,
        }
    }

    #[test]
    fn a_transfer_log_decodes_into_its_two_accounts_and_its_token() {
        let log = transfer_log("0x0000000000000000000000000000000000000000", BUYER, "4d2");
        let transfer = decode_transfer(&log).unwrap();

        assert_eq!(transfer.contract.as_str(), MADJACKET);
        assert_eq!(transfer.from, Address::parse(Address::ZERO).unwrap());
        assert_eq!(transfer.to.as_str(), BUYER);
        assert_eq!(transfer.token_id.to_decimal(), "1234");
        assert_eq!(transfer.block, Some(17_000_000));
        assert_eq!(transfer.log_index, Some(3));
        assert!(transfer.is_mint());
        assert!(!transfer.is_burn());
    }

    #[test]
    fn a_log_of_another_event_is_refused_rather_than_parsed_as_a_transfer() {
        let mut log = transfer_log(BUYER, BUYER, "1");
        log.topics[0] =
            LogTopic::parse("0xf8e1a15aba9398e019f0b49df1a4fde98ee17ae345cb5f6b5e2c27f5033e8ce7")
                .unwrap();

        assert!(matches!(
            decode_transfer(&log).unwrap_err(),
            AbiError::WrongEvent { .. }
        ));
    }

    #[test]
    fn a_log_with_the_wrong_number_of_topics_is_refused() {
        let mut log = transfer_log(BUYER, BUYER, "1");
        log.topics.pop();
        assert_eq!(
            decode_transfer(&log).unwrap_err(),
            AbiError::WrongTopics {
                expected: "3 or 4",
                got: 2
            }
        );
    }

    #[test]
    fn a_call_is_a_selector_followed_by_one_word() {
        let token = U256::from_decimal("1234").unwrap();
        assert_eq!(
            abi::calldata(OWNER_OF, &token.encode_word())
                .unwrap()
                .as_str(),
            format!("0x6352211e{:0>64}", "4d2")
        );

        let owner = Address::parse(MADJACKET).unwrap();
        assert_eq!(
            abi::calldata(BALANCE_OF, &owner.encode_word())
                .unwrap()
                .as_str(),
            format!("0x70a08231{}{}", "0".repeat(24), &MADJACKET[2..])
        );
    }
}
