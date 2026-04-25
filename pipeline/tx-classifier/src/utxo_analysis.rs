use crate::{AssetOpType, AssetOperation, RawTxData, RawTxDataExt};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use tracing::debug;

/// Result of UTXO analysis showing genuine economic activities vs housekeeping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoAnalysis {
    /// All asset operations in their original positions with classifications
    /// This preserves the positional relationship needed for datum access
    pub all_operations: Vec<AssetOperation>,
    /// Summary statistics about the analysis
    pub summary: UtxoSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoSummary {
    /// Total number of asset operations analyzed
    pub total_operations: usize,
    /// Number of genuine economic movements identified
    pub genuine_count: usize,
    /// Number of housekeeping operations identified
    pub housekeeping_count: usize,
    /// Unique assets involved in genuine movements
    pub genuine_assets: HashSet<String>,
    /// Unique addresses involved in genuine movements
    pub genuine_addresses: HashSet<String>,
}

/// Analyzes a transaction to classify asset movements as genuine vs housekeeping
///
/// This preserves the original positions while marking operations with their classifications:
/// - WHEAT: Genuine economic activities (mints, burns, transfers between different parties)
/// - CHAFF: UTXO housekeeping (same-address transfers, unlisting, consolidation)
pub fn analyze_utxo_operations(tx_data: &RawTxData) -> UtxoAnalysis {
    let mut asset_operations = tx_data.extract_asset_operations();

    debug!(
        "Starting UTXO analysis for transaction: {}",
        tx_data.tx_hash
    );
    debug!(
        "Total asset operations to analyze: {}",
        asset_operations.len()
    );

    let mut genuine_assets = HashSet::new();
    let mut genuine_addresses = HashSet::new();
    let mut genuine_count = 0;
    let mut housekeeping_count = 0;

    // Group operations by asset to analyze patterns
    let mut asset_groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, op) in asset_operations.iter().enumerate() {
        let asset_key = format!(
            "{}{}",
            op.policy_id().unwrap_or(&String::new()),
            op.asset_name().unwrap_or(&String::new())
        );
        asset_groups.entry(asset_key).or_default().push(idx);
    }

    // Classify each operation in place
    for (asset_id, operation_indices) in asset_groups {
        // Create a temporary vector for classification without borrowing asset_operations
        let mut classifications = Vec::new();

        for &idx in &operation_indices {
            let op = &asset_operations[idx];
            if is_genuine_movement(op) {
                classifications.push((idx, crate::OperationClassification::Genuine));
            } else {
                classifications.push((idx, crate::OperationClassification::Housekeeping));
            }
        }

        // Apply classifications
        for (idx, classification) in classifications {
            asset_operations[idx].classification = classification.clone();

            match classification {
                crate::OperationClassification::Genuine => {
                    genuine_count += 1;
                    genuine_assets.insert(asset_id.clone());

                    let op = &asset_operations[idx];
                    if let Some(from_utxo) = &op.input {
                        genuine_addresses.insert(from_utxo.address.clone());
                    }
                    if let Some(to_utxo) = &op.output {
                        genuine_addresses.insert(to_utxo.address.clone());
                    }
                }
                crate::OperationClassification::Housekeeping => {
                    housekeeping_count += 1;
                }
            }
        }
    }

    let summary = UtxoSummary {
        total_operations: asset_operations.len(),
        genuine_count,
        housekeeping_count,
        genuine_assets,
        genuine_addresses,
    };

    debug!(
        "UTXO analysis complete: {} genuine, {} housekeeping",
        summary.genuine_count, summary.housekeeping_count
    );

    UtxoAnalysis {
        all_operations: asset_operations,
        summary,
    }
}

/// Determines if an asset operation represents genuine economic activity
///
/// WHEAT vs CHAFF classification:
/// - WHEAT: Mints, burns, locks, unlocks, transfers between different addresses
/// - CHAFF: Same-address transfers (UTXO consolidation, unlisting)
fn is_genuine_movement(op: &AssetOperation) -> bool {
    match op.op_type {
        // Minting and burning are always genuine economic activities
        AssetOpType::Mint | AssetOpType::Burn => true,

        // Lock and unlock operations are genuine economic activities
        AssetOpType::Lock | AssetOpType::Unlock => true,

        // For transfers, check if assets actually move between different addresses
        // OR if they involve known script addresses (even same-address transfers can be meaningful)
        AssetOpType::Transfer => {
            match (&op.input, &op.output) {
                (Some(from_utxo), Some(to_utxo)) => {
                    let from = &from_utxo.address;
                    let to = &to_utxo.address;

                    // If asset moves to a different address, it's definitely genuine
                    if from != to {
                        return true;
                    }

                    // Same address transfers are usually housekeeping, EXCEPT:
                    // - If it involves known script addresses (marketplace, staking, etc.)
                    //   because script-to-script movements represent meaningful state changes
                    use crate::registry::lookup_address;
                    lookup_address(from).is_some() || lookup_address(to).is_some()
                }
                // If we can't determine addresses, err on the side of considering it genuine
                _ => true,
            }
        }
    }
}

/// Creates a filtered context containing only genuine asset movements for pattern analysis
pub fn create_filtered_context(tx_data: &RawTxData) -> (Vec<AssetOperation>, UtxoAnalysis) {
    let analysis = analyze_utxo_operations(tx_data);

    // Extract only genuine operations while preserving their original positions
    let genuine_operations: Vec<AssetOperation> = analysis
        .all_operations
        .iter()
        .filter(|op| matches!(op.classification, crate::OperationClassification::Genuine))
        .cloned()
        .collect();

    (genuine_operations, analysis)
}
