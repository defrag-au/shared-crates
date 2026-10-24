//! Recovering datum content an indexer reported by hash only.
//!
//! A UTxO locked with a datum *hash* carries no datum content of its own, and
//! the classifier cannot read a listing price out of a hash. Two places hold
//! the preimage:
//!
//! 1. The transaction's own metadata. JPG.store V2 listings publish the datum
//!    CBOR there (keys 50+) — free, no extra request.
//! 2. The indexer's datum-by-hash lookup, for whatever metadata did not cover.
//!
//! This module is the provider-neutral half: finding what is unresolved,
//! reading datums out of metadata, and writing resolved bytes back. The
//! provider adapter owns step 2.

use std::collections::HashMap;

use tracing::debug;
use transactions::{RawTxData, TxDatum};

/// Whether a datum is missing useful data: a hash, but no bytes and no JSON.
fn datum_needs_resolution(datum: &TxDatum) -> bool {
    match datum {
        TxDatum::Hash { hash } => !hash.is_empty(),
        TxDatum::Json {
            hash, json, bytes, ..
        } => !hash.is_empty() && bytes.is_none() && json.is_null(),
        TxDatum::Bytes { .. } => false, // Already have bytes
    }
}

/// Every distinct datum hash across inputs, reference inputs and outputs whose
/// content is still unknown, in first-seen order.
pub(crate) fn unresolved_datum_hashes(raw_tx: &RawTxData) -> Vec<String> {
    let mut unresolved: Vec<String> = Vec::new();

    let input_datums = raw_tx
        .inputs
        .iter()
        .chain(&raw_tx.reference_inputs)
        .filter_map(|input| input.datum.as_ref());
    let output_datums = raw_tx
        .outputs
        .iter()
        .filter_map(|output| output.datum.as_ref());

    for datum in input_datums.chain(output_datums) {
        if datum_needs_resolution(datum) {
            let hash = datum.hash().to_string();
            if !unresolved.contains(&hash) {
                unresolved.push(hash);
            }
        }
    }

    unresolved
}

/// Extract datum CBOR blobs from transaction metadata.
///
/// JPG.store V2 publishes datum content in tx metadata under sequential numeric keys
/// starting at 50. The values are hex-encoded CBOR chunks that may be split across
/// multiple keys. All chunks are concatenated and then split on commas to yield
/// individual datum blobs. Each blob is hashed with blake2b-256 to produce a datum hash
/// that maps 1:1 (positionally) with the output datum hashes.
pub(crate) fn extract_datums_from_metadata(raw_tx: &RawTxData) -> HashMap<String, String> {
    let mut result = HashMap::new();

    let metadata = match &raw_tx.metadata {
        Some(m) if !m.is_null() => m,
        _ => return result,
    };

    // Collect all string values from metadata keys >= 50 in numeric order
    let metadata_obj = match metadata.as_object() {
        Some(obj) => obj,
        None => return result,
    };

    let mut datum_keys: Vec<u64> = metadata_obj
        .keys()
        .filter_map(|k| k.parse::<u64>().ok())
        .filter(|&k| k >= 50)
        .collect();
    datum_keys.sort();

    if datum_keys.is_empty() {
        return result;
    }

    // Concatenate all hex chunks from sequential metadata keys
    let mut combined_hex = String::new();
    for key in &datum_keys {
        let key_str = key.to_string();
        if let Some(value) = metadata_obj.get(&key_str)
            && let Some(s) = value.as_str()
        {
            combined_hex.push_str(s);
        }
    }

    if combined_hex.is_empty() {
        return result;
    }

    // Split on commas to get individual datum hex blobs
    let datum_hexes: Vec<&str> = combined_hex.split(',').collect();

    debug!(
        "Found {} datum blob(s) in metadata keys {datum_keys:?}",
        datum_hexes.len()
    );

    // Hash each blob and build lookup map
    for datum_hex in &datum_hexes {
        let trimmed = datum_hex.trim();
        if trimmed.is_empty() {
            continue;
        }
        match hex::decode(trimmed) {
            Ok(datum_bytes) => {
                let hash = pallas_crypto::hash::Hasher::<256>::hash(&datum_bytes);
                result.insert(hash.to_string(), trimmed.to_string());
            }
            Err(e) => {
                debug!("Failed to decode metadata datum hex: {e}");
            }
        }
    }

    result
}

/// Apply resolved datum CBOR bytes back to all inputs, outputs, and reference inputs
/// that have matching unresolved datum hashes.
pub(crate) fn apply_resolved_datums(raw_tx: &mut RawTxData, resolved: &HashMap<String, String>) {
    let apply = |datum: &mut Option<TxDatum>| {
        if let Some(d) = datum
            && datum_needs_resolution(d)
        {
            let hash = d.hash().to_string();
            if let Some(bytes) = resolved.get(&hash) {
                *d = TxDatum::Bytes {
                    hash,
                    bytes: bytes.clone(),
                };
            }
        }
    };

    for input in &mut raw_tx.inputs {
        apply(&mut input.datum);
    }
    for input in &mut raw_tx.reference_inputs {
        apply(&mut input.datum);
    }
    for output in &mut raw_tx.outputs {
        apply(&mut output.datum);
    }
}
