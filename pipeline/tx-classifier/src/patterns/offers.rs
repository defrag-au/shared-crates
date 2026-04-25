//! Offer-related transaction pattern detection
//!
//! This module handles detection of:
//! - Offer creation
//! - Offer updates
//! - Offer cancellations
//! - Offer acceptances

use pipeline_types::OperationPayload;

use super::{PatternContext, PatternDetectionResult};
use crate::registry::{
    lookup_address, AddressCategory, Marketplace, MarketplacePurpose, ScriptCategory,
};
use crate::TxType;
use crate::*;

/// Key: (policy_id, encoded_asset_name, marketplace, offer_amount_lovelace)
/// Value: (count, bidder_address)
type OfferGroups = std::collections::HashMap<(String, Option<String>, String, u64), (u32, String)>;

/// Asset-based JPG.store offer creation rule
pub struct AssetBasedJpgStoreRule;

/// Asset-based offer creation rule
pub struct AssetBasedCreateOfferRule;

/// Detect offer creation transactions with enrichment
fn detect_create_offer_with_enrichment(context: &PatternContext) -> Vec<(TxType, f64)> {
    let mut results = Vec::new();
    let mut offer_groups: OfferGroups = std::collections::HashMap::new();

    // Group offer creation operations by (policy_id, encoded_asset_name, marketplace, offer_amount)
    for op in context
        .asset_operations
        .iter()
        .filter(|op| matches!(op.classification, crate::OperationClassification::Genuine))
    {
        if op.op_type == AssetOpType::Lock
            && matches!(op.payload, OperationPayload::Lovelace { .. })
        {
            // Check if this is going to a known offer address
            if let Some(output) = &op.output {
                if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
                    purpose: MarketplacePurpose::Offer,
                    ..
                })) = lookup_address(&output.address)
                {
                    let offer_amount = op.amount();
                    let (policy_id, encoded_asset_name) =
                        extract_asset_info_from_context(context, &output.address);
                    let bidder = op.seller(); // The one locking ADA is the bidder

                    let key = (
                        policy_id.clone(),
                        encoded_asset_name.clone(),
                        output.address.clone(),
                        offer_amount,
                    );
                    let entry = offer_groups.entry(key).or_insert((0, bidder));
                    entry.0 += 1; // Increment offer count
                }
            }
        }
    }

    // Create aggregated CreateOffer results
    for ((policy_id, encoded_asset_name, marketplace, offer_amount), (offer_count, bidder)) in
        offer_groups
    {
        results.push((
            TxType::CreateOffer {
                policy_id,
                encoded_asset_name,
                offer_count,
                offer_lovelace: offer_amount,
                total_lovelace: offer_amount * offer_count as u64,
                bidder,
                marketplace,
            },
            0.95, // High confidence for ADA locks to offer addresses
        ));
    }

    results
}

/// Extract both policy ID and asset name from offer context by analyzing script address datums
/// Returns (policy_id, asset_name) where asset_name is None for collection offers
#[allow(deprecated)]
fn extract_asset_info_from_context(
    context: &PatternContext,
    offer_address: &str,
) -> (String, Option<String>) {
    // For offer creation, check output datums first (new offers being created)
    #[allow(deprecated)]
    for output in &context.raw_tx_data.outputs {
        if output.address == offer_address {
            if let Some(datum) = &output.datum {
                if let Some((policy_id, asset_name)) =
                    super::sales::try_schema_driven_asset_extraction(datum, &output.address)
                {
                    return (policy_id, asset_name);
                }
            }
        }
    }

    // Fallback: check input datums (for updates/cancellations)
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if input.address == offer_address {
            if let Some(datum) = &input.datum {
                if let Some((policy_id, asset_name)) =
                    super::sales::try_schema_driven_asset_extraction(datum, &input.address)
                {
                    return (policy_id, asset_name);
                }
            }
        }
    }

    // Final fallback: look for policy IDs in any script address datum
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if lookup_address(&input.address).is_some() {
            if let Some(datum) = &input.datum {
                if let Some((policy_id, asset_name)) =
                    super::sales::try_schema_driven_asset_extraction(datum, &input.address)
                {
                    return (policy_id, asset_name);
                }
            }
        }
    }

    // Last resort fallback to policy-only extraction
    let policy_id = extract_policy_from_context(context, offer_address);
    (policy_id, None)
}

