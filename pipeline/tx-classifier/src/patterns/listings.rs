//! Listing transaction pattern detection

use super::{PatternContext, PatternDetectionResult};
use crate::registry::{
    lookup_address, no_fee_calculation, AddressCategory, FeeCalculationFn, MarketplacePurpose,
    ScriptCategory,
};
use crate::*;
use pipeline_types::{AssetId, OperationPayload, PricedAsset};
use std::collections::BTreeMap;
use tracing::debug;

/// Detect listing creation
pub fn detect_listing_create_wrapper(context: &PatternContext) -> PatternDetectionResult {
    PatternDetectionResult {
        transactions: detect_listing_create(context),
    }
}

/// Detect listing updates
pub fn detect_listing_update_wrapper(context: &PatternContext) -> PatternDetectionResult {
    PatternDetectionResult {
        transactions: detect_listing_update(context),
    }
}

/// Detect unlisting
pub fn detect_unlisting_wrapper(context: &PatternContext) -> PatternDetectionResult {
    PatternDetectionResult {
        transactions: detect_unlisting(context),
    }
}

/// Detect creation of new marketplace listings
fn detect_listing_create(context: &PatternContext) -> Vec<(TxType, f64)> {
    // Look for genuine asset operations where NFTs are being locked (sent to script addresses)
    let lock_operations: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            matches!(op.classification, crate::OperationClassification::Genuine) // Only genuine operations
                && matches!(op.op_type, AssetOpType::Lock)
                && op.is_native_token() // Only look at NFT listings, not ADA
                && op.amount() == 1 // NFTs typically have amount of 1
        })
        .collect();

    if lock_operations.is_empty() {
        return vec![];
    }

    debug!(
        "Found {} lock operations for potential listings",
        lock_operations.len()
    );

    // Group lock operations by destination address to identify marketplace listings
    let mut address_listings: std::collections::BTreeMap<String, Vec<&AssetOperation>> =
        std::collections::BTreeMap::new();

    for op in &lock_operations {
        if let Some(to_utxo) = &op.output {
            // Check if this address is a known marketplace sale address
            if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
                purpose: MarketplacePurpose::Sale,
                ..
            })) = lookup_address(&to_utxo.address)
            {
                address_listings
                    .entry(to_utxo.address.clone())
                    .or_default()
                    .push(op);
            }
        }
    }

    if address_listings.is_empty() {
        debug!("No lock operations to known marketplace sale addresses found");
        return vec![];
    }

    let mut results = Vec::new();

    // Process each marketplace address separately
    for (marketplace_address, ops) in address_listings {
        debug!(
            "Processing {} assets being listed at marketplace address: {}",
            ops.len(),
            marketplace_address
        );

        // Find the seller (source address) - should be consistent across operations
        let seller = ops[0]
            .input
            .as_ref()
            .map(|utxo| utxo.address.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // Verify all operations come from the same seller
        let all_same_seller = ops
            .iter()
            .all(|op| op.input.as_ref().map(|utxo| &utxo.address) == Some(&seller));

        if !all_same_seller {
            debug!("Multiple sellers detected, skipping this address");
            continue;
        }

        // Get marketplace fee calculation function from address registry
        let _fee_calculation = get_marketplace_fee_calculation(&marketplace_address);

        // Collect the assets being listed with individual pricing
        let mut priced_assets: Vec<PricedAsset> = Vec::new();
        for op in &ops {
            if let Some(asset_id_str) = op.asset_id() {
                let Some(asset) = AssetId::parse_concatenated(&asset_id_str).ok() else {
                    continue;
                };
                // Extract individual price for this specific asset using schema-driven approach
                let mut price_lovelace =
                    extract_asset_price_from_utxo(context, &asset_id_str, &marketplace_address);

                // If datum extraction failed, try metadata fallback
                if price_lovelace.is_none() {
                    #[allow(deprecated)]
                    {
                        price_lovelace =
                            super::extractors::extract_asset_price_from_utxo_or_metadata(
                                context.raw_tx_data,
                                &asset.policy_id,
                                &marketplace_address,
                            );
                    }
                }

                // Use schema-driven pricing extraction for marketplace datums
                if let Some(datum) = op.output_datum.as_ref() {
                    if let Some(marketplace_pricing) =
                        super::sales::try_schema_driven_pricing_extraction(
                            datum,
                            &marketplace_address,
                        )
                    {
                        // Use the complete pricing from schema-driven extraction
                        price_lovelace = Some(marketplace_pricing.total_price_lovelace);
                        debug!(
                            "Using schema-driven pricing: base={}, fee={}, total={}",
                            marketplace_pricing.base_price_lovelace,
                            marketplace_pricing.marketplace_fee_lovelace,
                            marketplace_pricing.total_price_lovelace
                        );
                    }
                }

                priced_assets.push(PricedAsset {
                    asset,
                    price_lovelace,
                    delta_lovelace: None, // Not applicable for new listings
                });
            }
        }

        if priced_assets.is_empty() {
            debug!("No valid assets found for listing");
            continue;
        }

        let listing_count = priced_assets.len() as u32;

        // Calculate total listing price
        let total_price: u64 = priced_assets
            .iter()
            .filter_map(|pa| pa.price_lovelace)
            .sum();

        results.push((
            TxType::ListingCreate {
                assets: priced_assets,
                total_listing_count: listing_count,
                seller: seller.clone(),
                marketplace: marketplace_address.clone(),
            },
            0.85, // High confidence for clear marketplace listings
        ));

        debug!(
            "Detected listing creation: {} assets (₳{:.2} total) from {} to {}",
            listing_count,
            total_price as f64 / 1_000_000.0,
            seller,
            marketplace_address
        );
    }

    results
}

