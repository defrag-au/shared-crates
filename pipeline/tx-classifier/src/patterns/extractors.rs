//! Value and price extraction utilities

use super::PatternContext;
use crate::registry::{lookup_address, AddressCategory, ScriptCategory};
use crate::{AssetOperation, Marketplace, TxInput};
use pipeline_types::AssetId;
use serde_json::Value;
use std::collections::BTreeMap;
use tracing::debug;
use transactions::{RawTxData, TxDatum};

/// Extract individual asset prices from transaction data
/// Returns (seller_amount, buyer_amount) for each unique asset sale
pub fn extract_individual_asset_prices(
    raw_tx_data: &RawTxData,
    asset_operations: &[AssetOperation],
    marketplace_address: &str,
) -> (Option<u64>, Option<u64>) {
    // Group operations by asset to handle multi-asset sales
    let mut asset_groups: BTreeMap<String, Vec<&AssetOperation>> = BTreeMap::new();

    for op in asset_operations {
        if let Some(asset) = op.payload.get_asset() {
            let asset_key = format!("{}.{}", asset.policy_id, asset.asset_name_hex);
            asset_groups.entry(asset_key).or_default().push(op);
        }
    }

    let mut individual_prices = Vec::new();

    // Extract pricing for each asset group
    for (_asset_key, ops) in asset_groups {
        // For now, return the first operation's pricing
        // This can be enhanced to handle complex multi-asset pricing
        if let Some(first_op) = ops.first() {
            if let Some(asset) = first_op.payload.get_asset() {
                let seller_amount = extract_asset_price_from_utxo_or_metadata(
                    raw_tx_data,
                    &asset.policy_id,
                    marketplace_address,
                );
                individual_prices.push((seller_amount, seller_amount));
            }
        }
    }

    // Return the first result for compatibility
    if let Some(result) = individual_prices.first() {
        *result
    } else {
        (None, None)
    }
}

/// Extract asset price from UTXO datum or metadata fallback
pub fn extract_asset_price_from_utxo_or_metadata(
    raw_tx_data: &RawTxData,
    policy_id: &str,
    marketplace_address: &str,
) -> Option<u64> {
    // Try to find UTXO with this asset and extract price from datum
    for output in &raw_tx_data.outputs {
        if output
            .assets
            .iter()
            .any(|(pid, _)| pid.starts_with(policy_id))
        {
            if let Some(datum) = &output.datum {
                if let Some(marketplace_pricing) =
                    super::sales::try_schema_driven_pricing_extraction(datum, marketplace_address)
                {
                    return Some(marketplace_pricing.total_price_lovelace);
                }
            }
        }
    }

    // Fallback to metadata extraction
    extract_price_from_metadata(raw_tx_data, marketplace_address)
}

/// Extract price from JSON datum value
pub fn extract_price_from_json_value(json: &Value) -> Option<u64> {
    fn find_price_recursive(value: &Value, depth: u8) -> Option<u64> {
        if depth > 10 {
            return None;
        } // Prevent infinite recursion

        match value {
            Value::Object(map) => {
                // Look for "int" fields that could be prices
                if let Some(Value::Number(n)) = map.get("int") {
                    if let Some(amount) = n.as_u64() {
                        // Reasonable price range: 0.1-10000 ADA
                        if (100_000..=10_000_000_000).contains(&amount) {
                            return Some(amount);
                        }
                    }
                }

                // Recursively search in all values
                for val in map.values() {
                    if let Some(amount) = find_price_recursive(val, depth + 1) {
                        return Some(amount);
                    }
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    if let Some(amount) = find_price_recursive(val, depth + 1) {
                        return Some(amount);
                    }
                }
            }
            _ => {}
        }
        None
    }

    find_price_recursive(json, 0)
}

/// Calculate marketplace fee based on the marketplace type and asset price
fn calculate_marketplace_fee(marketplace_address: &str, asset_price: u64) -> u64 {
    // Check marketplace type from address registry
    if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
        fee_calculation, ..
    })) = lookup_address(marketplace_address)
    {
        return fee_calculation(asset_price, marketplace_address);
    }

    0
}

