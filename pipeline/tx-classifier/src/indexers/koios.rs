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
//! - `/datum_info`, only when some datum is still missing its CBOR or its
//!   JSON after the metadata has been searched — Maestro sent both.
//!
//! A failure of any of the three fails the fetch: each supplies something the
//! rules misclassify without.

use std::collections::HashMap;

use koios::koios_transaction::{KoiosTransaction, KoisUtxo};
use koios::{KoiosApi, KoiosDatumInfo};
use serde::{Deserialize, Serialize};
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
    enrich_missing_datum_content(koios, &mut raw_tx).await?;
    Ok(raw_tx)
}

/// Fill in what the transaction itself says about its datums: hash-only
/// datums from its metadata (JPG.store V2 publishes them there), and the JSON
/// of every datum its script spends witnessed.
fn fill_datums_from_transaction(tx: &KoiosTransaction, raw_tx: &mut RawTxData) {
    if !unresolved_datum_hashes(raw_tx).is_empty() {
        let metadata_datums = extract_datums_from_metadata(raw_tx);
        if !metadata_datums.is_empty() {
            debug!(
                "Extracted {} datum(s) from tx metadata for {}",
                metadata_datums.len(),
                raw_tx.tx_hash
            );
            apply_resolved_datums(raw_tx, &metadata_datums);
        }
    }
    attach_witnessed_json(raw_tx, &witnessed_datums(tx));
}

/// Fill in datum content the transaction did not carry, from Koios
/// `/datum_info`.
///
/// Maestro's complete transaction carried every datum it could resolve as
/// BOTH CBOR and detailed-schema JSON, and rules read one or the other
/// (offer counting needs [`TxDatum::Json`]; marketplace datum decoding wants
/// the bytes). So one `/datum_info` request asks for every datum still
/// missing either half — hash-only ones, metadata-resolved ones (bytes, no
/// JSON), and inline or witnessed ones known as JSON only.
///
/// Unlike the Maestro adapter, which logged a datum-lookup failure and carried
/// on, a `/datum_info` failure fails the fetch. Maestro sent a spent script
/// UTxO's datum CBOR inline and only needed the lookup for stragglers; Koios
/// never sends it, so every script spend depends on this request — and a
/// listing classified without its datum bytes comes out as a phantom sale,
/// not as a less detailed unlisting.
async fn enrich_missing_datum_content(
    koios: &KoiosApi,
    raw_tx: &mut RawTxData,
) -> Result<(), TxClassifierError> {
    let incomplete = incomplete_datum_hashes(raw_tx);
    if incomplete.is_empty() {
        return Ok(());
    }

    debug!(
        "Asking Koios datum_info for {} datum(s) missing CBOR or JSON",
        incomplete.len()
    );
    let rows = koios.get_datum_info(&incomplete).await.map_err(|e| {
        TxClassifierError::ClassificationFailed(format!("Koios datum_info failed: {e}"))
    })?;
    debug!(
        "Koios datum_info answered for {}/{} datums",
        rows.len(),
        incomplete.len()
    );
    apply_datum_info(raw_tx, rows);
    Ok(())
}

/// Merge a `/datum_info` answer into the transaction's datums.
pub(crate) fn apply_datum_info(raw_tx: &mut RawTxData, rows: Vec<KoiosDatumInfo>) {
    let found: HashMap<String, KoiosDatumInfo> = rows
        .into_iter()
        .map(|row| (row.hash.clone(), row))
        .collect();
    complete_datums(raw_tx, &found);
}

/// Every datum hash whose datum is missing its CBOR, its JSON, or both.
pub(crate) fn incomplete_datum_hashes(raw_tx: &RawTxData) -> Vec<String> {
    let mut hashes: Vec<String> = Vec::new();
    let datums = raw_tx
        .inputs
        .iter()
        .chain(&raw_tx.reference_inputs)
        .filter_map(|input| input.datum.as_ref())
        .chain(raw_tx.outputs.iter().filter_map(|o| o.datum.as_ref()));
    for datum in datums {
        let complete = matches!(
            datum,
            TxDatum::Json { json, bytes: Some(_), .. } if !json.is_null()
        );
        let hash = datum.hash();
        if !complete && !hash.is_empty() && !hashes.iter().any(|h| h == hash) {
            hashes.push(hash.to_string());
        }
    }
    hashes
}

/// Merge what `/datum_info` returned into each datum, keeping whatever the
/// datum already had. A row with neither half changes nothing — a hash the
/// node holds no preimage for stays hash-only rather than gaining empty
/// content.
fn complete_datums(raw_tx: &mut RawTxData, found: &HashMap<String, KoiosDatumInfo>) {
    let merge = |datum: &mut Option<TxDatum>| {
        let Some(current) = datum.as_ref() else {
            return;
        };
        let Some(info) = found.get(current.hash()) else {
            return;
        };
        let (known_json, known_bytes) = match current {
            TxDatum::Hash { .. } => (None, None),
            TxDatum::Bytes { bytes, .. } => (None, Some(bytes.clone())),
            TxDatum::Json { json, bytes, .. } => {
                (Some(json.clone()).filter(|j| !j.is_null()), bytes.clone())
            }
        };
        let json = known_json.or_else(|| info.value.clone().filter(|j| !j.is_null()));
        let bytes = known_bytes.or_else(|| info.bytes.clone());
        let hash = current.hash().to_string();
        *datum = Some(match (json, bytes) {
            (Some(json), bytes) => TxDatum::Json { hash, json, bytes },
            (None, Some(bytes)) => TxDatum::Bytes { hash, bytes },
            (None, None) => return,
        });
    };

    for input in raw_tx
        .inputs
        .iter_mut()
        .chain(raw_tx.reference_inputs.iter_mut())
    {
        merge(&mut input.datum);
    }
    for output in &mut raw_tx.outputs {
        merge(&mut output.datum);
    }
}

