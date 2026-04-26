use crate::TxClassifierError;
pub use maestro::MaestroApi;
use tracing::{debug, warn};
use transactions::{MintOperation, RawTxData, TxDatum, TxInput, TxOutput};

/// Fetch transaction from Maestro
pub async fn get_tx_from_maestro(
    maestro: &MaestroApi,
    tx_hash: &str,
) -> Result<RawTxData, TxClassifierError> {
    debug!("Fetching from Maestro: {}", tx_hash);

    // Use the complete transaction endpoint if available (transactions feature)
    #[cfg(feature = "indexers")]
    {
        match maestro.get_complete_transaction(tx_hash).await {
            Ok(complete_tx) => {
                debug!(
                    "✅ Successfully fetched complete transaction data for {}",
                    tx_hash
                );
                let mut raw_tx = convert_complete_transaction_to_raw_data(&complete_tx)?;
                enrich_missing_datum_content(maestro, &mut raw_tx).await;
                return Ok(raw_tx);
            }
            Err(e) => {
                warn!("⚠️ Complete transaction endpoint failed for {}: {:?}, falling back to basic endpoints", tx_hash, e);
            }
        }
    }

    // Fallback to basic transaction details and UTXOs
    let tx_details = maestro.get_transaction(tx_hash).await?;

    // Try to fetch UTXOs, but don't fail if not available
    let tx_utxos = match maestro.get_transaction_utxos(tx_hash).await {
        Ok(utxos) => {
            debug!("✅ Successfully fetched UTXOs for transaction {}", tx_hash);
            utxos
        }
        Err(e) => {
            warn!(
                "⚠️ UTXOs endpoint failed for {}: {:?}, using transaction details only",
                tx_hash, e
            );
            // Create minimal UTXO structure from transaction details if possible
            return Ok(RawTxData {
                tx_hash: tx_details.tx_hash,
                inputs: vec![],  // No input data available without UTXOs endpoint
                outputs: vec![], // No output data available without UTXOs endpoint
                collateral_inputs: vec![], // No collateral data available without complete endpoint
                collateral_outputs: vec![], // No collateral data available without complete endpoint
                reference_inputs: vec![], // No reference input data available without complete endpoint
                mint: vec![],             // No mint data available without complete endpoint
                metadata: tx_details.metadata,
                fee: Some(tx_details.fee),
                block_height: Some(tx_details.block_height),
                timestamp: Some(tx_details.timestamp.timestamp() as u64),
                size: Some(tx_details.size),
                scripts: tx_details.scripts,
                redeemers: None,
            });
        }
    };

    // Convert Maestro data to our RawTxData format
    let inputs = tx_utxos
        .inputs
        .into_iter()
        .map(|input| {
            let mut assets = std::collections::HashMap::new();
            for asset in &input.assets {
                assets.insert(asset.unit.clone(), asset.amount);
            }

            TxInput {
                address: input.address.clone(),
                tx_hash: input.tx_hash.clone(),
                output_index: input.output_index,
                amount_lovelace: input.amount,
                assets,
                datum: None, // Basic UTXOs endpoint doesn't provide datum data for inputs
            }
        })
        .collect();

    let outputs = tx_utxos
        .outputs
        .into_iter()
        .map(|output| {
            let mut assets = std::collections::HashMap::new();
            for asset in &output.assets {
                assets.insert(asset.unit.clone(), asset.amount);
            }

            TxOutput {
                address: output.address.clone(),
                amount_lovelace: output.amount,
                assets,
                datum: if output.datum_hash.is_some() || output.inline_datum.is_some() {
                    Some(match (&output.datum_hash, &output.inline_datum) {
                        (Some(hash), None) => TxDatum::Hash { hash: hash.clone() },
                        (hash_opt, Some(json)) => TxDatum::Json {
                            hash: hash_opt.clone().unwrap_or_default(),
                            json: json.clone(),
                            bytes: None, // Basic UTXOs endpoint doesn't provide bytes
                        },
                        _ => unreachable!(), // We check above that at least one is Some
                    })
                } else {
                    None
                },
                script_ref: output.script_ref,
            }
        })
        .collect();

    Ok(RawTxData {
        tx_hash: tx_details.tx_hash,
        inputs,
        outputs,
        collateral_inputs: vec![], // Basic UTXOs endpoint doesn't provide collateral data
        collateral_outputs: vec![], // Basic UTXOs endpoint doesn't provide collateral data
        reference_inputs: vec![],  // Basic UTXOs endpoint doesn't provide reference input data
        mint: vec![],              // Basic UTXOs endpoint doesn't provide mint data
        metadata: tx_details.metadata,
        fee: Some(tx_details.fee),
        block_height: Some(tx_details.block_height),
        timestamp: Some(tx_details.timestamp.timestamp() as u64),
        size: Some(tx_details.size),
        scripts: tx_details.scripts,
        redeemers: None,
    })
}