/// Extract pricing information from transaction metadata when datum bytes are null
fn extract_price_from_metadata(raw_tx_data: &RawTxData, marketplace_address: &str) -> Option<u64> {
    use pipeline_types::cip::CIP68_ROYALTY_LABELS;

    // Check if there's metadata in the transaction
    let metadata = raw_tx_data.metadata.as_ref()?;

    // First try proper CBOR decoding for JPG.store metadata format
    if let Some(price) = extract_jpg_store_metadata_price(metadata, marketplace_address) {
        return Some(price);
    }

    // Fallback to legacy pattern matching
    let mut total_prices = Vec::new();

    for key in CIP68_ROYALTY_LABELS {
        if let Some(cbor_hex) = metadata.get(key).and_then(|v| v.as_str()) {
            debug!("Checking metadata key {}: {}", key, cbor_hex);

            // Look for CBOR integer patterns in the hex string
            // Pattern: 1a followed by 8 hex digits represents a 32-bit integer
            // Pattern: 1b followed by 16 hex digits represents a 64-bit integer
            let hex_without_prefix = cbor_hex.strip_prefix("0x").unwrap_or(cbor_hex);

            let mut i = 0;
            while i + 4 <= hex_without_prefix.len() {
                // Check for 32-bit integer (1a + 8 hex digits)
                if i + 10 <= hex_without_prefix.len() && &hex_without_prefix[i..i + 2] == "1a" {
                    if let Ok(value) = u32::from_str_radix(&hex_without_prefix[i + 2..i + 10], 16) {
                        // Only consider values that look like reasonable prices (between 0.1 ADA and 4000 ADA for u32)
                        if (100_000..=4_000_000_000).contains(&value) {
                            total_prices.push(value as u64);
                            debug!(
                                "Found 32-bit price in metadata key {}: {} lovelace ({})",
                                key,
                                value,
                                hex_without_prefix[i..i + 10].to_string()
                            );
                        }
                    }
                    i += 10;
                // Check for 64-bit integer (1b + 16 hex digits)
                } else if i + 18 <= hex_without_prefix.len()
                    && &hex_without_prefix[i..i + 2] == "1b"
                {
                    if let Ok(value) = u64::from_str_radix(&hex_without_prefix[i + 2..i + 18], 16) {
                        // Only consider values that look like reasonable prices (between 0.1 ADA and 10000 ADA)
                        if (100_000u64..=10_000_000_000u64).contains(&value) {
                            total_prices.push(value);
                            debug!(
                                "Found 64-bit price in metadata key {}: {} lovelace ({})",
                                key,
                                value,
                                hex_without_prefix[i..i + 18].to_string()
                            );
                        }
                    }
                    i += 18;
                // Skip 16-bit integer parsing for now as it's picking up spurious values
                // The 19xx patterns in JPG.store metadata are not price-related
                // } else if i + 6 <= hex_without_prefix.len() && &hex_without_prefix[i..i + 2] == "19"
                // {
                //     if let Ok(value) = u16::from_str_radix(&hex_without_prefix[i + 2..i + 6], 16) {
                //         // Convert to lovelace and check if it's a reasonable price component
                //         let value_lovelace = value as u64 * 1_000_000; // Assume it's in ADA
                //         if (100_000u64..=10_000_000_000u64).contains(&value_lovelace) {
                //             total_prices.push(value_lovelace);
                //             debug!(
                //                 "Found 16-bit price in metadata key {}: {} ADA = {} lovelace ({})",
                //                 key,
                //                 value,
                //                 value_lovelace,
                //                 hex_without_prefix[i..i + 6].to_string()
                //             );
                //         }
                //     }
                //     i += 6;
                } else {
                    i += 2;
                }
            }
        }
    }

    if total_prices.is_empty() {
        debug!("No prices found in metadata");
        return None;
    }

    // Sum the prices found (similar to how we sum datum components)
    let asset_price: u64 = total_prices.iter().sum();

    // Calculate marketplace-specific fee
    let marketplace_fee = calculate_marketplace_fee(marketplace_address, asset_price);
    let total_price = asset_price + marketplace_fee;

    debug!(
        "Metadata pricing: asset_price={}, marketplace_fee={}, total={}",
        asset_price, marketplace_fee, total_price
    );

    Some(total_price)
}

