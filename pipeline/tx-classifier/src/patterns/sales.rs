//! Sales transaction pattern detection

use super::{PatternContext, PatternDetectionResult};
use crate::registry::{
    lookup_address, no_fee_calculation, AddressCategory, FeeCalculationFn, MarketplacePurpose,
    ScriptCategory,
};
use crate::*;
use pipeline_types::{OperationPayload, PricedAsset};
use std::collections::HashMap;
use tracing::debug;

// Import for schema-driven pricing extraction
use datum_parsing::{
    MarketplaceDatumParser, MarketplaceOperation as DatumMarketplaceOperation, MarketplaceType,
};
use once_cell::sync::Lazy;
use std::sync::Mutex;

// Global cache for marketplace type lookups (thread-safe for Cloudflare Workers)
static MARKETPLACE_TYPE_CACHE: Lazy<Mutex<HashMap<String, MarketplaceType>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Detect sales based on asset and ADA flows
pub fn detect_sales_wrapper(context: &PatternContext<'_>) -> PatternDetectionResult {
    let mut transactions = Vec::new();

    // Run marketplace sales detection
    transactions.extend(detect_marketplace_sales(context));

    // Run direct sales detection
    transactions.extend(detect_direct_sale(context));

    PatternDetectionResult { transactions }
}