/// Convert a Koios `/tx_info` transaction into the pipeline's [`RawTxData`].
///
/// Koios does not return `/tx_info` rows in ledger order, so they are put in
/// it here: inputs (spent, reference, collateral) sorted by `(tx_hash,
/// index)` — the order spend-redeemer indices refer to — and outputs by
/// their index, which is what anything matching an output by position (a
/// metadata datum, a payout slot) relies on.
///
/// Uses only what `tx` carries — including its metadata, if the caller has
/// filled it in — and makes no request; [`get_tx_from_koios`] adds the
/// `/datum_info` pass on top.
pub fn convert_koios_tx_to_raw_data(tx: &KoiosTransaction) -> Result<RawTxData, TxClassifierError> {
    let inputs: Vec<TxInput> = ledger_ordered_inputs(&tx.inputs)
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

    let redeemers = serde_json::to_value(redeemer_set(tx, &inputs)).ok();

    let mut raw_tx = RawTxData {
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
    };
    fill_datums_from_transaction(tx, &mut raw_tx);
    Ok(raw_tx)
}

/// The redeemers in a transaction, grouped by purpose — the shape
/// [`RawTxData::redeemers`] has always carried, because the rules read it by
/// that shape (`spends[].data.json.constructor` tells a JPG.store buy from a
/// withdraw or update).
///
/// Koios has no such grouping: it reports one `plutus_contracts` entry per
/// redeemer, with the purpose and the redeemer's Plutus JSON nested inside.
/// Only what Koios states is carried across; the redeemer's raw CBOR, which
/// Maestro also sent, is not in `/tx_info`.
#[derive(Debug, Default, Serialize)]
struct RedeemerSet {
    spends: Vec<Redeemer>,
    mints: Vec<Redeemer>,
    withdrawals: Vec<Redeemer>,
    certificates: Vec<Redeemer>,
    votes: Vec<Redeemer>,
    proposals: Vec<Redeemer>,
}

#[derive(Debug, Serialize)]
struct Redeemer {
    script_hash: Option<String>,
    /// The UTxO a spend redeemer unlocks. Absent for every other purpose.
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<RedeemerInput>,
    /// That UTxO's position in the ledger-sorted inputs — the spend
    /// redeemer's index.
    #[serde(skip_serializing_if = "Option::is_none")]
    input_index: Option<usize>,
    data: RedeemerData,
    /// `[mem, steps]`.
    ex_units: Option<[u64; 2]>,
}

#[derive(Debug, Serialize)]
struct RedeemerInput {
    tx_hash: String,
    index: u32,
}

#[derive(Debug, Serialize)]
struct RedeemerData {
    json: serde_json::Value,
}

/// The parts of a Koios `plutus_contracts` entry a [`Redeemer`] is built from.
#[derive(Debug, Deserialize)]
struct KoiosPlutusContract {
    script_hash: Option<String>,
    spends_input: Option<KoiosSpendsInput>,
    input: Option<KoiosContractInput>,
}

#[derive(Debug, Deserialize)]
struct KoiosSpendsInput {
    tx_hash: String,
    tx_index: u32,
}

#[derive(Debug, Deserialize)]
struct KoiosContractInput {
    redeemer: Option<KoiosRedeemer>,
    /// The datum of the UTxO a spend unlocks, as witnessed by this
    /// transaction.
    datum: Option<KoiosWitnessedDatum>,
}

