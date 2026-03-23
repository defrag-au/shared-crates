//! Send transaction builders.
//!
//! Pure functions for building send-lovelace, send-max, send-assets, and
//! consolidation transactions. All operate on [`TxDeps`] and produce [`UnsignedTx`].

use cardano_assets::{AssetId, UtxoApi};
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
    let selected_utxo = selection::select_utxo_for_amount_prefer_pure_ada(
        &deps.utxos,
        amount,
        estimated_fee,
        &deps.params,
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
}
