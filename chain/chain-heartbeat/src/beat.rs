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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainPoint {
    pub slot: u64,
    pub hash: [u8; 32],
}

/// Whether a block arrived live or while replaying towards the tip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    AtTip,
    CatchingUp,
}

/// Everything a follower reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainEvent {
    /// The handshake completed.
    Connected {
        version: u64,
    },
    RollForward {
        beat: BlockBeat,
        sync: SyncState,
    },
    /// The chain now ends at `to` (`None` is the origin). Everything after it
    /// is no longer on the chain. Also sent once right after an intersect.
    RollBackward {
        to: Option<ChainPoint>,
    },
    /// The peer answered a keep-alive: the connection is alive even though no
    /// block has arrived.
    KeepAliveAcknowledged,
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
