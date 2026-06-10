//! Mint and burn transaction builders.
//!
//! Pure functions for building CIP-25 mint and burn transactions.
//! All operate on [`TxDeps`] and produce [`UnsignedTx`].

use cardano_assets::{AssetId, UtxoApi};
use pallas_addresses::Address;
use pallas_crypto::hash::{Hash, Hasher};
use pallas_txbuilder::StagingTransaction;
use std::collections::HashMap;

use super::{TxDeps, UnsignedTx};
use crate::error::TxBuildError;
use crate::helpers::decode::{decode_asset_name, decode_policy_id};
use crate::helpers::normalize_asset_name_to_hex;
use crate::helpers::output::{add_assets_from_map, create_ada_output};

/// Selected UTxOs, total input lovelace, and accumulated native assets.
type UtxoSelection<'a> = (Vec<&'a UtxoApi>, u64, HashMap<(String, String), u64>);
use crate::intents::MintingPolicy;

/// The change output a mint tx emits back to `deps.from_address`, surfaced so a
/// caller can **chain** the next tx off it without a chain/indexer round-trip:
/// `(tx_hash, output_index)` is a spendable input the moment this tx is built.
///
/// `output_index` is the index within the tx's output list. `has_assets` is true
/// when the change carries native assets the inputs dragged through (then it's
/// not a clean pure-ADA funding source — chainers should skip it). Absent
/// entirely (`None` from the builder) when the change was dust folded into the
/// fee, i.e. there is no change output to spend.
#[derive(Debug, Clone, Copy)]
pub struct ChangeOutput {
    pub output_index: u32,
    pub lovelace: u64,
    pub has_assets: bool,
}

/// Build a CIP-25 mint transaction.
///
/// Creates minted tokens with a native script policy and optional CIP-25 metadata.
/// Each asset in `assets` is `(asset_name, quantity)` where quantity > 0 for minting.
///
/// # Arguments
/// - `deps` — UTxOs, params, from_address, network_id
/// - `policy` — Native script minting policy
/// - `assets` — `(asset_name, quantity_i64)` pairs (positive = mint, negative = burn)
/// - `metadata` — Optional CIP-25 metadata JSON (must have "721" key)
/// - `recipient` — Where to send minted tokens (None = send to self)
pub fn build_cip25_mint(
    deps: &TxDeps,
    policy: &MintingPolicy,
    assets: &[(String, i64)],
    metadata: Option<&serde_json::Value>,
    recipient: Option<&Address>,
) -> Result<UnsignedTx, TxBuildError> {
    use pallas_primitives::Fragment;

    let policy_id_hex = policy
        .policy_id_hex()
        .map_err(|e| TxBuildError::BuildFailed(format!("Failed to derive policy ID: {e}")))?;

    let native_script = policy
        .to_native_script()
        .map_err(|e| TxBuildError::BuildFailed(format!("Failed to create native script: {e}")))?;

    let script_bytes = native_script
        .encode_fragment()
        .map_err(|e| TxBuildError::BuildFailed(format!("Failed to encode script: {e}")))?;

    verify_policy_id(&policy_id_hex, &script_bytes)?;

    let to_addr = recipient.unwrap_or(&deps.from_address);

    // Build list of AssetIds for min ADA calculation (only minted, not burned)
    let minted_asset_ids: Vec<AssetId> = assets
        .iter()
        .filter_map(|(name, qty)| {
            if *qty > 0 {
                let asset_name_hex = normalize_asset_name_to_hex(name);
                format!("{policy_id_hex}.{asset_name_hex}").parse().ok()
            } else {
                None
            }
        })
        .collect();

    let min_ada = crate::calculate_min_ada_with_params(
        &super::send::to_maestro_params(&deps.params),
        &minted_asset_ids,
        &crate::OutputParams { datum_size: None },
    );

    let estimated_fee = calculate_mint_fee(&deps.params, script_bytes.len(), metadata.is_some());

    // Select UTxOs
    let is_burn = assets.iter().any(|(_, qty)| *qty < 0);
    let (selected_utxos, total_input_lovelace, input_native_assets) = if is_burn {
        select_utxos_for_burn(assets, &policy_id_hex, min_ada, estimated_fee, deps)?
    } else {
        select_utxos_for_mint(min_ada, estimated_fee, deps)?
    };

    // Verify sufficient funds
    let total_needed = min_ada + estimated_fee;
    if total_input_lovelace < total_needed {
        return Err(TxBuildError::InsufficientFunds {
            needed: total_needed,
            available: total_input_lovelace,
        });
    }

    // Build transaction
    let policy_id_bytes = decode_policy_id(&policy_id_hex)?;
    let selected_cloned: Vec<UtxoApi> = selected_utxos.into_iter().cloned().collect();

    let has_mints = assets.iter().any(|(_, qty)| *qty > 0);

    // Build recipient output if we're minting
    let recipient_output = if has_mints {
        Some(build_mint_recipient_output(
            to_addr,
            min_ada,
            &policy_id_bytes,
            assets,
        )?)
    } else {
        None
    };

    // Prepare metadata bytes if provided
    let metadata_bytes = if let Some(metadata_value) = metadata {
        let final_metadata = prepare_cip25_metadata(metadata_value, &policy_id_hex, assets);
        Some(
            crate::metadata::cip25::build_cip25_auxiliary_data(&final_metadata)
                .map_err(|e| TxBuildError::BuildFailed(format!("Metadata encoding failed: {e}")))?,
        )
    } else {
        None
    };

    // Build change output data
    let change_assets = calculate_change_assets(&input_native_assets, &policy_id_hex, assets);
    let has_remaining_assets = !change_assets.is_empty();

    // Calculate recipient output cost
    let recipient_output_cost = if has_mints { min_ada } else { 0 };

    // Min change UTxO
    let min_change_utxo = if has_remaining_assets {
        min_ada
    } else {
        deps.params.min_pure_utxo()
    };

    let from_address = deps.from_address.clone();
    let network_id = deps.network_id;
    let assets_owned = assets.to_vec();

    // Build the TX (no fee convergence for mints — use estimated fee with safety margin)
    let refs: Vec<&UtxoApi> = selected_cloned.iter().collect();
    let mut tx = crate::helpers::input::add_utxo_inputs(StagingTransaction::new(), &refs)?;

    tx = tx
        .network_id(network_id)
        .script(pallas_txbuilder::ScriptKind::Native, script_bytes);

    // Apply the validity interval required by the policy's time locks. Without this a
    // TimeLocked/TimeBounded/MultiSigTimeLocked mint is rejected on-chain because the
    // native script's InvalidBefore/InvalidHereafter clauses have nothing to validate
    // against. Non-time-locked policies return (None, None) and leave the tx unbounded.
    let (valid_from_slot, invalid_hereafter_slot) = policy.validity_bounds();
    if let Some(slot) = valid_from_slot {
        tx = tx.valid_from_slot(slot);
    }
    if let Some(slot) = invalid_hereafter_slot {
        tx = tx.invalid_from_slot(slot);
    }

    // Add mint operations
    for (asset_name, quantity) in &assets_owned {
        let asset_name_bytes = decode_asset_name(asset_name);
        tx = tx
            .mint_asset(Hash::from(policy_id_bytes), asset_name_bytes, *quantity)
            .map_err(|e| TxBuildError::BuildFailed(format!("Failed to add mint: {e}")))?;
    }

    // Add recipient output
    if let Some(ro) = recipient_output {
        tx = tx.output(ro);
    }

    // Add metadata
    if let Some(aux_bytes) = metadata_bytes {
        tx = tx.add_auxiliary_data(aux_bytes);
    }

    // Add change output
    let change_lovelace = total_input_lovelace
        .checked_sub(recipient_output_cost)
        .and_then(|v| v.checked_sub(estimated_fee))
        .ok_or(TxBuildError::InsufficientFunds {
            needed: recipient_output_cost + estimated_fee,
            available: total_input_lovelace,
        })?;

    if change_lovelace >= min_change_utxo || has_remaining_assets {
        let change_lovelace = change_lovelace.max(min_change_utxo);
        let mut change_output = create_ada_output(from_address, change_lovelace);

        if has_remaining_assets {
            // Convert change_assets to AssetId-keyed map
            let asset_map: HashMap<AssetId, u64> = change_assets
                .into_iter()
                .filter_map(|((policy, name), qty)| {
                    AssetId::new(policy, name).ok().map(|id| (id, qty))
                })
                .collect();
            change_output = add_assets_from_map(change_output, &asset_map)?;
        }

        tx = tx.output(change_output);
    }

    tx = tx.fee(estimated_fee);

    Ok(UnsignedTx {
        staging: tx,
        fee: estimated_fee,
    })
}