/// Parse mint operations from Maestro's mint JSON format
pub fn parse_mint_operations(mint_data: &[serde_json::Value]) -> Vec<MintOperation> {
    let mut operations = Vec::new();

    for mint_value in mint_data {
        if let Some(mint_obj) = mint_value.as_object() {
            // Extract unit and amount fields from the mint object
            if let (Some(unit_value), Some(amount_value)) =
                (mint_obj.get("unit"), mint_obj.get("amount"))
            {
                if let (Some(unit), Some(amount)) = (unit_value.as_str(), amount_value.as_i64()) {
                    operations.push(MintOperation {
                        unit: unit.to_string(),
                        amount,
                    });
                }
            }
        }
    }

    operations
}

pub fn convert_complete_transaction_to_raw_data(
    complete_tx: &maestro::CompleteTransactionDetails,
) -> Result<RawTxData, TxClassifierError> {
    // Convert regular inputs (excluding collateral)
    let mut inputs = Vec::new();

    // Add regular inputs
    for input in &complete_tx.inputs {
        let mut assets = std::collections::HashMap::new();
        let mut amount_lovelace = 0u64;

        for asset in &input.assets {
            if asset.unit == "lovelace" {
                amount_lovelace = asset.amount;
            } else {
                assets.insert(asset.unit.clone(), asset.amount);
            }
        }

        inputs.push(TxInput {
            address: input.address.clone(),
            tx_hash: input.tx_hash.clone(),
            output_index: input.index,
            amount_lovelace,
            assets,
            datum: input.datum.as_ref().and_then(convert_maestro_datum),
        });
    }

    // Convert collateral inputs separately
    let mut collateral_inputs = Vec::new();

    for input in &complete_tx.collateral_inputs {
        let mut assets = std::collections::HashMap::new();
        let mut amount_lovelace = 0u64;

        for asset in &input.assets {
            if asset.unit == "lovelace" {
                amount_lovelace = asset.amount;
            } else {
                assets.insert(asset.unit.clone(), asset.amount);
            }
        }

        collateral_inputs.push(TxInput {
            address: input.address.clone(),
            tx_hash: input.tx_hash.clone(),
            output_index: input.index,
            amount_lovelace,
            assets,
            datum: input.datum.as_ref().and_then(convert_maestro_datum),
        });
    }

    // Convert regular outputs (excluding collateral return)
    let mut outputs = Vec::new();

    // Add regular outputs
    for output in &complete_tx.outputs {
        let mut assets = std::collections::HashMap::new();
        let mut amount_lovelace = 0u64;

        for asset in &output.assets {
            if asset.unit == "lovelace" {
                amount_lovelace = asset.amount;
            } else {
                assets.insert(asset.unit.clone(), asset.amount);
            }
        }

        outputs.push(TxOutput {
            address: output.address.clone(),
            amount_lovelace,
            assets,
            datum: output.datum.as_ref().and_then(convert_maestro_datum),
            script_ref: output
                .reference_script
                .clone()
                .map(|_| "script_present".to_string()),
        });
    }

    // Convert collateral outputs separately
    let mut collateral_outputs = Vec::new();

    // Add collateral return if present (captures return of excess collateral)
    if let Some(collateral_return) = &complete_tx.collateral_return {
        let mut assets = std::collections::HashMap::new();
        let mut amount_lovelace = 0u64;

        for asset in &collateral_return.assets {
            if asset.unit == "lovelace" {
                amount_lovelace = asset.amount;
            } else {
                assets.insert(asset.unit.clone(), asset.amount);
            }
        }

        collateral_outputs.push(TxOutput {
            address: collateral_return.address.clone(),
            amount_lovelace,
            assets,
            datum: collateral_return
                .datum
                .as_ref()
                .and_then(convert_maestro_datum),
            script_ref: collateral_return
                .reference_script
                .clone()
                .map(|_| "script_present".to_string()),
        });
    }

    // Extract script hashes from executed scripts
    let scripts: Vec<String> = complete_tx
        .scripts_executed
        .iter()
        .map(|script| script.hash.clone())
        .collect();

    // Parse mint operations from raw JSON
    let mint_operations = parse_mint_operations(&complete_tx.mint);

    // Convert redeemers to JSON value for analysis
    let redeemers_json = serde_json::to_value(&complete_tx.redeemers).ok();

    // Convert reference inputs if available
    let mut reference_inputs = Vec::new();

    // Check if reference_inputs exists in the complete transaction data
    if !complete_tx.reference_inputs.is_empty() {
        let ref_inputs = &complete_tx.reference_inputs;
        for input in ref_inputs {
            let mut assets = std::collections::HashMap::new();
            let mut amount_lovelace = 0u64;

            for asset in &input.assets {
                if asset.unit == "lovelace" {
                    amount_lovelace = asset.amount;
                } else {
                    assets.insert(asset.unit.clone(), asset.amount);
                }
            }

            reference_inputs.push(TxInput {
                address: input.address.clone(),
                tx_hash: input.tx_hash.clone(),
                output_index: input.index,
                amount_lovelace,
                assets,
                datum: input.datum.as_ref().and_then(convert_maestro_datum),
            });
        }
    }

    Ok(RawTxData {
        tx_hash: complete_tx.tx_hash.clone(),
        inputs,
        outputs,
        collateral_inputs,
        collateral_outputs,
        reference_inputs,
        mint: mint_operations,
        metadata: complete_tx.metadata.clone(),
        fee: Some(complete_tx.fee),
        block_height: Some(complete_tx.block_height),
        timestamp: Some(complete_tx.block_timestamp),
        size: Some(complete_tx.size),
        scripts,
        redeemers: redeemers_json,
    })
}