/// Extract pricing information from JPG.store metadata using proper CBOR decoding
fn extract_jpg_store_metadata_price(
    metadata: &serde_json::Value,
    marketplace_address: &str,
) -> Option<u64> {
    let metadata_obj = metadata.as_object()?;

    // Check if this is JPG.store metadata format (version 5+)
    let version = metadata_obj
        .get("30")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u32>().ok())?;

    if version < 5 {
        debug!(
            "JPG.store metadata version {} < 5, skipping CBOR decode",
            version
        );
        return None;
    }

    debug!(
        "JPG.store metadata version {}, attempting CBOR decode",
        version
    );

    // Analyze all metadata keys 50-56 to understand the structure
    analyze_jpg_store_metadata_structure(metadata_obj);

    // Try to decode each metadata key 50-56 as CBOR
    for key in &["50", "51", "52", "53", "54", "55", "56"] {
        if let Some(cbor_hex) = metadata_obj.get(*key).and_then(|v| v.as_str()) {
            if let Some(price) = decode_cbor_metadata_key(key, cbor_hex) {
                debug!(
                    "Successfully decoded price from metadata key {}: {} lovelace",
                    key, price
                );

                // Calculate marketplace-specific fee
                let marketplace_fee = calculate_marketplace_fee(marketplace_address, price);
                let total_price = price + marketplace_fee;

                debug!(
                    "JPG.store CBOR pricing: asset_price={}, marketplace_fee={}, total={}",
                    price, marketplace_fee, total_price
                );

                return Some(total_price);
            }
        }
    }

    debug!("No valid pricing data found in JPG.store metadata CBOR");
    None
}

/// Decode a single CBOR metadata key to extract pricing information
fn decode_cbor_metadata_key(key: &str, cbor_hex: &str) -> Option<u64> {
    use pallas_codec::minicbor::{Decode, Decoder};
    use pallas_primitives::alonzo::PlutusData;

    // Remove 0x prefix if present
    let hex_without_prefix = cbor_hex.strip_prefix("0x").unwrap_or(cbor_hex);

    // Decode hex string to bytes
    let cbor_bytes = match hex::decode(hex_without_prefix) {
        Ok(bytes) => bytes,
        Err(e) => {
            debug!("Failed to decode hex for metadata key {}: {}", key, e);
            return None;
        }
    };

    // Decode using Pallas
    let mut decoder = Decoder::new(&cbor_bytes);
    let plutus_data = match PlutusData::decode(&mut decoder, &mut ()) {
        Ok(data) => data,
        Err(e) => {
            debug!("Failed to decode CBOR for metadata key {}: {}", key, e);
            return None;
        }
    };

    debug!("Decoded CBOR for metadata key {}: {:?}", key, plutus_data);

    // Extract price from the PlutusData structure
    extract_price_from_plutus_data(&plutus_data)
}

