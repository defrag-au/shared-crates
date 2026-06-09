//! Send transaction builders.
//!
//! Pure functions for building send-lovelace, send-max, send-assets, and
//! consolidation transactions. All operate on [`TxDeps`] and produce [`UnsignedTx`].

use cardano_assets::{AssetId, UtxoApi, UtxoTag};
use pallas_addresses::Address;
use pallas_txbuilder::StagingTransaction;
use std::collections::HashSet;

use super::{converge_fee, TxDeps, UnsignedTx};
use crate::error::TxBuildError;
use crate::helpers::input::{add_utxo_input, add_utxo_inputs};
use crate::helpers::output::{build_change_output, create_ada_output};
use crate::helpers::utxo_query::{
    collect_asset_ids, collect_utxo_native_assets, total_utxo_lovelace,
};
use crate::selection;

/// Build a transaction sending a specific amount of lovelace.
///
/// Selects a single UTxO (preferring pure ADA), constructs a two-output TX
/// (recipient + change), and converges the fee.
pub fn build_send_lovelace(
    deps: &TxDeps,
    to_addr: &Address,
    amount: u64,
) -> Result<UnsignedTx, TxBuildError> {
    let estimated_fee = selection::estimate_simple_fee(&deps.params);
    let selected_utxo = selection::select_utxo_for_amount(
        &deps.utxos,
        amount,
        estimated_fee,
        &selection::UtxoSelectionConfig::new(&deps.params),
    )?
    .clone();

    let input_amount = selected_utxo.lovelace;
    if input_amount == 0 {
        return Err(TxBuildError::InsufficientFunds {
            needed: amount,
            available: 0,
        });
    }

    let has_native_assets = !selected_utxo.assets.is_empty();
    let from_address = deps.from_address.clone();
    let to_address = to_addr.clone();
    let network_id = deps.network_id;

    converge_fee(
        |fee| {
            let change = input_amount
                .checked_sub(amount)
                .and_then(|v| v.checked_sub(fee))
                .ok_or(TxBuildError::InsufficientFunds {
                    needed: amount + fee,
                    available: input_amount,
                })?;

            let mut tx = add_utxo_input(StagingTransaction::new(), &selected_utxo)?;
            tx = tx.output(create_ada_output(to_address.clone(), amount));

            if change > 0 {
                if has_native_assets {
                    let change_output =
                        build_change_output(from_address.clone(), change, &[&selected_utxo], None)?;
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

/// Build a send-max transaction (sweep all ADA to recipient).
///
/// Selects all UTxOs with extractable ADA, sends max ADA to recipient,
/// and creates change outputs for UTxOs with native assets (min ADA + assets back to self).
pub fn build_send_max(deps: &TxDeps, to_addr: &Address) -> Result<UnsignedTx, TxBuildError> {
    let selected_utxos = selection::select_all_utxos_for_max(&deps.utxos, &deps.params)?;

    // Calculate total input ADA and reserved ADA for native assets
    let mut total_input_ada = 0u64;
    let mut total_reserved_for_assets = 0u64;
    let mut utxo_min_ada: Vec<(&UtxoApi, u64)> = Vec::new();

    for utxo in &selected_utxos {
        total_input_ada += utxo.lovelace;

        if !utxo.assets.is_empty() {
            let asset_ids: Vec<AssetId> = utxo.assets.iter().map(|a| a.asset_id.clone()).collect();
            let min_ada = crate::calculate_min_ada_with_params(
                &to_maestro_params(&deps.params),
                &asset_ids,
                &crate::OutputParams { datum_size: None },
            );
            total_reserved_for_assets += min_ada;
            utxo_min_ada.push((utxo, min_ada));
        }
    }

    let total_free_ada = total_input_ada.saturating_sub(total_reserved_for_assets);
    let from_address = deps.from_address.clone();
    let to_address = to_addr.clone();
    let network_id = deps.network_id;

    // Clone data needed inside the closure
    let selected_cloned: Vec<UtxoApi> = selected_utxos.iter().copied().cloned().collect();
    let utxo_min_ada_indices: Vec<(usize, u64)> = utxo_min_ada
        .iter()
        .map(|(utxo, min_ada)| {
            let idx = selected_cloned
                .iter()
                .position(|u| u.tx_hash == utxo.tx_hash && u.output_index == utxo.output_index)
                .unwrap();
            (idx, *min_ada)
        })
        .collect();

    // Rough fee includes per-input overhead
    let rough_fee =
        selection::estimate_simple_fee(&deps.params) + (selected_cloned.len() as u64 * 50_000);

    converge_fee(
        |fee| {
            let send_amount =
                total_free_ada
                    .checked_sub(fee)
                    .ok_or(TxBuildError::InsufficientFunds {
                        needed: fee,
                        available: total_free_ada,
                    })?;

            let refs: Vec<&UtxoApi> = selected_cloned.iter().collect();
            let mut tx = add_utxo_inputs(StagingTransaction::new(), &refs)?;
            tx = tx.output(create_ada_output(to_address.clone(), send_amount));

            // Add change outputs for UTxOs with native assets
            for (idx, min_ada) in &utxo_min_ada_indices {
                let change_output = build_change_output(
                    from_address.clone(),
                    *min_ada,
                    &[&selected_cloned[*idx]],
                    None,
                )?;
                tx = tx.output(change_output);
            }

            Ok(tx.fee(fee).network_id(network_id))
        },
        rough_fee,
        &deps.params,
    )
}

/// Build a transaction sending native assets to a recipient.
///
/// Finds UTxOs containing the requested assets, selects additional UTxOs if needed
/// for fees, and builds recipient + change outputs.
pub fn build_send_assets(
    deps: &TxDeps,
    to_addr: &Address,
    assets: &[AssetId],
) -> Result<UnsignedTx, TxBuildError> {
    if assets.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "No assets specified to send".to_string(),
        ));
    }

    // 1. Find UTxOs containing each requested asset
    let assets_to_send: HashSet<AssetId> = assets.iter().cloned().collect();
    let mut input_utxos = find_utxos_for_assets(assets, &deps.utxos)?;

    // 2. Calculate totals
    let mut input_lovelace: u64 = input_utxos.iter().map(|u| u.lovelace).sum();

    // 3. Calculate minimum ADA for output with assets
    let min_ada_for_assets = crate::calculate_min_ada_with_params(
        &to_maestro_params(&deps.params),
        assets,
        &crate::OutputParams { datum_size: None },
    );

    // 4. Build recipient output with assets
    let recipient_output = build_recipient_output_with_assets(to_addr, min_ada_for_assets, assets)?;

    // 5. Calculate min ADA for change output
    let input_refs: Vec<&UtxoApi> = input_utxos.to_vec();
    let change_asset_ids = collect_asset_ids(&input_refs, Some(&assets_to_send));
    let mut min_ada_for_change = crate::calculate_min_ada_with_params(
        &to_maestro_params(&deps.params),
        &change_asset_ids,
        &crate::OutputParams { datum_size: None },
    );

    // 7. Rough fee estimate
    let rough_fee =
        selection::estimate_simple_fee(&deps.params) + (input_utxos.len() as u64 * 50_000);

    // 8. Check if we need additional UTxOs for fees
    let total_required = min_ada_for_assets
        .saturating_add(min_ada_for_change)
        .saturating_add(rough_fee);

    if input_lovelace < total_required {
        let shortfall = total_required - input_lovelace;
        let (additional_lovelace, additional_utxos) =
            select_additional_utxos_for_shortfall(&deps.utxos, &input_utxos, shortfall)?;

        input_utxos.extend(additional_utxos);
        input_lovelace += additional_lovelace;

        // Recalculate change with additional UTxOs
        let new_refs: Vec<&UtxoApi> = input_utxos.to_vec();
        let new_change_asset_ids = collect_asset_ids(&new_refs, Some(&assets_to_send));
        min_ada_for_change = crate::calculate_min_ada_with_params(
            &to_maestro_params(&deps.params),
            &new_change_asset_ids,
            &crate::OutputParams { datum_size: None },
        );
    }

    // 9. Clone data for closure
    let final_utxos: Vec<UtxoApi> = input_utxos.into_iter().cloned().collect();
    let final_change_output = build_change_output(
        deps.from_address.clone(),
        0,
        &final_utxos.iter().collect::<Vec<_>>(),
        Some(&assets_to_send),
    )?;
    let network_id = deps.network_id;

    // 10. Two-round fee convergence
    converge_fee(
        |fee| {
            let refs: Vec<&UtxoApi> = final_utxos.iter().collect();
            let mut tx = add_utxo_inputs(StagingTransaction::new(), &refs)?;

            // Calculate change lovelace
            let remaining = input_lovelace - min_ada_for_assets - fee;
            let change = remaining.max(min_ada_for_change);

            let mut co = final_change_output.clone();
            co.lovelace = change;

            tx = tx
                .output(recipient_output.clone())
                .output(co)
                .fee(fee)
                .network_id(network_id);

            Ok(tx)
        },
        rough_fee,
        &deps.params,
    )
}

/// Build a consolidation transaction.
///
/// Packs all native assets from consumed inputs into one output with minimum ADA.
/// Remaining pure ADA goes into a second output. Both outputs go back to self.
pub fn build_consolidate(deps: &TxDeps, max_inputs: u32) -> Result<UnsignedTx, TxBuildError> {
    if deps.utxos.len() <= 1 {
        return Err(TxBuildError::BuildFailed(
            "Nothing to consolidate — wallet has 1 or fewer UTxOs".to_string(),
        ));
    }

    // Sort by lovelace descending, take up to max_inputs
    let mut sorted: Vec<&UtxoApi> = deps.utxos.iter().collect();
    sorted.sort_by_key(|u| std::cmp::Reverse(u.lovelace));
    let selected: Vec<UtxoApi> = sorted
        .iter()
        .take(max_inputs as usize)
        .map(|u| (*u).clone())
        .collect();

    let refs: Vec<&UtxoApi> = selected.iter().collect();
    let all_native_assets = collect_utxo_native_assets(&refs, None);
    let total_lovelace = total_utxo_lovelace(&refs);
    let has_native_assets = !all_native_assets.is_empty();

    // Calculate min ADA for asset output
    let asset_ids = collect_asset_ids(&refs, None);
    let min_ada_for_assets = if has_native_assets {
        crate::calculate_min_ada_with_params(
            &to_maestro_params(&deps.params),
            &asset_ids,
            &crate::OutputParams { datum_size: None },
        )
    } else {
        0
    };

    let from_address = deps.from_address.clone();
    let network_id = deps.network_id;

    let rough_fee = selection::estimate_simple_fee(&deps.params) + (selected.len() as u64 * 50_000);

    converge_fee(
        |fee| {
            let r: Vec<&UtxoApi> = selected.iter().collect();
            let mut tx = add_utxo_inputs(StagingTransaction::new(), &r)?;

            if has_native_assets {
                // Output 1: all native assets + min ADA (to self)
                let asset_output = build_change_output(
                    from_address.clone(),
                    min_ada_for_assets,
                    &r,
                    None, // include all assets
                )?;
                tx = tx.output(asset_output);

                // Output 2: remaining pure ADA (to self)
                let remaining_ada = total_lovelace
                    .checked_sub(min_ada_for_assets)
                    .and_then(|v| v.checked_sub(fee))
                    .ok_or(TxBuildError::InsufficientFunds {
                        needed: min_ada_for_assets + fee,
                        available: total_lovelace,
                    })?;

                if remaining_ada > 0 {
                    tx = tx.output(create_ada_output(from_address.clone(), remaining_ada));
                }
            } else {
                // No native assets — single output with all ADA minus fee
                let send_amount =
                    total_lovelace
                        .checked_sub(fee)
                        .ok_or(TxBuildError::InsufficientFunds {
                            needed: fee,
                            available: total_lovelace,
                        })?;
                tx = tx.output(create_ada_output(from_address.clone(), send_amount));
            }

            Ok(tx.fee(fee).network_id(network_id))
        },
        rough_fee,
        &deps.params,
    )
}

/// Build a fan-out (refuel) transaction.
///
/// Sweeps the wallet's pure-ADA UTxOs (skipping anything carrying a
/// script reference or native assets — those have their own purpose)
/// and emits `slot_count` outputs of `slot_size` lovelace each back to
/// `deps.from_address`, plus a single change output for the remainder.
/// Result: a wallet primed with N parallel-spendable fuel UTxOs, ready
/// for high-throughput minting before the first mint lands.
///
/// Multi-input by design — a self-spend can sweep an arbitrary mix of
/// "bank" UTxOs (one big change UTxO, a few stragglers) into a clean
/// pool shape. Inputs are taken largest-first so a healthy wallet
/// usually only consumes one UTxO; smaller stragglers come along
/// only when needed to cover the deficit.
///
/// # Errors
///
/// - `BuildFailed` — `slot_count == 0`, or all UTxOs were filtered
///   out (every UTxO has assets or a script ref).
/// - `InsufficientFunds` — the spendable UTxOs together can't cover
///   `slot_size × slot_count + fee + min pure-change`.
///
/// Asset-bearing and `HasScriptRef` UTxOs are intentionally excluded
/// from selection — they may hold the collection's mint policy script
/// reference (CIP-68) or arbitrary native assets the operator put
/// there; refuel must not consume either. The filter is conservative:
/// it's better to fail loudly than to accidentally burn an asset.
pub fn build_fan_out(
    deps: &TxDeps,
    slot_size: u64,
    slot_count: u32,
    slot_datum: Option<Vec<u8>>,
) -> Result<UnsignedTx, TxBuildError> {
    if slot_count == 0 {
        return Err(TxBuildError::BuildFailed(
            "slot_count must be > 0 — caller should no-op when deficit is zero".to_string(),
        ));
    }

    // Filter to pure-ADA UTxOs without script refs. Largest-first so
    // healthy wallets sweep one UTxO; small stragglers join only when
    // the deficit needs them.
    let mut spendable: Vec<&UtxoApi> = deps
        .utxos
        .iter()
        .filter(|u| u.assets.is_empty() && !u.has_tag(UtxoTag::HasScriptRef))
        .collect();
    spendable.sort_by_key(|u| std::cmp::Reverse(u.lovelace));
    if spendable.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "no spendable pure-ADA UTxOs (all entries carry native assets or a script ref)"
                .to_string(),
        ));
    }

    let total_outputs_lovelace = slot_size
        .checked_mul(u64::from(slot_count))
        .ok_or_else(|| {
            TxBuildError::BuildFailed("slot_size * slot_count overflows u64".to_string())
        })?;

    // Min change cushion — the tx-builder will reject change outputs
    // below the pure-ADA min UTxO threshold (~1.1 ADA at current params).
    // Leave a 1.5 ADA buffer so converge_fee doesn't have to wrestle
    // with edge-case sub-minimum change.
    let min_change_cushion: u64 = 1_500_000;
    // Multi-input fee estimate: base + per-input overhead.
    let base_fee = selection::estimate_simple_fee(&deps.params);

    let mut selected: Vec<&UtxoApi> = Vec::new();
    let mut input_lovelace: u64 = 0;
    for u in &spendable {
        let projected_fee = base_fee + (selected.len() as u64 + 1) * 50_000;
        let needed = total_outputs_lovelace
            .saturating_add(projected_fee)
            .saturating_add(min_change_cushion);
        selected.push(u);
        input_lovelace = input_lovelace.saturating_add(u.lovelace);
        if input_lovelace >= needed {
            break;
        }
    }
    let final_fee_estimate = base_fee + (selected.len() as u64 * 50_000);
    let needed_lovelace = total_outputs_lovelace
        .saturating_add(final_fee_estimate)
        .saturating_add(min_change_cushion);
    if input_lovelace < needed_lovelace {
        return Err(TxBuildError::InsufficientFunds {
            needed: needed_lovelace,
            available: input_lovelace,
        });
    }

    let selected_cloned: Vec<UtxoApi> = selected.into_iter().cloned().collect();
    let from_address = deps.from_address.clone();
    let network_id = deps.network_id;
    let rough_fee = final_fee_estimate;

    converge_fee(
        |fee| {
            let refs: Vec<&UtxoApi> = selected_cloned.iter().collect();
            let mut tx = add_utxo_inputs(StagingTransaction::new(), &refs)?;

            // N equal fuel-slot outputs back to self, each optionally carrying an
            // inline datum tag (e.g. the engine's FUEL role marker) so the outputs
            // are self-describing on chain — distinguishable from inbound payments.
            for _ in 0..slot_count {
                let mut out = create_ada_output(from_address.clone(), slot_size);
                if let Some(datum) = &slot_datum {
                    out = out.set_inline_datum(datum.clone());
                }
                tx = tx.output(out);
            }

            // Change output for the remainder — fee converges around
            // this. Must exceed min-pure-change or the ledger rejects.
            let change = input_lovelace
                .checked_sub(total_outputs_lovelace)
                .and_then(|v| v.checked_sub(fee))
                .ok_or(TxBuildError::InsufficientFunds {
                    needed: total_outputs_lovelace + fee,
                    available: input_lovelace,
                })?;
            if change > 0 {
                tx = tx.output(create_ada_output(from_address.clone(), change));
            }

            Ok(tx.fee(fee).network_id(network_id))
        },
        rough_fee,
        &deps.params,
    )
}