/// Detect marketplace sales with bundle support
fn detect_marketplace_sales(context: &PatternContext<'_>) -> Vec<(TxType, f64)> {
    let mut result = Vec::new();

    // Group asset operations by input UTXO to detect bundle sales
    let mut utxo_groups: HashMap<(String, u32), Vec<&AssetOperation>> = HashMap::new();

    // Collect all sale operations and group by input UTXO
    for op in context.asset_operations {
        if let AssetOperation {
            op_type: AssetOpType::Unlock,
            input:
                Some(TxUtxo {
                    address: seller,
                    idx,
                }),
            output: Some(TxUtxo {
                address: _buyer, ..
            }),
            ..
        } = op
        {
            if let Some(MarketplacePurpose::Sale) = MarketplacePurpose::from_address(seller) {
                utxo_groups
                    .entry((seller.clone(), *idx))
                    .or_default()
                    .push(op);
            }
        }
    }

    // Process each UTXO group (bundle)
    for ((seller, _), ops) in utxo_groups {
        // Resolve marketplace name from address registry
        let marketplace_name = Marketplace::from_address(&seller)
            .filter(|m| !matches!(m, Marketplace::Unknown))
            .map(|m| m.to_string());

        // Get marketplace fee calculation function from address registry
        let fee_calculation = get_marketplace_fee_calculation(&seller);

        // Get assets from this bundle
        let bundle_assets: Vec<_> = ops.iter().filter_map(|op| op.payload.get_asset()).collect();
        if bundle_assets.is_empty() {
            continue;
        }

        // Extract individual asset prices using improved logic
        let asset_prices = extract_individual_asset_prices(&ops, &seller, fee_calculation);

        // Calculate bundle total for verification (sum of individual prices)
        let bundle_total: u64 = asset_prices.values().sum();

        // Verify this is actually a sale by checking for significant ADA flows to sellers
        // For transactions with successful schema-driven price extraction, we can be more lenient
        // since the datum provides strong evidence of a purchase transaction
        let has_schema_pricing = ops.iter().any(|op| {
            op.input_datum
                .as_ref()
                .and_then(|datum| try_schema_driven_pricing_extraction(datum, &seller))
                .is_some()
        });

        // Check if this is a V4 marketplace (datum confirmed but no price in datum)
        let is_v4_marketplace = matches!(
            determine_marketplace_type(&seller),
            MarketplaceType::JpgStoreV4
        );

        // For V4, the datum has no price — we must use ADA flow analysis exclusively.
        // Use a permissive expected amount (0) so verify_actual_sale_flows just checks
        // for any significant ADA movement.
        let flow_expected = if is_v4_marketplace { 0 } else { bundle_total };
        let actual_ada_flows = verify_actual_sale_flows(context, &seller, flow_expected);

        if is_v4_marketplace {
            // V4: price comes entirely from ADA flows — skip if no flows found
            if actual_ada_flows.is_none() {
                debug!(
                    "JPG.store V4 marketplace {} — no ADA flows detected, skipping",
                    seller
                );
                continue;
            }
        } else if actual_ada_flows.is_none() && !has_schema_pricing {
            debug!(
                "No actual ADA flows detected for marketplace {} with {} expected bundle total, and no schema pricing found - likely not a sale transaction",
                seller, bundle_total
            );
            continue; // Skip this - not a real sale
        } else if actual_ada_flows.is_none() && has_schema_pricing {
            debug!(
                "No significant ADA flows detected for marketplace {} but schema pricing found - proceeding as marketplace handles payment distribution",
                seller
            );
        }
        let total_ada_flows = actual_ada_flows.unwrap_or(0);

        // Determine final sale price.
        // The datum total typically captures seller payout + royalties but NOT the marketplace
        // platform fee. ADA flows capture all on-chain payments including fees.
        // Use ADA flows when they're slightly above datum (within ~15%), indicating the
        // difference is just the marketplace fee. When ADA flows are much higher, the extra
        // ADA is likely from other non-sale movements (sweeps, change, etc.) — use datum.
        let final_bundle_total = if is_v4_marketplace {
            debug!("JPG.store V4: using ADA flow total {total_ada_flows} lovelace as sale price");
            total_ada_flows
        } else if bundle_total > 0 && total_ada_flows > bundle_total {
            let ratio = total_ada_flows as f64 / bundle_total as f64;
            if ratio <= 1.15 {
                // ADA flows within 15% of datum — likely just the marketplace fee on top.
                debug!(
                    "Using ADA flow total {total_ada_flows} lovelace over datum total {bundle_total} lovelace ({:.1}% higher — marketplace fee)",
                    (ratio - 1.0) * 100.0
                );
                total_ada_flows
            } else {
                // ADA flows significantly higher — likely includes non-sale ADA movements.
                debug!(
                    "Using datum total {bundle_total} lovelace (ADA flows {total_ada_flows} are {:.1}% higher — likely non-sale movements)",
                    (ratio - 1.0) * 100.0
                );
                bundle_total
            }
        } else {
            // ADA flows are lower than datum or zero — fall back to datum total
            debug!(
                "Using datum total {bundle_total} lovelace (ADA flows {total_ada_flows} lovelace)"
            );
            bundle_total
        };

        // For marketplace sales, find the actual seller by looking at payment flows
        let actual_seller = find_marketplace_seller(context, &seller, final_bundle_total)
            .unwrap_or_else(|| seller.clone());

        // Create sale for each asset with its individual price
        let valid_asset_count = ops
            .iter()
            .filter(|op| op.payload.get_asset().is_some())
            .count() as u64;

        for op in &ops {
            if let (Some(asset), Some(buyer)) = (
                op.payload.get_asset(),
                op.output.as_ref().map(|utxo| &utxo.address),
            ) {
                // For V4 (no price in datum), distribute ADA flow total equally among assets
                let asset_price = if is_v4_marketplace {
                    (final_bundle_total)
                        .checked_div(valid_asset_count)
                        .unwrap_or(final_bundle_total)
                } else {
                    // Get the individual price for this specific asset, adjust if using ADA flows
                    let raw_asset_price = asset_prices
                        .get(&asset.concatenated())
                        .copied()
                        .unwrap_or(0);

                    if final_bundle_total != bundle_total && bundle_total > 0 {
                        // Scale the asset price proportionally to the corrected total
                        (raw_asset_price as f64 * final_bundle_total as f64 / bundle_total as f64)
                            as u64
                    } else {
                        raw_asset_price
                    }
                };

                if asset_price > 0 {
                    result.push((
                        TxType::Sale {
                            asset: PricedAsset {
                                asset,
                                price_lovelace: Some(asset_price),
                                delta_lovelace: None, // No delta information available for sales
                            },
                            breakdown: SaleBreakdown {
                                total_lovelace: asset_price,
                            },
                            seller: actual_seller.clone(),
                            buyer: buyer.clone(),
                            marketplace: marketplace_name.clone(),
                        },
                        0.9,
                    ));
                }
            }
        }
    }

    result
}

