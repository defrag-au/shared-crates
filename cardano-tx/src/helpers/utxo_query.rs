//! UTxO analysis utilities
//!
//! Functions for querying and analyzing UTxOs using the canonical
//! [`UtxoApi`](cardano_assets::UtxoApi) type.

use cardano_assets::{AssetId, AssetQuantity, UtxoApi};
use std::collections::{HashMap, HashSet};

/// Check if a UTxO is pure ADA (no native assets).
pub fn is_pure_ada_utxo(utxo: &UtxoApi) -> bool {
    utxo.assets.is_empty()
}

/// Calculate total lovelace across multiple UTxOs.
pub fn total_utxo_lovelace(utxos: &[&UtxoApi]) -> u64 {
    utxos.iter().map(|u| u.lovelace).sum()
}

/// Parse a concatenated asset unit (`policy_id_hex || asset_name_hex`) into components.
///
/// Returns `(policy_bytes, asset_name_bytes)` or `None` if invalid.
pub fn parse_concatenated_asset(unit: &str) -> Option<([u8; 28], Vec<u8>)> {
    if unit.len() < 56 {
        return None;
    }

    let policy_hex = &unit[..56];
    let asset_name_hex = &unit[56..];

    let policy_bytes: [u8; 28] = hex::decode(policy_hex).ok()?.try_into().ok()?;
    let asset_name_bytes = hex::decode(asset_name_hex).ok()?;

    Some((policy_bytes, asset_name_bytes))
}

/// Collect all native assets from UTxOs into a HashMap, summing quantities.
///
/// Optionally excludes assets in the provided set.
pub fn collect_utxo_native_assets(
    utxos: &[&UtxoApi],
    exclude: Option<&HashSet<AssetId>>,
) -> HashMap<AssetId, u64> {
    let mut assets: HashMap<AssetId, u64> = HashMap::new();

    for utxo in utxos {
        for AssetQuantity { asset_id, quantity } in &utxo.assets {
            if let Some(excluded) = exclude {
                if excluded.contains(asset_id) {
                    continue;
                }
            }
            *assets.entry(asset_id.clone()).or_insert(0) += quantity;
        }
    }

    assets
}

/// Collect unique [`AssetId`]s from UTxOs, optionally excluding some.
pub fn collect_asset_ids(utxos: &[&UtxoApi], exclude: Option<&HashSet<AssetId>>) -> Vec<AssetId> {
    let mut ids = Vec::new();

    for utxo in utxos {
        for AssetQuantity { asset_id, .. } in &utxo.assets {
            if let Some(excluded) = exclude {
                if excluded.contains(asset_id) {
                    continue;
                }
            }
            if !ids.contains(asset_id) {
                ids.push(asset_id.clone());
            }
        }
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_utxo(lovelace: u64, assets: Vec<(&str, &str, u64)>) -> UtxoApi {
        UtxoApi {
            tx_hash: "a".repeat(64),
            output_index: 0,
            lovelace,
            assets: assets
                .into_iter()
                .map(|(policy, name, qty)| AssetQuantity {
                    asset_id: AssetId::new(policy.to_string(), name.to_string()).unwrap(),
                    quantity: qty,
                })
                .collect(),
            tags: vec![],
        }
    }

    #[test]
    fn test_is_pure_ada() {
        let pure = UtxoApi {
            tx_hash: "a".repeat(64),
            output_index: 0,
            lovelace: 5_000_000,
            assets: vec![],
            tags: vec![],
        };
        assert!(is_pure_ada_utxo(&pure));
    }

    #[test]
    fn test_is_not_pure_ada() {
        let policy = "a".repeat(56);
        let with_asset = make_utxo(5_000_000, vec![(&policy, "4e4654", 1)]);
        assert!(!is_pure_ada_utxo(&with_asset));
    }

    #[test]
    fn test_total_lovelace() {
        let u1 = UtxoApi {
            tx_hash: "a".repeat(64),
            output_index: 0,
            lovelace: 3_000_000,
            assets: vec![],
            tags: vec![],
        };
        let u2 = UtxoApi {
            tx_hash: "b".repeat(64),
            output_index: 1,
            lovelace: 7_000_000,
            assets: vec![],
            tags: vec![],
        };
        assert_eq!(total_utxo_lovelace(&[&u1, &u2]), 10_000_000);
    }

    #[test]
    fn test_parse_concatenated_asset() {
        let policy_hex = "a".repeat(56);
        let name_hex = "4e4654"; // "NFT"
        let unit = format!("{policy_hex}{name_hex}");

        let (policy_bytes, name_bytes) = parse_concatenated_asset(&unit).unwrap();
        assert_eq!(policy_bytes.len(), 28);
        assert_eq!(name_bytes, b"NFT");
    }

    #[test]
    fn test_parse_concatenated_asset_too_short() {
        assert!(parse_concatenated_asset("abcd").is_none());
    }

    #[test]
    fn test_collect_native_assets_sums() {
        let policy = "a".repeat(56);
        let u1 = make_utxo(2_000_000, vec![(&policy, "4e4654", 3)]);
        let u2 = make_utxo(2_000_000, vec![(&policy, "4e4654", 7)]);

        let assets = collect_utxo_native_assets(&[&u1, &u2], None);
        let id = AssetId::new(policy, "4e4654".to_string()).unwrap();
        assert_eq!(assets[&id], 10);
    }

    #[test]
    fn test_collect_asset_ids_unique() {
        let policy = "a".repeat(56);
        let u1 = make_utxo(2_000_000, vec![(&policy, "4e4654", 3)]);
        let u2 = make_utxo(2_000_000, vec![(&policy, "4e4654", 7)]);

        let ids = collect_asset_ids(&[&u1, &u2], None);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_collect_with_exclusion() {
        let policy = "a".repeat(56);
        let u1 = make_utxo(
            2_000_000,
            vec![(&policy, "4e4654", 3), (&policy, "414243", 5)],
        );

        let exclude: HashSet<AssetId> =
            [AssetId::new(policy.clone(), "4e4654".to_string()).unwrap()]
                .into_iter()
                .collect();

        let assets = collect_utxo_native_assets(&[&u1], Some(&exclude));
        assert_eq!(assets.len(), 1);
        let remaining_id = AssetId::new(policy, "414243".to_string()).unwrap();
        assert_eq!(assets[&remaining_id], 5);
    }
}
