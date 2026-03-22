//! PlutusData construction helpers and script source types.
//!
//! Provides convenience functions for building Plutus datums/redeemers and
//! an enum for referencing scripts (inline in witness set vs reference UTxO).

use pallas_primitives::conway::PlutusData;
use pallas_primitives::{Constr, Fragment, MaybeIndefArray};
use pallas_txbuilder::{Input, ScriptKind};

use crate::error::TxBuildError;

// ============================================================================
// Script source
// ============================================================================

/// How a Plutus script is provided to the transaction.
///
/// - `Inline`: script bytes included in the TX witness set (preprod / small scripts)
/// - `Reference`: script lives in a UTxO referenced via CIP-33
#[derive(Debug, Clone)]
pub enum ScriptSource {
    Inline {
        language: ScriptKind,
        bytes: Vec<u8>,
    },
    Reference {
        utxo: Input,
    },
}

// ============================================================================
// Script input context
// ============================================================================

/// Everything needed to spend a Plutus script UTxO.
#[derive(Debug, Clone)]
pub struct ScriptInput {
    pub script: ScriptSource,
    /// Datum witness (if not inline on the UTxO)
    pub datum_cbor: Option<Vec<u8>>,
    pub redeemer_cbor: Vec<u8>,
    pub ex_units: pallas_txbuilder::ExUnits,
}

/// A minting operation with its script context.
#[derive(Debug, Clone)]
pub struct MintEntry {
    pub policy: pallas_crypto::hash::Hash<28>,
    /// (asset_name_bytes, quantity) — positive to mint, negative to burn
    pub assets: Vec<(Vec<u8>, i64)>,
    pub script: ScriptSource,
    pub redeemer_cbor: Vec<u8>,
    pub ex_units: pallas_txbuilder::ExUnits,
}

/// Validity interval for a transaction.
#[derive(Debug, Clone, Default)]
pub struct ValidityInterval {
    /// Lower bound slot (transaction valid from this slot)
    pub valid_from: Option<u64>,
    /// Upper bound slot (transaction invalid after this slot, i.e. TTL)
    pub invalid_after: Option<u64>,
}

/// Collateral configuration for Plutus transactions.
#[derive(Debug, Clone)]
pub enum CollateralConfig {
    /// Auto-select from available UTxOs
    Auto,
    /// Use a specific UTxO as collateral
    Manual(Input),
}

// ============================================================================
// PlutusData construction helpers
// ============================================================================

/// Encode PlutusData to CBOR bytes.
pub fn encode_plutus_data(data: &PlutusData) -> Result<Vec<u8>, TxBuildError> {
    data.encode_fragment()
        .map_err(|e| TxBuildError::CborParse(format!("Failed to encode PlutusData: {e}")))
}

/// Build a `Constr` PlutusData with the given constructor index and fields.
///
/// Follows CIP-121 tag encoding:
/// - index 0–6 → tags 121–127
/// - index 7+ → tag 102 with explicit any_constructor
pub fn constr(index: u32, fields: Vec<PlutusData>) -> PlutusData {
    let (tag, any_constructor) = if index <= 6 {
        (121 + index as u64, None)
    } else {
        (102, Some(index as u64))
    };
    PlutusData::Constr(Constr {
        tag,
        any_constructor,
        fields: MaybeIndefArray::Def(fields),
    })
}

/// Build an integer PlutusData.
pub fn int(value: i64) -> PlutusData {
    PlutusData::BigInt(pallas_primitives::BigInt::Int(value.into()))
}

/// Build a bounded bytes PlutusData.
pub fn bytes(data: Vec<u8>) -> PlutusData {
    PlutusData::BoundedBytes(data.into())
}

/// Build a bounded bytes PlutusData from a hex string.
pub fn bytes_hex(hex_str: &str) -> Result<PlutusData, TxBuildError> {
    let decoded = hex::decode(hex_str)
        .map_err(|e| TxBuildError::InvalidHex(format!("Invalid PlutusData hex: {e}")))?;
    Ok(PlutusData::BoundedBytes(decoded.into()))
}

/// Empty constructor: `Constr(0, [])` — commonly used for unit redeemers.
pub fn constr0_empty() -> PlutusData {
    constr(0, vec![])
}

/// Build a PlutusData list.
pub fn list(items: Vec<PlutusData>) -> PlutusData {
    PlutusData::Array(MaybeIndefArray::Def(items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constr0_empty_encodes() {
        let data = constr0_empty();
        let cbor = encode_plutus_data(&data).unwrap();
        // Constr 0 [] encodes as d87980
        assert_eq!(hex::encode(&cbor), "d87980");
    }

    #[test]
    fn test_constr_with_fields() {
        let data = constr(0, vec![int(42), bytes(vec![0xde, 0xad])]);
        let cbor = encode_plutus_data(&data).unwrap();
        assert!(!cbor.is_empty());
    }

    #[test]
    fn test_constr_high_index() {
        let data = constr(7, vec![]);
        if let PlutusData::Constr(c) = &data {
            assert_eq!(c.tag, 102);
            assert_eq!(c.any_constructor, Some(7));
        } else {
            panic!("expected Constr");
        }
    }

    #[test]
    fn test_bytes_hex() {
        let data = bytes_hex("deadbeef").unwrap();
        if let PlutusData::BoundedBytes(b) = data {
            assert_eq!(&b[..], &[0xde, 0xad, 0xbe, 0xef]);
        } else {
            panic!("expected BoundedBytes");
        }
    }

    #[test]
    fn test_bytes_hex_invalid() {
        assert!(bytes_hex("not_hex").is_err());
    }

    #[test]
    fn test_int_positive() {
        let data = int(100);
        let cbor = encode_plutus_data(&data).unwrap();
        assert!(!cbor.is_empty());
    }

    #[test]
    fn test_int_negative() {
        let data = int(-1);
        let cbor = encode_plutus_data(&data).unwrap();
        assert!(!cbor.is_empty());
    }

    #[test]
    fn test_list_encodes() {
        let data = list(vec![int(1), int(2), int(3)]);
        let cbor = encode_plutus_data(&data).unwrap();
        assert!(!cbor.is_empty());
    }
}