/// Build a burn transaction (convenience wrapper around `build_cip25_mint` with negative quantities).
pub fn build_burn(
    deps: &TxDeps,
    policy: &MintingPolicy,
    assets: &[(String, u64)],
) -> Result<UnsignedTx, TxBuildError> {
    let burn_assets: Vec<(String, i64)> = assets
        .iter()
        .map(|(name, qty)| (name.clone(), -(*qty as i64)))
        .collect();

    build_cip25_mint(deps, policy, &burn_assets, None, None)
}

/// A single NFT/RFT to mint and the address that receives it.
#[derive(Debug, Clone)]
pub struct MintRecipientEntry {
    /// Asset name (display string or hex — normalized internally).
    pub asset_name: String,
    /// Quantity to mint (must be > 0; burns have no recipient — use `build_burn`).
    pub quantity: u64,
    /// Address that receives this asset.
    pub recipient: Address,
}

/// Build a CIP-25 mint that fans out to many recipients in a single transaction.
///
/// Mints are grouped into one output per recipient, all under one native-script policy and
/// one CIP-25 metadata blob. This is the batched-mint primitive behind the vending-machine
/// batcher: many orders fulfilled per tx, bounded by tx size.
///
/// Fee and min-ADA are sized from the *actual* serialized metadata plus per-output/per-mint
/// counts, so it stays correct for both lean (CID-pointer) and heavy (on-chain code) metadata.
///
/// - `mints` — per-asset `(name, quantity > 0, recipient)`; must be non-empty.
/// - `metadata` — optional CIP-25 JSON (`"721"` …) covering all assets under the policy.
/// - `inputs` — explicit funding UTxOs to spend (e.g. a caller-managed pool UTxO for parallel
///   submission). `None` falls back to a single pure-ADA selection from `deps.utxos`.
/// - `extra_self_outputs` — additional pure-ADA outputs paid to `deps.from_address`, in
///   lovelace. Each becomes its own UTxO. The intended use is maintaining a fuel-UTxO pool
///   inside the mint tx: the caller computes how many slots short of target the wallet is
///   and passes that many lovelace amounts here. Drawn from the change side (input − recipient
///   outputs − fee − extras = remainder change). Each amount must be ≥ the pure-ADA min UTxO;
///   the builder rejects undersized entries before signing.
pub fn build_cip25_mint_multi(
    deps: &TxDeps,
    policy: &MintingPolicy,
    mints: &[MintRecipientEntry],
    metadata: Option<&serde_json::Value>,
    inputs: Option<&[UtxoApi]>,
    extra_self_outputs: Option<&[u64]>,
) -> Result<UnsignedTx, TxBuildError> {
    build_cip25_mint_multi_with_change(deps, policy, mints, metadata, inputs, extra_self_outputs)
        .map(|(tx, _change)| tx)
}

/// Like [`build_cip25_mint_multi`] but also returns the tx's [`ChangeOutput`]
/// (the pure-ADA remainder back to `deps.from_address`), so a caller can chain
/// the next tx off it. `None` when the change was dust folded into the fee
/// (no change output emitted). See [`ChangeOutput`].
///
/// Thin shim over [`build_cip25_mint_multi_with_change_and_refunds`] —
/// passes an empty refund list, preserving the legacy signature for
/// callers that don't issue inline refunds.
pub fn build_cip25_mint_multi_with_change(
    deps: &TxDeps,
    policy: &MintingPolicy,
    mints: &[MintRecipientEntry],
    metadata: Option<&serde_json::Value>,
    inputs: Option<&[UtxoApi]>,
    extra_self_outputs: Option<&[u64]>,
) -> Result<(UnsignedTx, Option<ChangeOutput>), TxBuildError> {
    build_cip25_mint_multi_with_change_and_refunds(
        deps,
        policy,
        mints,
        metadata,
        inputs,
        extra_self_outputs,
        &[],
    )
}