/// Extract policy ID from offer context by analyzing script address datums
/// Looks for policy IDs in the datum structure of script addresses
/// Extract both policy ID and asset name from context (for offer cancellations)
#[allow(deprecated)]
fn extract_policy_and_asset_from_context(
    context: &PatternContext,
    offer_address: &str,
) -> (String, Option<String>) {
    // For offer cancellation, check input datums (existing offers being cancelled)
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if input.address == offer_address {
            if let Some(datum) = &input.datum {
                // Try schema-driven extraction
                if let Some((policy_id, asset_name)) =
                    super::sales::try_schema_driven_asset_extraction(datum, &input.address)
                {
                    return (policy_id, asset_name);
                }
            }
        }
    }

    // Fallback: return just policy ID with no asset name
    let policy_id = extract_policy_from_context(context, offer_address);
    (policy_id, None)
}

#[allow(deprecated)]
fn extract_policy_from_context(context: &PatternContext, offer_address: &str) -> String {
    // For offer creation, check output datums first (new offers being created)
    #[allow(deprecated)]
    for output in &context.raw_tx_data.outputs {
        if output.address == offer_address {
            if let Some(datum) = &output.datum {
                if let Some((policy_id, _)) =
                    super::sales::try_schema_driven_asset_extraction(datum, &output.address)
                {
                    return policy_id;
                }
            }
        }
    }

    // Fallback: look at input datums (for updates/cancellations)
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if input.address == offer_address {
            if let Some(datum) = &input.datum {
                if let Some((policy_id, _)) =
                    super::sales::try_schema_driven_asset_extraction(datum, &input.address)
                {
                    return policy_id;
                }
            }
        }
    }

    // Final fallback: look for policy IDs in any script address datum
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if lookup_address(&input.address).is_some() {
            if let Some(datum) = &input.datum {
                if let Some((policy_id, _)) =
                    super::sales::try_schema_driven_asset_extraction(datum, &input.address)
                {
                    return policy_id;
                }
            }
        }
    }

    // Additional fallback: check transaction metadata for policy IDs
    if let Some(metadata) = &context.raw_tx_data.metadata {
        if let Some(policy_id) = extract_policy_id_from_metadata(metadata) {
            return policy_id;
        }
    }

    // Last resort fallback: look for policy IDs in asset operations themselves
    // This can happen when offers are being made on specific assets that are referenced in the transaction
    for op in context.asset_operations {
        if let Some(asset) = op.payload.get_asset() {
            return asset.policy_id.clone();
        }
    }

    String::new()
}

/// Extract policy ID from transaction metadata
fn extract_policy_id_from_metadata(metadata: &serde_json::Value) -> Option<String> {
    use super::marketplace_classification::find_policy_id_in_string;

    // Convert metadata to string and search for policy IDs
    let metadata_str = metadata.to_string();
    find_policy_id_in_string(&metadata_str).map(|s| s.to_string())
}

/// Detect offer updates based on asset operations
/// Detects individual UTXO-level offer updates rather than aggregating amounts
fn detect_offer_update(context: &PatternContext) -> Vec<(TxType, f64)> {
    let mut results = Vec::new();

    // Look for offer address interactions where ADA amounts change
    for marketplace in [Marketplace::JpgStore] {
        let offer_address = match marketplace {
            Marketplace::JpgStore => "addr1xxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tfvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eks2utwdd",
            _ => continue,
        };

        // Find all inputs and outputs for the offer address
        let mut offer_inputs = Vec::new();
        let mut offer_outputs = Vec::new();
        let mut bidder = String::new();

        for op in context
            .asset_operations
            .iter()
            .filter(|op| matches!(op.classification, crate::OperationClassification::Genuine))
        {
            if matches!(op.payload, OperationPayload::Lovelace { .. }) {
                // Only consider Transfer operations for offer updates
                // Unlock operations are likely cancellations or funding withdrawal
                if op.op_type == crate::AssetOpType::Transfer {
                    if let Some(input) = &op.input {
                        if input.address == offer_address {
                            // Get the actual input amount from raw transaction data
                            if let Some(raw_input) =
                                context.raw_tx_data.inputs.get(input.idx as usize)
                            {
                                offer_inputs.push(raw_input.amount_lovelace);
                            }
                        }
                    }
                    if let Some(output) = &op.output {
                        if output.address == offer_address {
                            offer_outputs.push(op.amount());
                            if bidder.is_empty() {
                                // The bidder should be the fee-paying user, not the script address
                                bidder = crate::patterns::utils::identify_fee_paying_address(
                                    context.raw_tx_data,
                                )
                                .unwrap_or_else(|| op.seller());
                            }
                        }
                    }
                }
            }
        }

        // Sort amounts to match inputs with outputs (assumes same policy/offer type)
        offer_inputs.sort();
        offer_outputs.sort();

        // Create individual offer updates for each input→output pair
        if offer_inputs.len() == offer_outputs.len() && !offer_inputs.is_empty() {
            // Extract both policy ID and asset name from the datum
            let (policy_id, encoded_asset_name) =
                extract_asset_info_from_context(context, offer_address);

            for (index, (input_amount, output_amount)) in
                offer_inputs.iter().zip(offer_outputs.iter()).enumerate()
            {
                if input_amount != output_amount {
                    results.push((
                        TxType::OfferUpdate {
                            policy_id: policy_id.clone(),
                            encoded_asset_name: encoded_asset_name.clone(),
                            original_amount: *input_amount,
                            updated_amount: *output_amount,
                            delta_amount: (*output_amount as i64) - (*input_amount as i64),
                            bidder: bidder.clone(),
                            marketplace: offer_address.to_string(),
                            offer_index: index as u32, // Use unique index for each offer
                        },
                        0.90,
                    ));
                }
            }
        }
    }

    results
}