/// Detect direct peer-to-peer sales
fn detect_direct_sale(context: &PatternContext) -> Vec<(TxType, f64)> {
    // Count genuine transfers to avoid conflicts
    let genuine_transfers: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            op.op_type == AssetOpType::Transfer
                && match (&op.input, &op.output) {
                    (Some(from_utxo), Some(to_utxo)) => from_utxo.address != to_utxo.address, // Only genuine transfers
                    _ => false,
                }
        })
        .collect();

    // Only handle single transfers - let detect_multiple_sales handle multiple transfers
    if genuine_transfers.len() != 1 {
        return vec![];
    }

    // Similar to marketplace sale but without smart contract
    if !context.scripts.is_empty() {
        return vec![]; // Has scripts, let detect_marketplace_sale handle this
    }

    // IMPORTANT: For direct sales, we should only run if we have clear evidence of payment
    // But since we're focusing on genuine operations only, we should not assume sale prices
    // without actual ADA flow analysis. Let AssetTransfer handle simple transfers.
    //
    // DirectSale should only run when there's clear evidence of payment in the operations themselves,
    // which is difficult to determine from asset operations alone.
    //
    // For now, return vec![] to let AssetTransfer handle asset movements without script involvement
    vec![]
}

/// Extract individual asset prices from a bundle
fn extract_individual_asset_prices(
    ops: &[&AssetOperation],
    marketplace_address: &str,
    fee_calculation: crate::registry::FeeCalculationFn,
) -> HashMap<String, u64> {
    let mut asset_prices = HashMap::new();

    // Try to extract individual prices from datum using schema-driven approach first
    if let Some(first_op) = ops.first() {
        if let Some(datum) = &first_op.input_datum {
            // Primary: Use schema-driven pricing extraction
            if let Some(marketplace_pricing) =
                try_schema_driven_pricing_extraction(datum, marketplace_address)
            {
                let total_price = marketplace_pricing.total_price_lovelace;
                debug!(
                    "Using schema-driven price extraction: {} lovelace total for {} assets",
                    total_price,
                    ops.len()
                );

                // Determine pricing distribution based on marketplace and asset count
                if ops.len() > 1 {
                    // Check if all assets come from the same input UTXO (bundle purchase)
                    let first_input = ops[0].input.as_ref().map(|utxo| (&utxo.address, utxo.idx));
                    let is_bundle = ops.iter().all(|op| {
                        op.input.as_ref().map(|utxo| (&utxo.address, utxo.idx)) == first_input
                    });

                    if is_bundle {
                        // Bundle purchase: divide total price among valid assets
                        let valid_asset_count = ops
                            .iter()
                            .filter(|op| op.payload.get_asset().is_some())
                            .count() as u64;
                        let price_per_asset = total_price
                            .checked_div(valid_asset_count)
                            .unwrap_or(total_price);
                        debug!("Bundle purchase detected: {} valid assets (of {} ops) sharing total price {} = {} each",
                               valid_asset_count, ops.len(), total_price, price_per_asset);
                        for op in ops {
                            if let Some(asset) = op.payload.get_asset() {
                                asset_prices.insert(asset.concatenated(), price_per_asset);
                            }
                        }
                    } else {
                        // Marketplace sweep: each asset gets full price from its own datum
                        debug!(
                            "Marketplace sweep detected: {} assets each getting full price {}",
                            ops.len(),
                            total_price
                        );
                        for op in ops {
                            if let Some(asset) = op.payload.get_asset() {
                                asset_prices.insert(asset.concatenated(), total_price);
                            }
                        }
                    }
                } else {
                    // Single asset: gets full price
                    for op in ops {
                        if let Some(asset) = op.payload.get_asset() {
                            asset_prices.insert(asset.concatenated(), total_price);
                        }
                    }
                }
            } else {
                // Fallback to legacy extraction methods only when schema-driven fails
                debug!(
                    "Schema-driven parsing failed, using legacy extraction for {}",
                    marketplace_address
                );
                let datum_values = datum.extract_all_monetary_values();
                let datum_total: u64 = datum_values.iter().sum();

                // Calculate marketplace fee based on the datum total
                let marketplace_fee = fee_calculation(datum_total, marketplace_address);

                // For bundles with multiple assets, try to extract individual prices
                if ops.len() == 1 {
                    // Single asset - use datum total + marketplace fee
                    if let Some(asset) = ops[0].payload.get_asset() {
                        let asset_price = datum_total + marketplace_fee;
                        asset_prices.insert(asset.concatenated(), asset_price);
                    }
                } else if datum_values.len() == ops.len() {
                    // If we have individual values matching asset count, use them
                    for (i, op) in ops.iter().enumerate() {
                        if let Some(asset) = op.payload.get_asset() {
                            let individual_price =
                                datum_values[i] + (marketplace_fee / ops.len() as u64);
                            asset_prices.insert(asset.concatenated(), individual_price);
                        }
                    }
                } else {
                    // Fallback: split datum total equally among assets
                    let price_per_asset = (datum_total + marketplace_fee) / ops.len() as u64;
                    for op in ops {
                        if let Some(asset) = op.payload.get_asset() {
                            asset_prices.insert(asset.concatenated(), price_per_asset);
                        }
                    }
                }
            }
        }
    }

    asset_prices
}