/// Superset of [`build_cip25_mint_multi_with_change`] that accepts
/// `refund_outputs` — additional pure-ADA outputs paid to arbitrary
/// (non-self) bech32 addresses, in lovelace. The intended use is
/// **Mode A inline refunds** (`MINT_REFUNDS.md`): when an order's
/// inventory came up short, the executor folds the refund-to-payer
/// into the same mint tx instead of waiting for a separate worker tx.
///
/// Each refund output must be ≥ the pure-ADA min UTxO (the builder
/// rejects undersized entries before signing). Callers should use
/// `minting_core::REFUND_OUTPUT_MIN_LOVELACE` as the policy floor;
/// the ledger min is lower but our platform-wide rule treats
/// sub-`REFUND_OUTPUT_MIN_LOVELACE` amounts as `dust_forfeit` rather
/// than emitting an output at all.
///
/// **Output ordering**: recipient mints → extra-self → refunds →
/// change. The `ChangeOutput.output_index` is `groups.len() +
/// extras.len() + refund_outputs.len()`.
pub fn build_cip25_mint_multi_with_change_and_refunds(
    deps: &TxDeps,
    policy: &MintingPolicy,
    mints: &[MintRecipientEntry],
    metadata: Option<&serde_json::Value>,
    inputs: Option<&[UtxoApi]>,
    extra_self_outputs: Option<&[u64]>,
    refund_outputs: &[(Address, u64)],
) -> Result<(UnsignedTx, Option<ChangeOutput>), TxBuildError> {
    use pallas_primitives::Fragment;

    if mints.is_empty() {
        return Err(TxBuildError::BuildFailed("no mints provided".to_string()));
    }
    if let Some(bad) = mints.iter().find(|m| m.quantity == 0) {
        return Err(TxBuildError::BuildFailed(format!(
            "mint quantity must be > 0 for asset '{}'",
            bad.asset_name
        )));
    }

    let policy_id_hex = policy
        .policy_id_hex()
        .map_err(|e| TxBuildError::BuildFailed(format!("Failed to derive policy ID: {e}")))?;
    let native_script = policy
        .to_native_script()
        .map_err(|e| TxBuildError::BuildFailed(format!("Failed to create native script: {e}")))?;
    let script_bytes = native_script
        .encode_fragment()
        .map_err(|e| TxBuildError::BuildFailed(format!("Failed to encode script: {e}")))?;
    verify_policy_id(&policy_id_hex, &script_bytes)?;
    let policy_id_bytes = decode_policy_id(&policy_id_hex)?;

    // Group mints by recipient (preserve first-seen order) → one output per recipient.
    // Tuple is `(recipient_bech32, recipient_address, Vec<(asset_name, quantity)>)`.
    type RecipientGroup = (String, Address, Vec<(String, u64)>);
    let mut groups: Vec<RecipientGroup> = Vec::new();
    for m in mints {
        let key = m
            .recipient
            .to_bech32()
            .map_err(|e| TxBuildError::BuildFailed(format!("invalid recipient address: {e}")))?;
        if let Some(g) = groups.iter_mut().find(|(k, _, _)| *k == key) {
            g.2.push((m.asset_name.clone(), m.quantity));
        } else {
            groups.push((
                key,
                m.recipient.clone(),
                vec![(m.asset_name.clone(), m.quantity)],
            ));
        }
    }

    let maestro_params = super::send::to_maestro_params(&deps.params);

    // Per-recipient min-ADA (summed across outputs).
    let mut per_group_min_ada: Vec<u64> = Vec::with_capacity(groups.len());
    for (_key, _addr, items) in &groups {
        let asset_ids: Vec<AssetId> = items
            .iter()
            .filter_map(|(name, _)| {
                let hex = normalize_asset_name_to_hex(name);
                format!("{policy_id_hex}.{hex}").parse().ok()
            })
            .collect();
        per_group_min_ada.push(crate::calculate_min_ada_with_params(
            &maestro_params,
            &asset_ids,
            &crate::OutputParams { datum_size: None },
        ));
    }
    let total_output_min_ada: u64 = per_group_min_ada.iter().sum();

    // Metadata aux bytes — measured so the fee is sized from the real metadata weight.
    let assets_i64: Vec<(String, i64)> = mints
        .iter()
        .map(|m| (m.asset_name.clone(), m.quantity as i64))
        .collect();
    let metadata_bytes = if let Some(v) = metadata {
        let final_metadata = prepare_cip25_metadata(v, &policy_id_hex, &assets_i64);
        Some(
            crate::metadata::cip25::build_cip25_auxiliary_data(&final_metadata)
                .map_err(|e| TxBuildError::BuildFailed(format!("Metadata encoding failed: {e}")))?,
        )
    } else {
        None
    };

    // Total extra-self-output lovelace and count (used by fee + needed + change calcs below).
    let extras: &[u64] = extra_self_outputs.unwrap_or(&[]);
    let total_extras_lovelace: u64 = extras.iter().sum();

    // Refund outputs: pure-ADA outputs to arbitrary (non-self)
    // bech32 addresses. Mode A inline refunds (`MINT_REFUNDS.md`).
    let total_refund_lovelace: u64 = refund_outputs.iter().map(|(_, l)| *l).sum();

    // Pure-ADA min UTxO under current params. Sanity-check each extra + refund rather
    // than letting the ledger reject after a fee has been paid.
    let min_pure_change = deps.params.min_pure_utxo();
    for (i, &amount) in extras.iter().enumerate() {
        if amount < min_pure_change {
            return Err(TxBuildError::BuildFailed(format!(
                "extra_self_outputs[{i}] = {amount} lovelace < min_pure_utxo {min_pure_change}"
            )));
        }
    }
    for (i, (_addr, amount)) in refund_outputs.iter().enumerate() {
        if *amount < min_pure_change {
            return Err(TxBuildError::BuildFailed(format!(
                "refund_outputs[{i}] = {amount} lovelace < min_pure_utxo {min_pure_change} \
                 (use minting_core::REFUND_OUTPUT_MIN_LOVELACE + classify_refund to filter \
                 sub-floor amounts before they reach the builder)"
            )));
        }
    }

    // Fee from real sizes: base + script + aux + per-output + per-mint + extra outputs +
    // refund outputs + change output.
    let estimated_tx_size = 300usize
        + script_bytes.len()
        + metadata_bytes.as_ref().map_or(0, |b| b.len())
        + groups.len() * 70
        + mints.len() * 50
        + extras.len() * 70
        + refund_outputs.len() * 70
        + 70;
    let base_fee =
        deps.params.min_fee_coefficient * estimated_tx_size as u64 + deps.params.min_fee_constant;
    let mut fee = base_fee + base_fee / 10;

    // Inputs: explicit (caller-managed pool UTxO) or a single pure-ADA selection.
    let chosen: Vec<UtxoApi> = match inputs {
        Some(utxos) => {
            if utxos.is_empty() {
                return Err(TxBuildError::BuildFailed(
                    "explicit inputs were empty".to_string(),
                ));
            }
            utxos.to_vec()
        }
        None => {
            let (selected, _total, _assets) =
                select_utxos_for_mint(total_output_min_ada, fee, deps)?;
            selected.into_iter().cloned().collect()
        }
    };

    let total_input_lovelace: u64 = chosen.iter().map(|u| u.lovelace).sum();
    let total_needed = total_output_min_ada + total_extras_lovelace + total_refund_lovelace + fee;
    if total_input_lovelace < total_needed {
        return Err(TxBuildError::InsufficientFunds {
            needed: total_needed,
            available: total_input_lovelace,
        });
    }

    // Native assets carried by the inputs pass straight through to change (mint-only).
    let mut input_native_assets: HashMap<(String, String), u64> = HashMap::new();
    for u in &chosen {
        accumulate_native_assets(u, &mut input_native_assets);
    }
    let has_remaining_assets = !input_native_assets.is_empty();

    // Assemble the transaction.
    let refs: Vec<&UtxoApi> = chosen.iter().collect();
    let mut tx = crate::helpers::input::add_utxo_inputs(StagingTransaction::new(), &refs)?;
    tx = tx
        .network_id(deps.network_id)
        .script(pallas_txbuilder::ScriptKind::Native, script_bytes);

    // Validity interval from the policy's time locks (same as the single-recipient path).
    let (valid_from_slot, invalid_hereafter_slot) = policy.validity_bounds();
    if let Some(slot) = valid_from_slot {
        tx = tx.valid_from_slot(slot);
    }
    if let Some(slot) = invalid_hereafter_slot {
        tx = tx.invalid_from_slot(slot);
    }

    // Mint entries.
    for m in mints {
        let name_bytes = decode_asset_name(&m.asset_name);
        tx = tx
            .mint_asset(Hash::from(policy_id_bytes), name_bytes, m.quantity as i64)
            .map_err(|e| TxBuildError::BuildFailed(format!("Failed to add mint: {e}")))?;
    }

    // One output per recipient.
    for ((_key, addr, items), min_ada) in groups.iter().zip(per_group_min_ada.iter()) {
        let mut output = create_ada_output(addr.clone(), *min_ada);
        for (name, qty) in items {
            let name_bytes = decode_asset_name(name);
            output = output
                .add_asset(Hash::from(policy_id_bytes), name_bytes, *qty)
                .map_err(|e| {
                    TxBuildError::BuildFailed(format!("Failed to add asset to output: {e}"))
                })?;
        }
        tx = tx.output(output);
    }

    // Extra self-outputs — pure-ADA UTxOs back to `deps.from_address`, one per requested
    // pool slot. Sized + sanity-checked above; drawn from input change.
    for &amount in extras {
        tx = tx.output(create_ada_output(deps.from_address.clone(), amount));
    }

    // Refund outputs — pure-ADA UTxOs to the payer's address. Mode A
    // inline refunds (`MINT_REFUNDS.md`). Same sized + sanity-checked
    // shape as extras, just targeting a non-self address.
    for (addr, amount) in refund_outputs {
        tx = tx.output(create_ada_output(addr.clone(), *amount));
    }

    // Metadata.
    if let Some(aux) = metadata_bytes {
        tx = tx.add_auxiliary_data(aux);
    }

    // Change. Inputs already cover `total_needed` (recipient outputs + extras + refunds + fee).
    // The change output (when emitted) is appended after the recipient + extra +
    // refund outputs, so its index is `groups.len() + extras.len() + refund_outputs.len()`.
    let change_lovelace = total_input_lovelace
        - total_output_min_ada
        - total_extras_lovelace
        - total_refund_lovelace
        - fee;
    let change_index = (groups.len() + extras.len() + refund_outputs.len()) as u32;
    let change: Option<ChangeOutput>;
    if has_remaining_assets {
        // Must emit a change output to carry the inputs' native assets.
        if change_lovelace < min_pure_change {
            return Err(TxBuildError::InsufficientFunds {
                needed: total_output_min_ada
                    + total_extras_lovelace
                    + total_refund_lovelace
                    + fee
                    + min_pure_change,
                available: total_input_lovelace,
            });
        }
        let mut change_output = create_ada_output(deps.from_address.clone(), change_lovelace);
        let asset_map: HashMap<AssetId, u64> = input_native_assets
            .into_iter()
            .filter_map(|((p, n), q)| AssetId::new(p, n).ok().map(|id| (id, q)))
            .collect();
        change_output = add_assets_from_map(change_output, &asset_map)?;
        tx = tx.output(change_output);
        change = Some(ChangeOutput {
            output_index: change_index,
            lovelace: change_lovelace,
            has_assets: true,
        });
    } else if change_lovelace >= min_pure_change {
        tx = tx.output(create_ada_output(
            deps.from_address.clone(),
            change_lovelace,
        ));
        change = Some(ChangeOutput {
            output_index: change_index,
            lovelace: change_lovelace,
            has_assets: false,
        });
    } else {
        // Dust below a viable change output → fold into the fee to keep the tx balanced.
        // No change output emitted, so nothing to chain off.
        fee += change_lovelace;
        change = None;
    }

    tx = tx.fee(fee);

    Ok((UnsignedTx { staging: tx, fee }, change))
}