/// Build a multi-recipient pure-ADA payment transaction with optional
/// transaction metadata, funded from `deps.from_address` with change
/// back to it.
///
/// The shared substrate for the **Mode B refund worker**
/// (`MINT_REFUNDS.md`) and the **settlement worker**
/// (`MINT_SETTLEMENT.md`): both pay N arbitrary addresses pure ADA and
/// tag the tx with CIP-674 (`refund:…` / `settle:…`) — neither mints,
/// so the CIP-25 mint builders don't fit.
///
/// - `outputs` — `(recipient, lovelace)`. Each amount must clear the
///   pure-ADA min UTxO (rejected before signing, like the inline-refund
///   builder), so callers filter sub-floor amounts upstream
///   (`minting_core::classify_refund`).
/// - `metadata` — an optional label→value JSON object (e.g.
///   `{ "674": { "msg": [...] } }`) emitted verbatim via
///   [`build_metadata_auxiliary_data`](crate::metadata::cip25::build_metadata_auxiliary_data).
///   `None` for a bare payment.
///
/// Funding mirrors [`build_fan_out`]: pure-ADA UTxOs only (asset-bearing
/// and script-ref entries are excluded so a refund/settlement tx never
/// consumes a policy script ref or native tokens), largest-first, with
/// two-round fee convergence around the change output. The
/// `min_change_cushion` guarantees any emitted change clears the pure-ADA
/// minimum.
pub fn build_send_many(
    deps: &TxDeps,
    outputs: &[(Address, u64)],
    metadata: Option<&serde_json::Value>,
) -> Result<UnsignedTx, TxBuildError> {
    if outputs.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "build_send_many: no outputs".to_string(),
        ));
    }

    // Each output must clear the pure-ADA min UTxO or the ledger rejects
    // after the fee is paid.
    let min_pure_utxo = 228 * deps.params.coins_per_utxo_byte;
    for (i, (_addr, amount)) in outputs.iter().enumerate() {
        if *amount < min_pure_utxo {
            return Err(TxBuildError::BuildFailed(format!(
                "build_send_many: outputs[{i}] = {amount} lovelace < min_pure_utxo {min_pure_utxo}"
            )));
        }
    }

    // Pre-encode metadata so the fee is sized from its real weight (the
    // converge closure re-attaches the same bytes each round).
    let metadata_bytes = match metadata {
        Some(v) => Some(
            crate::metadata::cip25::build_metadata_auxiliary_data(v)
                .map_err(|e| TxBuildError::BuildFailed(format!("metadata encoding failed: {e}")))?,
        ),
        None => None,
    };

    // Spendable pure-ADA UTxOs only (no assets, no script ref), largest-first.
    let mut spendable: Vec<&UtxoApi> = deps
        .utxos
        .iter()
        .filter(|u| u.assets.is_empty() && !u.has_tag(UtxoTag::HasScriptRef))
        .collect();
    spendable.sort_by_key(|u| std::cmp::Reverse(u.lovelace));
    if spendable.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "no spendable pure-ADA UTxOs (all entries carry native assets or a script ref)"
                .to_string(),
        ));
    }

    let total_outputs_lovelace: u64 = outputs.iter().map(|(_, l)| *l).sum();
    let min_change_cushion: u64 = 1_500_000;
    let base_fee = selection::estimate_simple_fee(&deps.params)
        + metadata_bytes.as_ref().map_or(0, |b| b.len() as u64);

    let mut selected: Vec<&UtxoApi> = Vec::new();
    let mut input_lovelace: u64 = 0;
    for u in &spendable {
        let projected_fee = base_fee + (selected.len() as u64 + 1) * 50_000;
        let needed = total_outputs_lovelace
            .saturating_add(projected_fee)
            .saturating_add(min_change_cushion);
        selected.push(u);
        input_lovelace = input_lovelace.saturating_add(u.lovelace);
        if input_lovelace >= needed {
            break;
        }
    }
    let final_fee_estimate = base_fee + (selected.len() as u64 * 50_000);
    let needed_lovelace = total_outputs_lovelace
        .saturating_add(final_fee_estimate)
        .saturating_add(min_change_cushion);
    if input_lovelace < needed_lovelace {
        return Err(TxBuildError::InsufficientFunds {
            needed: needed_lovelace,
            available: input_lovelace,
        });
    }

    let selected_cloned: Vec<UtxoApi> = selected.into_iter().cloned().collect();
    let outputs_cloned: Vec<(Address, u64)> = outputs.to_vec();
    let from_address = deps.from_address.clone();
    let network_id = deps.network_id;

    converge_fee(
        |fee| {
            let refs: Vec<&UtxoApi> = selected_cloned.iter().collect();
            let mut tx = add_utxo_inputs(StagingTransaction::new(), &refs)?;

            for (addr, amount) in &outputs_cloned {
                tx = tx.output(create_ada_output(addr.clone(), *amount));
            }
            if let Some(bytes) = &metadata_bytes {
                tx = tx.add_auxiliary_data(bytes.clone());
            }

            // Change back to self; fee converges around it. The cushion
            // keeps it above the pure-ADA minimum.
            let change = input_lovelace
                .checked_sub(total_outputs_lovelace)
                .and_then(|v| v.checked_sub(fee))
                .ok_or(TxBuildError::InsufficientFunds {
                    needed: total_outputs_lovelace + fee,
                    available: input_lovelace,
                })?;
            if change > 0 {
                tx = tx.output(create_ada_output(from_address.clone(), change));
            }

            Ok(tx.fee(fee).network_id(network_id))
        },
        final_fee_estimate,
        &deps.params,
    )
}

