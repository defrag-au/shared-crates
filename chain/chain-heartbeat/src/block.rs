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

/// The transactions of a Shelley-or-later block, as spans of the block's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTransactions<'b> {
    /// Each transaction body's exact bytes, in block order. A transaction's
    /// hash is blake2b-256 of these bytes as they stand; re-encoding them would
    /// hash something else.
    pub bodies: Vec<&'b [u8]>,
    /// Indices into `bodies` of transactions that failed phase-2 validation.
    /// They are still in the block and still paid collateral, but did nothing
    /// else. Empty before Alonzo, whose blocks carry no such list.
    pub invalid: Vec<u32>,
}

impl BlockTransactions<'_> {
    /// Every transaction's hash, in block order.
    pub fn hashes(&self) -> Vec<[u8; 32]> {
        self.bodies
            .iter()
            .map(|body| crate::blake2b::<32>(body))
            .collect()
    }
}

/// Find each transaction in a whole era-wrapped block.
pub fn block_transactions(block: &[u8]) -> Result<BlockTransactions<'_>, BlockError> {
    let mut d = Decoder::new(block);
    d.array()?;
    match d.u8()? {
        0 | 1 => return Err(BlockError::Byron),
        2..=7 => {}
        other => return Err(BlockError::UnknownEra(other)),
    }
    let fields = d.array()?;
    d.skip()?; // header
    let bodies = item_spans(&mut d, block)?;
    d.skip()?; // witness sets
    d.skip()?; // auxiliary data
    // Shelley to Mary blocks have four fields; Alonzo added the fifth.
    let has_invalid = match fields {
        Some(len) => len >= 5,
        None => d.datatype()? != Type::Break,
    };
    let invalid = if has_invalid {
        indices(&mut d)?
    } else {
        Vec::new()
    };
    Ok(BlockTransactions { bodies, invalid })
}

/// The exact bytes of each item of the array at the cursor, definite or
/// indefinite, leaving the cursor after the array.
fn item_spans<'b>(d: &mut Decoder<'b>, block: &'b [u8]) -> Result<Vec<&'b [u8]>, BlockError> {
    let len = d.array()?;
    let mut spans = Vec::new();
    loop {
        match len {
            Some(n) if spans.len() as u64 >= n => break,
            None if d.datatype()? == Type::Break => {
                skip_break(d);
                break;
            }
            _ => {}
        }
        let start = d.position();
        d.skip()?;
        spans.push(&block[start..d.position()]);
    }
    Ok(spans)
}

/// An array of transaction indices, definite or indefinite, optionally tagged
/// as a set (258).
fn indices(d: &mut Decoder<'_>) -> Result<Vec<u32>, BlockError> {
    if d.datatype()? == Type::Tag {
        d.tag()?;
    }
    let len = d.array()?;
    let mut out = Vec::new();
    loop {
        match len {
            Some(n) if out.len() as u64 >= n => break,
            None if d.datatype()? == Type::Break => {
                skip_break(d);
                break;
            }
            _ => {}
        }
        out.push(d.u32()?);
    }
    Ok(out)
}

/// Step over the one-byte break that ends an indefinite-length item.
fn skip_break(d: &mut Decoder<'_>) {
    d.set_position(d.position() + 1);
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
    fn hashes_a_real_blocks_transactions_as_the_chain_does() {
        // Koios `block_txs` for block 13,358,656 (5f39eaa0…411a).
        let mut expected = vec![
            "22ef4bf62480f87fa0a97ec5239b3d682a20cdff3ef6dbde426a6b4e9aeb1be3",
            "f41e2d220cbf3b309ea966a1870c2f699a5951ec5e13d4a86cb6fc342a803160",
            "e103f8ad8300627040d4788a8b01de529de435cee9fd90a67c2614eb44fa310f",
            "6b40346eb9297abb7a298f1573b09223b3981c13c44a8e5c440f275560df4d15",
        ];
        let txs = block_transactions(BLOCK).unwrap();
        let mut hashes: Vec<String> = txs.hashes().iter().map(hex::encode).collect();
        // Koios promises no order, so compare as sets.
        hashes.sort();
        expected.sort();
        assert_eq!(hashes, expected);
        assert_eq!(
            txs.bodies.len() as u32,
            split_block(BLOCK).unwrap().tx_count
        );
        assert!(txs.invalid.is_empty());
    }

    #[test]
    fn reads_indefinite_bodies_and_the_failed_list() {
        // [7, [header=0, [_ {0:1}, {0:2}], [], {}, [1]]]
        let block = [
            0x82, 0x07, 0x85, 0x00, 0x9F, 0xA1, 0x00, 0x01, 0xA1, 0x00, 0x02, 0xFF, 0x80, 0xA0,
            0x81, 0x01,
        ];
        let txs = block_transactions(&block).unwrap();
        assert_eq!(
            txs.bodies,
            vec![&[0xA1, 0x00, 0x01][..], &[0xA1, 0x00, 0x02][..]]
        );
        assert_eq!(txs.invalid, vec![1]);
    }

    #[test]
    fn a_pre_alonzo_block_has_no_failed_list() {
        // [4, [header=0, [{0:1}], [], {}]]
        let block = [0x82, 0x04, 0x84, 0x00, 0x81, 0xA1, 0x00, 0x01, 0x80, 0xA0];
        let txs = block_transactions(&block).unwrap();
        assert_eq!(txs.bodies.len(), 1);
        assert!(txs.invalid.is_empty());
    }

    #[test]
    fn byron_blocks_are_refused() {
        assert!(matches!(
            split_block(&[0x82, 0x01, 0x80]),
            Err(BlockError::Byron)
        ));
    }
}