// ============================================================================
// Helpers
// ============================================================================

/// Verify that the policy ID matches the script hash.
pub fn verify_policy_id(policy_id_hex: &str, script_bytes: &[u8]) -> Result<(), TxBuildError> {
    let mut prefixed_bytes = Vec::with_capacity(1 + script_bytes.len());
    prefixed_bytes.push(0x00);
    prefixed_bytes.extend_from_slice(script_bytes);

    let actual_hash = Hasher::<224>::hash(&prefixed_bytes);
    let actual_hex = hex::encode(actual_hash);

    if policy_id_hex != actual_hex {
        return Err(TxBuildError::PolicyMismatch {
            expected: policy_id_hex.to_string(),
            actual: actual_hex,
        });
    }

    Ok(())
}

/// Calculate estimated fee for a minting transaction.
pub fn calculate_mint_fee(
    params: &crate::params::TxBuildParams,
    script_size: usize,
    has_metadata: bool,
) -> u64 {
    let base_tx_size = 300u64;
    let metadata_size = if has_metadata { 500u64 } else { 0 };
    let estimated_tx_size = base_tx_size + script_size as u64 + metadata_size;

    let base_fee = params.min_fee_coefficient * estimated_tx_size + params.min_fee_constant;

    // Add 10% safety margin
    base_fee + (base_fee / 10)
}

/// Select UTxOs for a mint operation (prefer pure ADA).
fn select_utxos_for_mint(
    min_ada: u64,
    estimated_fee: u64,
    deps: &TxDeps,
) -> Result<UtxoSelection<'_>, TxBuildError> {
    let selected_utxo = crate::selection::select_utxo_for_amount(
        &deps.utxos,
        min_ada,
        estimated_fee,
        &crate::selection::UtxoSelectionConfig::new(&deps.params),
    )?;

    let total_input_lovelace = selected_utxo.lovelace;
    if total_input_lovelace == 0 {
        return Err(TxBuildError::InsufficientFunds {
            needed: min_ada + estimated_fee,
            available: 0,
        });
    }

    let mut input_native_assets: HashMap<(String, String), u64> = HashMap::new();
    accumulate_native_assets(selected_utxo, &mut input_native_assets);

    Ok((
        vec![selected_utxo],
        total_input_lovelace,
        input_native_assets,
    ))
}

