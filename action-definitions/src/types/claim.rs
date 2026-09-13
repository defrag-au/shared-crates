//! The claim tag — the inline datum on a burn's sink output.
//!
//! This is the **third frozen envelope** and the only other positional
//! structure in the format. It is positional because the escrow validator
//! reads all four fields on the settlement path, once per referenced claim,
//! where walking a map is execution budget spent for nothing.
//!
//! Why each field exists:
//!
//! - `definition_tx` — the terms this burn was made under. Content-addressed,
//!   so a re-posted definition is a new hash and old burns keep their terms.
//! - `claim_id` — 16 random bytes minted by the worker at build time, so two
//!   burns of the same value in the same block are still distinguishable.
//! - `recipient` — **a validator cannot resolve another transaction's
//!   inputs**, so it cannot know who burned. The builder writes the
//!   connected wallet's address here and the escrow validator pays there
//!   (protocol §6.2). It is a [`ChainAddress`] — Plutus's own address
//!   shape — and not raw ledger bytes, because `escrow.ak` compares it
//!   against a transaction output's `address` on every settlement, and the
//!   two encodings are not the same thing. Raw bytes would leave a
//!   validator hand-parsing a header byte to find out whether a credential
//!   is a script.
//! - `claim_slot` — **a validator cannot see a reference input's creation
//!   slot** either, so window checks at settlement read this (protocol
//!   §6.1). The service uses the real slot it observed; this is the
//!   builder's estimate and is only load-bearing on chain.

use serde::{Deserialize, Serialize};

use crate::codec::{DecodeError, PlutusCodec, constr_zero, read_constr_zero};
use crate::types::scalars::{ChainAddress, ClaimId, TxHash};
use pallas_primitives::PlutusData;

/// `Constr 0 [definition_tx, claim_id, recipient, claim_slot]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimTag {
    pub definition_tx: TxHash,
    pub claim_id: ClaimId,
    pub recipient: ChainAddress,
    pub claim_slot: u64,
}

impl ClaimTag {
    /// The frozen field count. A later addition is a **trailing** field, and
    /// an older decoder ignores it.
    pub const FIELDS: usize = 4;
}

impl PlutusCodec for ClaimTag {
    fn to_data(&self) -> PlutusData {
        constr_zero(vec![
            self.definition_tx.to_data(),
            self.claim_id.to_data(),
            self.recipient.to_data(),
            self.claim_slot.to_data(),
        ])
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        let fields = read_constr_zero(data, Self::FIELDS)?;
        Ok(Self {
            definition_tx: TxHash::from_data(&fields[0])?,
            claim_id: ClaimId::from_data(&fields[1])?,
            recipient: ChainAddress::from_data(&fields[2])?,
            claim_slot: u64::from_data(&fields[3])?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pallas_primitives::Fragment;

    fn tag() -> ClaimTag {
        ClaimTag {
            definition_tx: TxHash([1u8; 32]),
            claim_id: ClaimId([2u8; 16]),
            recipient: ChainAddress::from_bytes(&enterprise(0x61)).unwrap(),
            claim_slot: 987_654,
        }
    }

    /// A mainnet enterprise address: header, then 28 bytes of key hash.
    fn enterprise(header: u8) -> Vec<u8> {
        let mut raw = vec![header];
        raw.extend_from_slice(&[9u8; 28]);
        raw
    }

    /// The recipient must survive the round trip **as an address**, not just
    /// as bytes — `escrow.ak` compares the decoded form against a
    /// transaction output.
    #[test]
    fn a_recipient_round_trips_through_the_plutus_address_shape() {
        let base = {
            let mut raw = vec![0x01u8]; // script payment + key stake, mainnet
            raw.extend_from_slice(&[1u8; 28]);
            raw.extend_from_slice(&[2u8; 28]);
            raw
        };
        for raw in [enterprise(0x61), enterprise(0x71), base] {
            let address = ChainAddress::from_bytes(&raw).unwrap();
            assert_eq!(address.to_bytes(raw[0] & 0x0f), raw, "rebuilt differs");
            assert_eq!(
                ChainAddress::from_data(&address.to_data()).unwrap(),
                address
            );
        }
    }

    /// A pointer address or a reward account is not a place a prize is
    /// paid, and accepting one would encode something `to_bytes` could not
    /// rebuild.
    #[test]
    fn an_address_kind_we_cannot_rebuild_is_refused() {
        let mut pointer = vec![0x41u8];
        pointer.extend_from_slice(&[1u8; 28]);
        assert!(ChainAddress::from_bytes(&pointer).is_err());
        assert!(ChainAddress::from_bytes(&[]).is_err());
    }

    /// A base address truncated to enterprise length must not decode as an
    /// enterprise address — the length is checked against the kind.
    #[test]
    fn a_truncated_base_address_is_refused() {
        let mut truncated = vec![0x01u8];
        truncated.extend_from_slice(&[1u8; 28]);
        assert!(ChainAddress::from_bytes(&truncated).is_err());
    }

    #[test]
    fn a_claim_tag_round_trips_through_real_cbor() {
        let tag = tag();
        let bytes = tag.to_data().encode_fragment().unwrap();
        let reparsed = PlutusData::decode_fragment(&bytes).unwrap();
        assert_eq!(ClaimTag::from_data(&reparsed).unwrap(), tag);
    }

    #[test]
    fn it_is_positional_so_the_validator_can_index_it() {
        let data = tag().to_data();
        let fields = read_constr_zero(&data, ClaimTag::FIELDS).unwrap();
        assert_eq!(fields.len(), 4);
        assert_eq!(crate::codec::shape_of(&fields[3]), "int");
    }

    /// True of **this** reader. An aiken validator casting into a
    /// four-field record rejects a fifth field outright (proven by
    /// `contracts/lib/schema.ak`'s `the_envelope_rejects_a_trailing_field`),
    /// so `escrow.ak` must read the claim tag with `builtin.un_constr_data`
    /// and index the field list if this tolerance is ever to be used.
    #[test]
    fn a_trailing_field_added_later_does_not_break_this_reader() {
        let mut fields: Vec<PlutusData> = read_constr_zero(&tag().to_data(), 4).unwrap().to_vec();
        fields.push(99u64.to_data());
        let future = constr_zero(fields);
        assert_eq!(ClaimTag::from_data(&future).unwrap(), tag());
    }

    #[test]
    fn a_short_tag_is_refused() {
        let fields = vec![TxHash([1u8; 32]).to_data(), ClaimId([2u8; 16]).to_data()];
        assert!(matches!(
            ClaimTag::from_data(&constr_zero(fields)),
            Err(DecodeError::ShortEnvelope { expected: 4, .. })
        ));
    }

    #[test]
    fn the_datum_stays_small_enough_to_be_cheap() {
        // ~0.25 ADA of extra min-ADA is the figure the burn app quotes; the
        // datum it is quoting is this one.
        let bytes = tag().to_data().encode_fragment().unwrap();
        assert!(bytes.len() < 100, "claim tag grew to {} bytes", bytes.len());
    }
}