/// Helper function to convert Maestro datum format to TxDatum
fn convert_maestro_datum(maestro_datum: &serde_json::Value) -> Option<TxDatum> {
    if maestro_datum.is_null() {
        return None;
    }

    // For complete transaction API, datum comes as structured object
    if let Some(datum_obj) = maestro_datum.as_object() {
        let datum_type_str = datum_obj
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");

        let hash = datum_obj
            .get("hash")
            .and_then(|h| h.as_str())
            .unwrap_or("")
            .to_string();

        let bytes = datum_obj
            .get("bytes")
            .and_then(|b| b.as_str())
            .map(|s| s.to_string());

        let json = datum_obj.get("json").cloned();

        debug!(
            "convert_maestro_datum: type={}, has_bytes={}, has_json={}, bytes_len={}",
            datum_type_str,
            bytes.is_some(),
            json.is_some(),
            bytes.as_ref().map(|b| b.len()).unwrap_or(0)
        );

        return Some(match (json, bytes) {
            (Some(json), bytes) => TxDatum::Json { hash, json, bytes },
            (None, Some(bytes)) => TxDatum::Bytes { hash, bytes },
            (None, None) => TxDatum::Hash { hash },
        });
    }

    // For basic UTXOs API or inline datum, treat as JSON datum
    Some(TxDatum::Json {
        hash: "".to_string(), // We don't have hash for inline datums from basic API
        json: maestro_datum.clone(),
        bytes: None,
    })
}

/// Check if a datum is missing useful data (has hash but no bytes and null/missing JSON)
fn datum_needs_resolution(datum: &TxDatum) -> bool {
    match datum {
        TxDatum::Hash { hash } => !hash.is_empty(),
        TxDatum::Json {
            hash, json, bytes, ..
        } => !hash.is_empty() && bytes.is_none() && json.is_null(),
        TxDatum::Bytes { .. } => false, // Already have bytes
    }
}