/// Select UTxOs for a burn operation.
fn select_utxos_for_burn<'a>(
    assets: &[(String, i64)],
    policy_id_hex: &str,
    min_ada: u64,
    estimated_fee: u64,
    deps: &'a TxDeps,
) -> Result<UtxoSelection<'a>, TxBuildError> {
    let mut selected_utxos: Vec<&UtxoApi> = Vec::new();
    let mut total_input_lovelace: u64 = 0;
    let mut input_native_assets: HashMap<(String, String), u64> = HashMap::new();

    let assets_to_burn: Vec<(String, u64)> = assets
        .iter()
        .filter(|(_, qty)| *qty < 0)
        .map(|(name, qty)| (name.clone(), qty.unsigned_abs()))
        .collect();

    for (asset_name_hex, burn_qty) in &assets_to_burn {
        let utxo = find_utxo_with_asset(&deps.utxos, policy_id_hex, asset_name_hex, *burn_qty)?;

        let already_selected = selected_utxos
            .iter()
            .any(|u| u.tx_hash == utxo.tx_hash && u.output_index == utxo.output_index);

        if !already_selected {
            selected_utxos.push(utxo);
        }
    }

    for utxo in &selected_utxos {
        total_input_lovelace += utxo.lovelace;
        accumulate_native_assets(utxo, &mut input_native_assets);
    }

    // Check if we need additional lovelace
    let total_needed = min_ada + estimated_fee;
    if total_input_lovelace < total_needed {
        if let Some(utxo) = find_additional_utxo_for_lovelace(
            &deps.utxos,
            &selected_utxos,
            total_needed - total_input_lovelace,
        ) {
            total_input_lovelace += utxo.lovelace;
            accumulate_native_assets(utxo, &mut input_native_assets);
            selected_utxos.push(utxo);
        } else {
            return Err(TxBuildError::InsufficientFunds {
                needed: total_needed,
                available: total_input_lovelace,
            });
        }
    }

    Ok((selected_utxos, total_input_lovelace, input_native_assets))
}

/// Find a UTxO containing the specified asset with sufficient quantity.
fn find_utxo_with_asset<'a>(
    utxos: &'a [UtxoApi],
    policy_id_hex: &str,
    asset_name_hex: &str,
    min_qty: u64,
) -> Result<&'a UtxoApi, TxBuildError> {
    let target_concatenated = format!("{policy_id_hex}{asset_name_hex}");

    utxos
        .iter()
        .find(|utxo| {
            utxo.assets.iter().any(|a| {
                let concatenated = a.asset_id.concatenated();
                concatenated == target_concatenated && a.quantity >= min_qty
            })
        })
        .ok_or_else(|| TxBuildError::AssetNotFound(format!("{policy_id_hex}.{asset_name_hex}")))
}

/// Find an additional UTxO with sufficient lovelace.
fn find_additional_utxo_for_lovelace<'a>(
    utxos: &'a [UtxoApi],
    already_selected: &[&UtxoApi],
    min_lovelace: u64,
) -> Option<&'a UtxoApi> {
    utxos.iter().find(|utxo| {
        let already = already_selected
            .iter()
            .any(|u| u.tx_hash == utxo.tx_hash && u.output_index == utxo.output_index);
        !already && utxo.lovelace >= min_lovelace
    })
}

/// Accumulate native assets from a UTxO into a HashMap.
fn accumulate_native_assets(utxo: &UtxoApi, assets: &mut HashMap<(String, String), u64>) {
    for asset in &utxo.assets {
        let key = (
            asset.asset_id.policy_id().to_string(),
            asset.asset_id.asset_name_hex().to_string(),
        );
        *assets.entry(key).or_insert(0) += asset.quantity;
    }
}

/// Build recipient output with minted assets.
fn build_mint_recipient_output(
    to_addr: &Address,
    lovelace: u64,
    policy_id_bytes: &[u8; 28],
    assets: &[(String, i64)],
) -> Result<pallas_txbuilder::Output, TxBuildError> {
    let mut output = create_ada_output(to_addr.clone(), lovelace);

    for (asset_name, quantity) in assets {
        if *quantity > 0 {
            let asset_name_bytes = decode_asset_name(asset_name);
            output = output
                .add_asset(
                    Hash::from(*policy_id_bytes),
                    asset_name_bytes,
                    *quantity as u64,
                )
                .map_err(|e| {
                    TxBuildError::BuildFailed(format!("Failed to add asset to output: {e}"))
                })?;
        }
    }

    Ok(output)
}

/// Calculate remaining native assets after mint/burn operations.
fn calculate_change_assets(
    input_native_assets: &HashMap<(String, String), u64>,
    policy_id_hex: &str,
    assets: &[(String, i64)],
) -> Vec<((String, String), u64)> {
    let mut remaining: HashMap<(String, String), i64> = input_native_assets
        .iter()
        .map(|(k, v)| (k.clone(), *v as i64))
        .collect();

    // Apply burn operations only — minted assets go to recipient, not change
    for (asset_name, quantity) in assets {
        if *quantity < 0 {
            let asset_name_hex = normalize_asset_name_to_hex(asset_name);
            let key = (policy_id_hex.to_string(), asset_name_hex);
            *remaining.entry(key).or_insert(0) += quantity;
        }
    }

    remaining
        .into_iter()
        .filter(|(_, qty)| *qty > 0)
        .map(|(k, qty)| (k, qty as u64))
        .collect()
}