/// Detect listing price updates
fn detect_listing_update(context: &PatternContext) -> Vec<(TxType, f64)> {
    // Look for assets that come FROM marketplace sale script addresses (inputs)
    // and go TO marketplace sale script addresses (outputs) - indicating a listing update
    let mut input_assets: BTreeMap<String, (String, Option<transactions::TxDatum>)> =
        BTreeMap::new();
    let mut output_assets: BTreeMap<String, (String, Option<transactions::TxDatum>)> =
        BTreeMap::new();

    // Collect input assets from marketplace sale addresses (genuine operations only)
    for op in context
        .asset_operations
        .iter()
        .filter(|op| matches!(op.classification, crate::OperationClassification::Genuine))
    {
        if let Some(input_utxo) = &op.input {
            if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
                purpose: MarketplacePurpose::Sale,
                ..
            })) = lookup_address(&input_utxo.address)
            {
                if op.is_native_token() && op.amount() == 1 {
                    if let Some(asset) = op.payload.get_asset() {
                        let asset_key = asset.dot_delimited();
                        input_assets.insert(
                            asset_key,
                            (input_utxo.address.clone(), op.input_datum.clone()),
                        );
                    }
                }
            }
        }
    }

    // Collect output assets to marketplace sale addresses (genuine operations only)
    for op in context
        .asset_operations
        .iter()
        .filter(|op| matches!(op.classification, crate::OperationClassification::Genuine))
    {
        if let Some(output_utxo) = &op.output {
            if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
                purpose: MarketplacePurpose::Sale,
                ..
            })) = lookup_address(&output_utxo.address)
            {
                if op.is_native_token() && op.amount() == 1 {
                    if let Some(asset) = op.payload.get_asset() {
                        let asset_key = asset.dot_delimited();
                        output_assets.insert(
                            asset_key,
                            (output_utxo.address.clone(), op.output_datum.clone()),
                        );
                    }
                }
            }
        }
    }

    let mut results = Vec::new();

    // Find assets that appear in both input and output (listing updates)
    for (asset_key, (input_address, input_datum)) in &input_assets {
        if let Some((output_address, output_datum)) = output_assets.get(asset_key) {
            // Same asset going from marketplace to marketplace = listing update
            debug!(
                "Potential listing update detected for asset: {} from {} to {}",
                asset_key, input_address, output_address
            );

            // Check if datum actually changed (different hashes = update)
            let datum_changed = match (input_datum, output_datum) {
                (Some(input_d), Some(output_d)) => {
                    // Compare datum hashes or content
                    input_d != output_d
                }
                (Some(_), None) | (None, Some(_)) => true, // One has datum, other doesn't
                (None, None) => false,                     // Both None, no change detectable
            };

            // Only proceed if datum actually changed
            if datum_changed {
                // Get marketplace fee calculation function from address registry
                let _fee_calculation = get_marketplace_fee_calculation(output_address);

                // Extract old and new prices from datums (if available)
                // For JPG.store v3 listings, add marketplace fee to get the gross listing price
                // Use schema-driven pricing extraction for both old and new prices
                let old_price = if let Some(datum) = input_datum.as_ref() {
                    super::sales::try_schema_driven_pricing_extraction(datum, input_address)
                        .map(|pricing| pricing.total_price_lovelace)
                        .unwrap_or(0)
                } else {
                    0
                };

                let new_price = if let Some(datum) = output_datum.as_ref() {
                    super::sales::try_schema_driven_pricing_extraction(datum, output_address)
                        .map(|pricing| pricing.total_price_lovelace)
                        .unwrap_or(0)
                } else {
                    0
                };

                // Identify the actual seller (fee payer) from the transaction
                let seller = super::utils::identify_fee_paying_address(context.raw_tx_data)
                    .unwrap_or_else(|| "unknown".to_string());

                // Parse delimited asset ID (policy_id.asset_name_hex) back to AssetId
                let parts: Vec<&str> = asset_key.split('.').collect();
                let Some(asset) = (if parts.len() == 2 {
                    AssetId::new(parts[0].to_string(), parts[1].to_string()).ok()
                } else {
                    AssetId::parse_concatenated(asset_key).ok()
                }) else {
                    continue;
                };

                // Calculate price delta if both prices are available
                let (final_price, delta) = if old_price > 0 && new_price > 0 {
                    let price_delta = new_price as i64 - old_price as i64;
                    (Some(new_price), Some(price_delta))
                } else if old_price > 0 {
                    // Old price known, new price unknown - still provide old price for partial info
                    (Some(old_price), None)
                } else if new_price > 0 {
                    // New price known, old price unknown
                    (Some(new_price), None)
                } else {
                    // Neither price available
                    (None, None)
                };

                results.push((
                    TxType::ListingUpdate {
                        assets: vec![PricedAsset {
                            asset,
                            price_lovelace: final_price,
                            delta_lovelace: delta,
                        }],
                        total_listing_count: 1,
                        seller,
                        marketplace: output_address.clone(),
                    },
                    0.80, // Good confidence for marketplace-to-marketplace with datum change
                ));

                debug!(
                    "DETECTED LISTING UPDATE: {} datum changed (old price: {}, new price: {})",
                    asset_key,
                    if old_price > 0 {
                        format!("₳{:.2}", old_price as f64 / 1_000_000.0)
                    } else {
                        "unknown".to_string()
                    },
                    if new_price > 0 {
                        format!("₳{:.2}", new_price as f64 / 1_000_000.0)
                    } else {
                        "unknown".to_string()
                    }
                );
            }
        }
    }

    results
}