/// Enrich datums that only have a hash by extracting content from tx metadata or Maestro API.
///
/// JPG.store V2 listings create UTXOs with datum-hash references but publish the actual
/// datum CBOR in transaction metadata (keys 50+). This function first tries to extract
/// datums from metadata (free, no API call), then falls back to Maestro's batch datum
/// endpoint for any remaining unresolved datums.
async fn enrich_missing_datum_content(maestro: &MaestroApi, raw_tx: &mut RawTxData) {
    // Collect all datum hashes that need resolution from inputs, outputs, and reference inputs
    let mut unresolved_hashes: Vec<String> = Vec::new();

    for utxo_list in [&raw_tx.inputs, &raw_tx.reference_inputs] {
        for input in utxo_list {
            if let Some(datum) = &input.datum {
                if datum_needs_resolution(datum) {
                    let hash = datum.hash().to_string();
                    if !unresolved_hashes.contains(&hash) {
                        unresolved_hashes.push(hash);
                    }
                }
            }
        }
    }

    for output in &raw_tx.outputs {
        if let Some(datum) = &output.datum {
            if datum_needs_resolution(datum) {
                let hash = datum.hash().to_string();
                if !unresolved_hashes.contains(&hash) {
                    unresolved_hashes.push(hash);
                }
            }
        }
    }

    if unresolved_hashes.is_empty() {
        return;
    }

    debug!(
        "Enriching {} unresolved datum(s) for tx {}",
        unresolved_hashes.len(),
        raw_tx.tx_hash
    );

    // Step 1: Try extracting datums from tx metadata (free, no API call needed)
    let metadata_datums = extract_datums_from_metadata(raw_tx);
    if !metadata_datums.is_empty() {
        debug!(
            "Extracted {} datum(s) from tx metadata",
            metadata_datums.len()
        );
        apply_resolved_datums(raw_tx, &metadata_datums);

        // Recount unresolved
        unresolved_hashes.retain(|hash| !metadata_datums.contains_key(hash));
    }

    if unresolved_hashes.is_empty() {
        return;
    }

    // Step 2: Fall back to Maestro batch datum endpoint for remaining unresolved datums
    debug!(
        "Falling back to Maestro API for {} remaining unresolved datum(s)",
        unresolved_hashes.len()
    );
    let hash_refs: Vec<&str> = unresolved_hashes.iter().map(|s| s.as_str()).collect();
    let resolved = match maestro.get_datums_by_hashes(&hash_refs).await {
        Ok(data) => data,
        Err(e) => {
            warn!("Failed to resolve datums by hash: {e}");
            return;
        }
    };

    debug!(
        "Resolved {}/{} datums from Maestro API",
        resolved.len(),
        unresolved_hashes.len()
    );

    // Convert Maestro DatumData to CBOR hex strings for uniform application
    let api_datums: std::collections::HashMap<String, String> = resolved
        .into_iter()
        .map(|(hash, data)| (hash, data.bytes))
        .collect();
    apply_resolved_datums(raw_tx, &api_datums);
}

/// Extract datum CBOR blobs from transaction metadata.
///
/// JPG.store V2 publishes datum content in tx metadata under sequential numeric keys
/// starting at 50. The values are hex-encoded CBOR chunks that may be split across
/// multiple keys. All chunks are concatenated and then split on commas to yield
/// individual datum blobs. Each blob is hashed with blake2b-256 to produce a datum hash
/// that maps 1:1 (positionally) with the output datum hashes.
fn extract_datums_from_metadata(raw_tx: &RawTxData) -> std::collections::HashMap<String, String> {
    let mut result = std::collections::HashMap::new();

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
        if let Some(value) = metadata_obj.get(&key_str) {
            if let Some(s) = value.as_str() {
                combined_hex.push_str(s);
            }
        }
    }

    if combined_hex.is_empty() {
        return result;
    }

    // Split on commas to get individual datum hex blobs
    let datum_hexes: Vec<&str> = combined_hex.split(',').collect();

    debug!(
        "Found {} datum blob(s) in metadata keys {:?}",
        datum_hexes.len(),
        datum_keys
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
fn apply_resolved_datums(
    raw_tx: &mut RawTxData,
    resolved: &std::collections::HashMap<String, String>,
) {
    let apply = |datum: &mut Option<TxDatum>| {
        if let Some(d) = datum {
            if datum_needs_resolution(d) {
                let hash = d.hash().to_string();
                if let Some(bytes) = resolved.get(&hash) {
                    *d = TxDatum::Bytes {
                        hash,
                        bytes: bytes.clone(),
                    };
                }
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