/// Detect offer cancellations based on asset operations
fn detect_offer_cancel(context: &PatternContext) -> Vec<(TxType, f64)> {
    let mut results = Vec::new();

    // Look for ADA being unlocked from offer addresses without corresponding asset transfers
    for op in context
        .asset_operations
        .iter()
        .filter(|op| matches!(op.classification, crate::OperationClassification::Genuine))
    {
        if op.op_type == AssetOpType::Unlock
            && matches!(op.payload, OperationPayload::Lovelace { .. })
        {
            if let Some(input) = &op.input {
                if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
                    purpose: MarketplacePurpose::Offer,
                    ..
                })) = lookup_address(&input.address)
                {
                    // Check if there are any meaningful asset transfers in this transaction
                    let has_asset_transfers = context.asset_operations.iter().any(|asset_op| {
                        matches!(
                            asset_op.classification,
                            crate::OperationClassification::Genuine
                        ) && asset_op.is_native_token()
                            && asset_op.op_type == AssetOpType::Transfer
                    });

                    if !has_asset_transfers {
                        // Extract both policy ID and asset name from the cancellation context
                        let (policy_id, encoded_asset_name) =
                            extract_policy_and_asset_from_context(context, &input.address);

                        // Count the number of offers in the datum to determine offer_count
                        let offer_count =
                            count_offers_in_datum(context, &input.address, &policy_id);

                        results.push((
                            TxType::OfferCancel {
                                policy_id,
                                encoded_asset_name,
                                offer_count,
                                total_cancelled_lovelace: op.amount(),
                                bidder: op.buyer(), // The one receiving the ADA back
                                marketplace: input.address.clone(),
                            },
                            0.90,
                        ));
                    }
                }
            }
        }
    }

    results
}

/// Count the number of offers for a specific policy ID in the datum
/// Specifically counts appearances as map keys across ALL inputs from the offer address
#[allow(deprecated)]
fn count_offers_in_datum(context: &PatternContext, offer_address: &str, policy_id: &str) -> u32 {
    if policy_id.is_empty() {
        return 1; // Default to 1 if no policy ID found
    }

    let mut total_count = 0;

    // Count across ALL inputs with the offer address (there can be multiple offer UTXOs)
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if input.address == offer_address {
            if let Some(transactions::TxDatum::Json { json, .. }) = &input.datum {
                let count = count_policy_id_as_map_keys(json, policy_id, 0);
                total_count += count;
            }
        }
    }

    total_count.max(1) // At least 1 offer if any are found
}

