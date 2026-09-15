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
    /// The producer's VRF verification key.
    pub vrf_vkey: [u8; 32],
    /// The VRF certificates the header carries.
    pub vrf: HeaderVrf,
}

/// A VRF certificate: the output, and the proof that the key produced it.
/// ECVRF-ED25519-SHA512-Elligator2, so a 64-byte output and an 80-byte proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VrfCert {
    pub output: [u8; 64],
    pub proof: [u8; 80],
}

/// The VRF results in a header. The producer cannot choose them: they are fixed
/// by its VRF key and the slot's input, unlike the block hash, which moves with
/// the order of transactions it packs. That makes the leader output a seed a
/// producer cannot grind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderVrf {
    /// Shelley to Alonzo (TPraos): separate certificates for the epoch nonce
    /// and for leader election.
    TPraos { nonce: VrfCert, leader: VrfCert },
    /// Babbage and Conway (Praos): one certificate. The chain derives its
    /// leader and nonce values from this output by domain-separated hashing;
    /// this is the raw certificate, before any of that.
    Praos { result: VrfCert },
}

impl HeaderVrf {
    /// The certificate that decided this producer led the slot: `leader` before
    /// Babbage, the single `result` from Babbage on.
    pub fn leader(&self) -> &VrfCert {
        match self {
            Self::TPraos { leader, .. } => leader,
            Self::Praos { result } => result,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HeaderError {
    #[error("byron-era headers are not supported (the heartbeat follows from the tip)")]
    Byron,
    #[error("unknown header era variant {0}")]
    UnknownEra(u8),
    #[error("issuer key is {0} bytes, expected 32")]
    IssuerKeyLength(usize),
    #[error("{field} is {len} bytes, expected {expected}")]
    FieldLength {
        field: &'static str,
        len: usize,
        expected: usize,
    },
    #[error("cbor decode error: {0}")]
    Cbor(String),
}

impl From<minicbor::decode::Error> for HeaderError {
    fn from(e: minicbor::decode::Error) -> Self {
        Self::Cbor(e.to_string())
    }
}

/// Which header body shape a variant uses: it decides how many VRF certificates
/// sit between `vrf_vkey` and `block_body_size`.
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
}

/// `vrf_cert = [bytes, bytes .size 80]`
fn vrf_cert(d: &mut Decoder<'_>) -> Result<VrfCert, HeaderError> {
    d.array()?;
    Ok(VrfCert {
        output: fixed(d.bytes()?, "vrf output")?,
        proof: fixed(d.bytes()?, "vrf proof")?,
    })
}

fn fixed<const N: usize>(bytes: &[u8], field: &'static str) -> Result<[u8; N], HeaderError> {
    bytes.try_into().map_err(|_| HeaderError::FieldLength {
        field,
        len: bytes.len(),
        expected: N,
    })
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
        let vrf_vkey = fixed(d.bytes()?, "vrf key")?;
        // Struct fields evaluate in source order: the nonce certificate comes
        // first in a pre-Babbage body, then the leader's.
        let vrf = match layout {
            BodyLayout::ShelleyCompatible => HeaderVrf::TPraos {
                nonce: vrf_cert(&mut d)?,
                leader: vrf_cert(&mut d)?,
            },
            BodyLayout::BabbageCompatible => HeaderVrf::Praos {
                result: vrf_cert(&mut d)?,
            },
        };
        let body_size = d.u32()?;
        Ok(Self {
            height,
            slot,
            hash: blake2b::<32>(cbor),
            issuer_pool,
            body_size,
            vrf_vkey,
            vrf,
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
    fn reads_the_vrf_key_and_result_of_a_real_conway_header() {
        let parts = split_block(BLOCK).unwrap();
        let header = BlockHeader::decode(parts.header_variant, parts.header_cbor).unwrap();
        // Koios `block_info` for this block: vrf_vk1k8h6dcy…pg6hwq, from bech32.
        assert_eq!(
            hex::encode(header.vrf_vkey),
            "b1efa6e09d68776d9a2ab605c951e3d6d281977bb6b8cc2ef8ae42fb28543005"
        );
        let HeaderVrf::Praos { result } = header.vrf else {
            panic!("a Conway header carries one VRF result");
        };
        assert_eq!(header.vrf.leader(), &result);
        // Read from the header rather than left zeroed.
        assert!(result.output.iter().any(|b| *b != 0));
        assert!(result.proof.iter().any(|b| *b != 0));
    }

    fn pre_babbage_header(leader_proof_len: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut e = minicbor::Encoder::new(&mut buf);
        e.array(2).unwrap().array(15).unwrap();
        e.u64(7).unwrap().u64(42).unwrap().null().unwrap();
        e.bytes(&[1; 32]).unwrap(); // issuer_vkey
        e.bytes(&[2; 32]).unwrap(); // vrf_vkey
        e.array(2)
            .unwrap()
            .bytes(&[3; 64])
            .unwrap()
            .bytes(&[4; 80])
            .unwrap(); // nonce_vrf
        e.array(2)
            .unwrap()
            .bytes(&[5; 64])
            .unwrap()
            .bytes(&vec![6; leader_proof_len])
            .unwrap(); // leader_vrf
        e.u32(1234).unwrap(); // block_body_size
        buf
    }

    #[test]
    fn reads_both_vrf_certificates_of_a_pre_babbage_header() {
        let header = BlockHeader::decode(4, &pre_babbage_header(80)).unwrap();
        assert_eq!(header.vrf_vkey, [2; 32]);
        assert_eq!(header.body_size, 1234);
        assert_eq!(
            header.vrf,
            HeaderVrf::TPraos {
                nonce: VrfCert {
                    output: [3; 64],
                    proof: [4; 80],
                },
                leader: VrfCert {
                    output: [5; 64],
                    proof: [6; 80],
                },
            }
        );
        assert_eq!(header.vrf.leader().output, [5; 64]);
    }

    #[test]
    fn a_vrf_field_of_the_wrong_size_is_refused() {
        assert!(matches!(
            BlockHeader::decode(4, &pre_babbage_header(79)),
            Err(HeaderError::FieldLength {
                field: "vrf proof",
                len: 79,
                expected: 80,
            })
        ));
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