/// Prepare CIP-25 metadata, injecting policy ID if needed.
pub fn prepare_cip25_metadata(
    metadata_value: &serde_json::Value,
    policy_id_hex: &str,
    assets: &[(String, i64)],
) -> serde_json::Value {
    if metadata_value.get("721").is_some() {
        if let Some(obj_721) = metadata_value.get("721").and_then(|v| v.as_object()) {
            if obj_721.is_empty() || obj_721.contains_key("__POLICY_ID__") {
                let mut metadata_clone = metadata_value.clone();
                if let Some(obj_721_mut) = metadata_clone
                    .get_mut("721")
                    .and_then(|v| v.as_object_mut())
                {
                    if let Some(inner) = obj_721_mut.remove("__POLICY_ID__") {
                        obj_721_mut.insert(policy_id_hex.to_string(), inner);
                    }
                }
                return metadata_clone;
            }
        }
        metadata_value.clone()
    } else {
        let mut policy_metadata = serde_json::Map::new();
        for (asset_name, _qty) in assets {
            if let Some(asset_meta) = metadata_value.get(asset_name) {
                policy_metadata.insert(asset_name.clone(), asset_meta.clone());
            }
        }
        if policy_metadata.is_empty() && !assets.is_empty() {
            policy_metadata.insert(assets[0].0.clone(), metadata_value.clone());
        }

        let mut metadata_721 = serde_json::Map::new();
        metadata_721.insert(
            policy_id_hex.to_string(),
            serde_json::Value::Object(policy_metadata),
        );

        let mut wrapped = serde_json::Map::new();
        wrapped.insert("721".to_string(), serde_json::Value::Object(metadata_721));

        serde_json::Value::Object(wrapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_params() -> crate::params::TxBuildParams {
        crate::params::TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155381,
            coins_per_utxo_byte: 4310,
            max_tx_size: 16384,
            max_value_size: 5000,
            price_mem: None,
            price_step: None,
            ..Default::default()
        }
    }

    fn test_address() -> Address {
        Address::from_bech32("addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp").unwrap()
    }

    #[test]
    fn test_verify_policy_id_correct() {
        let policy = MintingPolicy::SingleKey {
            key_hash: "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29".to_string(),
        };
        let script = policy.to_native_script().unwrap();
        use pallas_primitives::Fragment;
        let script_bytes = script.encode_fragment().unwrap();
        let policy_id = policy.policy_id_hex().unwrap();

        assert!(verify_policy_id(&policy_id, &script_bytes).is_ok());
    }

    #[test]
    fn test_verify_policy_id_mismatch() {
        let result = verify_policy_id("abcd".repeat(14).as_str(), &[0x82, 0x00, 0x58]);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TxBuildError::PolicyMismatch { .. }
        ));
    }

    #[test]
    fn test_calculate_mint_fee() {
        let params = test_params();
        let fee = calculate_mint_fee(&params, 50, true);
        // (300 + 50 + 500) * 44 + 155381 = 37400 + 155381 = 192781, +10% = 212059
        assert!(fee > 190_000);
        assert!(fee < 250_000);
    }

    #[test]
    fn test_prepare_cip25_metadata_wraps() {
        let raw = serde_json::json!({
            "TestNFT": {
                "name": "Test NFT",
                "image": "ipfs://abc"
            }
        });

        let result = prepare_cip25_metadata(&raw, "deadbeef", &[("TestNFT".to_string(), 1)]);
        assert!(result.get("721").is_some());
        assert!(result["721"]["deadbeef"]["TestNFT"]["name"].as_str() == Some("Test NFT"));
    }

    #[test]
    fn test_prepare_cip25_metadata_passthrough() {
        let raw = serde_json::json!({
            "721": {
                "deadbeef": {
                    "TestNFT": { "name": "Test" }
                }
            }
        });

        let result = prepare_cip25_metadata(&raw, "deadbeef", &[("TestNFT".to_string(), 1)]);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_calculate_change_assets_burn() {
        // Input map uses hex-encoded asset names (as produced by accumulate_native_assets)
        let asset1_hex = hex::encode("asset1");
        let asset2_hex = hex::encode("asset2");
        let mut input = HashMap::new();
        input.insert(("policy1".to_string(), asset1_hex.clone()), 5u64);
        input.insert(("policy1".to_string(), asset2_hex.clone()), 3u64);

        // Burn list uses display names — function normalizes to hex internally
        let assets = vec![("asset1".to_string(), -2i64)];
        let result = calculate_change_assets(&input, "policy1", &assets);

        let map: HashMap<_, _> = result.into_iter().collect();
        assert_eq!(map[&("policy1".to_string(), asset1_hex)], 3);
        assert_eq!(map[&("policy1".to_string(), asset2_hex)], 3);
    }

    #[test]
    fn test_build_cip25_mint_simple() {
        let policy = MintingPolicy::SingleKey {
            key_hash: "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29".to_string(),
        };

        let deps = super::super::TxDeps {
            utxos: vec![UtxoApi {
                tx_hash: "a".repeat(64),
                output_index: 0,
                lovelace: 10_000_000,
                assets: vec![],
                tags: vec![],
            }],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };

        let assets = vec![("TestNFT001".to_string(), 1i64)];
        let result = build_cip25_mint(&deps, &policy, &assets, None, None);
        assert!(result.is_ok(), "build_cip25_mint failed: {result:?}");

        let unsigned = result.unwrap();
        assert!(unsigned.fee > 0);
    }

    #[test]
    fn test_build_cip25_mint_time_locked_applies_validity() {
        // A TimeLocked policy must build without error: the builder is responsible for
        // applying the validity interval the native script requires. Before the fix this
        // path produced a tx the ledger would reject (no validity bound for the time lock).
        let policy = MintingPolicy::TimeLocked {
            key_hash: "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29".to_string(),
            before_slot: 90_000_000,
        };

        let deps = super::super::TxDeps {
            utxos: vec![UtxoApi {
                tx_hash: "a".repeat(64),
                output_index: 0,
                lovelace: 10_000_000,
                assets: vec![],
                tags: vec![],
            }],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };

        let assets = vec![("TimeLockedNFT".to_string(), 1i64)];
        let result = build_cip25_mint(&deps, &policy, &assets, None, None);
        assert!(
            result.is_ok(),
            "time-locked mint failed to build: {result:?}"
        );

        // Sanity: the policy reports the upper bound the builder must apply.
        assert_eq!(policy.validity_bounds(), (None, Some(90_000_000)));
    }

    fn test_policy() -> MintingPolicy {
        MintingPolicy::SingleKey {
            key_hash: "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29".to_string(),
        }
    }

    /// Deterministic, valid testnet address from a seed byte (enterprise / no stake).
    fn test_recipient(seed: u8) -> Address {
        let kh = Hash::<28>::from([seed; 28]);
        pallas_addresses::ShelleyAddress::new(
            pallas_addresses::Network::Testnet,
            pallas_addresses::ShelleyPaymentPart::key_hash(kh),
            pallas_addresses::ShelleyDelegationPart::Null,
        )
        .into()
    }

    fn deps_with(lovelace: u64) -> super::super::TxDeps {
        super::super::TxDeps {
            utxos: vec![UtxoApi {
                tx_hash: "a".repeat(64),
                output_index: 0,
                lovelace,
                assets: vec![],
                tags: vec![],
            }],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        }
    }

    #[test]
    fn test_build_cip25_mint_multi_two_recipients() {
        let mints = vec![
            MintRecipientEntry {
                asset_name: "Foo001".to_string(),
                quantity: 1,
                recipient: test_recipient(1),
            },
            MintRecipientEntry {
                asset_name: "Foo002".to_string(),
                quantity: 1,
                recipient: test_recipient(2),
            },
        ];
        let result = build_cip25_mint_multi(
            &deps_with(20_000_000),
            &test_policy(),
            &mints,
            None,
            None,
            None,
        );
        assert!(result.is_ok(), "multi-recipient mint failed: {result:?}");
        assert!(result.unwrap().fee > 0);
    }

    #[test]
    fn test_build_cip25_mint_multi_surfaces_change() {
        // 2 recipients, big pure-ADA input, no extras → a pure-ADA change output
        // at index 2 (after the two recipient outputs), chainable.
        let mints = vec![
            MintRecipientEntry {
                asset_name: "Foo001".to_string(),
                quantity: 1,
                recipient: test_recipient(1),
            },
            MintRecipientEntry {
                asset_name: "Foo002".to_string(),
                quantity: 1,
                recipient: test_recipient(2),
            },
        ];
        let (unsigned, change) = build_cip25_mint_multi_with_change(
            &deps_with(50_000_000),
            &test_policy(),
            &mints,
            None,
            None,
            None,
        )
        .expect("build failed");
        let change = change.expect("expected a change output");
        assert_eq!(
            change.output_index, 2,
            "change follows the 2 recipient outputs"
        );
        assert!(!change.has_assets, "pure-ADA input → pure-ADA change");
        assert!(change.lovelace > 0);
        // The change index must point at a real output in the staging tx.
        let n_outputs = unsigned
            .staging
            .outputs
            .as_ref()
            .map(|o| o.len())
            .unwrap_or(0);
        assert!(
            (change.output_index as usize) < n_outputs,
            "change index {} out of bounds ({n_outputs} outputs)",
            change.output_index
        );
    }

    /// Golden: inline refund outputs land between the recipient mints
    /// and the change, each carrying the exact requested lovelace to the
    /// payer's address. Proves the on-chain shape `MINT_REFUNDS.md` Mode
    /// A/C produce: `[mint…, refund…, change]`.
    #[test]
    fn test_refunds_place_outputs_before_change() {
        let mints = vec![
            MintRecipientEntry {
                asset_name: "Foo001".to_string(),
                quantity: 1,
                recipient: test_recipient(1),
            },
            MintRecipientEntry {
                asset_name: "Foo002".to_string(),
                quantity: 1,
                recipient: test_recipient(2),
            },
        ];
        let refund_addr = test_recipient(9);
        let refund_amount = 5_000_000u64;
        let (unsigned, change) = build_cip25_mint_multi_with_change_and_refunds(
            &deps_with(50_000_000),
            &test_policy(),
            &mints,
            None,
            None,
            None,
            std::slice::from_ref(&(refund_addr.clone(), refund_amount)),
        )
        .expect("build with refund failed");

        // 2 mints + 1 refund → change at index 3.
        let change = change.expect("expected a change output");
        assert_eq!(change.output_index, 3, "change follows 2 mints + 1 refund");
        assert!(change.lovelace > 0);

        let outputs = unsigned.staging.outputs.as_ref().expect("outputs present");
        assert_eq!(outputs.len(), 4, "2 mints + 1 refund + 1 change");

        // The refund output sits at index 2 (after the 2 recipient mints),
        // pays exactly the requested lovelace, and is addressed to the payer.
        let refund_out = &outputs[2];
        assert_eq!(
            refund_out.lovelace, refund_amount,
            "refund pays the owed amount"
        );
        assert_eq!(
            refund_out.address.to_bech32().unwrap(),
            refund_addr.to_bech32().unwrap(),
            "refund output addressed to the payer"
        );
        assert!(refund_out.assets.is_none(), "refund is a pure-ADA output");
    }

    /// Golden: multiple refunds (e.g. several short-filled orders in one
    /// bin) each get their own output, all before the change. Change
    /// index = mints + refunds.
    #[test]
    fn test_multiple_refund_outputs_each_emitted() {
        let mints = vec![MintRecipientEntry {
            asset_name: "Foo001".to_string(),
            quantity: 1,
            recipient: test_recipient(1),
        }];
        let refunds = [
            (test_recipient(8), 4_000_000u64),
            (test_recipient(9), 6_000_000u64),
        ];
        let (unsigned, change) = build_cip25_mint_multi_with_change_and_refunds(
            &deps_with(50_000_000),
            &test_policy(),
            &mints,
            None,
            None,
            None,
            &refunds,
        )
        .expect("build with refunds failed");

        let change = change.expect("expected a change output");
        assert_eq!(change.output_index, 3, "change follows 1 mint + 2 refunds");
        let outputs = unsigned.staging.outputs.as_ref().expect("outputs present");
        assert_eq!(outputs.len(), 4, "1 mint + 2 refunds + 1 change");
        assert_eq!(outputs[1].lovelace, 4_000_000);
        assert_eq!(outputs[2].lovelace, 6_000_000);
    }

    /// Golden: a sub-min-UTxO refund output is rejected at build time
    /// (defence-in-depth — `classify_refund` already forfeits these, but
    /// the builder must never emit an output the ledger would reject
    /// after the fee is paid).
    #[test]
    fn test_refunds_reject_sub_min_utxo() {
        let mints = vec![MintRecipientEntry {
            asset_name: "Foo001".to_string(),
            quantity: 1,
            recipient: test_recipient(1),
        }];
        // 0.5 ADA is below the pure-ADA min UTxO under test params.
        let refunds = [(test_recipient(9), 500_000u64)];
        let result = build_cip25_mint_multi_with_change_and_refunds(
            &deps_with(50_000_000),
            &test_policy(),
            &mints,
            None,
            None,
            None,
            &refunds,
        );
        assert!(
            matches!(result, Err(TxBuildError::BuildFailed(_))),
            "sub-min-UTxO refund must be rejected"
        );
    }

    #[test]
    fn test_build_cip25_mint_multi_explicit_inputs() {
        // deps carries no UTxOs — the explicit pool UTxO must be used instead.
        let deps = super::super::TxDeps {
            utxos: vec![],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let input = UtxoApi {
            tx_hash: "b".repeat(64),
            output_index: 1,
            lovelace: 10_000_000,
            assets: vec![],
            tags: vec![],
        };
        let mints = vec![MintRecipientEntry {
            asset_name: "Bar001".to_string(),
            quantity: 1,
            recipient: test_recipient(3),
        }];
        let result = build_cip25_mint_multi(
            &deps,
            &test_policy(),
            &mints,
            None,
            Some(std::slice::from_ref(&input)),
            None,
        );
        assert!(result.is_ok(), "explicit-inputs mint failed: {result:?}");
    }

    #[test]
    fn test_build_cip25_mint_multi_rejects_zero_quantity() {
        let mints = vec![MintRecipientEntry {
            asset_name: "X".to_string(),
            quantity: 0,
            recipient: test_recipient(1),
        }];
        assert!(build_cip25_mint_multi(
            &deps_with(10_000_000),
            &test_policy(),
            &mints,
            None,
            None,
            None
        )
        .is_err());
    }

    #[test]
    fn test_build_cip25_mint_multi_empty_errors() {
        assert!(build_cip25_mint_multi(
            &deps_with(10_000_000),
            &test_policy(),
            &[],
            None,
            None,
            None
        )
        .is_err());
    }

    /// `extra_self_outputs` adds pool-slot UTxOs back to `from_address` and the change
    /// covers the leftover. With a 100 ADA input, a 1 ADA recipient, ~0.5 ADA fee,
    /// and 4 × 10 ADA pool slots, ~58 ADA remains as change. Builder should succeed
    /// and the resulting tx fee should be sized to include the extra outputs.
    #[test]
    fn test_build_cip25_mint_multi_with_extra_self_outputs() {
        let mints = vec![MintRecipientEntry {
            asset_name: "Pool001".to_string(),
            quantity: 1,
            recipient: test_recipient(7),
        }];
        let extras = [10_000_000u64; 4]; // 4 pool slots × 10 ADA
        let result = build_cip25_mint_multi(
            &deps_with(100_000_000),
            &test_policy(),
            &mints,
            None,
            None,
            Some(&extras),
        );
        assert!(
            result.is_ok(),
            "mint with extra_self_outputs failed: {result:?}"
        );
    }

    /// Pool slots below `min_pure_change` (under typical params ~1.1 ADA) must be rejected
    /// before signing — the ledger would reject the tx and burn the fee otherwise.
    #[test]
    fn test_build_cip25_mint_multi_rejects_undersized_extras() {
        let mints = vec![MintRecipientEntry {
            asset_name: "TinyExtra".to_string(),
            quantity: 1,
            recipient: test_recipient(7),
        }];
        let extras = [500_000u64]; // 0.5 ADA — below min_pure_change
        let result = build_cip25_mint_multi(
            &deps_with(50_000_000),
            &test_policy(),
            &mints,
            None,
            None,
            Some(&extras),
        );
        assert!(matches!(result, Err(TxBuildError::BuildFailed(_))));
    }

    // ── Inline-distribution size spike ────────────────────────────────────
    //
    // Foundational check for the settle-as-you-mint pivot
    // (docs/design/INLINE_MINT_DISTRIBUTION.md): does a single on-chain-art
    // mint — metadata near the ~15 KB ceiling — still fit under the 16384-byte
    // Cardano tx limit once we add the artist/platform payout outputs to the
    // delivery tx? The payout outputs are pure-ADA, byte-identical to the
    // builder's `refund_outputs`, so we measure with those.

    /// CIP-25 metadata sized near the on-chain-art ceiling. Large generative
    /// data is stored as an array of ≤64-byte string chunks per CIP-25.
    fn art_metadata(n_chunks: usize) -> serde_json::Value {
        use serde_json::{Map, Value};
        // 60-char chunk (safely ≤ 64-byte metadatum limit).
        let chunk = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab";
        let src: Vec<Value> = (0..n_chunks)
            .map(|_| Value::String(chunk.to_string()))
            .collect();
        let mut asset = Map::new();
        asset.insert("name".into(), Value::String("On-Chain Art Piece #1".into()));
        asset.insert("mediaType".into(), Value::String("image/svg+xml".into()));
        asset.insert("artist".into(), Value::String("CM_GenArt".into()));
        asset.insert("src".into(), Value::Array(src));
        let mut by_asset = Map::new();
        by_asset.insert("ArtPiece001".into(), Value::Object(asset));
        let mut by_policy = Map::new();
        by_policy.insert(
            "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29".into(),
            Value::Object(by_asset),
        );
        let mut root = Map::new();
        root.insert("721".into(), Value::Object(by_policy));
        Value::Object(root)
    }

    /// Build + sign an inline mint: 1 payment input, mint 1 NFT to the buyer,
    /// `n_payouts` pure-ADA payout outputs (refund + platform + founders),
    /// change, and the art metadata. Returns the **signed** tx byte length.
    fn inline_signed_tx_size(metadata: &serde_json::Value, n_payouts: usize) -> usize {
        let deps = super::super::TxDeps {
            utxos: vec![UtxoApi {
                tx_hash: "a".repeat(64),
                output_index: 0,
                lovelace: 200_000_000, // the buyer's payment — covers everything
                assets: vec![],
                tags: vec![],
            }],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let mints = vec![MintRecipientEntry {
            asset_name: "ArtPiece001".to_string(),
            quantity: 1,
            recipient: test_recipient(1),
        }];
        let payouts: Vec<(Address, u64)> = (0..n_payouts)
            .map(|i| (test_recipient(100 + i as u8), 2_000_000))
            .collect();
        let (unsigned, _change) = build_cip25_mint_multi_with_change_and_refunds(
            &deps,
            &test_policy(),
            &mints,
            Some(metadata),
            None,
            None,
            &payouts,
        )
        .expect("inline mint builds");
        let sk = pallas_crypto::key::ed25519::SecretKey::from([1u8; 32]);
        let signed = unsigned.build_and_sign(&sk).expect("sign");
        signed.tx_cbor_hex.len() / 2 // hex → bytes
    }

    #[test]
    fn spike_inline_distribution_15kb_metadata_fits_tx_limit() {
        const MAX_TX: usize = 16384;

        // ~14.9 KB of metadata — at the observed on-chain-art ceiling.
        let md = art_metadata(240);
        let aux = crate::metadata::cip25::build_cip25_auxiliary_data(&md).unwrap();
        eprintln!(
            "metadata aux_data: {} bytes ({:.1}% of {MAX_TX})",
            aux.len(),
            aux.len() as f64 / MAX_TX as f64 * 100.0
        );
        assert!(
            (13_500..=15_800).contains(&aux.len()),
            "metadata not in the representative ~15KB range: {} bytes",
            aux.len()
        );

        eprintln!("payouts | signed tx bytes | headroom");
        for n in 0..=8 {
            let size = inline_signed_tx_size(&md, n);
            eprintln!(
                "   {n}    |     {size:>6}    |  {}",
                MAX_TX as i64 - size as i64
            );
        }

        // The realistic inline case: refund-to-buyer + platform + 2 founders.
        let realistic = inline_signed_tx_size(&md, 4);
        assert!(
            realistic < MAX_TX,
            "inline mint with 4 payout outputs + ~15KB metadata = {realistic} bytes, \
             exceeds the {MAX_TX} limit — the pivot's headroom assumption is wrong"
        );
    }

    /// How much metadata fits with the *minimal* inline distribution — a
    /// single payout output (one artist) and platform fees waived (no
    /// platform output). Sweeps metadata up until the signed tx would exceed
    /// the limit and reports the largest that fits.
    #[test]
    fn spike_max_metadata_single_payout_fee_waived() {
        const MAX_TX: usize = 16384;

        // Build helper that returns None if the tx won't build / serialise.
        fn try_size(metadata: &serde_json::Value, n_payouts: usize) -> Option<usize> {
            let deps = super::super::TxDeps {
                utxos: vec![UtxoApi {
                    tx_hash: "a".repeat(64),
                    output_index: 0,
                    lovelace: 200_000_000,
                    assets: vec![],
                    tags: vec![],
                }],
                params: test_params(),
                from_address: test_address(),
                network_id: 0,
            };
            let mints = vec![MintRecipientEntry {
                asset_name: "ArtPiece001".to_string(),
                quantity: 1,
                recipient: test_recipient(1),
            }];
            let payouts: Vec<(Address, u64)> = (0..n_payouts)
                .map(|i| (test_recipient(100 + i as u8), 2_000_000))
                .collect();
            let (unsigned, _) = build_cip25_mint_multi_with_change_and_refunds(
                &deps,
                &test_policy(),
                &mints,
                Some(metadata),
                None,
                None,
                &payouts,
            )
            .ok()?;
            let sk = pallas_crypto::key::ed25519::SecretKey::from([1u8; 32]);
            let signed = unsigned.build_and_sign(&sk).ok()?;
            Some(signed.tx_cbor_hex.len() / 2)
        }

        let mut best = (0usize, 0usize, 0usize); // (chunks, metadata_bytes, tx_bytes)
        for chunks in (220..420).step_by(1) {
            let md = art_metadata(chunks);
            let aux = match crate::metadata::cip25::build_cip25_auxiliary_data(&md) {
                Ok(a) => a,
                Err(_) => break,
            };
            match try_size(&md, 1) {
                Some(tx) if tx < MAX_TX => best = (chunks, aux.len(), tx),
                _ => break,
            }
        }
        eprintln!(
            "single payout + fee waived: max metadata = {} bytes (signed tx {}, {} chunks, {} bytes spare)",
            best.1,
            best.2,
            best.0,
            MAX_TX - best.2,
        );
        assert!(
            best.1 >= 15_000,
            "expected to fit well over 15KB, got {}",
            best.1
        );
    }
}