/// Count occurrences of a policy ID specifically as map keys
fn count_policy_id_as_map_keys(value: &serde_json::Value, policy_id: &str, depth: u8) -> u32 {
    if depth > 15 {
        return 0; // Prevent infinite recursion
    }

    let mut count = 0;

    match value {
        serde_json::Value::Object(map) => {
            // Check if this object contains a "map" field (indicates it's a Cardano map structure)
            if let Some(serde_json::Value::Array(map_entries)) = map.get("map") {
                // Process map entries, looking for our specific policy ID as keys
                for entry in map_entries {
                    if let serde_json::Value::Object(entry_obj) = entry {
                        if let Some(k_obj) = entry_obj.get("k") {
                            if let Some(serde_json::Value::String(bytes_str)) = k_obj.get("bytes") {
                                if bytes_str == policy_id {
                                    count += 1;
                                }
                            }
                        }
                        // Also recursively check the value part
                        if let Some(v_obj) = entry_obj.get("v") {
                            count += count_policy_id_as_map_keys(v_obj, policy_id, depth + 1);
                        }
                    }
                }
            } else {
                // Recursively search in all values
                for val in map.values() {
                    count += count_policy_id_as_map_keys(val, policy_id, depth + 1);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for val in arr {
                count += count_policy_id_as_map_keys(val, policy_id, depth + 1);
            }
        }
        _ => {}
    }

    count
}

/// Detect offer acceptances based on asset operations
pub fn detect_offer_accepts(context: &PatternContext) -> Vec<(TxType, f64)> {
    let mut results = Vec::new();

    // Look for NFT transfers that could be offer acceptances
    let nft_transfers: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            matches!(op.classification, crate::OperationClassification::Genuine)
                && op.is_native_token()
                && op.op_type == AssetOpType::Transfer
        })
        .collect();

    if nft_transfers.is_empty() {
        return results;
    }

    // Look for ADA unlocks from offer script addresses (offers being fulfilled)
    let offer_unlocks: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            matches!(op.classification, crate::OperationClassification::Genuine)
                && matches!(op.payload, OperationPayload::Lovelace { .. })
                && op.op_type == AssetOpType::Unlock
                && if let Some(input) = &op.input {
                    matches!(
                        lookup_address(&input.address),
                        Some(AddressCategory::Script(ScriptCategory::Marketplace {
                            purpose: MarketplacePurpose::Offer,
                            ..
                        }))
                    )
                } else {
                    false
                }
        })
        .collect();

    debug!(
        "Found {} NFT transfers and {} offer unlocks",
        nft_transfers.len(),
        offer_unlocks.len()
    );

    // If we have both NFT transfers AND offer unlocks, this is likely offer acceptance
    if !offer_unlocks.is_empty() && !nft_transfers.is_empty() {
        for nft_op in nft_transfers {
            if let Some(asset) = nft_op.payload.get_asset() {
                // Find corresponding offer unlock for this NFT (by policy or general)
                let matching_offer = offer_unlocks
                    .iter()
                    .find(|offer_op| {
                        // Try to match by policy ID from offer datum
                        if let Some(input) = &offer_op.input {
                            let policy_from_offer =
                                extract_policy_from_context(context, &input.address);
                            !policy_from_offer.is_empty() && asset.policy_id == policy_from_offer
                        } else {
                            false
                        }
                    })
                    .or_else(|| offer_unlocks.first()); // Fallback to first offer if no policy match

                if let Some(offer_op) = matching_offer {
                    // Get the total offer price using JPG.store v3 extraction logic
                    let offer_lovelace = if let Some(input) = &offer_op.input {
                        // Try to extract total price from JPG.store v3 datum first
                        #[allow(deprecated)]
                        if let Some(raw_input) = context.raw_tx_data.inputs.get(input.idx as usize)
                        {
                            if let Some(datum) = &raw_input.datum {
                                // Use schema-driven pricing extraction (use input address as marketplace address)
                                let datum_pricing =
                                    super::sales::try_schema_driven_pricing_extraction(
                                        datum,
                                        &input.address,
                                    );
                                debug!("Offer acceptance pricing: UTXO amount={} lovelace, schema pricing={:?}",
                                       raw_input.amount_lovelace, datum_pricing);
                                if let Some(pricing) = datum_pricing {
                                    let datum_price = pricing.total_price_lovelace;
                                    if datum_price > raw_input.amount_lovelace {
                                        debug!("Using datum price {} lovelace over UTXO amount {} lovelace", datum_price, raw_input.amount_lovelace);
                                        datum_price // Use extracted price if it's higher (includes all payouts)
                                    } else {
                                        debug!("Using UTXO amount {} lovelace over datum price {} lovelace", raw_input.amount_lovelace, datum_price);
                                        raw_input.amount_lovelace // Fallback to UTXO amount
                                    }
                                } else {
                                    debug!(
                                        "No price found in datum, using UTXO amount {} lovelace",
                                        raw_input.amount_lovelace
                                    );
                                    raw_input.amount_lovelace // No price found in datum, use UTXO amount
                                }
                            } else {
                                raw_input.amount_lovelace // No datum, use UTXO amount
                            }
                        } else {
                            offer_op.amount() // Fallback to operation amount
                        }
                    } else {
                        offer_op.amount() // Fallback to operation amount
                    };

                    // Seller is the NFT source, buyer is the NFT destination
                    let seller = nft_op.seller();
                    let buyer = nft_op.buyer();

                    debug!(
                        "Detected offer accept: asset {} for ₳{:.2} from {} to {}",
                        asset.dot_delimited(),
                        offer_lovelace as f64 / 1_000_000.0,
                        seller,
                        buyer
                    );

                    results.push((
                        TxType::OfferAccept {
                            asset: asset.clone(),
                            offer_lovelace,
                            seller,
                            buyer,
                        },
                        0.95,
                    ));
                }
            }
        }
    }

    results
}

