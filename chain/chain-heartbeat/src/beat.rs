//! What happened on the chain, in the shape hosts pass around and persist.

use serde::{Deserialize, Serialize};

use crate::block::{BlockError, split_block};
use crate::header::{BlockHeader, HeaderError};
use crate::network::Network;

/// One block, as the heartbeat reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockBeat {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub height: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub slot: u64,
    /// Block hash, hex.
    pub hash: String,
    /// Producing pool's key hash, hex.
    pub issuer_pool: String,
    pub body_size: u32,
    /// `None` when the host followed headers only.
    #[serde(default)]
    pub tx_count: Option<u32>,
    /// When the slot began. `None` only for a Byron-era slot.
    #[serde(default, with = "wasm_safe_serde::u64_option")]
    pub block_time_unix: Option<u64>,
}

/// A point to resume following from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainPoint {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub slot: u64,
    /// Block hash, hex on the wire.
    #[serde(with = "hex::serde")]
    pub hash: [u8; 32],
}

/// Whether a block arrived live or while replaying towards the tip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    AtTip,
    CatchingUp,
}

/// Bytes of each transaction hash a [`BlockTxs`] keeps. Eight is plenty to
/// recognise the handful of transactions one page is waiting on (a false match
/// is a 2^-64 chance per comparison), at a quarter of the size of a whole hash.
pub const TX_PREFIX_BYTES: usize = 8;

/// Where a transaction stands in a block that includes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxInBlock {
    Valid,
    /// In the block, but a script rejected it (phase-2): its collateral was
    /// taken, and nothing else it did happened.
    FailedValidation,
}

/// The transactions of one block, as a subscriber needs them to spot its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockTxs {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub height: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub slot: u64,
    /// The first [`TX_PREFIX_BYTES`] of every transaction hash, in block order,
    /// concatenated. Hex on the wire.
    #[serde(with = "hex::serde")]
    pub prefixes: Vec<u8>,
    /// Indices of the transactions that failed phase-2 validation.
    #[serde(default)]
    pub invalid: Vec<u32>,
}

impl BlockTxs {
    pub fn new(height: u64, slot: u64, hashes: &[[u8; 32]], invalid: Vec<u32>) -> Self {
        Self {
            height,
            slot,
            prefixes: hashes
                .iter()
                .flat_map(|hash| hash[..TX_PREFIX_BYTES].iter().copied())
                .collect(),
            invalid,
        }
    }

    pub fn len(&self) -> usize {
        self.prefixes.len() / TX_PREFIX_BYTES
    }

    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty()
    }

    /// Whether the transaction with this hash (or at least its first
    /// [`TX_PREFIX_BYTES`]) is in the block, and how it fared.
    pub fn find(&self, tx_hash: &[u8]) -> Option<TxInBlock> {
        let prefix = tx_hash.get(..TX_PREFIX_BYTES)?;
        let (prefixes, _) = self.prefixes.as_chunks::<TX_PREFIX_BYTES>();
        let index = prefixes.iter().position(|p| p.as_slice() == prefix)?;
        let index = u32::try_from(index).ok()?;
        Some(if self.invalid.contains(&index) {
            TxInBlock::FailedValidation
        } else {
            TxInBlock::Valid
        })
    }

    /// [`Self::find`] for a hex hash, as a wallet or cart holds one.
    pub fn find_hex(&self, tx_hash: &str) -> Option<TxInBlock> {
        let mut prefix = [0u8; TX_PREFIX_BYTES];
        hex::decode_to_slice(tx_hash.get(..TX_PREFIX_BYTES * 2)?, &mut prefix).ok()?;
        self.find(&prefix)
    }
}

/// Everything a follower reports. Serialisable so hosts can relay it inside a
/// [`crate::HeartbeatFrame`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChainEvent {
    /// The handshake completed.
    Connected {
        #[serde(with = "wasm_safe_serde::u64_required")]
        version: u64,
    },
    RollForward {
        beat: BlockBeat,
        sync: SyncState,
    },
    /// The chain now ends at `to` (`None` is the origin). Everything after it
    /// is no longer on the chain. Also sent once right after an intersect.
    RollBackward {
        #[serde(default)]
        to: Option<ChainPoint>,
    },
    /// The peer answered a keep-alive: the connection is alive even though no
    /// block has arrived.
    KeepAliveAcknowledged,
    /// The transactions of the block about to be reported, sent just before its
    /// [`ChainEvent::RollForward`]. Only from a follower that fetches bodies.
    BlockTransactions {
        txs: BlockTxs,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum BeatError {
    #[error(transparent)]
    Block(#[from] BlockError),
    #[error(transparent)]
    Header(#[from] HeaderError),
}

impl BlockBeat {
    pub fn new(network: Network, header: &BlockHeader, tx_count: Option<u32>) -> Self {
        Self {
            height: header.height,
            slot: header.slot,
            hash: hex::encode(header.hash),
            issuer_pool: hex::encode(header.issuer_pool),
            body_size: header.body_size,
            tx_count,
            block_time_unix: network.slot_to_unix_secs(header.slot),
        }
    }

    /// Build a beat from a whole era-wrapped block, for hosts that receive
    /// blocks without following the chain themselves.
    pub fn from_block(network: Network, block: &[u8]) -> Result<Self, BeatError> {
        let parts = split_block(block)?;
        let header = BlockHeader::decode(parts.header_variant, parts.header_cbor)?;
        Ok(Self::new(network, &header, Some(parts.tx_count)))
    }

    /// The point this block sits at, if its stored hash is well-formed.
    pub fn point(&self) -> Option<ChainPoint> {
        let mut hash = [0u8; 32];
        hex::decode_to_slice(&self.hash, &mut hash).ok()?;
        Some(ChainPoint {
            slot: self.slot,
            hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &[u8] = include_bytes!("../tests/fixtures/186000000.block.cbor");

    #[test]
    fn block_txs_find_a_hash_by_its_prefix_and_say_whether_it_failed() {
        let valid = [0x11; 32];
        let failed = [0x22; 32];
        let txs = BlockTxs::new(10, 20, &[valid, failed], vec![1]);
        assert_eq!(txs.len(), 2);
        assert_eq!(txs.find(&valid), Some(TxInBlock::Valid));
        assert_eq!(
            txs.find_hex(&hex::encode(failed)),
            Some(TxInBlock::FailedValidation)
        );
        assert_eq!(txs.find(&[0x33; 32]), None);
        assert_eq!(txs.find_hex("not hex"), None);

        let event = ChainEvent::BlockTransactions { txs };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("11111111111111112222222222222222"), "{json}");
        assert_eq!(serde_json::from_str::<ChainEvent>(&json).unwrap(), event);
    }

    #[test]
    fn a_whole_block_becomes_a_beat() {
        let beat = BlockBeat::from_block(Network::Mainnet, BLOCK).unwrap();
        assert_eq!(beat.height, 13_358_656);
        assert_eq!(beat.slot, 186_000_000);
        assert_eq!(beat.block_time_unix, Some(1_777_566_291));
        assert!(beat.tx_count.unwrap() > 0);
        assert_eq!(beat.issuer_pool.len(), 56);
        assert_eq!(beat.point().unwrap().slot, 186_000_000);
    }

    #[test]
    fn beats_serialise_heights_and_slots_safely() {
        let beat = BlockBeat::from_block(Network::Mainnet, BLOCK).unwrap();
        let json = serde_json::to_string(&beat).unwrap();
        let back: BlockBeat = serde_json::from_str(&json).unwrap();
        assert_eq!(back, beat);
    }
}