/// One split tx in the parcel-funding chain (`MINT_PARCEL_FUNDING.md`): carve
/// `source` — the buyer payment, or the prior split's change — into `parcel_count`
/// equal pure-ADA parcels back to self, plus an optional buyer `refund` output
/// (CIP-674-tagged via `metadata`).
///
/// `source` is ALWAYS the first input, so the funding chain is traceable
/// (payment → split → parcels) and never silently funded from float instead.
/// EVERY `extra_float` UTxO is spent and folds into the change — the caller passes
/// a bounded batch of small wallet UTxOs to ROLL UP dust into this tx's single
/// change output (N dust → 1) for free; pass `&[]` for none. (Caller bounds the
/// batch to keep the tx under the size limit.)
///
/// No change cushion is reserved: when the leftover after parcels + fee is below
/// the min-UTxO floor (the self-funding case — the caller carved the payment into
/// parcels holding back only the fee), it's **folded into the last parcel** so no
/// standalone change UTxO is needed and the payment funds its own split with zero
/// operator float. A large leftover (an intermediate split, whose change funds the
/// next batch) is emitted as a normal change output. Returns that change's
/// tx-relative index + value ([`ChangeOutput`]) so the next split chains off it
/// deterministically (the tx hash is fixed the moment it's signed); `lovelace` is
/// `0` when the leftover was folded (a final split with no chainable change).
#[allow(clippy::too_many_arguments)]
pub fn build_parcel_split(
    deps: &TxDeps,
    source: &UtxoApi,
    extra_float: &[UtxoApi],
    float_pool: &[UtxoApi],
    parcel_size: u64,
    parcel_count: u32,
    refund: Option<(&Address, u64)>,
    metadata: Option<&serde_json::Value>,
) -> Result<(UnsignedTx, super::mint::ChangeOutput), TxBuildError> {
    if parcel_count == 0 {
        return Err(TxBuildError::BuildFailed(
            "build_parcel_split: parcel_count must be > 0".to_string(),
        ));
    }
    let min_pure_utxo = 228 * deps.params.coins_per_utxo_byte;
    if parcel_size < min_pure_utxo {
        return Err(TxBuildError::BuildFailed(format!(
            "build_parcel_split: parcel_size {parcel_size} < min_pure_utxo {min_pure_utxo}"
        )));
    }
    if let Some((_, amt)) = refund {
        if amt < min_pure_utxo {
            return Err(TxBuildError::BuildFailed(format!(
                "build_parcel_split: refund {amt} < min_pure_utxo {min_pure_utxo}"
            )));
        }
    }

    // Pre-encode metadata so the fee is sized from its real weight.
    let metadata_bytes = match metadata {
        Some(v) => Some(
            crate::metadata::cip25::build_metadata_auxiliary_data(v)
                .map_err(|e| TxBuildError::BuildFailed(format!("metadata encoding failed: {e}")))?,
        ),
        None => None,
    };

    let parcels_lovelace = parcel_size
        .checked_mul(u64::from(parcel_count))
        .ok_or_else(|| {
            TxBuildError::BuildFailed("parcel_size * parcel_count overflows u64".to_string())
        })?;
    let refund_lovelace = refund.map_or(0, |(_, l)| l);
    let total_outputs_lovelace = parcels_lovelace.saturating_add(refund_lovelace);

    // Inputs: `source` ALWAYS first (the buyer payment / prior split's change —
    // spent so the funding chain stays traceable), then ALL of `extra_float`. Every
    // provided float UTxO is SPENT and folds into the change. The caller uses this
    // to ROLL UP dust: it passes a bounded batch of small wallet UTxOs and they
    // collapse into this tx's single change output (N dust → 1) for FREE — no extra
    // tx, no extra scan, riding a split we're building anyway. The caller bounds the
    // batch so the tx stays under the size limit. Pure-ADA only, no script ref,
    // never the source itself.
    //
    // NO change cushion is reserved: when the post-parcel remainder is below the
    // min-UTxO floor (the self-funding case with no dust to absorb), it's folded
    // into the LAST parcel rather than forced out as a standalone change UTxO —
    // so a payment self-funds its split with no operator float. A larger remainder
    // (chain-link change, or absorbed dust) clears the floor and is emitted.
    let base_fee = selection::estimate_simple_fee(&deps.params)
        + metadata_bytes.as_ref().map_or(0, |b| b.len() as u64);
    let mut selected: Vec<UtxoApi> = vec![source.clone()];
    selected.extend(
        extra_float
            .iter()
            .filter(|u| {
                u.assets.is_empty()
                    && !u.has_tag(UtxoTag::HasScriptRef)
                    && !(u.tx_hash == source.tx_hash && u.output_index == source.output_index)
            })
            .cloned(),
    );
    let mut input_lovelace: u64 = selected.iter().map(|u| u.lovelace).sum();

    let from_address = deps.from_address.clone();
    let network_id = deps.network_id;
    let refund_owned: Option<(Address, u64)> = refund.map(|(a, l)| (a.clone(), l));

    // A sub-floor leftover (input − parcels − fee) can't form a valid change UTxO,
    // so it's absorbed into the fee (the self-funding final split); a larger
    // leftover clears the floor and is emitted as a normal chain-link change.
    let emit_change_floor = min_pure_utxo.saturating_add(200_000);

    // v2 coin selection (CARDANO_TX_BUILDER_V2): when the must-spend inputs (the
    // `source` + any `extra_float` dust) can't cover parcels + fee + a valid change,
    // draw the shortfall from `float_pool`. This is the fragmented-float case for
    // zero-cost splits — when the largest single float UTxO is only ~parcels-worth,
    // the old self-funding math floored the fee to 0; selection pulls additional
    // float so the fee converges normally. An EMPTY pool (a PAID order, which
    // self-funds from its own payment) is a no-op. Fails cleanly with
    // `InsufficientFunds` if the pool can't reach the target, so the caller defers
    // instead of submitting an underpaying tx.
    let select_target = total_outputs_lovelace
        .saturating_add(base_fee + (selected.len() as u64 * 50_000))
        .saturating_add(emit_change_floor);
    if input_lovelace < select_target && !float_pool.is_empty() {
        let new_selected: Vec<UtxoApi> = {
            let exclude: std::collections::HashSet<(String, u32)> = selected
                .iter()
                .map(|u| (u.tx_hash.clone(), u.output_index))
                .collect();
            let must: Vec<&UtxoApi> = selected.iter().collect();
            let sel = crate::select::Selection {
                must_spend: must,
                pool: float_pool,
                exclude: &exclude,
                strategy: crate::select::Strategy::SmallestSufficient,
            };
            crate::select::select(&sel, select_target)
                .map_err(|crate::select::SelectError::Insufficient { target, available }| {
                    TxBuildError::InsufficientFunds { needed: target, available }
                })?
                .into_iter()
                .cloned()
                .collect()
        };
        selected = new_selected;
        input_lovelace = selected.iter().map(|u| u.lovelace).sum();
    }

    // Fee estimate AFTER selection (the input count is now final).
    let final_fee_estimate = base_fee + (selected.len() as u64 * 50_000);
    let est_remainder = input_lovelace
        .saturating_sub(total_outputs_lovelace)
        .saturating_sub(final_fee_estimate);
    let emit_change = est_remainder >= emit_change_floor;

    let unsigned = if emit_change {
        // Chain-link change: converge the fee around a normal change output back
        // to self (the next split spends it).
        converge_fee(
            |fee| {
                let refs: Vec<&UtxoApi> = selected.iter().collect();
                let mut tx = add_utxo_inputs(StagingTransaction::new(), &refs)?;
                for _ in 0..parcel_count {
                    tx = tx.output(create_ada_output(from_address.clone(), parcel_size));
                }
                if let Some((addr, lov)) = &refund_owned {
                    tx = tx.output(create_ada_output(addr.clone(), *lov));
                }
                if let Some(bytes) = &metadata_bytes {
                    tx = tx.add_auxiliary_data(bytes.clone());
                }
                let change = input_lovelace
                    .checked_sub(total_outputs_lovelace)
                    .and_then(|v| v.checked_sub(fee))
                    .ok_or(TxBuildError::InsufficientFunds {
                        needed: total_outputs_lovelace + fee,
                        available: input_lovelace,
                    })?;
                if change > 0 {
                    tx = tx.output(create_ada_output(from_address.clone(), change));
                }
                Ok(tx.fee(fee).network_id(network_id))
            },
            final_fee_estimate,
            &deps.params,
        )?
    } else {
        // Self-funding final split: the sub-floor leftover is too small for a
        // standalone change UTxO, so there is NO change output and the leftover
        // is simply paid as fee (`fee = input − parcels − refund`). Crucially the
        // parcels stay UNIFORM, so the recorded parcel size matches the on-chain
        // UTxO and the mint funds from it exactly. The fee equals the caller's
        // conservative split-fee reserve (it sized the parcels to leave exactly
        // this), which is ≥ the real min fee, so the tx never underpays — the
        // small excess (reserve − real fee) is the only cost, paid to the chain.
        //
        // GUARD: this fee is `input − parcels` taken DIRECTLY (no converge), so it's
        // only valid when the source genuinely left fee headroom. If the inputs
        // can't cover parcels + the min fee, it would otherwise floor below the
        // minimum (the `fee=0` / `ValueNotConservedUTxO` reject seen on fragmented
        // zero-cost float). Fail cleanly instead — the caller defers / sources
        // differently rather than submitting an underpaying tx.
        if input_lovelace < total_outputs_lovelace.saturating_add(base_fee) {
            return Err(TxBuildError::InsufficientFunds {
                needed: total_outputs_lovelace + base_fee,
                available: input_lovelace,
            });
        }
        let fee = input_lovelace.saturating_sub(total_outputs_lovelace);
        let refs: Vec<&UtxoApi> = selected.iter().collect();
        let mut tx = add_utxo_inputs(StagingTransaction::new(), &refs)?;
        for _ in 0..parcel_count {
            tx = tx.output(create_ada_output(from_address.clone(), parcel_size));
        }
        if let Some((addr, lov)) = &refund_owned {
            tx = tx.output(create_ada_output(addr.clone(), *lov));
        }
        if let Some(bytes) = &metadata_bytes {
            tx = tx.add_auxiliary_data(bytes.clone());
        }
        UnsignedTx {
            staging: tx.fee(fee).network_id(network_id),
            fee,
        }
    };

    // Change ref = the chain-link change UTxO (after the parcels + the optional
    // refund) when one was emitted; `0` lovelace when the leftover was absorbed
    // into the fee (a final, self-funded split with no chainable change).
    let change_index = parcel_count + u32::from(refund.is_some());
    let change_lovelace = if emit_change {
        input_lovelace
            .saturating_sub(total_outputs_lovelace)
            .saturating_sub(unsigned.fee)
    } else {
        0
    };
    let change = super::mint::ChangeOutput {
        output_index: change_index,
        lovelace: change_lovelace,
        has_assets: false,
    };
    Ok((unsigned, change))
}