/// Extract price information from PlutusData structure
fn extract_price_from_plutus_data(data: &pallas_primitives::alonzo::PlutusData) -> Option<u64> {
    use pallas_primitives::alonzo::PlutusData;

    match data {
        PlutusData::BigInt(big_int) => {
            // Convert BigInt to u64 if possible
            match big_int {
                pallas_primitives::alonzo::BigInt::Int(int) => {
                    let val: i128 = (*int).into();
                    if val >= 0 && val <= u64::MAX as i128 {
                        let value = val as u64;
                        if (100_000u64..=100_000_000_000u64).contains(&value) {
                            debug!("Found direct price: {} lovelace", value);
                            return Some(value);
                        }
                    }
                }
                pallas_primitives::alonzo::BigInt::BigUInt(bytes) => {
                    // Try to convert bytes to u64 if reasonable size
                    if bytes.len() <= 8 {
                        let mut value = 0u64;
                        for &byte in bytes.as_slice() {
                            value = value * 256 + byte as u64;
                        }
                        if (100_000u64..=100_000_000_000u64).contains(&value) {
                            debug!("Found BigUInt price: {} lovelace", value);
                            return Some(value);
                        }
                    }
                }
                pallas_primitives::alonzo::BigInt::BigNInt(_) => {
                    // Negative integers, skip for pricing
                }
            }
        }
        PlutusData::Constr(constr) => {
            // Constructor with fields - recursively search
            for field in constr.fields.iter() {
                if let Some(price) = extract_price_from_plutus_data(field) {
                    return Some(price);
                }
            }
        }
        PlutusData::Array(array) => {
            // Array of values - recursively search
            for item in array.iter() {
                if let Some(price) = extract_price_from_plutus_data(item) {
                    return Some(price);
                }
            }
        }
        PlutusData::Map(map) => {
            // Map of key-value pairs - recursively search values
            for entry in map.iter() {
                if let Some(price) = extract_price_from_plutus_data(&entry.1) {
                    return Some(price);
                }
            }
        }
        _ => {}
    }

    None
}

/// Analyze JPG.store metadata structure to understand CBOR fragments
fn analyze_jpg_store_metadata_structure(metadata: &serde_json::Map<String, serde_json::Value>) {
    debug!("=== JPG.store Metadata Structure Analysis ===");

    for key in &["50", "51", "52", "53", "54", "55", "56"] {
        if let Some(cbor_hex) = metadata.get(*key).and_then(|v| v.as_str()) {
            debug!("Key {}: {}", key, cbor_hex);
            analyze_cbor_chunk(key, cbor_hex);
        }
    }

    debug!("=== End Metadata Analysis ===");
}

/// Analyze a single CBOR chunk to understand its structure
fn analyze_cbor_chunk(key: &str, cbor_hex: &str) {
    // Remove 0x prefix if present
    let hex_clean = cbor_hex.strip_prefix("0x").unwrap_or(cbor_hex);

    // Decode hex to bytes
    let bytes = match hex::decode(hex_clean) {
        Ok(b) => b,
        Err(e) => {
            debug!("Key {}: Failed to decode hex: {}", key, e);
            return;
        }
    };

    debug!("Key {}: {} bytes", key, bytes.len());

    // Analyze CBOR structure manually
    analyze_cbor_bytes(key, &bytes);
}

/// Manual CBOR analysis to understand structure
fn analyze_cbor_bytes(key: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }

    let mut pos = 0;
    debug!("Key {}: CBOR analysis:", key);

    while pos < bytes.len() {
        let byte = bytes[pos];
        let major_type = (byte >> 5) & 0x07;
        let additional_info = byte & 0x1f;

        debug!(
            "  Pos {}: 0x{:02x} - Major type: {}, Additional: {}",
            pos, byte, major_type, additional_info
        );

        match major_type {
            0 => {
                // Unsigned integer
                let (value, consumed) = decode_cbor_uint(&bytes[pos..]);
                debug!(
                    "    → Unsigned int: {} (consumed {} bytes)",
                    value, consumed
                );
                pos += consumed;
            }
            1 => {
                // Negative integer
                debug!("    → Negative integer");
                pos += 1 + get_additional_bytes(additional_info);
            }
            2 => {
                // Byte string
                let (length, consumed) = decode_cbor_length(&bytes[pos..]);
                debug!(
                    "    → Byte string length: {} (header {} bytes)",
                    length, consumed
                );
                pos += consumed + length;
            }
            3 => {
                // Text string
                let (length, consumed) = decode_cbor_length(&bytes[pos..]);
                debug!(
                    "    → Text string length: {} (header {} bytes)",
                    length, consumed
                );
                pos += consumed + length;
            }
            4 => {
                // Array
                let (length, consumed) = decode_cbor_length(&bytes[pos..]);
                debug!("    → Array length: {} (header {} bytes)", length, consumed);
                pos += consumed;
            }
            5 => {
                // Map
                let (length, consumed) = decode_cbor_length(&bytes[pos..]);
                debug!("    → Map length: {} (header {} bytes)", length, consumed);
                pos += consumed;
            }
            6 => {
                // Tag
                let (tag, consumed) = decode_cbor_uint(&bytes[pos..]);
                debug!("    → Tag: {} (consumed {} bytes)", tag, consumed);
                pos += consumed;
            }
            7 => {
                // Primitives
                debug!("    → Primitive/special");
                pos += 1 + get_additional_bytes(additional_info);
            }
            _ => {
                debug!("    → Unknown major type");
                pos += 1;
            }
        }

        // Safety break to avoid infinite loops
        if pos >= bytes.len() + 10 {
            debug!("  Breaking analysis to avoid infinite loop");
            break;
        }
    }
}

