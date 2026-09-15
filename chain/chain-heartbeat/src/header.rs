//! Decode the handful of header fields a heartbeat needs.
//!
//! `header = [header_body, body_signature]`
//!
//! Shelley to Alonzo (15-field body):
//! `[block_number, slot, prev_hash, issuer_vkey, vrf_vkey, nonce_vrf, leader_vrf,
//!   block_body_size, block_body_hash, hot_vkey, seq, kes_period, sigma,
//!   proto_major, proto_minor]`
//!
//! Babbage and Conway (10-field body):
//! `[block_number, slot, prev_hash, issuer_vkey, vrf_vkey, vrf_result,
//!   block_body_size, block_body_hash, operational_cert, protocol_version]`

use minicbor::Decoder;

use crate::blake2b;

/// The fields of a Shelley-or-later header a heartbeat reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeader {
    pub height: u64,
    pub slot: u64,
    /// Block hash: blake2b-256 of the header bytes.
    pub hash: [u8; 32],
    /// Pool key hash of the producer: blake2b-224 of the issuer key. The same
    /// bytes a bech32 `pool1…` id encodes.
    pub issuer_pool: [u8; 28],
    /// Size of the block body in bytes, as the header commits to it.
    pub body_size: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum HeaderError {
    #[error("byron-era headers are not supported (the heartbeat follows from the tip)")]
    Byron,
    #[error("unknown header era variant {0}")]
    UnknownEra(u8),
    #[error("issuer key is {0} bytes, expected 32")]
    IssuerKeyLength(usize),
    #[error("cbor decode error: {0}")]
    Cbor(String),
}

impl From<minicbor::decode::Error> for HeaderError {
    fn from(e: minicbor::decode::Error) -> Self {
        Self::Cbor(e.to_string())
    }
}

/// Where the body-size field sits.
enum BodyLayout {
    ShelleyCompatible,
    BabbageCompatible,
}

impl BodyLayout {
    fn for_variant(variant: u8) -> Result<Self, HeaderError> {
        match variant {
            0 => Err(HeaderError::Byron),
            1..=4 => Ok(Self::ShelleyCompatible),
            5 | 6 => Ok(Self::BabbageCompatible),
            other => Err(HeaderError::UnknownEra(other)),
        }
    }

    /// Fields between `issuer_vkey` and `block_body_size`.
    fn fields_before_body_size(&self) -> usize {
        match self {
            Self::ShelleyCompatible => 3,
            Self::BabbageCompatible => 2,
        }
    }
}

impl BlockHeader {
    /// Decode a header. `variant` is the chain-sync hard-fork era index
    /// (1 Shelley … 6 Conway), which is [`crate::BlockParts::header_variant`]
    /// when the header came out of a whole block.
    pub fn decode(variant: u8, cbor: &[u8]) -> Result<Self, HeaderError> {
        let layout = BodyLayout::for_variant(variant)?;
        let mut d = Decoder::new(cbor);
        d.array()?;
        d.array()?;
        let height = d.u64()?;
        let slot = d.u64()?;
        d.skip()?; // prev_hash (null for the first block)
        let issuer_vkey = d.bytes()?;
        if issuer_vkey.len() != 32 {
            return Err(HeaderError::IssuerKeyLength(issuer_vkey.len()));
        }
        let issuer_pool = blake2b::<28>(issuer_vkey);
        for _ in 0..layout.fields_before_body_size() {
            d.skip()?;
        }
        let body_size = d.u32()?;
        Ok(Self {
            height,
            slot,
            hash: blake2b::<32>(cbor),
            issuer_pool,
            body_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::split_block;

    const BLOCK: &[u8] = include_bytes!("../tests/fixtures/186000000.block.cbor");

    #[test]
    fn decodes_a_real_conway_header() {
        let parts = split_block(BLOCK).unwrap();
        let header = BlockHeader::decode(parts.header_variant, parts.header_cbor).unwrap();
        assert_eq!(header.height, 13_358_656);
        assert_eq!(header.slot, 186_000_000);
        assert_eq!(header.hash, blake2b::<32>(parts.header_cbor));
    }

    /// The header commits to the body's size and hash. Recomputing both from
    /// the block proves the body-size field was read from the right position
    /// (the two neighbouring fields are the only other integers and hashes
    /// nearby), without trusting any constant from this crate.
    #[test]
    fn body_size_and_hash_match_what_the_header_commits_to() {
        let parts = split_block(BLOCK).unwrap();
        let header = BlockHeader::decode(parts.header_variant, parts.header_cbor).unwrap();

        let mut d = Decoder::new(BLOCK);
        d.array().unwrap();
        d.u8().unwrap();
        d.array().unwrap();
        d.skip().unwrap(); // header
        let mut component_hashes = Vec::new();
        let mut body_bytes = 0;
        for _ in 0..4 {
            let start = d.position();
            d.skip().unwrap();
            let component = &BLOCK[start..d.position()];
            body_bytes += component.len();
            component_hashes.extend_from_slice(&blake2b::<32>(component));
        }
        assert_eq!(header.body_size as usize, body_bytes);

        let mut h = Decoder::new(parts.header_cbor);
        h.array().unwrap();
        h.array().unwrap();
        for _ in 0..7 {
            h.skip().unwrap();
        }
        let committed_hash = h.bytes().unwrap();
        assert_eq!(committed_hash, &blake2b::<32>(&component_hashes)[..]);
    }

    #[test]
    fn byron_and_unknown_variants_are_refused() {
        assert!(matches!(
            BlockHeader::decode(0, &[]),
            Err(HeaderError::Byron)
        ));
        assert!(matches!(
            BlockHeader::decode(9, &[]),
            Err(HeaderError::UnknownEra(9))
        ));
    }
}
