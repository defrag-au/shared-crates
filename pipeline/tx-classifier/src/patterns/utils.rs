//! Shared utilities for pattern detection

use crate::registry::{
    lookup_address, AddressCategory, Marketplace, MarketplacePurpose, ScriptCategory,
};
use crate::*;
use pipeline_types::OperationPayload;
use std::collections::HashMap;
use transactions::RawTxData;

/// Get marketplace information from address
pub fn get_marketplace_info(address: &str) -> Option<(Marketplace, Option<MarketplacePurpose>)> {
    match lookup_address(address) {
        Some(AddressCategory::Script(ScriptCategory::Marketplace {
            marketplace,
            purpose,
            ..
        })) => Some((*marketplace, Some(*purpose))),
        Some(AddressCategory::Marketplace(marketplace)) => Some((*marketplace, None)),
        _ => None,
    }
}

/// Detect marketplaces from asset operation addresses
pub fn detect_marketplace_from_addresses(
    asset_operations: &[AssetOperation],
) -> HashMap<Marketplace, Vec<MarketplacePurpose>> {
    let mut marketplace_info: HashMap<Marketplace, Vec<MarketplacePurpose>> = HashMap::new();

    for op in asset_operations {
        // Check input address
        if let Some(input) = &op.input {
            if let Some((marketplace, purpose_opt)) = get_marketplace_info(&input.address) {
                let purposes = marketplace_info.entry(marketplace).or_default();
                if let Some(purpose) = purpose_opt {
                    if !purposes.contains(&purpose) {
                        purposes.push(purpose);
                    }
                }
            }
        }

        // Check output address
        if let Some(output) = &op.output {
            if let Some((marketplace, purpose_opt)) = get_marketplace_info(&output.address) {
                let purposes = marketplace_info.entry(marketplace).or_default();
                if let Some(purpose) = purpose_opt {
                    if !purposes.contains(&purpose) {
                        purposes.push(purpose);
                    }
                }
            }
        }
    }

    marketplace_info
}

/// Check if an address is a JPG.store script address
pub fn is_jpg_store_address(address: &str) -> bool {
    matches!(
        lookup_address(address),
        Some(AddressCategory::Script(ScriptCategory::Marketplace {
            marketplace: Marketplace::JpgStore,
            ..
        }))
    )
}

/// Classify payment type based on amount
pub fn classify_payment_type(amount: u64) -> crate::DistributionType {
    // Classify ADA amounts based on reasonable thresholds
    if amount < 2_000_000 {
        // < 2 ADA: likely fee or datum deposit
        crate::DistributionType::Royalty
    } else if amount < 10_000_000 {
        // 2-10 ADA: could be royalty or small payment
        crate::DistributionType::Royalty
    } else {
        // > 10 ADA: likely sale proceeds or significant payment
        crate::DistributionType::Royalty
    }
}

/// Identify the fee-paying address from transaction inputs/outputs
/// This is typically the transaction executor/sender
pub fn identify_fee_paying_address(raw_tx_data: &RawTxData) -> Option<String> {
    // Calculate net ADA flow for each address (positive = received, negative = sent)
    let mut address_flows: HashMap<String, i64> = HashMap::new();

    // Process inputs (negative flow - ADA leaving the address)
    for input in &raw_tx_data.inputs {
        let flow = address_flows.entry(input.address.clone()).or_insert(0);
        *flow -= input.amount_lovelace as i64;
    }

    // Process outputs (positive flow - ADA coming to the address)
    for output in &raw_tx_data.outputs {
        let flow = address_flows.entry(output.address.clone()).or_insert(0);
        *flow += output.amount_lovelace as i64;
    }

    // Find the address with the largest net outflow (most negative)
    // This is typically the fee payer / transaction initiator
    let (fee_payer, _net_outflow) = address_flows
        .iter()
        .filter(|(address, _)| {
            // Exclude known script addresses from fee payer detection
            lookup_address(address).is_none()
        })
        .min_by_key(|(_, flow)| **flow)?;

    Some(fee_payer.clone())
}

/// Extract policy ID from offer context by analyzing script address datums
/// Looks for policy IDs in the datum structure of script addresses
pub fn extract_policy_from_context(context: &super::PatternContext, offer_address: &str) -> String {
    // Find the input with the offer address and extract policy ID from its datum
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if input.address == offer_address {
            if let Some(datum) = &input.datum {
                if let Some(policy_id) = extract_policy_id_from_datum(datum) {
                    return policy_id;
                }
            }
        }
    }

    // Fallback: look for policy IDs in any script address datum
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if lookup_address(&input.address).is_some() {
            if let Some(datum) = &input.datum {
                if let Some(policy_id) = extract_policy_id_from_datum(datum) {
                    return policy_id;
                }
            }
        }
    }

    String::new()
}

/// Extract policy ID from datum JSON structure
/// Looks for 56-character hex strings in datum "bytes" fields
fn extract_policy_id_from_datum(datum: &transactions::TxDatum) -> Option<String> {
    let json_value = match datum {
        transactions::TxDatum::Json { json, .. } => json,
        _ => return None,
    };

    // Recursively search for policy IDs in the JSON structure
    find_policy_id_recursive(json_value, 0)
}

/// Recursively search for policy ID in JSON datum structure
fn find_policy_id_recursive(value: &serde_json::Value, depth: u8) -> Option<String> {
    if depth > 15 {
        return None; // Prevent infinite recursion
    }

    match value {
        serde_json::Value::Object(map) => {
            // Look for "bytes" fields that could be policy IDs
            if let Some(serde_json::Value::String(bytes_str)) = map.get("bytes") {
                if bytes_str.len() == 56 {
                    // Policy IDs are 56 characters long
                    if bytes_str.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Some(bytes_str.clone());
                    }
                }
            }

            // Recursively search in all values
            for val in map.values() {
                if let Some(policy_id) = find_policy_id_recursive(val, depth + 1) {
                    return Some(policy_id);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for val in arr {
                if let Some(policy_id) = find_policy_id_recursive(val, depth + 1) {
                    return Some(policy_id);
                }
            }
        }
        _ => {}
    }
    None
}

/// Verify that actual sale flows are present in the transaction
/// This helps distinguish real sales from unlisting transactions
pub fn verify_actual_sale_flows(
    raw_tx_data: &RawTxData,
    asset_operations: &[AssetOperation],
    expected_total: u64,
) -> bool {
    let reasonable_payment_threshold = expected_total / 10; // 10% of expected total

    // Track significant ADA flows
    let ada_flows: Vec<(u64, String)> = raw_tx_data
        .outputs
        .iter()
        .filter_map(|output| {
            if output.amount_lovelace > reasonable_payment_threshold {
                Some((output.amount_lovelace, output.address.clone()))
            } else {
                None
            }
        })
        .collect();

    let _total_to_users = ada_flows.iter().map(|(amount, _)| amount).sum::<u64>();

    // Check if ADA is flowing to different addresses than where assets are going
    let asset_recipients: Vec<String> = asset_operations
        .iter()
        .filter_map(|op| {
            if let OperationPayload::NativeToken { .. } = &op.payload {
                op.output.as_ref().map(|output| output.address.clone())
            } else {
                None
            }
        })
        .collect();

    let ada_to_different_addresses = ada_flows.iter().any(|(amount, addr)| {
        *amount > reasonable_payment_threshold && !asset_recipients.contains(addr)
    });

    if !ada_to_different_addresses {
        return false;
    }

    true
}
