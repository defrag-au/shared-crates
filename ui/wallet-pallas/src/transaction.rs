//! Transaction parsing, inspection, and assembly utilities
//!
//! Parse Cardano transactions and witness sets to inspect their contents.
//! Also provides `assemble_signed_tx` for merging CIP-30 witness sets
//! into unsigned transactions.

use crate::PallasError;
use pallas_codec::minicbor;
use pallas_codec::utils::NonEmptySet;
use pallas_primitives::conway::{Tx, VKeyWitness, WitnessSet};

/// Information about a parsed transaction
#[derive(Debug, Clone)]
pub struct TransactionInfo {
    /// Number of inputs
    pub input_count: usize,
    /// Number of outputs
    pub output_count: usize,
    /// Fee in lovelace
    pub fee: u64,
    /// TTL (time to live) slot if set
    pub ttl: Option<u64>,
    /// Whether the transaction has metadata
    pub has_metadata: bool,
    /// Whether the transaction has script witnesses
    pub has_scripts: bool,
    /// Number of VKey witnesses
    pub vkey_witness_count: usize,
}

/// Information about a witness set
#[derive(Debug, Clone)]
pub struct WitnessSetInfo {
    /// Number of VKey (signature) witnesses
    pub vkey_witnesses: usize,
    /// Number of native scripts
    pub native_scripts: usize,
    /// Number of Plutus V1 scripts
    pub plutus_v1_scripts: usize,
    /// Number of Plutus V2 scripts
    pub plutus_v2_scripts: usize,
    /// Number of Plutus V3 scripts
    pub plutus_v3_scripts: usize,
    /// Whether redeemers are present
    pub has_redeemers: bool,
    /// Number of datums
    pub datums: usize,
}

/// Parse a transaction from hex-encoded CBOR
pub fn parse_transaction(tx_hex: &str) -> Result<TransactionInfo, PallasError> {
    let tx_bytes = hex::decode(tx_hex)?;

    let tx: Tx =
        minicbor::decode(&tx_bytes).map_err(|e| PallasError::TransactionParse(e.to_string()))?;

    let body = &tx.transaction_body;
    let witness = &tx.transaction_witness_set;

    let vkey_witness_count = witness.vkeywitness.as_ref().map(|v| v.len()).unwrap_or(0);

    let has_scripts = witness.native_script.is_some()
        || witness.plutus_v1_script.is_some()
        || witness.plutus_v2_script.is_some()
        || witness.plutus_v3_script.is_some();

    Ok(TransactionInfo {
        input_count: body.inputs.len(),
        output_count: body.outputs.len(),
        fee: body.fee,
        ttl: body.ttl,
        has_metadata: !matches!(tx.auxiliary_data, pallas_codec::utils::Nullable::Null),
        has_scripts,
        vkey_witness_count,
    })
}

/// Parse a witness set from hex-encoded CBOR
///
/// This is useful for inspecting the witness set returned by `signTx`
pub fn parse_witness_set(witness_hex: &str) -> Result<WitnessSetInfo, PallasError> {
    let witness_bytes = hex::decode(witness_hex)?;

    let witness: WitnessSet =
        minicbor::decode(&witness_bytes).map_err(|e| PallasError::CborDecode(e.to_string()))?;

    Ok(WitnessSetInfo {
        vkey_witnesses: witness.vkeywitness.as_ref().map(|v| v.len()).unwrap_or(0),
        native_scripts: witness.native_script.as_ref().map(|v| v.len()).unwrap_or(0),
        plutus_v1_scripts: witness
            .plutus_v1_script
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(0),
        plutus_v2_scripts: witness
            .plutus_v2_script
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(0),
        plutus_v3_scripts: witness
            .plutus_v3_script
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(0),
        has_redeemers: witness.redeemer.is_some(),
        datums: witness.plutus_data.as_ref().map(|v| v.len()).unwrap_or(0),
    })
}