// ============================================================================
// Helpers
// ============================================================================

/// Find UTxOs containing the requested assets.
fn find_utxos_for_assets<'a>(
    assets: &[AssetId],
    utxos: &'a [UtxoApi],
) -> Result<Vec<&'a UtxoApi>, TxBuildError> {
    let mut utxo_refs: Vec<(&str, u32)> = Vec::new();

    for asset in assets {
        let concatenated = asset.concatenated();
        let utxo = utxos
            .iter()
            .find(|u| {
                u.assets
                    .iter()
                    .any(|a| a.asset_id.concatenated() == concatenated)
            })
            .ok_or(TxBuildError::AssetNotFound(concatenated))?;

        utxo_refs.push((&utxo.tx_hash, utxo.output_index));
    }

    // Dedupe
    utxo_refs.sort();
    utxo_refs.dedup();

    let mut result = Vec::new();
    for (tx_hash, index) in &utxo_refs {
        let utxo = utxos
            .iter()
            .find(|u| u.tx_hash == *tx_hash && u.output_index == *index)
            .ok_or_else(|| {
                TxBuildError::BuildFailed(format!("UTxO {tx_hash}:{index} not found"))
            })?;
        result.push(utxo);
    }

    Ok(result)
}

/// Select additional UTxOs to cover a shortfall.
fn select_additional_utxos_for_shortfall<'a>(
    all_utxos: &'a [UtxoApi],
    already_selected: &[&UtxoApi],
    shortfall: u64,
) -> Result<(u64, Vec<&'a UtxoApi>), TxBuildError> {
    let used: HashSet<(&str, u32)> = already_selected
        .iter()
        .map(|u| (u.tx_hash.as_str(), u.output_index))
        .collect();

    let mut candidates: Vec<&UtxoApi> = all_utxos
        .iter()
        .filter(|u| !used.contains(&(u.tx_hash.as_str(), u.output_index)))
        .collect();

    // Sort by lovelace descending
    candidates.sort_by_key(|u| std::cmp::Reverse(u.lovelace));

    let mut additional_lovelace = 0u64;
    let mut selected = Vec::new();

    for utxo in candidates {
        if additional_lovelace >= shortfall {
            break;
        }
        additional_lovelace += utxo.lovelace;
        selected.push(utxo);
    }

    if additional_lovelace < shortfall {
        return Err(TxBuildError::InsufficientFunds {
            needed: shortfall,
            available: additional_lovelace,
        });
    }

    Ok((additional_lovelace, selected))
}