#[derive(Debug, Deserialize)]
struct KoiosWitnessedDatum {
    hash: Option<String>,
    value: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct KoiosRedeemer {
    purpose: String,
    unit: Option<KoiosExUnits>,
    datum: Option<KoiosRedeemerDatum>,
}

#[derive(Debug, Deserialize)]
struct KoiosExUnits {
    /// Koios sends these as decimal strings.
    mem: serde_json::Value,
    steps: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct KoiosRedeemerDatum {
    value: Option<serde_json::Value>,
}

/// A count Koios may send as a JSON number or a decimal string.
fn as_count(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

/// The `plutus_contracts` entries that parse; one that does not is logged and
/// skipped rather than failing the whole transaction.
fn plutus_contracts(tx: &KoiosTransaction) -> impl Iterator<Item = KoiosPlutusContract> + '_ {
    tx.plutus_contracts.iter().filter_map(|raw| {
        serde_json::from_value::<KoiosPlutusContract>(raw.clone())
            .inspect_err(|e| {
                warn!("Skipping a Koios plutus_contracts entry that does not parse: {e}")
            })
            .ok()
    })
}

/// Datum JSON for the UTxOs this transaction spends from a script, by datum
/// hash — the datums in its witness set.
///
/// Koios reports a spent UTxO's datum by hash alone on the input itself, and
/// carries the witnessed content (as detailed-schema JSON) on the contract
/// entry instead. Maestro reported it inline on the input, and the rules
/// that read [`TxDatum::Json`] were written against that.
fn witnessed_datums(tx: &KoiosTransaction) -> HashMap<String, serde_json::Value> {
    plutus_contracts(tx)
        .filter_map(|contract| {
            let datum = contract.input?.datum?;
            match (datum.hash, datum.value) {
                (Some(hash), Some(value)) if !value.is_null() => Some((hash, value)),
                _ => None,
            }
        })
        .collect()
}

/// Give every datum whose content this transaction witnessed its JSON
/// alongside whatever bytes it already has.
fn attach_witnessed_json(raw_tx: &mut RawTxData, witnessed: &HashMap<String, serde_json::Value>) {
    let attach = |datum: &mut Option<TxDatum>| {
        let Some(current) = datum.as_ref() else {
            return;
        };
        let Some(json) = witnessed.get(current.hash()) else {
            return;
        };
        let replacement = match current {
            TxDatum::Hash { hash } => TxDatum::Json {
                hash: hash.clone(),
                json: json.clone(),
                bytes: None,
            },
            TxDatum::Bytes { hash, bytes } => TxDatum::Json {
                hash: hash.clone(),
                json: json.clone(),
                bytes: Some(bytes.clone()),
            },
            TxDatum::Json { .. } => return,
        };
        *datum = Some(replacement);
    };

    for input in raw_tx
        .inputs
        .iter_mut()
        .chain(raw_tx.reference_inputs.iter_mut())
    {
        attach(&mut input.datum);
    }
    for output in &mut raw_tx.outputs {
        attach(&mut output.datum);
    }
}

fn redeemer_set(tx: &KoiosTransaction, ledger_inputs: &[TxInput]) -> RedeemerSet {
    let mut set = RedeemerSet::default();

    for contract in plutus_contracts(tx) {
        let Some(redeemer) = contract.input.and_then(|input| input.redeemer) else {
            continue;
        };

        let input_index = contract.spends_input.as_ref().and_then(|spent| {
            ledger_inputs
                .iter()
                .position(|i| i.tx_hash == spent.tx_hash && i.output_index == spent.tx_index)
        });
        let entry = Redeemer {
            script_hash: contract.script_hash,
            input: contract.spends_input.map(|spent| RedeemerInput {
                tx_hash: spent.tx_hash,
                index: spent.tx_index,
            }),
            input_index,
            data: RedeemerData {
                json: redeemer
                    .datum
                    .and_then(|d| d.value)
                    .unwrap_or(serde_json::Value::Null),
            },
            ex_units: redeemer
                .unit
                .and_then(|u| Some([as_count(&u.mem)?, as_count(&u.steps)?])),
        };

        // Koios spells purposes as db-sync does; Maestro grouped them under
        // plural keys.
        let group = match redeemer.purpose.as_str() {
            "spend" => &mut set.spends,
            "mint" => &mut set.mints,
            "reward" => &mut set.withdrawals,
            "cert" => &mut set.certificates,
            "vote" => &mut set.votes,
            "propose" => &mut set.proposals,
            other => {
                warn!("Skipping a redeemer with unknown purpose `{other}`");
                continue;
            }
        };
        group.push(entry);
    }

    set
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

/// Map a Koios UTxO's datum into [`TxDatum`]: an inline datum carries both its
/// raw CBOR and Koios's Plutus detailed-schema JSON, else the bare datum hash.
///
/// The JSON is kept, as Maestro's was: several rules (offer counting, offer
/// amounts, policy-id extraction) read only [`TxDatum::Json`], and Koios
/// renders `value` in the same detailed schema Maestro's `json` used.
///
/// `/tx_info` leaves an inline datum's `bytes` null unless asked for
/// bytecode, so an inline datum usually arrives as JSON alone; its CBOR is
/// filled in afterwards from `/datum_info` (see [`enrich_missing_datum_content`]).
fn koios_utxo_datum(utxo: &KoisUtxo) -> Option<TxDatum> {
    if let Some(inline) = &utxo.inline_datum {
        let hash = utxo.datum_hash.clone().unwrap_or_default();
        match (&inline.value, &inline.bytes) {
            (Some(json), bytes) if !json.is_null() => {
                return Some(TxDatum::Json {
                    hash,
                    json: json.clone(),
                    bytes: bytes.clone(),
                });
            }
            (_, Some(bytes)) => {
                return Some(TxDatum::Bytes {
                    hash,
                    bytes: bytes.clone(),
                });
            }
            _ => {}
        }
    }
    utxo.datum_hash
        .as_ref()
        .map(|hash| TxDatum::Hash { hash: hash.clone() })
}
