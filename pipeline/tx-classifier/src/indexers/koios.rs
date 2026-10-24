//! Koios transaction-detail adapter for the classifier.
//!
//! Fetches a full transaction from Koios `/tx_info` (inputs+assets+scripts
//! resolved) and converts it into the pipeline's provider-agnostic
//! [`RawTxData`]. Koios is the classifier's only transaction-detail provider.
//!
//! Three requests at most, where the retired Maestro adapter made one or two:
//!
//! - `/tx_info` for the transaction itself.
//! - `/tx_metadata`, because [`KoiosApi::get_tx_details`] does not ask
//!   `/tx_info` for metadata — and mint, offer and marketplace rules all read
//!   it, as does JPG.store datum recovery.
//! - `/datum_info`, only when an input or output is still known by datum hash
//!   alone after the metadata has been searched.

use std::collections::HashMap;

use koios::KoiosApi;
use koios::koios_transaction::{KoiosTransaction, KoisUtxo};
use tracing::{debug, info, warn};
use transactions::{MintOperation, RawTxData, TxDatum, TxInput, TxOutput};

use super::datums::{apply_resolved_datums, extract_datums_from_metadata, unresolved_datum_hashes};
use crate::TxClassifierError;

/// Fetch a transaction from Koios `/tx_info` and convert it to [`RawTxData`],
/// with its metadata and any hash-only datums filled in.
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

    let mut tx = txs
        .into_iter()
        .next()
        .ok_or_else(|| TxClassifierError::TransactionNotFound(tx_hash.to_string()))?;

    if tx.metadata.is_none() {
        // A failure here fails the fetch rather than classifying without
        // metadata: a mint or listing read without it is misclassified, not
        // merely less detailed.
        let rows = koios
            .get_tx_metadata(&[tx_hash.to_string()])
            .await
            .map_err(|e| {
                TxClassifierError::ClassificationFailed(format!("Koios tx_metadata failed: {e}"))
            })?;
        tx.metadata = rows
            .into_iter()
            .find(|row| row.tx_hash == tx_hash)
            .and_then(|row| row.metadata);
    }

    let mut raw_tx = convert_koios_tx_to_raw_data(&tx)?;
    enrich_missing_datum_content(koios, &mut raw_tx).await;
    Ok(raw_tx)
}

/// Fill in datums Koios reported by hash only: first from the transaction's
/// own metadata (JPG.store V2 publishes them there), then from Koios
/// `/datum_info` for whatever is left.
///
/// A `/datum_info` failure is logged and the datums stay hash-only, as the
/// Maestro adapter did — the transaction itself was fetched successfully.
async fn enrich_missing_datum_content(koios: &KoiosApi, raw_tx: &mut RawTxData) {
    let mut unresolved = unresolved_datum_hashes(raw_tx);
    if unresolved.is_empty() {
        return;
    }

    debug!(
        "Enriching {} unresolved datum(s) for tx {}",
        unresolved.len(),
        raw_tx.tx_hash
    );

    let metadata_datums = extract_datums_from_metadata(raw_tx);
    if !metadata_datums.is_empty() {
        debug!(
            "Extracted {} datum(s) from tx metadata",
            metadata_datums.len()
        );
        apply_resolved_datums(raw_tx, &metadata_datums);
        unresolved.retain(|hash| !metadata_datums.contains_key(hash));
    }

    if unresolved.is_empty() {
        return;
    }

    debug!(
        "Falling back to Koios datum_info for {} remaining unresolved datum(s)",
        unresolved.len()
    );
    let resolved = match koios.get_datum_info(&unresolved).await {
        Ok(rows) => rows,
        Err(e) => {
            warn!("Failed to resolve datums by hash: {e}");
            return;
        }
    };

    // A row without `bytes` is a hash the node holds no preimage for; it stays
    // unresolved rather than being given empty content.
    let api_datums: HashMap<String, String> = resolved
        .into_iter()
        .filter_map(|row| Some((row.hash, row.bytes?)))
        .collect();
    debug!(
        "Resolved {}/{} datums from Koios datum_info",
        api_datums.len(),
        unresolved.len()
    );
    apply_resolved_datums(raw_tx, &api_datums);
}

/// Convert a Koios `/tx_info` transaction into the pipeline's [`RawTxData`].
///
/// Koios does not return `/tx_info` rows in ledger order, so they are put in
/// it here: inputs (spent, reference, collateral) sorted by `(tx_hash,
/// index)` — the order spend-redeemer indices refer to — and outputs by
/// their index, which is what anything matching an output by position (a
/// metadata datum, a payout slot) relies on.
pub fn convert_koios_tx_to_raw_data(tx: &KoiosTransaction) -> Result<RawTxData, TxClassifierError> {
    let inputs = ledger_ordered_inputs(&tx.inputs)
        .map(koios_utxo_to_input)
        .collect();
    let mut outputs_by_index: Vec<&KoisUtxo> = tx.outputs.iter().collect();
    outputs_by_index.sort_by_key(|utxo| utxo.tx_index);
    let outputs = outputs_by_index
        .into_iter()
        .map(koios_utxo_to_output)
        .collect();
    let collateral_inputs = ledger_ordered_inputs(&tx.collateral_inputs)
        .map(koios_utxo_to_input)
        .collect();
    // Koios returns a single optional collateral return output.
    let collateral_outputs = tx
        .collateral_output
        .iter()
        .map(koios_utxo_to_output)
        .collect();
    let reference_inputs = ledger_ordered_inputs(&tx.reference_inputs)
        .map(koios_utxo_to_input)
        .collect();

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

    // Executed Plutus script hashes, each once. Koios lists a contract once
    // per redeemer — a sweep of ten listings at one script is ten entries —
    // where the list means "which scripts ran".
    //
    // Native scripts are NOT here, unlike Maestro's `scripts_executed`:
    // Koios's `native_scripts` only lists a script in the transaction that
    // first put it on chain, so a mint under an established native policy
    // reports none, and reading the witness set would mean fetching and
    // decoding the CBOR.
    let mut scripts: Vec<String> = Vec::new();
    for hash in tx
        .plutus_contracts
        .iter()
        .filter_map(|c| c.get("script_hash").and_then(|h| h.as_str()))
    {
        if !scripts.iter().any(|seen| seen == hash) {
            scripts.push(hash.to_string());
        }
    }

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

/// Inputs in the ledger's order: by `(tx_hash, index)`.
fn ledger_ordered_inputs(utxos: &[KoisUtxo]) -> impl Iterator<Item = &KoisUtxo> {
    let mut ordered: Vec<&KoisUtxo> = utxos.iter().collect();
    ordered.sort_by(|a, b| (&a.tx_hash, a.tx_index).cmp(&(&b.tx_hash, b.tx_index)));
    ordered.into_iter()
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
        .map(|a| {
            (
                format!("{}{}", a.policy_id, a.asset_name),
                a.quantity as u64,
            )
        })
        .collect()
}

/// Map a Koios UTxO's datum into [`TxDatum`]: prefer the inline datum's raw CBOR
/// (schema-independent — the classifier decodes it via pallas), else the bare
/// datum hash.
fn koios_utxo_datum(utxo: &KoisUtxo) -> Option<TxDatum> {
    if let Some(inline) = &utxo.inline_datum
        && let Some(bytes) = &inline.bytes
    {
        return Some(TxDatum::Bytes {
            hash: utxo.datum_hash.clone().unwrap_or_default(),
            bytes: bytes.clone(),
        });
    }
    utxo.datum_hash
        .as_ref()
        .map(|hash| TxDatum::Hash { hash: hash.clone() })
}