/// Detect asset unlisting (removal from marketplace)
fn detect_unlisting(context: &PatternContext) -> Vec<(TxType, f64)> {
    let mut results = Vec::new();

    // Look for genuine asset operations where NFTs are being unlocked (coming FROM marketplace script addresses)
    let unlock_operations: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            matches!(op.classification, crate::OperationClassification::Genuine) // Only genuine operations
                && matches!(op.op_type, AssetOpType::Unlock)
                && op.is_native_token() // Only look at NFT unlisting, not ADA
                && op.amount() == 1 // NFTs typically have amount of 1
        })
        .collect();

    if unlock_operations.is_empty() {
        return results;
    }

    debug!(
        "Found {} unlock operations for potential unlisting",
        unlock_operations.len()
    );

    // Group unlocks by marketplace address to detect batch unlisting
    let mut marketplace_unlocks: BTreeMap<String, Vec<&AssetOperation>> = BTreeMap::new();

    for op in &unlock_operations {
        if let Some(from_utxo) = &op.input {
            // Check if this is coming from a known marketplace sale address (JPG.store pattern)
            if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
                purpose: MarketplacePurpose::Sale,
                ..
            })) = lookup_address(&from_utxo.address)
            {
                marketplace_unlocks
                    .entry(from_utxo.address.clone())
                    .or_default()
                    .push(op);
            }
        }
    }

    // If no traditional marketplace unlocks found, check for Wayup pattern
    if marketplace_unlocks.is_empty() {
        debug!("No unlock operations from known marketplace sale addresses found, checking for Wayup pattern");

        // Check if this transaction has Wayup reference inputs and Wayup datums
        if let Some(wayup_marketplace_address) = detect_wayup_unlisting(context, &unlock_operations)
        {
            debug!(
                "Detected Wayup unlisting pattern, processing {} unlock operations",
                unlock_operations.len()
            );
            marketplace_unlocks.insert(wayup_marketplace_address, unlock_operations);
        }
    }

    if marketplace_unlocks.is_empty() {
        debug!("No marketplace unlisting patterns detected");
        return results;
    }

    // Process each marketplace's unlocks
    for (marketplace_address, ops) in marketplace_unlocks {
        debug!(
            "Processing {} assets being unlisted from marketplace: {}",
            ops.len(),
            marketplace_address
        );

        // Find the destination address (should be consistent - the seller reclaiming assets)
        let seller = ops[0]
            .output
            .as_ref()
            .map(|utxo| utxo.address.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // Verify all operations go to the same seller
        let all_same_seller = ops
            .iter()
            .all(|op| op.output.as_ref().map(|utxo| &utxo.address) == Some(&seller));

        if !all_same_seller {
            debug!("Multiple destination addresses detected, skipping");
            continue;
        }

        // Before proceeding with unlisting, check if there are significant ADA flows
        // which would indicate this is actually a sale transaction, not an unlisting
        // For bundle sales, all assets share the same datum price, so we should take just one sample
        let expected_bundle_value = ops
            .first()
            .and_then(|op| {
                op.input_datum.as_ref().and_then(|datum| {
                    super::sales::try_schema_driven_pricing_extraction(datum, &marketplace_address)
                        .map(|pricing| pricing.total_price_lovelace)
                })
            })
            .unwrap_or(0);

        debug!(
            "Unlisting detection: found {} ops with expected bundle value {} lovelace (₳{:.2})",
            ops.len(),
            expected_bundle_value,
            expected_bundle_value as f64 / 1_000_000.0
        );

        if expected_bundle_value > 0 {
            // Check for significant ADA flows that would indicate a sale
            // Note: For Wayup, large ADA flows are normal due to reclaimed locked ADA (1.34 ADA per asset)
            let is_wayup =
                Marketplace::from_address(&marketplace_address) == Some(Marketplace::Wayup);

            if !is_wayup {
                // Only apply ADA flow analysis for non-Wayup marketplaces
                let ada_flows: Vec<_> = context
                    .asset_operations
                    .iter()
                    .filter_map(|op| {
                        if let OperationPayload::Lovelace { amount } = &op.payload {
                            if *amount > 1_000_000 {
                                // More than 1 ADA
                                Some(amount)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect();

                let total_ada_flows: u64 = ada_flows.iter().map(|&amount| *amount).sum();
                let reasonable_threshold = expected_bundle_value / 4; // At least 25% of expected value

                if total_ada_flows >= reasonable_threshold {
                    debug!(
                        "Significant ADA flows detected ({} ADA) for expected bundle value ({} ADA) - likely a sale, not unlisting",
                        total_ada_flows as f64 / 1_000_000.0,
                        expected_bundle_value as f64 / 1_000_000.0
                    );
                    continue; // Skip this marketplace - it's likely a sale transaction
                }
            } else {
                debug!(
                    "Wayup marketplace detected - skipping ADA flow analysis (large ADA flows are normal due to reclaimed locked ADA)"
                );
            }
        }

        // Collect the assets being unlisted
        let mut unlisted_assets: Vec<AssetId> = Vec::new();

        for op in &ops {
            if let Some(asset_id_str) = op.asset_id() {
                let Some(asset) = AssetId::parse_concatenated(&asset_id_str).ok() else {
                    continue;
                };

                // Try to extract the listing price from input datum using schema-driven approach
                let _listing_price = op.input_datum.as_ref().and_then(|datum| {
                    super::sales::try_schema_driven_pricing_extraction(datum, &marketplace_address)
                        .map(|pricing| pricing.total_price_lovelace)
                });

                unlisted_assets.push(asset);
            }
        }

        if unlisted_assets.is_empty() {
            debug!("No valid assets found for unlisting");
            continue;
        }

        let unlisting_count = unlisted_assets.len() as u32;

        results.push((
            TxType::Unlisting {
                assets: unlisted_assets,
                total_unlisting_count: unlisting_count,
                seller: seller.clone(),
                marketplace: marketplace_address.clone(),
            },
            0.85, // High confidence for clear marketplace unlisting
        ));

        debug!(
            "Detected unlisting: {} assets from {} back to {}",
            unlisting_count, marketplace_address, seller
        );
    }

    results
}

/// Extract asset price from transaction UTXO data using schema-driven parsing
fn extract_asset_price_from_utxo(
    context: &PatternContext,
    asset_id: &str,
    marketplace_address: &str,
) -> Option<u64> {
    // Look for the asset in output UTXOs and extract price from datum
    for output in &context.raw_tx_data.outputs {
        if output.assets.contains_key(asset_id) {
            if let Some(datum) = &output.datum {
                if let Some(marketplace_pricing) =
                    super::sales::try_schema_driven_pricing_extraction(datum, marketplace_address)
                {
                    return Some(marketplace_pricing.total_price_lovelace);
                }
            }
        }
    }
    None
}

/// Detect Wayup unlisting pattern by checking for:
/// 1. Reference inputs with Wayup script addresses
/// 2. Unlock operations with Wayup datum patterns
fn detect_wayup_unlisting(
    context: &PatternContext,
    unlock_operations: &[&AssetOperation],
) -> Option<String> {
    use address_registry::{AddressCategory, Marketplace, ScriptCategory};

    // Check if transaction has reference inputs with Wayup scripts
    let has_wayup_reference = context
        .raw_tx_data
        .reference_inputs
        .iter()
        .any(|ref_input| {
            if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
                marketplace: Marketplace::Wayup,
                ..
            })) = lookup_address(&ref_input.address)
            {
                debug!(
                    "Found Wayup reference input at address: {}",
                    ref_input.address
                );
                return true;
            }
            false
        });

    if !has_wayup_reference {
        debug!("No Wayup reference inputs found");
        return None;
    }

    // Check if unlock operations have Wayup datum patterns
    let wayup_marketplace_addr = "addr1zxnk7racqx3f7kg7npc4weggmpdskheu8pm57egr9av0mtvasazx8r5xwqtnfjsfrnat3h6yrycd2hfm9qpg7d0hf50s7x4y79";
    let wayup_datum_count = unlock_operations
        .iter()
        .filter(|op| {
            if let Some(datum) = &op.input_datum {
                // Try to parse with Wayup schema to confirm this is a Wayup listing
                super::sales::try_schema_driven_pricing_extraction(datum, wayup_marketplace_addr)
                    .is_some()
            } else {
                false
            }
        })
        .count();

    debug!(
        "Found {} unlock operations with Wayup datum patterns out of {}",
        wayup_datum_count,
        unlock_operations.len()
    );

    // If we have Wayup reference inputs and at least some Wayup datums, consider this a Wayup unlisting
    if wayup_datum_count > 0 {
        // Return the known Wayup marketplace address for consistency
        Some(wayup_marketplace_addr.to_string())
    } else {
        None
    }
}

/// Get the fee calculation function for a marketplace address
fn get_marketplace_fee_calculation(marketplace_address: &str) -> FeeCalculationFn {
    match lookup_address(marketplace_address) {
        Some(AddressCategory::Script(ScriptCategory::Marketplace {
            fee_calculation, ..
        })) => *fee_calculation,
        _ => no_fee_calculation,
    }
}
