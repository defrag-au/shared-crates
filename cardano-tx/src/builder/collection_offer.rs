//! Collection Offer (CO) builder for jpg.store contracts.
//!
//! Builds the PlutusData datum and TX output for creating a collection offer
//! on the jpg.store CO script. The offer locks ADA at the script address with
//! a datum specifying:
//! - marketplace fee payout (2.5%, min 2 ADA)
//! - creator royalty payout (from collection data, min 1 ADA)
//! - NFT delivery to the buyer (any NFT from the target policy)

use cardano_assets::UtxoApi;
use pallas_addresses::{Address, Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart};
use pallas_crypto::hash::Hash;
use pallas_primitives::conway::PlutusData;

use super::script::{bytes, encode_plutus_data, int};
use crate::error::TxBuildError;

/// Indefinite-length constructor — matches jpg.store's CBOR encoding.
/// Uses `9f...ff` encoding instead of `82`/`83` definite arrays, producing
/// identical datum hashes to COs created by jpg.store's frontend.
fn constr_indef(index: u32, fields: Vec<PlutusData>) -> PlutusData {
    let (tag, any_constructor) = if index <= 6 {
        (121 + index as u64, None)
    } else {
        (102, Some(index as u64))
    };
    PlutusData::Constr(pallas_primitives::Constr {
        tag,
        any_constructor,
        fields: pallas_primitives::MaybeIndefArray::Indef(fields),
    })
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// jpg.store marketplace fee: 2.5%
const MARKETPLACE_FEE_BPS: u64 = 250;

/// Minimum marketplace fee (2 ADA)
const MIN_MARKETPLACE_FEE: u64 = 2_000_000;

/// Minimum creator royalty (1 ADA — min UTxO requirement)
const MIN_ROYALTY: u64 = 1_000_000;

/// jpg.store marketplace fee script payment credential.
const MARKETPLACE_FEE_SCRIPT_HASH: &str = "84cc25ea4c29951d40b443b95bbc5676bc425470f96376d1984af9ab";

/// jpg.store marketplace fee staking credential (script).
const MARKETPLACE_FEE_STAKE_HASH: &str = "2c967f4bd28944b06462e13c5e3f5d5fa6e03f8567569438cd833e6d";

/// jpg.store CO script payment credential (V2 — the version their engine indexes).
const CO_SCRIPT_PAYMENT_HASH: &str = "9068a7a3f008803edac87af1619860f2cdcde40c26987325ace138ad";

/// jpg.store CO script staking credential (shared across V2/V3).
const CO_SCRIPT_STAKE_HASH: &str = "2c967f4bd28944b06462e13c5e3f5d5fa6e03f8567569438cd833e6d";

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// Parameters for creating a collection offer.
#[derive(Debug, Clone)]
pub struct CollectionOfferRequest {
    /// Target collection policy ID (56 hex chars).
    pub policy_id: String,
    /// Total ADA to lock at the script (the offer amount in lovelace).
    /// This is the full amount including fees and royalties.
    pub total_lovelace: u64,
    /// Buyer's payment key hash (28 bytes hex).
    pub buyer_pkh: String,
    /// Buyer's staking key hash (28 bytes hex, optional).
    pub buyer_stake_hash: Option<String>,
    /// Creator royalty percentage (e.g., 0.03 = 3%).
    pub royalty_pct: f64,
    /// Creator royalty address — payment key hash (28 bytes hex).
    pub royalty_pkh: String,
    /// Creator royalty address — staking key hash (28 bytes hex, optional).
    pub royalty_stake_hash: Option<String>,
    /// Whether the royalty address uses a key (true) or script (false) payment credential.
    pub royalty_is_key: bool,
    /// Whether the royalty staking uses a key (true) or script (false) credential.
    pub royalty_stake_is_key: bool,
    /// Network (mainnet or testnet).
    pub network: Network,
}

/// Result of building a collection offer.
#[derive(Debug, Clone)]
pub struct CollectionOfferResult {
    /// The CO script address to send the ADA to.
    pub script_address: Address,
    /// Total lovelace to lock at the script.
    pub total_lovelace: u64,
    /// CBOR-encoded datum to attach to the output.
    pub datum_bytes: Vec<u8>,
    /// Breakdown of how the locked ADA will be distributed.
    pub fee_breakdown: FeeBreakdown,
}

/// Fee breakdown for display purposes.
#[derive(Debug, Clone)]
pub struct FeeBreakdown {
    pub marketplace_fee: u64,
    pub creator_royalty: u64,
    pub seller_receives: u64,
}

// ---------------------------------------------------------------------------
// Fee calculation
// ---------------------------------------------------------------------------

/// Calculate marketplace fee (2.5% of total, min 2 ADA).
pub fn calculate_marketplace_fee(total_lovelace: u64) -> u64 {
    let proportional = total_lovelace * MARKETPLACE_FEE_BPS / 10_000;
    proportional.max(MIN_MARKETPLACE_FEE)
}

/// Calculate creator royalty (royalty_pct of total, min 1 ADA).
pub fn calculate_royalty(total_lovelace: u64, royalty_pct: f64) -> u64 {
    if royalty_pct <= 0.0 {
        return MIN_ROYALTY;
    }
    let proportional = (total_lovelace as f64 * royalty_pct) as u64;
    proportional.max(MIN_ROYALTY)
}

/// Calculate what the seller actually receives after fees.
pub fn calculate_seller_receives(total_lovelace: u64, royalty_pct: f64) -> FeeBreakdown {
    let marketplace_fee = calculate_marketplace_fee(total_lovelace);
    let creator_royalty = calculate_royalty(total_lovelace, royalty_pct);
    let seller_receives = total_lovelace
        .saturating_sub(marketplace_fee)
        .saturating_sub(creator_royalty);

    FeeBreakdown {
        marketplace_fee,
        creator_royalty,
        seller_receives,
    }
}

// ---------------------------------------------------------------------------
// Datum construction
// ---------------------------------------------------------------------------

/// Build the full CO datum as PlutusData.
///
/// Structure:
/// ```text
/// Constructor(0) [
///     owner_pkh: ByteString(28),
///     payouts: [
///         Payout(marketplace_fee_address, {ADA: marketplace_fee}),
///         Payout(royalty_address, {ADA: royalty}),
///         Payout(buyer_address, {policy_id: (1, {})}),
///     ]
/// ]
/// ```
fn build_co_datum(req: &CollectionOfferRequest) -> Result<PlutusData, TxBuildError> {
    let marketplace_fee = calculate_marketplace_fee(req.total_lovelace);
    let royalty = calculate_royalty(req.total_lovelace, req.royalty_pct);

    // Payout 1: marketplace fee → script address
    let marketplace_payout = build_ada_payout(
        &hex_to_28_bytes(MARKETPLACE_FEE_SCRIPT_HASH)?,
        false, // script credential
        Some(&hex_to_28_bytes(MARKETPLACE_FEE_STAKE_HASH)?),
        false, // script staking
        marketplace_fee,
    );

    // Payout 2: creator royalty → royalty address
    let royalty_payout = build_ada_payout(
        &hex_to_28_bytes(&req.royalty_pkh)?,
        req.royalty_is_key,
        req.royalty_stake_hash
            .as_ref()
            .map(|h| hex_to_28_bytes(h))
            .transpose()?
            .as_ref(),
        req.royalty_stake_is_key,
        royalty,
    );

    // Payout 3: NFT delivery → buyer address
    let nft_payout = build_nft_payout(
        &hex_to_28_bytes(&req.buyer_pkh)?,
        req.buyer_stake_hash
            .as_ref()
            .map(|h| hex_to_28_bytes(h))
            .transpose()?
            .as_ref(),
        &req.policy_id,
    )?;

    // Full datum: Constructor(0) [owner_pkh, [payout1, payout2, payout3]]
    let datum = constr_indef(
        0,
        vec![
            bytes(hex_to_28_bytes(&req.buyer_pkh)?.to_vec()),
            PlutusData::Array(pallas_primitives::MaybeIndefArray::Indef(vec![
                marketplace_payout,
                royalty_payout,
                nft_payout,
            ])),
        ],
    );

    Ok(datum)
}

/// Build a payout entry for an ADA-only payment.
///
/// ```text
/// Constructor(0) [
///     address,
///     Map { "": Constructor(0) [0, Map { "": lovelace }] }
/// ]
/// ```
fn build_ada_payout(
    payment_hash: &[u8; 28],
    is_key: bool,
    stake_hash: Option<&[u8; 28]>,
    stake_is_key: bool,
    lovelace: u64,
) -> PlutusData {
    let address = build_datum_address(payment_hash, is_key, stake_hash, stake_is_key);
    let value = build_ada_value(lovelace);
    constr_indef(0, vec![address, value])
}

/// Build a payout entry for NFT delivery (1 NFT from a policy, any asset name).
///
/// ```text
/// Constructor(0) [
///     address,
///     Map { policy_id: Constructor(0) [1, Map {}] }
/// ]
/// ```
fn build_nft_payout(
    payment_hash: &[u8; 28],
    stake_hash: Option<&[u8; 28]>,
    policy_id_hex: &str,
) -> Result<PlutusData, TxBuildError> {
    let address = build_datum_address(payment_hash, true, stake_hash, true);
    let policy_bytes = hex::decode(policy_id_hex)
        .map_err(|e| TxBuildError::InvalidHex(format!("policy_id: {e}")))?;

    // Value: Map { policy_id_bytes: Constructor(0) [1, Map {}] }
    let nft_value = PlutusData::Map(pallas_primitives::KeyValuePairs::Def(vec![(
        bytes(policy_bytes),
        constr_indef(0, vec![int(1), PlutusData::Map(pallas_primitives::KeyValuePairs::Def(vec![]))]),
    )]));

    Ok(constr_indef(0, vec![address, nft_value]))
}

/// Build a datum address (Shelley address encoded as PlutusData).
///
/// ```text
/// Constructor(0) [
///     payment: Constructor(0=key|1=script) [ByteString(28)],
///     staking: Constructor(0=Some|1=None) [
///         Constructor(0) [Constructor(0=key|1=script) [ByteString(28)]]
///     ]
/// ]
/// ```
fn build_datum_address(
    payment_hash: &[u8; 28],
    is_key: bool,
    stake_hash: Option<&[u8; 28]>,
    stake_is_key: bool,
) -> PlutusData {
    let pay_tag = if is_key { 0 } else { 1 };
    let payment_cred = constr_indef(pay_tag, vec![bytes(payment_hash.to_vec())]);

    let staking_cred = match stake_hash {
        Some(hash) => {
            let stake_tag = if stake_is_key { 0 } else { 1 };
            // Some(StakingHash(credential))
            constr_indef(
                0,
                vec![constr_indef(0, vec![constr_indef(stake_tag, vec![bytes(hash.to_vec())])])],
            )
        }
        None => constr_indef(1, vec![]), // None
    };

    constr_indef(0, vec![payment_cred, staking_cred])
}

/// Build an ADA-only value in the datum format.
///
/// `Map { "": Constructor(0) [0, Map { "": lovelace }] }`
fn build_ada_value(lovelace: u64) -> PlutusData {
    PlutusData::Map(pallas_primitives::KeyValuePairs::Def(vec![(
        bytes(vec![]), // empty bytes = ADA policy
        constr_indef(
            0,
            vec![
                int(0),
                PlutusData::Map(pallas_primitives::KeyValuePairs::Def(vec![(
                    bytes(vec![]), // empty bytes = lovelace token name
                    int(lovelace as i64),
                )])),
            ],
        ),
    )]))
}

// ---------------------------------------------------------------------------
// Address construction
// ---------------------------------------------------------------------------

/// Build the CO script address for a given network.
pub fn co_script_address(network: Network) -> Result<Address, TxBuildError> {
    let payment_hash = hex_to_28_bytes(CO_SCRIPT_PAYMENT_HASH)?;
    let stake_hash = hex_to_28_bytes(CO_SCRIPT_STAKE_HASH)?;

    let address = ShelleyAddress::new(
        network,
        ShelleyPaymentPart::Script(Hash::from(payment_hash)),
        ShelleyDelegationPart::Script(Hash::from(stake_hash)),
    );

    Ok(Address::Shelley(address))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build a collection offer: datum + script address + fee breakdown.
pub fn build_collection_offer(
    req: &CollectionOfferRequest,
) -> Result<CollectionOfferResult, TxBuildError> {
    let datum = build_co_datum(req)?;
    let datum_bytes = encode_plutus_data(&datum)?;
    let script_address = co_script_address(req.network)?;
    let fee_breakdown = calculate_seller_receives(req.total_lovelace, req.royalty_pct);

    Ok(CollectionOfferResult {
        script_address,
        total_lovelace: req.total_lovelace,
        datum_bytes,
        fee_breakdown,
    })
}

// ---------------------------------------------------------------------------
// TX builder (follows send.rs pattern)
// ---------------------------------------------------------------------------

/// Build a complete unsigned transaction that places a collection offer.
///
/// Uses `TxDeps` for UTxO selection and fee convergence, following the same
/// pattern as `send::build_send_lovelace`. The output is sent to the V2 CO
/// script address with a datum hash. The datum content is published via TX
/// metadata (labels 30 + 50-62) matching jpg.store's format so their matching
/// engine can discover the offer.
pub fn build_collection_offer_tx(
    deps: &super::TxDeps,
    req: &CollectionOfferRequest,
) -> Result<super::UnsignedTx, TxBuildError> {
    use crate::helpers::input::add_utxo_input;
    use crate::helpers::output::{build_change_output, create_ada_output};
    use crate::helpers::utxo_query::is_simple_utxo;
    use crate::selection;
    use pallas_crypto::hash::Hasher;
    use pallas_txbuilder::StagingTransaction;

    let co = build_collection_offer(req)?;
    let estimated_fee = selection::estimate_simple_fee(&deps.params);

    // Select UTxOs (multi-input if needed)
    let selected_utxos = selection::select_utxos_for_amount(
        &deps.utxos,
        co.total_lovelace,
        estimated_fee,
        &deps.params,
    )?;

    let input_amount: u64 = selected_utxos.iter().map(|u| u.lovelace).sum();
    let has_native_assets = selected_utxos.iter().any(|u| !u.assets.is_empty());
    let from_address = deps.from_address.clone();
    let network_id = deps.network_id;

    // Compute datum hash (Blake2b-256 of the CBOR-encoded datum)
    let datum_hash = Hasher::<256>::hash(&co.datum_bytes);

    // Build metadata CBOR: datum content published as TX metadata
    // matching jpg.store's format (labels 30, 50-62)
    let metadata_cbor = build_co_metadata(&co.datum_bytes, &req.policy_id)?;

    super::converge_fee(
        |fee| {
            let change = input_amount
                .checked_sub(co.total_lovelace)
                .and_then(|v| v.checked_sub(fee))
                .ok_or(TxBuildError::InsufficientFunds {
                    needed: co.total_lovelace + fee,
                    available: input_amount,
                })?;

            let mut tx = StagingTransaction::new();

            // Add all selected inputs
            for utxo in &selected_utxos {
                tx = add_utxo_input(tx, utxo)?;
            }

            // CO output: ADA to V2 script address with datum hash
            let script_output = create_ada_output(co.script_address.clone(), co.total_lovelace)
                .set_datum_hash(datum_hash);
            tx = tx.output(script_output);

            // Attach datum as metadata (not witness set — avoids scriptDataHash)
            tx = tx.add_auxiliary_data(metadata_cbor.clone());

            // Change output (preserves native assets if present)
            if change > 0 {
                if has_native_assets {
                    let input_refs: Vec<&UtxoApi> = selected_utxos.iter().collect();
                    let change_output = build_change_output(
                        from_address.clone(),
                        change,
                        &input_refs,
                        None,
                    )?;
                    tx = tx.output(change_output);
                } else {
                    tx = tx.output(create_ada_output(from_address.clone(), change));
                }
            }

            Ok(tx.fee(fee).network_id(network_id))
        },
        estimated_fee,
        &deps.params,
    )
}

/// Build a TX that places multiple collection offers in a single transaction.
///
/// Each offer gets its own output at the script address with its own datum hash.
/// All datum content is published via TX metadata with sequential labels,
/// matching jpg.store's multi-CO format (labels 50+, each CO's chunks followed
/// by a `policy_id::index` separator).
pub fn build_collection_offers_tx(
    deps: &super::TxDeps,
    requests: &[CollectionOfferRequest],
) -> Result<super::UnsignedTx, TxBuildError> {
    use crate::helpers::input::add_utxo_input;
    use crate::helpers::output::{build_change_output, create_ada_output};
    use crate::helpers::utxo_query::is_simple_utxo;
    use crate::selection;
    use pallas_crypto::hash::Hasher;
    use pallas_txbuilder::StagingTransaction;

    if requests.is_empty() {
        return Err(TxBuildError::BuildFailed("No offer requests".into()));
    }

    // Build all offers and compute total lovelace needed
    let offers: Vec<CollectionOfferResult> = requests
        .iter()
        .map(build_collection_offer)
        .collect::<Result<Vec<_>, _>>()?;

    let total_lovelace: u64 = offers.iter().map(|o| o.total_lovelace).sum();
    let estimated_fee = selection::estimate_simple_fee(&deps.params);

    // Select UTxOs (multi-input if needed)
    let selected_utxos = selection::select_utxos_for_amount(
        &deps.utxos,
        total_lovelace,
        estimated_fee,
        &deps.params,
    )?;

    let input_amount: u64 = selected_utxos.iter().map(|u| u.lovelace).sum();
    let has_native_assets = selected_utxos.iter().any(|u| !u.assets.is_empty());
    let from_address = deps.from_address.clone();
    let network_id = deps.network_id;

    // Precompute datum hashes
    let datum_hashes: Vec<_> = offers
        .iter()
        .map(|o| Hasher::<256>::hash(&o.datum_bytes))
        .collect();

    // Build metadata for all offers
    let datum_entries: Vec<(&[u8], &str)> = offers
        .iter()
        .zip(requests.iter())
        .map(|(o, r)| (o.datum_bytes.as_slice(), r.policy_id.as_str()))
        .collect();
    let metadata_cbor = build_co_metadata_multi(&datum_entries)?;

    super::converge_fee(
        |fee| {
            let change = input_amount
                .checked_sub(total_lovelace)
                .and_then(|v| v.checked_sub(fee))
                .ok_or(TxBuildError::InsufficientFunds {
                    needed: total_lovelace + fee,
                    available: input_amount,
                })?;

            let mut tx = StagingTransaction::new();

            // Add all selected inputs
            for utxo in &selected_utxos {
                tx = add_utxo_input(tx, utxo)?;
            }

            // One output per CO
            for (i, offer) in offers.iter().enumerate() {
                let script_output =
                    create_ada_output(offer.script_address.clone(), offer.total_lovelace)
                        .set_datum_hash(datum_hashes[i]);
                tx = tx.output(script_output);
            }

            // Metadata with all datums
            tx = tx.add_auxiliary_data(metadata_cbor.clone());

            // Change output
            if change > 0 {
                if has_native_assets {
                    let input_refs: Vec<&UtxoApi> = selected_utxos.iter().collect();
                    let change_output = build_change_output(
                        from_address.clone(),
                        change,
                        &input_refs,
                        None,
                    )?;
                    tx = tx.output(change_output);
                } else {
                    tx = tx.output(create_ada_output(from_address.clone(), change));
                }
            }

            Ok(tx.fee(fee).network_id(network_id))
        },
        estimated_fee,
        &deps.params,
    )
}

/// Build the TX metadata CBOR for a single collection offer.
fn build_co_metadata(datum_cbor: &[u8], policy_id: &str) -> Result<Vec<u8>, TxBuildError> {
    build_co_metadata_multi(&[(datum_cbor, policy_id)])
}

/// Build TX metadata for multiple collection offers.
///
/// Each offer's datum CBOR is hex-encoded, chunked into 64-char strings across
/// sequential labels starting at 50. Each offer ends with a `policy_id::index`
/// label. Label 30 contains version "5".
fn build_co_metadata_multi(
    offers: &[(&[u8], &str)],
) -> Result<Vec<u8>, TxBuildError> {
    use pallas_codec::minicbor::Encoder;

    // Collect all metadata entries: (label, value_string)
    let mut entries: Vec<(u64, String)> = vec![(30, "5".into())];
    let mut label = 50u64;

    for (i, (datum_cbor, policy_id)) in offers.iter().enumerate() {
        let datum_hex = hex::encode(datum_cbor);
        let chunks: Vec<&[u8]> = datum_hex.as_bytes().chunks(64).collect();
        for (ci, chunk) in chunks.iter().enumerate() {
            let mut s = std::str::from_utf8(chunk).unwrap().to_string();
            // jpg.store format: last datum chunk ends with a comma separator
            if ci == chunks.len() - 1 {
                s.push(',');
            }
            entries.push((label, s));
            label += 1;
        }
        entries.push((label, format!("{policy_id}::{i:02}")));
        label += 1;
    }

    let mut buf = Vec::with_capacity(entries.len() * 72);
    let mut enc = Encoder::new(&mut buf);

    enc.map(entries.len() as u64)
        .map_err(|e| TxBuildError::BuildFailed(format!("metadata encode: {e}")))?;

    for (lbl, val) in &entries {
        enc.u64(*lbl)
            .map_err(|e| TxBuildError::BuildFailed(format!("metadata encode: {e}")))?;
        enc.str(val)
            .map_err(|e| TxBuildError::BuildFailed(format!("metadata encode: {e}")))?;
    }

    Ok(buf)
}

// ---------------------------------------------------------------------------
// Cancel CO TX builder
// ---------------------------------------------------------------------------

/// Script reference UTxO for the jpg.store CO contract (PlutusV2).
const SCRIPT_REF_TX: &str = "9a32459bd4ef6bbafdeb8cf3b909d0e3e2ec806e4cc6268529280b0fc1d06f5b";
const SCRIPT_REF_INDEX: u64 = 0;

/// Cancel redeemer: Constructor(1, []) = d87a80
/// (Verified from on-chain cancel TX a79712998e7e1bcf...)
const CANCEL_REDEEMER_HEX: &str = "d87a80";

/// Ex-units budget for cancel — generous defaults for evaluation pass.
/// Real values come from Maestro evaluate in the second pass.
const CANCEL_EX_UNITS_MEM: u64 = 2_000_000;
const CANCEL_EX_UNITS_STEPS: u64 = 600_000_000;

/// Parameters for cancelling a collection offer.
#[derive(Debug, Clone)]
pub struct CancelOfferRequest {
    /// TX hash of the CO UTxO to cancel.
    pub co_tx_hash: String,
    /// Output index of the CO UTxO.
    pub co_output_index: u64,
    /// Lovelace locked in the CO.
    pub co_lovelace: u64,
    /// Owner's payment key hash (28 bytes hex).
    pub owner_pkh: String,
    /// Datum CBOR bytes (hex). Required for hash-datum COs to include in witness set.
    pub datum_cbor_hex: Option<String>,
    /// Script execution memory units. If None, uses default estimate.
    pub ex_units_mem: Option<u64>,
    /// Script execution CPU steps. If None, uses default estimate.
    pub ex_units_steps: Option<u64>,
}

/// Build an unsigned cancel TX for a collection offer.
///
/// This is a Plutus spending TX that:
/// - Spends the CO UTxO at the script address (with cancel redeemer)
/// - Uses a script reference input (no inline script needed)
/// - Requires the owner's signature (disclosed_signer)
/// - Returns all ADA to the owner
/// - Fee paid from a separate wallet UTxO (also used as collateral)
pub fn build_cancel_offer_tx(
    deps: &super::TxDeps,
    req: &CancelOfferRequest,
) -> Result<super::UnsignedTx, TxBuildError> {
    use crate::builder::cost_models::PLUTUS_V2_COST_MODEL;
    use crate::helpers::input::add_utxo_input;
    use crate::helpers::output::create_ada_output;
    use crate::helpers::utxo_query::is_simple_utxo;
    use crate::selection;
    use pallas_crypto::hash::Hash;
    use pallas_txbuilder::{ExUnits, Input, ScriptKind, StagingTransaction};

    let estimated_fee = selection::estimate_simple_fee(&deps.params);

    // Select a wallet UTxO to pay the fee (also serves as collateral).
    // No min-change overhead needed — the CO value covers the output min-UTxO,
    // and any remaining fee UTxO value merges into the single output.
    let spendable: Vec<_> = deps.utxos.iter().filter(|u| is_simple_utxo(u)).cloned().collect();
    let fee_utxo = selection::select_utxo_for_amount(
        &spendable,
        0,
        estimated_fee,
    )?
    .clone();

    let fee_utxo_lovelace = fee_utxo.lovelace;
    let from_address = deps.from_address.clone();
    let network_id = deps.network_id;

    // Parse hashes
    let co_tx_bytes: [u8; 32] = hex::decode(&req.co_tx_hash)
        .map_err(|e| TxBuildError::InvalidHex(format!("{e}")))?
        .try_into()
        .map_err(|_| TxBuildError::InvalidHex("co tx hash must be 32 bytes".into()))?;

    let script_ref_bytes: [u8; 32] = hex::decode(SCRIPT_REF_TX)
        .map_err(|e| TxBuildError::InvalidHex(format!("{e}")))?
        .try_into()
        .map_err(|_| TxBuildError::InvalidHex("script ref tx hash".into()))?;

    let owner_pkh_bytes: [u8; 28] = hex_to_28_bytes(&req.owner_pkh)?;

    let redeemer_bytes = hex::decode(CANCEL_REDEEMER_HEX)
        .map_err(|e| TxBuildError::InvalidHex(format!("{e}")))?;

    let script_input = Input::new(Hash::from(co_tx_bytes), req.co_output_index);
    let ref_input = Input::new(Hash::from(script_ref_bytes), SCRIPT_REF_INDEX);

    let fee_tx_bytes: [u8; 32] = hex::decode(&fee_utxo.tx_hash)
        .map_err(|e| TxBuildError::InvalidHex(format!("{e}")))?
        .try_into()
        .map_err(|_| TxBuildError::InvalidHex("fee tx hash".into()))?;
    let fee_input = Input::new(Hash::from(fee_tx_bytes), fee_utxo.output_index as u64);

    let co_lovelace = req.co_lovelace;

    // Total input: CO lovelace + fee UTxO lovelace
    let total_input = co_lovelace + fee_utxo_lovelace;

    super::converge_fee(
        |fee| {
            let output_value = total_input
                .checked_sub(fee)
                .ok_or(TxBuildError::InsufficientFunds {
                    needed: fee,
                    available: total_input,
                })?;

            let mut tx = StagingTransaction::new()
                .input(script_input.clone())
                .input(fee_input.clone())
                .reference_input(ref_input.clone())
                .add_spend_redeemer(
                    script_input.clone(),
                    redeemer_bytes.clone(),
                    Some(ExUnits {
                        mem: req.ex_units_mem.unwrap_or(CANCEL_EX_UNITS_MEM),
                        steps: req.ex_units_steps.unwrap_or(CANCEL_EX_UNITS_STEPS),
                    }),
                )
                .language_view(ScriptKind::PlutusV2, PLUTUS_V2_COST_MODEL.to_vec())
                .disclosed_signer(Hash::from(owner_pkh_bytes))
                .collateral_input(fee_input.clone());

            // Include datum in witness set if provided (needed for hash-datum COs)
            if let Some(ref hex) = req.datum_cbor_hex {
                if let Ok(bytes) = hex::decode(hex) {
                    tx = tx.datum(bytes);
                }
            }

            // Single output: CO value + fee UTxO value - fee, all back to wallet
            tx = tx.output(create_ada_output(from_address.clone(), output_value));

            Ok(tx.fee(fee).network_id(network_id))
        },
        estimated_fee,
        &deps.params,
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hex_to_28_bytes(hex_str: &str) -> Result<[u8; 28], TxBuildError> {
    let decoded = hex::decode(hex_str)
        .map_err(|e| TxBuildError::InvalidHex(format!("{e}")))?;
    decoded
        .try_into()
        .map_err(|_| TxBuildError::InvalidHex("expected 28 bytes".into()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marketplace_fee_minimum() {
        // Below threshold: fee should be minimum 2 ADA
        assert_eq!(calculate_marketplace_fee(5_000_000), 2_000_000);
        assert_eq!(calculate_marketplace_fee(10_000_000), 2_000_000);
    }

    #[test]
    fn test_marketplace_fee_proportional() {
        // 410 ADA = 410_000_000 lovelace → 2.5% = 10_250_000
        assert_eq!(calculate_marketplace_fee(410_000_000), 10_250_000);
    }

    #[test]
    fn test_royalty_minimum() {
        // 5 ADA with 5% royalty = 250_000 → should be min 1 ADA
        assert_eq!(calculate_royalty(5_000_000, 0.05), 1_000_000);
    }

    #[test]
    fn test_royalty_proportional() {
        // 410 ADA with 3% royalty = 12_300_000
        assert_eq!(calculate_royalty(410_000_000, 0.03), 12_300_000);
    }

    #[test]
    fn test_seller_receives() {
        let breakdown = calculate_seller_receives(410_000_000, 0.03);
        assert_eq!(breakdown.marketplace_fee, 10_250_000);
        assert_eq!(breakdown.creator_royalty, 12_300_000);
        assert_eq!(breakdown.seller_receives, 387_450_000);
    }

    #[test]
    fn test_build_datum_encodes() {
        let req = CollectionOfferRequest {
            policy_id: "4523c5e21d409b81c95b45b0aea275b8ea1406e6cafea5583b9f8a5f".into(),
            total_lovelace: 50_000_000,
            buyer_pkh: "4a00e5040c2d7e201a9c20744ace64bf28d1cda55999a4931e406922".into(),
            buyer_stake_hash: Some(
                "cba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82ad".into(),
            ),
            royalty_pct: 0.03,
            royalty_pkh: "c39b3cb9232fd69e4ce8c0b19a613b795f60eb16ba8db7a27967014f".into(),
            royalty_stake_hash: Some(
                "d69540f78321389a261db0e195b9cd69a9c19eee6e4466a4ee8199dc".into(),
            ),
            royalty_is_key: true,
            royalty_stake_is_key: true,
            network: Network::Mainnet,
        };

        let result = build_collection_offer(&req).unwrap();
        assert!(!result.datum_bytes.is_empty());
        assert_eq!(result.total_lovelace, 50_000_000);

        // Verify fee breakdown
        assert_eq!(result.fee_breakdown.marketplace_fee, 2_000_000); // 2.5% of 50M = 1.25M < min 2M
        assert_eq!(result.fee_breakdown.creator_royalty, 1_500_000); // 3% of 50M = 1.5M
        assert_eq!(result.fee_breakdown.seller_receives, 46_500_000);
    }

    #[test]
    fn test_extract_co_script_hashes() {
        // Two known CO script addresses (different contract versions)
        let addresses = [
            ("V2 (active)", "addr1xxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tfvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eks2utwdd"),
            ("V3 (deprecated)", "addr1xxzvcf02fs5e282qk3pmjkau2emtcsj5wrukxak3np90n2evjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eksg6pw3p"),
        ];
        for (label, bech32) in &addresses {
            let addr = Address::from_bech32(bech32).unwrap();
            if let Address::Shelley(shelley) = &addr {
                let payment = hex::encode(shelley.payment().as_hash().as_slice());
                let delegation = shelley.delegation().as_hash().map(|h| hex::encode(h.as_slice())).unwrap_or_default();
                eprintln!("{label}: payment={payment} staking={delegation}");
            }
        }

        // Verify our constructed address matches one of the known addresses
        let constructed = co_script_address(Network::Mainnet).unwrap();
        let bech32 = constructed.to_bech32().unwrap();
        let known_addresses: Vec<&str> = addresses.iter().map(|(_, a)| *a).collect();
        assert!(
            known_addresses.contains(&bech32.as_str()),
            "CO script address doesn't match any known address.\nGot: {bech32}"
        );
    }

    #[test]
    fn test_datum_matches_known_co() {
        // Reconstruct the 410 ADA SpaceBudz CO datum and verify it matches
        let req = CollectionOfferRequest {
            policy_id: "4523c5e21d409b81c95b45b0aea275b8ea1406e6cafea5583b9f8a5f".into(),
            total_lovelace: 410_000_000,
            buyer_pkh: "4a00e5040c2d7e201a9c20744ace64bf28d1cda55999a4931e406922".into(),
            buyer_stake_hash: Some(
                "cba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82ad".into(),
            ),
            royalty_pct: 0.03,
            royalty_pkh: "c39b3cb9232fd69e4ce8c0b19a613b795f60eb16ba8db7a27967014f".into(),
            royalty_stake_hash: Some(
                "d69540f78321389a261db0e195b9cd69a9c19eee6e4466a4ee8199dc".into(),
            ),
            royalty_is_key: true,
            royalty_stake_is_key: true,
            network: Network::Mainnet,
        };

        let result = build_collection_offer(&req).unwrap();

        // Fee breakdown should match what we decoded from the real CO
        assert_eq!(result.fee_breakdown.marketplace_fee, 10_250_000);
        assert_eq!(result.fee_breakdown.creator_royalty, 12_300_000);
        assert_eq!(result.fee_breakdown.seller_receives, 387_450_000);
    }
}
