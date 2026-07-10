//! Koios transaction-detail adapter for the classifier.
//!
//! Fetches a full transaction from Koios `/tx_info` (inputs+assets+scripts
//! resolved) and converts it into the pipeline's provider-agnostic
//! [`RawTxData`]. This is the Koios sibling of [`super::maestro`]; the
//! [`IndexerPool`](super::IndexerPool) picks between them at runtime so the two
//! can be A/B'd on live traffic.

use std::collections::HashMap;

use koios::koios_transaction::{KoiosTransaction, KoisUtxo};
use koios::KoiosApi;
use tracing::info;
use transactions::{MintOperation, RawTxData, TxDatum, TxInput, TxOutput};

use crate::TxClassifierError;

/// Fetch a transaction from Koios `/tx_info` and convert it to [`RawTxData`].
pub async fn get_tx_from_koios(
    koios: &KoiosApi,
    tx_hash: &str,
) -> Result<RawTxData, TxClassifierError> {
    info!("Fetching transaction data from Koios for: {tx_hash}");

    let txs = koios
        .get_tx_details(&[tx_hash.to_string()])
        .await
        .map_err(|e| {
            TxClassifierError::ClassificationFailed(format!("Koios tx_info failed: {e}"))
        })?;

    let tx = txs
        .into_iter()
        .next()
        .ok_or_else(|| TxClassifierError::TransactionNotFound(tx_hash.to_string()))?;

    convert_koios_tx_to_raw_data(&tx)
}

/// Convert a Koios `/tx_info` transaction into the pipeline's [`RawTxData`].
pub fn convert_koios_tx_to_raw_data(
    tx: &KoiosTransaction,
) -> Result<RawTxData, TxClassifierError> {
    let inputs = tx.inputs.iter().map(koios_utxo_to_input).collect();
    let outputs = tx.outputs.iter().map(koios_utxo_to_output).collect();
    let collateral_inputs = tx.collateral_inputs.iter().map(koios_utxo_to_input).collect();
    // Koios returns a single optional collateral return output.
    let collateral_outputs = tx
        .collateral_output
        .iter()
        .map(koios_utxo_to_output)
        .collect();
    let reference_inputs = tx.reference_inputs.iter().map(koios_utxo_to_input).collect();

    // Mint/burn from the typed `assets_minted` list (quantity is signed: a
    // negative value is a burn).
    let mint = tx
        .assets_minted
        .iter()
        .filter_map(|m| {
            let amount = m.quantity.parse::<i64>().ok()?;
            Some(MintOperation {
                unit: format!("{}{}", m.policy_id, m.asset_name),
                amount,
            })
        })
        .collect();

    // Executed Plutus script hashes.
    let scripts = tx
        .plutus_contracts
        .iter()
        .filter_map(|c| {
            c.get("script_hash")
                .and_then(|h| h.as_str())
                .map(String::from)
        })
        .collect();

    // Redeemer data lives inside each plutus_contract entry; hand the raw list
    // to the classifier for redeemer-level analysis.
    let redeemers = serde_json::to_value(&tx.plutus_contracts).ok();

    Ok(RawTxData {
        tx_hash: tx.tx_hash.clone(),
        inputs,
        outputs,
        collateral_inputs,
        collateral_outputs,
        reference_inputs,
        mint,
        metadata: tx.metadata.clone(),
        fee: tx.fee.parse::<u64>().ok(),
        block_height: Some(tx.block_height),
        timestamp: Some(tx.tx_timestamp),
        size: u32::try_from(tx.tx_size).ok(),
        scripts,
        redeemers,
    })
}

fn koios_utxo_to_input(utxo: &KoisUtxo) -> TxInput {
    TxInput {
        address: utxo.payment_addr.bech32.clone(),
        tx_hash: utxo.tx_hash.clone(),
        output_index: utxo.tx_index,
        amount_lovelace: utxo.value.parse::<u64>().unwrap_or(0),
        assets: koios_assets_to_map(utxo),
        datum: koios_utxo_datum(utxo),
    }
}

fn koios_utxo_to_output(utxo: &KoisUtxo) -> TxOutput {
    TxOutput {
        address: utxo.payment_addr.bech32.clone(),
        amount_lovelace: utxo.value.parse::<u64>().unwrap_or(0),
        assets: koios_assets_to_map(utxo),
        datum: koios_utxo_datum(utxo),
        script_ref: utxo
            .reference_script
            .as_ref()
            .map(|_| "script_present".to_string()),
    }
}

/// Native assets on a UTxO as `unit -> quantity` (`unit` = `policy_id || asset_name`).
fn koios_assets_to_map(utxo: &KoisUtxo) -> HashMap<String, u64> {
    (&utxo.asset_list)
        .into_iter()
        .map(|a| (format!("{}{}", a.policy_id, a.asset_name), a.quantity as u64))
        .collect()
}

/// Map a Koios UTxO's datum into [`TxDatum`]: prefer the inline datum's raw CBOR
/// (schema-independent — the classifier decodes it via pallas), else the bare
/// datum hash.
fn koios_utxo_datum(utxo: &KoisUtxo) -> Option<TxDatum> {
    if let Some(inline) = &utxo.inline_datum {
        if let Some(bytes) = &inline.bytes {
            return Some(TxDatum::Bytes {
                hash: utxo.datum_hash.clone().unwrap_or_default(),
                bytes: bytes.clone(),
            });
        }
    }
    utxo.datum_hash
        .as_ref()
        .map(|hash| TxDatum::Hash { hash: hash.clone() })
}