/// Decode CBOR unsigned integer
fn decode_cbor_uint(bytes: &[u8]) -> (u64, usize) {
    if bytes.is_empty() {
        return (0, 0);
    }

    let first_byte = bytes[0];
    let additional_info = first_byte & 0x1f;

    match additional_info {
        0..=23 => (additional_info as u64, 1),
        24 if bytes.len() >= 2 => (bytes[1] as u64, 2),
        25 if bytes.len() >= 3 => (u16::from_be_bytes([bytes[1], bytes[2]]) as u64, 3),
        26 if bytes.len() >= 5 => (
            u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as u64,
            5,
        ),
        27 if bytes.len() >= 9 => (
            u64::from_be_bytes([
                bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8],
            ]),
            9,
        ),
        _ => (0, 1),
    }
}

/// Decode CBOR length field
fn decode_cbor_length(bytes: &[u8]) -> (usize, usize) {
    let (value, consumed) = decode_cbor_uint(bytes);
    (value as usize, consumed)
}

/// Get number of additional bytes for CBOR additional info
fn get_additional_bytes(additional_info: u8) -> usize {
    match additional_info {
        0..=23 => 0,
        24 => 1,
        25 => 2,
        26 => 4,
        27 => 8,
        _ => 0,
    }
}

/// Extract the actual offer value from datum data or ADA flows
/// Uses marketplace-first, datum-first approach with UTXO fallback
pub fn extract_offer_value_from_datum_or_flows(context: &PatternContext, asset: &AssetId) -> u64 {
    use crate::registry::MarketplacePurpose;

    // Step 1: Identify marketplace from offer script inputs and try marketplace-specific datum parsing
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
            marketplace,
            purpose: MarketplacePurpose::Offer,
            ..
        })) = lookup_address(&input.address)
        {
            debug!(
                "Found offer input from {:?} marketplace: {}",
                marketplace, input.address
            );

            // Try marketplace-specific datum parsing first using schema-driven approach
            if let Some(datum) = &input.datum {
                if let Some(offer_amount) =
                    super::sales::try_schema_driven_offer_extraction(datum, &input.address)
                {
                    debug!(
                        "Extracted offer amount from {:?} datum using schema-driven approach: {} lovelace",
                        marketplace, offer_amount
                    );
                    return offer_amount;
                } else if let Some(offer_amount) =
                    extract_marketplace_specific_offer_amount(datum, *marketplace, context, asset)
                {
                    debug!(
                        "Extracted offer amount from {:?} datum using legacy approach: {} lovelace",
                        marketplace, offer_amount
                    );
                    return offer_amount;
                }
            }

            // Fallback to UTXO amount for this marketplace if datum parsing failed
            if input.amount_lovelace > 1_000_000 {
                debug!(
                    "Using UTXO amount as fallback for {:?} marketplace: {} lovelace",
                    marketplace, input.amount_lovelace
                );
                return input.amount_lovelace;
            }
        }
    }

    // Step 2: No known marketplace found, try generic datum parsing
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if let Some(datum) = &input.datum {
            if let Some(offer_amount) = extract_offer_amount_from_datum(datum) {
                if offer_amount > 1_000_000 {
                    debug!(
                        "Extracted offer amount from generic datum parsing: {} lovelace",
                        offer_amount
                    );
                    return offer_amount;
                }
            }
        }
    }

    // Step 3: Fallback to ADA flow analysis for unknown structures
    #[allow(deprecated)]
    let significant_transfers: Vec<u64> = context
        .raw_tx_data
        .inputs
        .iter()
        .zip(context.raw_tx_data.outputs.iter())
        .filter_map(|(input, output)| {
            let amount_diff = output.amount_lovelace.saturating_sub(input.amount_lovelace);
            if amount_diff > 5_000_000 && amount_diff < 1_000_000_000 {
                Some(amount_diff)
            } else {
                None
            }
        })
        .collect();

    if let Some(&largest_transfer) = significant_transfers.iter().max() {
        debug!(
            "Using largest significant ADA transfer as offer amount: {} lovelace",
            largest_transfer
        );
        return largest_transfer;
    }

    // Final fallback: return a reasonable default
    debug!(
        "Could not extract offer amount for asset {}, using 1 ADA default",
        asset.policy_id
    );
    1_000_000
}

