//! Test-only: turn the Maestro-shaped fixtures under `resources/test/txs`
//! into [`RawTxData`].
//!
//! Those fixtures are real mainnet transactions captured from Maestro's
//! `/transactions/{hash}` (complete) endpoint while it was the classifier's
//! provider. Maestro has shut down, so nothing fetches that shape any more —
//! but the classification rules run on [`RawTxData`], not on a provider's
//! shape, so the fixtures still specify them. This is the conversion the
//! production Maestro adapter used, kept verbatim so the suite keeps testing
//! the same inputs; `maestro` is a dev-dependency for its types alone.

use maestro::CompleteTransactionDetails;
use tracing::debug;
use transactions::{MintOperation, RawTxData, TxDatum, TxInput, TxOutput};

use crate::TxClassifierError;

/// Parse mint operations from Maestro's mint JSON format
fn parse_mint_operations(mint_data: &[serde_json::Value]) -> Vec<MintOperation> {
    let mut operations = Vec::new();

    for mint_value in mint_data {
        if let Some(mint_obj) = mint_value.as_object() {
            // Extract unit and amount fields from the mint object
            if let (Some(unit_value), Some(amount_value)) =
                (mint_obj.get("unit"), mint_obj.get("amount"))
                && let (Some(unit), Some(amount)) = (unit_value.as_str(), amount_value.as_i64())
            {
                operations.push(MintOperation {
                    unit: unit.to_string(),
                    amount,
                });
            }
        }
    }

    operations
}

pub fn convert_complete_transaction_to_raw_data(
    complete_tx: &CompleteTransactionDetails,
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
            "convert_maestro_datum: type={datum_type_str}, has_bytes={}, has_json={}, bytes_len={}",
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
