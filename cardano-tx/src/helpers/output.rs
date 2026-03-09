//! Transaction output construction helpers

use cardano_assets::{AssetId, UtxoApi};
use pallas_addresses::Address;
use pallas_crypto::hash::Hash;
use pallas_txbuilder::Output;
use std::collections::{HashMap, HashSet};

use crate::error::TxBuildError;
use crate::helpers::decode::{decode_asset_name, decode_policy_id};

/// Create a simple output with just lovelace (no assets).
pub fn create_ada_output(address: Address, lovelace: u64) -> Output {
    Output {
        address: address.into(),
        lovelace,
        assets: None,
        datum: None,
        script: None,
    }
}

/// Add native assets to an output from a slice of `(policy_hex, asset_name_hex, quantity)`.
pub fn add_assets_to_output(
    mut output: Output,
    assets: &[(&str, &str, u64)],
) -> Result<Output, TxBuildError> {
    for (policy_hex, asset_name_hex, quantity) in assets {
        let policy_bytes = decode_policy_id(policy_hex)?;
        let asset_name_bytes = decode_asset_name(asset_name_hex);

        output = output
            .add_asset(Hash::from(policy_bytes), asset_name_bytes, *quantity)
            .map_err(|e| {
                TxBuildError::BuildFailed(format!("Failed to add asset to output: {e}"))
            })?;
    }
    Ok(output)
}

/// Add native assets to an output from a HashMap of [`AssetId`] to quantity.
pub fn add_assets_from_map(
    mut output: Output,
    assets: &HashMap<AssetId, u64>,
) -> Result<Output, TxBuildError> {
    for (asset_id, quantity) in assets {
        let policy_bytes = decode_policy_id(asset_id.policy_id())?;
        let asset_name_bytes = decode_asset_name(asset_id.asset_name_hex());

        output = output
            .add_asset(Hash::from(policy_bytes), asset_name_bytes, *quantity)
            .map_err(|e| {
                TxBuildError::BuildFailed(format!("Failed to add asset to output: {e}"))
            })?;
    }
    Ok(output)
}

/// Build a change output with native assets from UTxOs.
///
/// Collects all native assets from the provided UTxOs (excluding specified ones)
/// and adds them to a new output at the given address with the given lovelace.
pub fn build_change_output(
    address: Address,
    lovelace: u64,
    utxos: &[&UtxoApi],
    exclude: Option<&HashSet<AssetId>>,
) -> Result<Output, TxBuildError> {
    let assets = crate::helpers::utxo_query::collect_utxo_native_assets(utxos, exclude);
    let mut output = create_ada_output(address, lovelace);

    for (asset_id, quantity) in &assets {
        let policy_bytes = decode_policy_id(asset_id.policy_id())?;
        let asset_name_bytes = decode_asset_name(asset_id.asset_name_hex());

        output = output
            .add_asset(Hash::from(policy_bytes), asset_name_bytes, *quantity)
            .map_err(|e| {
                TxBuildError::BuildFailed(format!("Failed to add asset to change: {e}"))
            })?;
    }

    Ok(output)
}