/// Find the actual seller from marketplace payment flows
fn find_marketplace_seller(
    context: &PatternContext,
    marketplace_address: &str,
    expected_payment: u64,
) -> Option<String> {
    // First, try to extract payment distribution from datum (most accurate)
    if let Some(seller) = extract_seller_from_datum_payments(context, marketplace_address) {
        debug!("Found seller from datum payment distribution: {}", seller);
        return Some(seller);
    }

    // Fallback to payment flow analysis
    find_seller_from_payment_flows(context, marketplace_address, expected_payment)
}

/// Extract seller from datum payment distributions (JPG.store style)
fn extract_seller_from_datum_payments(
    context: &PatternContext,
    marketplace_address: &str,
) -> Option<String> {
    // Look for marketplace operations with datums
    for op in context.asset_operations {
        if let AssetOperation {
            input: Some(input_utxo),
            input_datum: Some(datum),
            ..
        } = op
        {
            if input_utxo.address == marketplace_address {
                // Try schema-driven payment extraction first
                if let Some(payments) =
                    try_schema_driven_payment_extraction(datum, marketplace_address)
                {
                    // Find the largest payment to a valid Cardano address (likely the seller)
                    if let Some((seller_address, amount)) = payments
                        .iter()
                        .filter(|(addr, _)| addr.starts_with("addr"))
                        .max_by_key(|(_, amount)| *amount)
                    {
                        debug!(
                            "Schema-driven payment distribution: {} payments, largest ₳{:.2} to {}",
                            payments.len(),
                            *amount as f64 / 1_000_000.0,
                            seller_address
                        );
                        return Some(seller_address.clone());
                    }
                }
            }
        }
    }
    None
}

/// Fallback: Find seller from payment flows when datum parsing fails
fn find_seller_from_payment_flows(
    context: &PatternContext,
    _marketplace_address: &str,
    expected_payment: u64,
) -> Option<String> {
    let tolerance = expected_payment / 10; // 10% tolerance

    // Look for ADA flows that match expected payment amounts
    for op in context.asset_operations {
        if let OperationPayload::Lovelace { amount } = &op.payload {
            if (*amount as i64 - expected_payment as i64).abs() < tolerance as i64 {
                if let Some(output) = &op.output {
                    // Found a payment that matches our expectation
                    return Some(output.address.clone());
                }
            }
        }
    }

    None
}

