//! Transaction parsing and inspection utilities
//!
//! Parse Cardano transactions and witness sets to inspect their contents.

use crate::PallasError;
use pallas_codec::minicbor;
use pallas_primitives::conway::{Tx, WitnessSet};

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
