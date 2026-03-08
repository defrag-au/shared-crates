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
use crate::intents::MintingPolicy;

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
        228 * deps.params.coins_per_utxo_byte
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
fn select_utxos_for_mint<'a>(
    min_ada: u64,
    estimated_fee: u64,
    deps: &'a TxDeps,
) -> Result<(Vec<&'a UtxoApi>, u64, HashMap<(String, String), u64>), TxBuildError> {
    let selected_utxo = crate::selection::select_utxo_for_amount_prefer_pure_ada(
        &deps.utxos,
        min_ada,
        estimated_fee,
        &deps.params,
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
) -> Result<(Vec<&'a UtxoApi>, u64, HashMap<(String, String), u64>), TxBuildError> {
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
}