/// Verify that actual sale flows are present in the transaction and return the total
fn verify_actual_sale_flows(
    context: &PatternContext,
    _marketplace_address: &str,
    expected_total_payment: u64,
) -> Option<u64> {
    // Check for significant ADA operations that would indicate actual payments
    let ada_flows: Vec<_> = context
        .asset_operations
        .iter()
        .filter_map(|op| {
            if let OperationPayload::Lovelace { amount } = &op.payload {
                // Look for ADA transfers that could be seller payments
                // Exclude small amounts that are likely just fees/change
                if *amount > 1_000_000u64 {
                    // More than 1 ADA
                    Some((amount, op.output.as_ref()?.address.clone()))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if ada_flows.is_empty() {
        debug!("No significant ADA flows found in transaction");
        return None;
    }

    // Check if total ADA flows are reasonable compared to expected payment
    let total_ada_flows: u64 = ada_flows.iter().map(|(amount, _)| **amount).sum();
    let reasonable_threshold = expected_total_payment / 2; // At least 50% of expected

    if total_ada_flows < reasonable_threshold {
        debug!(
            "Total ADA flows ({} ADA) below reasonable threshold ({} ADA) for expected payment ({} ADA)",
            total_ada_flows as f64 / 1_000_000.0,
            reasonable_threshold as f64 / 1_000_000.0,
            expected_total_payment as f64 / 1_000_000.0
        );
        return None;
    }

    debug!(
        "Verified actual sale flows: {} ADA total flows",
        total_ada_flows as f64 / 1_000_000.0
    );
    Some(total_ada_flows)
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

/// Try schema-driven payment distribution extraction using the new datum-parsing crate
pub fn try_schema_driven_payment_extraction(
    datum: &transactions::TxDatum,
    marketplace_address: &str,
) -> Option<Vec<(String, u64)>> {
    // Determine marketplace type based on address
    let marketplace_type = determine_marketplace_type(marketplace_address);

    // Extract CBOR bytes from datum
    let cbor_bytes = match datum.bytes() {
        Some(bytes) => hex::decode(bytes).ok()?,
        None => {
            debug!(
                "No CBOR bytes available in datum for marketplace {}",
                marketplace_address
            );
            return None;
        }
    };

    // Try new datum-parsing approach
    let parser = MarketplaceDatumParser::new(marketplace_type);
    match parser.parse_cbor(&cbor_bytes) {
        Ok(datum_operation) => {
            debug!(
                "Successfully parsed datum for payment distribution with new parser for marketplace {}",
                marketplace_address
            );

            // Extract payment distribution from the operation
            let payments = datum_operation.get_payment_distribution();
            if payments.is_empty() {
                None
            } else {
                Some(payments)
            }
        }
        Err(e) => {
            debug!(
                "New datum parser failed for payment distribution for marketplace {}: {}",
                marketplace_address, e
            );
            None
        }
    }
}

/// Try schema-driven offer amount extraction using the new datum-parsing crate
pub fn try_schema_driven_offer_extraction(
    datum: &transactions::TxDatum,
    marketplace_address: &str,
) -> Option<u64> {
    // Determine marketplace type based on address
    let marketplace_type = determine_marketplace_type(marketplace_address);

    // Extract CBOR bytes from datum
    let cbor_bytes = match datum.bytes() {
        Some(bytes) => hex::decode(bytes).ok()?,
        None => {
            debug!(
                "No CBOR bytes available in datum for marketplace {}",
                marketplace_address
            );
            return None;
        }
    };

    // Try new datum-parsing approach
    let parser = MarketplaceDatumParser::new(marketplace_type);
    match parser.parse_cbor(&cbor_bytes) {
        Ok(datum_operation) => {
            debug!(
                "Successfully parsed datum for offer extraction with new parser for marketplace {}",
                marketplace_address
            );

            // Extract offer amount from the operation
            match datum_operation {
                DatumMarketplaceOperation::Bid { offer_lovelace, .. } => Some(offer_lovelace),
                DatumMarketplaceOperation::Ask { .. } => {
                    // Ask operations don't have offer amounts
                    None
                }
            }
        }
        Err(e) => {
            debug!(
                "New datum parser failed for offer extraction for marketplace {}: {}",
                marketplace_address, e
            );
            None
        }
    }
}

/// Try schema-driven policy and asset extraction using the new datum-parsing crate
pub fn try_schema_driven_asset_extraction(
    datum: &transactions::TxDatum,
    marketplace_address: &str,
) -> Option<(String, Option<String>)> {
    // Determine marketplace type based on address
    let marketplace_type = determine_marketplace_type(marketplace_address);

    // Extract CBOR bytes from datum
    let cbor_bytes = match datum.bytes() {
        Some(bytes) => hex::decode(bytes).ok()?,
        None => {
            debug!(
                "No CBOR bytes available in datum for marketplace {}",
                marketplace_address
            );
            return None;
        }
    };

    // Try new datum-parsing approach
    let parser = MarketplaceDatumParser::new(marketplace_type);
    match parser.parse_cbor(&cbor_bytes) {
        Ok(datum_operation) => {
            debug!(
                "Successfully parsed datum for asset extraction with new parser for marketplace {}",
                marketplace_address
            );

            // Extract policy ID and asset name from the operation
            let policy_id = datum_operation.get_policy_id()?.to_string();
            let asset_name = datum_operation
                .get_asset()
                .map(|asset| asset.asset_name_hex);
            Some((policy_id, asset_name))
        }
        Err(e) => {
            debug!(
                "New datum parser failed for asset extraction for marketplace {}: {}",
                marketplace_address, e
            );
            None
        }
    }
}

/// Try schema-driven pricing extraction using the new datum-parsing crate
/// This is the primary datum parsing method for all marketplaces
pub fn try_schema_driven_pricing_extraction(
    datum: &transactions::TxDatum,
    marketplace_address: &str,
) -> Option<MarketplacePricing> {
    // Determine marketplace type based on address
    let marketplace_type = determine_marketplace_type(marketplace_address);

    // Extract CBOR bytes from datum
    let cbor_bytes = match datum.bytes() {
        Some(bytes) => hex::decode(bytes).ok()?,
        None => {
            debug!(
                "No CBOR bytes available in datum for marketplace {}",
                marketplace_address
            );
            return None;
        }
    };

    // For Wayup, use schema-driven approach with legacy semantics

    // Try new datum-parsing approach
    let parser = MarketplaceDatumParser::new(marketplace_type);
    match parser.parse_cbor(&cbor_bytes) {
        Ok(datum_operation) => {
            debug!(
                "Successfully parsed datum with new parser for marketplace {}",
                marketplace_address
            );

            // Convert datum-parsing result to MarketplacePricing
            convert_datum_operation_to_pricing(datum_operation, marketplace_address)
        }
        Err(e) => {
            debug!(
                "Schema-driven datum parser failed for marketplace {}: {}",
                marketplace_address, e
            );

            // No legacy fallback - rely on schema-driven approach only
            None
        }
    }
}

/// Determine marketplace type from address using cached lookup
fn determine_marketplace_type(marketplace_address: &str) -> MarketplaceType {
    // Check cache first
    if let Ok(cache) = MARKETPLACE_TYPE_CACHE.lock() {
        if let Some(&cached_type) = cache.get(marketplace_address) {
            debug!(
                "Using cached marketplace type {:?} for address {}",
                cached_type, marketplace_address
            );
            return cached_type;
        }
    }

    // Not in cache, compute and store
    let marketplace_type = determine_marketplace_type_uncached(marketplace_address);

    // Store in cache (ignore errors to avoid blocking)
    if let Ok(mut cache) = MARKETPLACE_TYPE_CACHE.lock() {
        cache.insert(marketplace_address.to_string(), marketplace_type);
    }

    marketplace_type
}

/// Determine marketplace type from address using address registry (uncached)
pub fn determine_marketplace_type_uncached(marketplace_address: &str) -> MarketplaceType {
    // Use address registry to determine marketplace type - this should eliminate the churn
    // of trying multiple schemas by getting the exact type directly from the registry
    match lookup_address(marketplace_address) {
        Some(AddressCategory::Script(ScriptCategory::Marketplace { kind, .. })) => {
            debug!(
                "Found marketplace type {:?} for address {} in registry",
                kind, marketplace_address
            );
            *kind
        }
        _ => {
            debug!(
                "Address {} not found in registry, using Unknown marketplace type",
                marketplace_address
            );
            MarketplaceType::Unknown
        }
    }
}

/// Convert datum-parsing MarketplaceOperation to MarketplacePricing
fn convert_datum_operation_to_pricing(
    operation: DatumMarketplaceOperation,
    marketplace_address: &str,
) -> Option<MarketplacePricing> {
    match operation {
        DatumMarketplaceOperation::Ask { targets, .. } => {
            // Calculate total price from targets
            let total_price: u64 = targets
                .iter()
                .filter_map(|target| {
                    if let OperationPayload::Lovelace { amount } = &target.payload {
                        Some(*amount)
                    } else {
                        None
                    }
                })
                .sum();

            if total_price > 0 {
                // Get fee calculation for proper breakdown
                let fee_calculation = get_marketplace_fee_calculation(marketplace_address);

                // Determine marketplace type to handle fee semantics correctly
                let marketplace_type = determine_marketplace_type_uncached(marketplace_address);

                let (base_price, marketplace_fee, final_total_price) = match marketplace_type {
                    MarketplaceType::Wayup => {
                        // For Wayup, our analysis shows the schema extraction gives us base seller prices
                        // (same as legacy approach), so we add fees to get total buyer price
                        let base_price = total_price; // Schema extracted amount is base price
                        let calculated_fee = fee_calculation(base_price, marketplace_address);
                        let total_with_fees = base_price + calculated_fee;

                        debug!(
                            "Wayup schema-driven pricing (legacy semantics): base={}, fee={}, total={}",
                            base_price, calculated_fee, total_with_fees
                        );

                        (base_price, calculated_fee, total_with_fees)
                    }
                    _ => {
                        // For JPG.store and others, the schema extraction gives us base prices
                        // We need to add external marketplace fees
                        let base_price = total_price; // Schema extracted amount is base price
                        let marketplace_fee = fee_calculation(base_price, marketplace_address);
                        let total_with_fees = base_price + marketplace_fee;

                        debug!(
                            "JPG.store schema-driven pricing: base={}, external_fee={}, total={}",
                            base_price, marketplace_fee, total_with_fees
                        );

                        (base_price, marketplace_fee, total_with_fees)
                    }
                };

                Some(MarketplacePricing {
                    base_price_lovelace: base_price,
                    marketplace_fee_lovelace: marketplace_fee,
                    total_price_lovelace: final_total_price,
                    payout_count: targets.len(),
                    expires_at: None,
                    extraction_method: super::marketplace_pricing::ExtractionMethod::CddlValidated, // New parser
                    schema_key: Some("toml_schema".to_string()),
                })
            } else {
                None
            }
        }
        DatumMarketplaceOperation::Bid { offer_lovelace, .. } => {
            // For bids, the offer amount is the total price
            Some(MarketplacePricing {
                base_price_lovelace: offer_lovelace,
                marketplace_fee_lovelace: 0, // Bids typically don't have marketplace fees
                total_price_lovelace: offer_lovelace,
                payout_count: 1,
                expires_at: None,
                extraction_method: super::marketplace_pricing::ExtractionMethod::CddlValidated,
                schema_key: Some("toml_schema".to_string()),
            })
        }
    }
}