/// Extract offer amount using marketplace-specific logic
#[deprecated(
    note = "Use super::sales::try_schema_driven_offer_extraction with new datum-parsing crate instead"
)]
fn extract_marketplace_specific_offer_amount(
    datum: &TxDatum,
    marketplace: Marketplace,
    context: &PatternContext,
    asset: &AssetId,
) -> Option<u64> {
    let _json_value = match datum {
        TxDatum::Json { json, .. } => json,
        _ => return None,
    };

    match marketplace {
        Marketplace::JpgStore => {
            debug!("Using JPG.store-specific offer extraction");
            // For JPG.store offer accepts, the datum contains payout breakdown
            // but the actual offer amount is in the UTXO value
            // Try to find the specific UTXO that correlates with this asset
            extract_jpg_store_offer_amount_from_utxo(context, asset)
        }
        // Add other marketplaces here as they're implemented
        // Marketplace::SpaceBudz => extract_space_budz_offer_amount(json_value),
        // Marketplace::CnftIo => extract_cnft_io_offer_amount(json_value),
        _ => {
            debug!(
                "No specific datum parser for marketplace {:?}, using generic fallback",
                marketplace
            );
            None
        }
    }
}

/// Extract offer amount from marketplace datum structure (deprecated - use schema-driven approach)
#[deprecated(
    note = "Use super::sales::try_schema_driven_offer_extraction with new datum-parsing crate instead"
)]
fn extract_offer_amount_from_datum(datum: &TxDatum) -> Option<u64> {
    let json_value = match datum {
        TxDatum::Json { json, .. } => json,
        _ => return None,
    };

    // Try JPG.store-specific parsing first
    if let Some(amount) = extract_jpg_store_offer_amount(json_value) {
        return Some(amount);
    }

    // Fallback to generic recursive search
    find_offer_amount_recursive(json_value, 0)
}

/// Extract offer amount for JPG.store by finding the correlated UTXO
/// For offer accepts, the actual bidAmount is stored as the UTXO value, not in the datum
fn extract_jpg_store_offer_amount_from_utxo(
    context: &PatternContext,
    asset: &AssetId,
) -> Option<u64> {
    use crate::registry::MarketplacePurpose;

    // Strategy: Find offer script inputs that contain references to this specific asset
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
            purpose: MarketplacePurpose::Offer,
            ..
        })) = lookup_address(&input.address)
        {
            // Check if this offer UTXO is related to our asset
            if is_offer_utxo_for_asset(input, asset) {
                debug!(
                    "Found correlated JPG.store offer UTXO for asset {}: {} lovelace at {}",
                    asset.policy_id, input.amount_lovelace, input.address
                );
                return Some(input.amount_lovelace);
            }
        }
    }

    // Fallback: if we can't correlate, return the first reasonable offer UTXO
    #[allow(deprecated)]
    for input in &context.raw_tx_data.inputs {
        if let Some(AddressCategory::Script(ScriptCategory::Marketplace {
            purpose: MarketplacePurpose::Offer,
            ..
        })) = lookup_address(&input.address)
        {
            if input.amount_lovelace > 1_000_000 {
                debug!(
                    "Using fallback JPG.store offer UTXO: {} lovelace at {}",
                    input.amount_lovelace, input.address
                );
                return Some(input.amount_lovelace);
            }
        }
    }

    None
}