// Wrapper functions for pattern registration
pub fn detect_create_offer_with_enrichment_wrapper(
    context: &PatternContext,
) -> PatternDetectionResult {
    PatternDetectionResult {
        transactions: detect_create_offer_with_enrichment(context),
    }
}

pub fn detect_offer_update_wrapper(context: &PatternContext) -> PatternDetectionResult {
    PatternDetectionResult {
        transactions: detect_offer_update(context),
    }
}

pub fn detect_offer_cancel_wrapper(context: &PatternContext) -> PatternDetectionResult {
    PatternDetectionResult {
        transactions: detect_offer_cancel(context),
    }
}

pub fn detect_offer_accepts_wrapper(context: &PatternContext) -> PatternDetectionResult {
    PatternDetectionResult {
        transactions: detect_offer_accepts(context),
    }
}

/// Post-process pattern results: collapse matching CreateOffer + OfferCancel pairs into OfferUpdate.
///
/// When a collection offer is "updated" on-chain, the marketplace creates new offer UTXOs
/// and cancels old ones in a single tx. We detect this by matching on (policy_id, bidder)
/// and replace the pair with an OfferUpdate carrying the price delta.
pub fn collapse_offer_deltas(results: &mut Vec<(TxType, f64)>) {
    // Index CreateOffers by (policy_id, bidder) -> (index, confidence)
    let mut creates: std::collections::HashMap<(String, String), (usize, f64)> =
        std::collections::HashMap::new();
    // Index OfferCancels by (policy_id, bidder) -> (index, confidence)
    let mut cancels: std::collections::HashMap<(String, String), (usize, f64)> =
        std::collections::HashMap::new();

    for (i, (tx_type, confidence)) in results.iter().enumerate() {
        match tx_type {
            TxType::CreateOffer {
                policy_id, bidder, ..
            } => {
                creates.insert((policy_id.clone(), bidder.clone()), (i, *confidence));
            }
            TxType::OfferCancel {
                policy_id, bidder, ..
            } => {
                cancels.insert((policy_id.clone(), bidder.clone()), (i, *confidence));
            }
            _ => {}
        }
    }

    // Find matching pairs
    let mut indices_to_remove = Vec::new();
    let mut updates_to_add = Vec::new();

    for (key, (create_idx, create_conf)) in &creates {
        if let Some((cancel_idx, cancel_conf)) = cancels.get(key) {
            // We have a matching pair — collapse into OfferUpdate
            let (create_type, _) = &results[*create_idx];
            let (cancel_type, _) = &results[*cancel_idx];

            if let (
                TxType::CreateOffer {
                    policy_id,
                    encoded_asset_name,
                    offer_lovelace,
                    bidder,
                    marketplace,
                    ..
                },
                TxType::OfferCancel {
                    offer_count: cancel_count,
                    total_cancelled_lovelace,
                    ..
                },
            ) = (create_type, cancel_type)
            {
                let original_amount = if *cancel_count > 0 {
                    total_cancelled_lovelace / *cancel_count as u64
                } else {
                    *total_cancelled_lovelace
                };
                let updated_amount = *offer_lovelace;
                let delta_amount = updated_amount as i64 - original_amount as i64;
                let confidence = create_conf.max(*cancel_conf);

                updates_to_add.push((
                    TxType::OfferUpdate {
                        policy_id: policy_id.clone(),
                        encoded_asset_name: encoded_asset_name.clone(),
                        original_amount,
                        updated_amount,
                        delta_amount,
                        bidder: bidder.clone(),
                        marketplace: marketplace.clone(),
                        offer_index: 0,
                    },
                    confidence,
                ));

                indices_to_remove.push(*create_idx);
                indices_to_remove.push(*cancel_idx);
            }
        }
    }

    if indices_to_remove.is_empty() {
        return;
    }

    // Remove matched pairs (highest index first to preserve positions)
    indices_to_remove.sort_unstable_by(|a, b| b.cmp(a));
    for idx in indices_to_remove {
        results.remove(idx);
    }

    // Add the collapsed OfferUpdates
    results.extend(updates_to_add);
}
