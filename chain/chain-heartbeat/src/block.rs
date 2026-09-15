//! Split a whole era-wrapped block into what a heartbeat needs.
//!
//! `block = [era_tag, [header, tx_bodies, witness_sets, auxiliary_data, invalid_txs]]`
//!
//! `era_tag` counts Byron's two block kinds separately (0 epoch-boundary,
//! 1 Byron, 2 Shelley … 7 Conway), so it is one higher than the chain-sync
//! header variant from Shelley on. This is the shape block-fetch returns, the
//! shape mitos archives, and the shape `pallas_traverse::MultiEraBlock` decodes.

use minicbor::Decoder;
use minicbor::data::Type;

/// The pieces of a Shelley-or-later block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockParts<'b> {
    pub era_tag: u8,
    /// The matching chain-sync header variant, for [`crate::BlockHeader::decode`].
    pub header_variant: u8,
    /// The header's exact bytes (hash these for the block hash).
    pub header_cbor: &'b [u8],
    /// Transactions in the block, including any that failed phase-2 validation
    /// (they are still in the block and still paid collateral).
    pub tx_count: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum BlockError {
    #[error("byron-era blocks are not supported")]
    Byron,
    #[error("unknown block era tag {0}")]
    UnknownEra(u8),
    #[error("cbor decode error: {0}")]
    Cbor(String),
}

impl From<minicbor::decode::Error> for BlockError {
    fn from(e: minicbor::decode::Error) -> Self {
        Self::Cbor(e.to_string())
    }
}

pub fn split_block(block: &[u8]) -> Result<BlockParts<'_>, BlockError> {
    let mut d = Decoder::new(block);
    d.array()?;
    let era_tag = d.u8()?;
    let header_variant = match era_tag {
        0 | 1 => return Err(BlockError::Byron),
        2..=7 => era_tag - 1,
        other => return Err(BlockError::UnknownEra(other)),
    };
    d.array()?;
    let start = d.position();
    d.skip()?;
    let header_cbor = &block[start..d.position()];
    let tx_count = count_items(&mut d)?;
    Ok(BlockParts {
        era_tag,
        header_variant,
        header_cbor,
        tx_count,
    })
}

/// Count the items of the array at the cursor, definite or indefinite.
fn count_items(d: &mut Decoder<'_>) -> Result<u32, BlockError> {
    match d.array()? {
        Some(len) => u32::try_from(len)
            .map_err(|_| BlockError::Cbor(format!("transaction array length {len} overflows"))),
        None => {
            let mut count = 0u32;
            while d.datatype()? != Type::Break {
                d.skip()?;
                count += 1;
            }
            Ok(count)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &[u8] = include_bytes!("../tests/fixtures/186000000.block.cbor");

    #[test]
    fn splits_a_real_conway_block() {
        let parts = split_block(BLOCK).unwrap();
        assert_eq!(parts.era_tag, 7);
        assert_eq!(parts.header_variant, 6);

        // Every transaction has exactly one witness set, so the witness array
        // is an independent count of the same thing.
        let mut d = Decoder::new(BLOCK);
        d.array().unwrap();
        d.u8().unwrap();
        d.array().unwrap();
        d.skip().unwrap();
        d.skip().unwrap();
        let witness_sets = d.array().unwrap().unwrap();
        assert_eq!(u64::from(parts.tx_count), witness_sets);
        assert!(parts.tx_count > 0);
    }

    #[test]
    fn counts_an_indefinite_transaction_array() {
        // [2, [header=0, [_ 1, 2, 3], [], {}, []]]
        let block = [
            0x82, 0x02, 0x85, 0x00, 0x9F, 0x01, 0x02, 0x03, 0xFF, 0x80, 0xA0, 0x80,
        ];
        let parts = split_block(&block).unwrap();
        assert_eq!(parts.tx_count, 3);
        assert_eq!(parts.header_variant, 1);
    }

    #[test]
    fn byron_blocks_are_refused() {
        assert!(matches!(
            split_block(&[0x82, 0x01, 0x80]),
            Err(BlockError::Byron)
        ));
    }
}