/// Check if an offer UTXO is related to a specific asset
/// This uses datum analysis and transaction context to correlate offers with assets
fn is_offer_utxo_for_asset(input: &TxInput, asset: &AssetId) -> bool {
    // Method 1: Check if the datum contains references to this asset's policy ID
    if let Some(datum) = &input.datum {
        if datum_references_asset(datum, asset) {
            return true;
        }
    }

    // Method 2: For now, if we can't correlate specifically, assume it's related
    // In a more sophisticated implementation, we could analyze the spending patterns
    // or look at asset transfer patterns in the transaction
    true
}

/// Check if a datum contains references to a specific asset
fn datum_references_asset(datum: &TxDatum, asset: &AssetId) -> bool {
    let json_value = match datum {
        TxDatum::Json { json, .. } => json,
        _ => return false,
    };

    // Look for the asset's policy ID in the datum structure
    contains_policy_id_recursive(json_value, &asset.policy_id, 0)
}

/// Recursively search for a policy ID in JSON structure
fn contains_policy_id_recursive(value: &Value, policy_id: &str, depth: u8) -> bool {
    if depth > 10 {
        return false; // Prevent infinite recursion
    }

    match value {
        Value::String(s) => s.contains(policy_id),
        Value::Object(map) => {
            for v in map.values() {
                if contains_policy_id_recursive(v, policy_id, depth + 1) {
                    return true;
                }
            }
            false
        }
        Value::Array(arr) => {
            for item in arr {
                if contains_policy_id_recursive(item, policy_id, depth + 1) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// Extract offer amount from JPG.store v2 datum structure
/// Based on the Swap { sOwner, sSwapPayouts } structure from contract analysis
/// NOTE: This extracts payout amounts, not the original offer amounts
fn extract_jpg_store_offer_amount(json_value: &Value) -> Option<u64> {
    // JPG.store v2 structure: Swap { sOwner: PubKeyHash, sSwapPayouts: [Payout] }
    // Looking for constructor 0 with fields: [owner_hash, payout_list]
    if let Value::Object(root) = json_value {
        if let (Some(Value::Number(constructor)), Some(Value::Array(fields))) =
            (root.get("constructor"), root.get("fields"))
        {
            if constructor.as_u64() == Some(0) && fields.len() >= 2 {
                // Field 1 should be the payout list
                if let Some(Value::Object(payout_list_obj)) = fields.get(1) {
                    if let Some(Value::Array(payout_list)) = payout_list_obj.get("list") {
                        debug!(
                            "Found JPG.store payout list with {} entries",
                            payout_list.len()
                        );

                        // Each payout entry contains address and expected value
                        // Look for the largest reasonable offer amount in the list
                        let mut best_amount = None;
                        for payout in payout_list {
                            if let Some(amount) = extract_jpg_store_payout_amount(payout) {
                                // Prefer larger amounts as they're more likely to be the actual offer
                                if best_amount.is_none() || amount > best_amount.unwrap_or(0) {
                                    best_amount = Some(amount);
                                }
                            }
                        }

                        if let Some(amount) = best_amount {
                            debug!("Extracted JPG.store offer amount: {} lovelace", amount);
                            return Some(amount);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Extract amount from a single JPG.store payout entry
fn extract_jpg_store_payout_amount(payout: &Value) -> Option<u64> {
    // Payout structure: constructor 0 with fields [address, expected_value]
    if let Value::Object(payout_obj) = payout {
        if let (Some(Value::Number(constructor)), Some(Value::Array(fields))) =
            (payout_obj.get("constructor"), payout_obj.get("fields"))
        {
            if constructor.as_u64() == Some(0) && fields.len() >= 2 {
                // Field 1 contains the expected value (payment map)
                if let Some(expected_value) = fields.get(1) {
                    return extract_expected_value_amount(expected_value);
                }
            }
        }
    }
    None
}

/// Extract amount from JPG.store ExpectedValue structure (map of currency -> amount)
fn extract_expected_value_amount(expected_value: &Value) -> Option<u64> {
    // ExpectedValue is a map where ADA is represented by empty key ""
    if let Value::Object(map_obj) = expected_value {
        if let Some(Value::Array(map_entries)) = map_obj.get("map") {
            for entry in map_entries {
                if let Value::Object(entry_obj) = entry {
                    // Look for entries with empty key (ADA) and extract the value
                    if let (Some(Value::Object(key_obj)), Some(value)) =
                        (entry_obj.get("k"), entry_obj.get("v"))
                    {
                        if let Some(Value::String(key_bytes)) = key_obj.get("bytes") {
                            if key_bytes.is_empty() {
                                // This is ADA (empty key), extract the amount
                                return extract_ada_amount_from_value(value);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract ADA amount from JPG.store value structure
fn extract_ada_amount_from_value(value: &Value) -> Option<u64> {
    // Value structure: constructor 0 with fields [type_indicator, amount_map]
    if let Value::Object(value_obj) = value {
        if let (Some(Value::Number(constructor)), Some(Value::Array(fields))) =
            (value_obj.get("constructor"), value_obj.get("fields"))
        {
            if constructor.as_u64() == Some(0) && fields.len() >= 2 {
                // Field 1 contains the amount map
                if let Some(amount_map) = fields.get(1) {
                    return extract_amount_from_map(amount_map);
                }
            }
        }
    }
    None
}

/// Extract amount from the inner amount map structure
fn extract_amount_from_map(amount_map: &Value) -> Option<u64> {
    if let Value::Object(map_obj) = amount_map {
        if let Some(Value::Array(map_entries)) = map_obj.get("map") {
            for entry in map_entries {
                if let Value::Object(entry_obj) = entry {
                    if let (Some(Value::Object(key_obj)), Some(Value::Object(value_obj))) =
                        (entry_obj.get("k"), entry_obj.get("v"))
                    {
                        // Look for empty key (ADA) and integer value
                        if let Some(Value::String(key_bytes)) = key_obj.get("bytes") {
                            if key_bytes.is_empty() {
                                if let Some(Value::Number(amount)) = value_obj.get("int") {
                                    if let Some(amount_u64) = amount.as_u64() {
                                        // Validate reasonable offer amount range
                                        if (1_000_000..=1_000_000_000).contains(&amount_u64) {
                                            return Some(amount_u64);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Generic fallback parsing for unknown marketplace datum structures
fn find_offer_amount_recursive(value: &Value, depth: u8) -> Option<u64> {
    if depth > 10 {
        return None;
    } // Prevent infinite recursion

    match value {
        Value::Object(map) => {
            // Look for "int" fields that could be offer amounts
            if let Some(Value::Number(n)) = map.get("int") {
                if let Some(amount) = n.as_u64() {
                    // Reasonable offer amount range: 1-1000 ADA
                    if (1_000_000..=1_000_000_000).contains(&amount) {
                        return Some(amount);
                    }
                }
            }

            // Recursively search in all values
            for val in map.values() {
                if let Some(amount) = find_offer_amount_recursive(val, depth + 1) {
                    return Some(amount);
                }
            }
        }
        Value::Array(arr) => {
            for val in arr {
                if let Some(amount) = find_offer_amount_recursive(val, depth + 1) {
                    return Some(amount);
                }
            }
        }
        _ => {}
    }
    None
}
