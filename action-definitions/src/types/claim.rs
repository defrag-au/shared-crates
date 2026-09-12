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
//!   (protocol §6.2).
//! - `claim_slot` — **a validator cannot see a reference input's creation
//!   slot** either, so window checks at settlement read this (protocol
//!   §6.1). The service uses the real slot it observed; this is the
//!   builder's estimate and is only load-bearing on chain.

use serde::{Deserialize, Serialize};

use crate::codec::{constr_zero, read_constr_zero, DecodeError, PlutusCodec};
use crate::types::scalars::{Address, ClaimId, TxHash};
use pallas_primitives::PlutusData;

/// `Constr 0 [definition_tx, claim_id, recipient, claim_slot]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimTag {
    pub definition_tx: TxHash,
    pub claim_id: ClaimId,
    pub recipient: Address,
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
            recipient: Address::from_data(&fields[2])?,
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
            recipient: Address::from(vec![0x61u8; 29]),
            claim_slot: 987_654,
        }
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