/// Build a recipient output containing specific assets at quantity 1.
fn build_recipient_output_with_assets(
    to_addr: &Address,
    lovelace: u64,
    assets: &[AssetId],
) -> Result<pallas_txbuilder::Output, TxBuildError> {
    let mut output = create_ada_output(to_addr.clone(), lovelace);

    for asset in assets {
        let policy_bytes = crate::helpers::decode::decode_policy_id(asset.policy_id())?;
        let asset_name_bytes = crate::helpers::decode::decode_asset_name(asset.asset_name_hex());

        output = output
            .add_asset(
                pallas_crypto::hash::Hash::from(policy_bytes),
                asset_name_bytes,
                1,
            )
            .map_err(|e| {
                TxBuildError::BuildFailed(format!("Failed to add asset to output: {e}"))
            })?;
    }

    Ok(output)
}

/// Convert TxBuildParams back to maestro ProtocolParameters for min_ada calculation.
/// (Temporary bridge until calculate_min_ada is refactored to accept TxBuildParams)
pub(crate) fn to_maestro_params(
    params: &crate::params::TxBuildParams,
) -> maestro::ProtocolParameters {
    maestro::ProtocolParameters {
        min_fee_coefficient: params.min_fee_coefficient,
        min_fee_constant: maestro::AdaLovelace {
            ada: maestro::AdaAmount {
                lovelace: params.min_fee_constant,
            },
        },
        min_utxo_deposit_coefficient: params.coins_per_utxo_byte,
        script_execution_prices: None,
        max_execution_units_per_transaction: None,
        max_transaction_size: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::{AssetQuantity, UtxoApi};

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

    fn make_utxo(lovelace: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: format!("{:064x}", lovelace), // unique per lovelace amount
            output_index: 0,
            lovelace,
            assets: vec![],
            tags: vec![],
        }
    }

    fn make_utxo_with_asset(lovelace: u64, policy: &str, name: &str, qty: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: format!("{:064x}", lovelace + 1000),
            output_index: 0,
            lovelace,
            assets: vec![AssetQuantity {
                asset_id: AssetId::new(policy.to_string(), name.to_string()).unwrap(),
                quantity: qty,
            }],
            tags: vec![],
        }
    }

    #[test]
    fn test_build_send_lovelace_simple() {
        let deps = TxDeps {
            utxos: vec![make_utxo(10_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };

        let result = build_send_lovelace(&deps, &test_address(), 2_000_000);
        assert!(result.is_ok(), "build_send_lovelace failed: {result:?}");

        let unsigned = result.unwrap();
        assert!(unsigned.fee > 0);
        assert!(unsigned.fee < 500_000);
    }

    #[test]
    fn test_build_send_lovelace_insufficient() {
        let deps = TxDeps {
            utxos: vec![make_utxo(1_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };

        let result = build_send_lovelace(&deps, &test_address(), 5_000_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_send_max_pure_ada() {
        let deps = TxDeps {
            utxos: vec![make_utxo(5_000_000), make_utxo(3_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };

        let result = build_send_max(&deps, &test_address());
        assert!(result.is_ok(), "build_send_max failed: {result:?}");

        let unsigned = result.unwrap();
        assert!(unsigned.fee > 0);
    }

    #[test]
    fn test_build_consolidate_pure_ada() {
        let deps = TxDeps {
            utxos: vec![make_utxo(5_000_000), make_utxo(3_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };

        let result = build_consolidate(&deps, 80);
        assert!(result.is_ok(), "build_consolidate failed: {result:?}");

        let unsigned = result.unwrap();
        assert!(unsigned.fee > 0);
    }

    #[test]
    fn test_build_consolidate_single_utxo_errors() {
        let deps = TxDeps {
            utxos: vec![make_utxo(5_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };

        let result = build_consolidate(&deps, 80);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_utxos_for_assets_found() {
        let policy = "a".repeat(56);
        let utxos = vec![
            make_utxo(5_000_000),
            make_utxo_with_asset(3_000_000, &policy, "4e4654", 1),
        ];

        let asset_id = AssetId::new(policy, "4e4654".to_string()).unwrap();
        let result = find_utxos_for_assets(&[asset_id], &utxos);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_find_utxos_for_assets_not_found() {
        let utxos = vec![make_utxo(5_000_000)];
        let policy = "b".repeat(56);
        let asset_id = AssetId::new(policy, "4e4654".to_string()).unwrap();

        let result = find_utxos_for_assets(&[asset_id], &utxos);
        assert!(result.is_err());
    }

    fn make_script_ref_utxo(lovelace: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: format!("{:064x}", lovelace + 9_000_000),
            output_index: 0,
            lovelace,
            assets: vec![],
            tags: vec![cardano_assets::UtxoTag::HasScriptRef],
        }
    }

    #[test]
    fn test_build_fan_out_splits_one_bank_utxo() {
        // One 200 ADA "bank" UTxO → fan out into 5 × 10 ADA slots + change.
        let deps = TxDeps {
            utxos: vec![make_utxo(200_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let result = build_fan_out(&deps, 10_000_000, 5, None);
        assert!(result.is_ok(), "build_fan_out failed: {result:?}");
        let unsigned = result.unwrap();
        assert!(unsigned.fee > 0);
        // 5 fuel slots + 1 change output.
        assert_eq!(unsigned.staging.outputs.as_ref().map(|o| o.len()), Some(6));
    }

    #[test]
    fn test_build_fan_out_sweeps_multiple_inputs() {
        // No single UTxO covers 5 × 10 ADA + fee + change; the sweep
        // must pull in several stragglers.
        let deps = TxDeps {
            utxos: vec![
                make_utxo(30_000_000),
                make_utxo(25_000_000),
                make_utxo(20_000_000),
            ],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let result = build_fan_out(&deps, 10_000_000, 5, None);
        assert!(result.is_ok(), "multi-input fan_out failed: {result:?}");
        let unsigned = result.unwrap();
        assert!(
            unsigned
                .staging
                .inputs
                .as_ref()
                .map(|i| i.len())
                .unwrap_or(0)
                >= 2
        );
    }

    #[test]
    fn test_build_fan_out_skips_script_ref_and_assets() {
        // The script-ref UTxO + the asset-bearing UTxO must be left
        // untouched; only the bare pure-ADA UTxO is spendable, and it
        // can't cover the deficit → InsufficientFunds.
        let policy = "a".repeat(56);
        let deps = TxDeps {
            utxos: vec![
                make_script_ref_utxo(50_000_000),
                make_utxo_with_asset(50_000_000, &policy, "4e4654", 1),
                make_utxo(5_000_000),
            ],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let result = build_fan_out(&deps, 10_000_000, 5, None);
        assert!(
            matches!(result, Err(TxBuildError::InsufficientFunds { .. })),
            "expected InsufficientFunds (script-ref + asset UTxOs excluded), got {result:?}"
        );
    }

    #[test]
    fn test_build_fan_out_zero_slots_errors() {
        let deps = TxDeps {
            utxos: vec![make_utxo(100_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        assert!(build_fan_out(&deps, 10_000_000, 0, None).is_err());
    }

    // ── build_parcel_split (parcel funding + chaining) ────────────────

    #[test]
    fn test_build_parcel_split_carves_source_and_returns_change() {
        // 200 ADA payment → 5 × 10 ADA parcels + change; source alone covers it.
        let deps = TxDeps {
            utxos: vec![],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let source = make_utxo(200_000_000);
        let (unsigned, change) =
            build_parcel_split(&deps, &source, &[], &[], 10_000_000, 5, None, None).unwrap();
        // 5 parcels + 1 change output, single input (the source).
        assert_eq!(unsigned.staging.outputs.as_ref().map(|o| o.len()), Some(6));
        assert_eq!(unsigned.staging.inputs.as_ref().map(|i| i.len()), Some(1));
        // Change is the last output; value = 200 − 50 − fee (≈ 149.7 ADA).
        assert_eq!(change.output_index, 5);
        assert!(change.lovelace > 149_000_000 && change.lovelace < 150_000_000);
        // The returned change value reconciles exactly: input − parcels − fee.
        assert_eq!(change.lovelace, 200_000_000 - 50_000_000 - unsigned.fee);
    }

    #[test]
    fn test_build_parcel_split_always_spends_source() {
        // Source is SMALL (15 ADA) but a much bigger float (500 ADA) exists. A
        // greedy builder would fund from the 500 ADA alone (1 input); the parcel
        // split MUST spend the source for traceability → source + float = 2 inputs.
        let deps = TxDeps {
            utxos: vec![],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let source = make_utxo(15_000_000);
        let float = vec![make_utxo(500_000_000)];
        let (unsigned, _change) =
            build_parcel_split(&deps, &source, &float, &[], 10_000_000, 3, None, None).unwrap();
        assert_eq!(
            unsigned.staging.inputs.as_ref().map(|i| i.len()),
            Some(2),
            "source must be spent alongside the float, not skipped for the bigger UTxO"
        );
    }

    #[test]
    fn test_build_parcel_split_with_refund_and_metadata() {
        // 3 parcels + a buyer refund output + CIP-674 tag. Change index sits after
        // the parcels AND the refund.
        let deps = TxDeps {
            utxos: vec![],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let source = make_utxo(200_000_000);
        let buyer = other_address();
        let mut cip674 = serde_json::Map::new();
        cip674.insert(
            "msg".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("refund:abc".to_string())]),
        );
        let mut root = serde_json::Map::new();
        root.insert("674".to_string(), serde_json::Value::Object(cip674));
        let meta = serde_json::Value::Object(root);
        let (unsigned, change) = build_parcel_split(
            &deps,
            &source,
            &[],
            &[],
            10_000_000,
            3,
            Some((&buyer, 20_000_000)),
            Some(&meta),
        )
        .unwrap();
        // 3 parcels + 1 refund + 1 change = 5 outputs; change after parcels+refund.
        assert_eq!(unsigned.staging.outputs.as_ref().map(|o| o.len()), Some(5));
        assert_eq!(change.output_index, 4);
        assert!(
            unsigned.staging.auxiliary_data.is_some(),
            "CIP-674 aux data present"
        );
    }

    #[test]
    fn test_build_parcel_split_self_funds_absorbing_dust_into_fee() {
        // The self-funding case: the source covers the parcels with only a
        // sub-floor remainder left over (the caller carved the payment into
        // parcels holding back just the fee reserve). No standalone change UTxO is
        // emitted — the leftover is paid as fee — so the split needs NO operator
        // float, and the parcels stay UNIFORM so each recorded size matches the
        // on-chain UTxO (the mint funds from it exactly).
        let deps = TxDeps {
            utxos: vec![],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        // 5 × 10 ADA parcels = 50 ADA; source is 50 ADA + 0.8 ADA — enough for the
        // parcels + fee, leaving a sub-floor remainder (< min-UTxO).
        let source = make_utxo(50_800_000);
        let (unsigned, change) =
            build_parcel_split(&deps, &source, &[], &[], 10_000_000, 5, None, None).unwrap();
        // 5 parcels, NO change output (leftover absorbed into fee), single input.
        assert_eq!(
            unsigned.staging.outputs.as_ref().map(|o| o.len()),
            Some(5),
            "the sub-floor remainder must be absorbed into the fee, not a 6th change output"
        );
        assert_eq!(unsigned.staging.inputs.as_ref().map(|i| i.len()), Some(1));
        // No chainable change — this is a final, self-funded split.
        assert_eq!(change.lovelace, 0);
        // Parcels stay UNIFORM (no dust folded into any output) so the recorded
        // size matches the on-chain UTxO.
        let out_sum: u64 = unsigned
            .staging
            .outputs
            .as_ref()
            .map(|os| os.iter().map(|o| o.lovelace).sum())
            .unwrap_or(0);
        assert_eq!(out_sum, 50_000_000, "5 × 10 ADA parcels, uniform");
        let all_uniform = unsigned
            .staging
            .outputs
            .as_ref()
            .map(|os| os.iter().all(|o| o.lovelace == 10_000_000))
            .unwrap_or(false);
        assert!(all_uniform, "every parcel is exactly the flat parcel size");
        // The leftover (input − parcels) is the fee; value is conserved.
        assert_eq!(unsigned.fee, 800_000);
        assert_eq!(out_sum + unsigned.fee, 50_800_000);
    }

    #[test]
    fn test_build_parcel_split_rolls_up_dust_into_change() {
        // Dust rollup: the source covers parcels + fee on its own; the extra_float
        // dust UTxOs are ALL spent and collapse into the single change output (N
        // dust → 1), no extra tx.
        let deps = TxDeps {
            utxos: vec![],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let source = make_utxo(200_000_000); // 200 ADA — covers 5×10 + fee alone
        let dust = vec![
            make_utxo(1_500_000),
            make_utxo(1_400_000),
            make_utxo(1_600_000),
        ];
        let (unsigned, change) =
            build_parcel_split(&deps, &source, &dust, &[], 10_000_000, 5, None, None).unwrap();
        // 1 source + 3 dust = 4 inputs (every dust UTxO spent).
        assert_eq!(unsigned.staging.inputs.as_ref().map(|i| i.len()), Some(4));
        // 5 parcels + 1 change.
        assert_eq!(unsigned.staging.outputs.as_ref().map(|o| o.len()), Some(6));
        assert_eq!(change.output_index, 5);
        // Change consolidates the source leftover + ALL the dust: input − parcels − fee.
        let input = 200_000_000u64 + 1_500_000 + 1_400_000 + 1_600_000;
        assert_eq!(change.lovelace, input - 50_000_000 - unsigned.fee);
    }

    #[test]
    fn test_build_parcel_split_draws_fee_shortfall_from_pool() {
        // Fragmented-float case: the `source` is only ~parcels-worth (3 × 4 ADA =
        // 12 ADA), leaving NO headroom for the split fee — the bug that floored the
        // fee to 0. With a `float_pool`, v2 selection draws an extra UTxO to cover
        // the fee + a valid change, and the source is still spent.
        let deps = TxDeps {
            utxos: vec![],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let source = make_utxo(12_000_000); // exactly 3 × 4 ADA parcels — no fee room
        let pool = vec![make_utxo(5_000_000), make_utxo(8_000_000)];
        let (unsigned, _change) =
            build_parcel_split(&deps, &source, &[], &pool, 4_000_000, 3, None, None).unwrap();
        // The fee is real (never 0) and clears the protocol minimum.
        assert!(
            unsigned.fee >= 165_000,
            "fee must be a real min fee, not floored to 0 (got {})",
            unsigned.fee
        );
        // Source is spent (always), plus at least one pool UTxO to cover the fee.
        let n_inputs = unsigned.staging.inputs.as_ref().map(|i| i.len()).unwrap_or(0);
        assert!(n_inputs >= 2, "drew the fee shortfall from the pool (got {n_inputs} inputs)");
        // 3 uniform parcels still present.
        let parcels: u64 = unsigned
            .staging
            .outputs
            .as_ref()
            .map(|os| os.iter().filter(|o| o.lovelace == 4_000_000).count() as u64)
            .unwrap_or(0);
        assert_eq!(parcels, 3, "parcels stay uniform at the flat size");
    }

    #[test]
    fn test_build_parcel_split_too_small_source_no_pool_fails_cleanly() {
        // The guard: a source that can't cover parcels + the min fee, and NO pool to
        // draw from, must return `InsufficientFunds` — NOT a fee=0 tx that the node
        // rejects (FeeTooSmallUTxO / ValueNotConservedUTxO).
        let deps = TxDeps {
            utxos: vec![],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let source = make_utxo(12_000_000); // == 3 × 4 ADA parcels, no fee headroom
        let err = build_parcel_split(&deps, &source, &[], &[], 4_000_000, 3, None, None);
        assert!(
            matches!(err, Err(TxBuildError::InsufficientFunds { .. })),
            "expected InsufficientFunds, got {err:?}"
        );
    }

    // ── build_send_many (refund / settlement payout txs) ──────────────

    /// A second distinct recipient — a script-payment enterprise address,
    /// just to exercise paying two different addresses in one tx.
    fn other_address() -> Address {
        use pallas_addresses::{
            Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart,
        };
        use pallas_crypto::hash::Hash;
        ShelleyAddress::new(
            Network::Testnet,
            ShelleyPaymentPart::Key(Hash::from([7u8; 28])),
            ShelleyDelegationPart::Null,
        )
        .into()
    }

    /// Multi-recipient payout with a CIP-674 tag: two recipient outputs +
    /// change, the recipient amounts land verbatim, and the `674` msg
    /// bytes ride in the tx's auxiliary data.
    #[test]
    fn test_build_send_many_with_metadata() {
        let deps = TxDeps {
            utxos: vec![make_utxo(100_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let outs = vec![
            (test_address(), 5_000_000u64),
            (other_address(), 6_000_000u64),
        ];
        let meta = serde_json::json!({ "674": { "msg": ["settle:abc:1"] } });

        let unsigned = build_send_many(&deps, &outs, Some(&meta)).expect("build failed");
        assert!(unsigned.fee > 0);

        let outputs = unsigned.staging.outputs.as_ref().expect("outputs");
        // 2 recipients + 1 change.
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].lovelace, 5_000_000);
        assert_eq!(outputs[1].lovelace, 6_000_000);
        assert!(outputs[2].lovelace > 0, "change back to self");

        // The CIP-674 tag is attached (auxiliary data present).
        assert!(
            unsigned.staging.auxiliary_data.is_some(),
            "metadata auxiliary data should be attached"
        );
    }

    /// No metadata → a plain multi-recipient payment (no auxiliary data).
    #[test]
    fn test_build_send_many_without_metadata() {
        let deps = TxDeps {
            utxos: vec![make_utxo(50_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let outs = vec![(test_address(), 4_000_000u64)];
        let unsigned = build_send_many(&deps, &outs, None).expect("build failed");
        assert!(unsigned.staging.auxiliary_data.is_none());
        assert_eq!(
            unsigned.staging.outputs.as_ref().unwrap()[0].lovelace,
            4_000_000
        );
    }

    /// A sub-min-UTxO output is rejected before signing.
    #[test]
    fn test_build_send_many_rejects_sub_min_output() {
        let deps = TxDeps {
            utxos: vec![make_utxo(50_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let outs = vec![(test_address(), 500_000u64)]; // 0.5 ADA < min
        assert!(matches!(
            build_send_many(&deps, &outs, None),
            Err(TxBuildError::BuildFailed(_))
        ));
    }

    /// Empty output list is rejected.
    #[test]
    fn test_build_send_many_rejects_empty() {
        let deps = TxDeps {
            utxos: vec![make_utxo(50_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        assert!(build_send_many(&deps, &[], None).is_err());
    }

    /// Underfunded wallet → InsufficientFunds.
    #[test]
    fn test_build_send_many_insufficient() {
        let deps = TxDeps {
            utxos: vec![make_utxo(5_000_000)],
            params: test_params(),
            from_address: test_address(),
            network_id: 0,
        };
        let outs = vec![
            (test_address(), 4_000_000u64),
            (other_address(), 4_000_000u64),
        ];
        assert!(matches!(
            build_send_many(&deps, &outs, None),
            Err(TxBuildError::InsufficientFunds { .. })
        ));
    }
}