/// Extract VKey witness public keys and signatures from a witness set
pub fn extract_vkey_witnesses(witness_hex: &str) -> Result<Vec<(String, String)>, PallasError> {
    let witness_bytes = hex::decode(witness_hex)?;

    let witness: WitnessSet =
        minicbor::decode(&witness_bytes).map_err(|e| PallasError::CborDecode(e.to_string()))?;

    let result: Vec<(String, String)> = witness
        .vkeywitness
        .map(|witnesses| {
            witnesses
                .iter()
                .map(|w| {
                    let vkey_bytes: &[u8] = w.vkey.as_ref();
                    let sig_bytes: &[u8] = w.signature.as_ref();
                    let vkey_hex = hex::encode(vkey_bytes);
                    let sig_hex = hex::encode(sig_bytes);
                    (vkey_hex, sig_hex)
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(result)
}

/// Merge a CIP-30 witness set into an unsigned transaction to produce a signed TX.
///
/// CIP-30's `signTx()` returns only the witness set CBOR (containing VKey witnesses),
/// not a full signed transaction. This function merges those witnesses into the
/// unsigned TX to produce a fully assembled transaction ready for submission.
///
/// # Arguments
/// * `unsigned_tx_hex` - Hex-encoded CBOR of the unsigned transaction
/// * `witness_set_hex` - Hex-encoded CBOR of the witness set from CIP-30 `signTx()`
///
/// # Returns
/// Hex-encoded CBOR of the fully assembled (signed) transaction
pub fn assemble_signed_tx(
    unsigned_tx_hex: &str,
    witness_set_hex: &str,
) -> Result<String, PallasError> {
    let tx_bytes = hex::decode(unsigned_tx_hex)?;
    let witness_bytes = hex::decode(witness_set_hex)?;

    // Decode the unsigned TX — a 4-element CBOR array:
    // [tx_body, witness_set, is_valid, auxiliary_data]
    let mut tx: Tx =
        minicbor::decode(&tx_bytes).map_err(|e| PallasError::TransactionParse(e.to_string()))?;

    // Decode the CIP-30 witness set (contains VKey witnesses from the wallet)
    let cip30_witness: WitnessSet =
        minicbor::decode(&witness_bytes).map_err(|e| PallasError::CborDecode(e.to_string()))?;

    // Extract VKey witnesses from CIP-30 response
    let new_vkeys: Vec<VKeyWitness> = cip30_witness
        .vkeywitness
        .map(|set| set.to_vec())
        .unwrap_or_default();

    if new_vkeys.is_empty() {
        return Err(PallasError::TransactionParse(
            "CIP-30 witness set contains no VKey witnesses".into(),
        ));
    }

    // Merge VKey witnesses into the TX's witness set.
    // DerefMut on KeepRaw clears raw bytes, forcing re-encode from inner value.
    let tx_witness = &mut *tx.transaction_witness_set;

    let mut all_vkeys: Vec<VKeyWitness> = tx_witness
        .vkeywitness
        .take()
        .map(|set| set.to_vec())
        .unwrap_or_default();

    all_vkeys.extend(new_vkeys);

    tx_witness.vkeywitness = NonEmptySet::from_vec(all_vkeys);

    // Re-encode the full transaction.
    // tx_body and auxiliary_data retain their KeepRaw bytes (untouched).
    // witness_set will re-encode from the modified inner value (raw was cleared).
    let signed_bytes = minicbor::to_vec(&tx)
        .map_err(|e| PallasError::TransactionParse(format!("failed to encode signed TX: {e}")))?;

    Ok(hex::encode(signed_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid unsigned TX CBOR (no witnesses) and a witness set,
    /// then verify assemble_signed_tx merges them correctly.
    #[test]
    fn test_assemble_signed_tx_adds_vkey_witness() {
        // Minimal unsigned TX: we build a 4-element CBOR array manually.
        // [tx_body, empty_witness_set, true, null]
        //
        // tx_body = { 0: [{0: h'aa..aa', 1: 0}], 1: [{0: h'bb..bb', 1: 2000000}], 2: 200000 }
        // empty_witness_set = {}
        let mut enc = minicbor::Encoder::new(Vec::new());

        // Start TX array(4)
        enc.array(4).unwrap();

        // tx_body: map with inputs, outputs, fee
        enc.map(3).unwrap();
        // key 0: inputs (set of [tx_hash, index])
        enc.u32(0).unwrap();
        enc.array(1).unwrap(); // 1 input
        enc.array(2).unwrap();
        enc.bytes(&[0xaa; 32]).unwrap(); // tx_hash
        enc.u32(0).unwrap(); // index

        // key 1: outputs
        enc.u32(1).unwrap();
        enc.array(1).unwrap(); // 1 output
                               // Post-alonzo output as map
        enc.map(2).unwrap();
        enc.u32(0).unwrap(); // address key
        enc.bytes(&[0x01; 57]).unwrap(); // 57-byte shelley address
        enc.u32(1).unwrap(); // amount key
        enc.u64(2_000_000).unwrap(); // 2 ADA

        // key 2: fee
        enc.u32(2).unwrap();
        enc.u64(200_000).unwrap();

        // empty witness_set: map(0)
        enc.map(0).unwrap();

        // is_valid: true
        enc.bool(true).unwrap();

        // auxiliary_data: null
        enc.null().unwrap();

        let unsigned_tx_hex = hex::encode(enc.into_writer());

        // Verify it parses as unsigned (no witnesses)
        let info = parse_transaction(&unsigned_tx_hex).unwrap();
        assert_eq!(info.input_count, 1);
        assert_eq!(info.output_count, 1);
        assert_eq!(info.fee, 200_000);
        assert_eq!(info.vkey_witness_count, 0);

        // Build a CIP-30 witness set with 1 VKey witness
        // witness_set = { 0: [[vkey(32 bytes), signature(64 bytes)]] }
        let mut ws_enc = minicbor::Encoder::new(Vec::new());
        ws_enc.map(1).unwrap();
        ws_enc.u32(0).unwrap(); // key 0 = vkeywitness
        ws_enc.array(1).unwrap(); // 1 witness
        ws_enc.array(2).unwrap(); // [vkey, sig]
        ws_enc.bytes(&[0xcc; 32]).unwrap(); // fake 32-byte vkey
        ws_enc.bytes(&[0xdd; 64]).unwrap(); // fake 64-byte signature
        let witness_set_hex = hex::encode(ws_enc.into_writer());

        // Assemble
        let signed_hex = assemble_signed_tx(&unsigned_tx_hex, &witness_set_hex).unwrap();

        // Parse the result — should have 1 VKey witness now
        let signed_info = parse_transaction(&signed_hex).unwrap();
        assert_eq!(signed_info.input_count, 1);
        assert_eq!(signed_info.output_count, 1);
        assert_eq!(signed_info.fee, 200_000);
        assert_eq!(signed_info.vkey_witness_count, 1);

        // Verify the witness content
        let witnesses = extract_vkey_witnesses(&{
            // Extract witness set from the signed TX for inspection
            let signed_bytes = hex::decode(&signed_hex).unwrap();
            let tx: Tx = minicbor::decode(&signed_bytes).unwrap();
            hex::encode(tx.transaction_witness_set.raw_cbor())
        })
        .unwrap();
        assert_eq!(witnesses.len(), 1);
        assert_eq!(witnesses[0].0, hex::encode([0xcc; 32]));
        assert_eq!(witnesses[0].1, hex::encode([0xdd; 64]));
    }

    #[test]
    fn test_assemble_merges_existing_witnesses() {
        // Build unsigned TX that already has 1 VKey witness
        let mut enc = minicbor::Encoder::new(Vec::new());
        enc.array(4).unwrap();

        // tx_body
        enc.map(3).unwrap();
        enc.u32(0).unwrap();
        enc.array(1).unwrap();
        enc.array(2).unwrap();
        enc.bytes(&[0xaa; 32]).unwrap();
        enc.u32(0).unwrap();
        enc.u32(1).unwrap();
        enc.array(1).unwrap();
        enc.map(2).unwrap();
        enc.u32(0).unwrap();
        enc.bytes(&[0x01; 57]).unwrap();
        enc.u32(1).unwrap();
        enc.u64(2_000_000).unwrap();
        enc.u32(2).unwrap();
        enc.u64(200_000).unwrap();

        // witness_set with 1 existing vkey
        enc.map(1).unwrap();
        enc.u32(0).unwrap();
        enc.array(1).unwrap();
        enc.array(2).unwrap();
        enc.bytes(&[0x11; 32]).unwrap(); // existing vkey
        enc.bytes(&[0x22; 64]).unwrap(); // existing sig

        enc.bool(true).unwrap();
        enc.null().unwrap();
        let unsigned_hex = hex::encode(enc.into_writer());

        // CIP-30 witness set with a different vkey
        let mut ws_enc = minicbor::Encoder::new(Vec::new());
        ws_enc.map(1).unwrap();
        ws_enc.u32(0).unwrap();
        ws_enc.array(1).unwrap();
        ws_enc.array(2).unwrap();
        ws_enc.bytes(&[0x33; 32]).unwrap(); // new vkey
        ws_enc.bytes(&[0x44; 64]).unwrap(); // new sig
        let ws_hex = hex::encode(ws_enc.into_writer());

        let signed_hex = assemble_signed_tx(&unsigned_hex, &ws_hex).unwrap();
        let signed_info = parse_transaction(&signed_hex).unwrap();
        assert_eq!(signed_info.vkey_witness_count, 2);
    }

    #[test]
    fn test_assemble_rejects_empty_witness_set() {
        // Build minimal unsigned TX
        let mut enc = minicbor::Encoder::new(Vec::new());
        enc.array(4).unwrap();
        enc.map(3).unwrap();
        enc.u32(0).unwrap();
        enc.array(1).unwrap();
        enc.array(2).unwrap();
        enc.bytes(&[0xaa; 32]).unwrap();
        enc.u32(0).unwrap();
        enc.u32(1).unwrap();
        enc.array(1).unwrap();
        enc.map(2).unwrap();
        enc.u32(0).unwrap();
        enc.bytes(&[0x01; 57]).unwrap();
        enc.u32(1).unwrap();
        enc.u64(2_000_000).unwrap();
        enc.u32(2).unwrap();
        enc.u64(200_000).unwrap();
        enc.map(0).unwrap();
        enc.bool(true).unwrap();
        enc.null().unwrap();
        let unsigned_hex = hex::encode(enc.into_writer());

        // Empty witness set (no vkeys)
        let mut ws_enc = minicbor::Encoder::new(Vec::new());
        ws_enc.map(0).unwrap();
        let ws_hex = hex::encode(ws_enc.into_writer());

        let result = assemble_signed_tx(&unsigned_hex, &ws_hex);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no VKey witnesses"));
    }
}
