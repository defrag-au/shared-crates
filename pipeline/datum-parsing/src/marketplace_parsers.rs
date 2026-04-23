//! Marketplace-specific datum parsers without CDDL dependency
//!
//! This module provides lightweight datum parsing for various marketplace protocols
//! using direct CBOR parsing with pallas/minicbor instead of heavy CDDL validation.

use crate::{DatumParsingError, Result};
use once_cell::sync::Lazy;
use pipeline_types::{AssetId, OperationPayload};
use std::sync::Mutex;
use tracing::debug;

/// Lightweight marketplace operation representation
#[derive(Debug, Clone)]
pub enum MarketplaceOperation {
    /// A listing/sale operation (ask) - represents assets being offered for sale
    Ask {
        asset: Option<AssetId>,
        targets: Vec<LockedTarget>,
    },
    /// An offer/bid operation - represents an offer to buy assets
    Bid {
        /// Policy ID of the asset being bid on
        policy_id: String,
        /// Specific asset name (None for collection bids)
        asset_name_hex: Option<String>,
        /// Offer amount in lovelace
        offer_lovelace: u64,
    },
}

impl MarketplaceOperation {
    /// Calculate the total listing price for Ask operations
    pub fn total_price_lovelace(&self) -> Option<u64> {
        match self {
            MarketplaceOperation::Ask { targets, .. } => {
                let mut total = 0u64;
                for target in targets {
                    if let OperationPayload::Lovelace { amount } = &target.payload {
                        total = total.saturating_add(*amount);
                    }
                }
                Some(total)
            }
            MarketplaceOperation::Bid { offer_lovelace, .. } => Some(*offer_lovelace),
        }
    }

    /// Get the specific asset being operated on (None for collection bids or unidentified asks)
    pub fn get_asset(&self) -> Option<AssetId> {
        match self {
            MarketplaceOperation::Ask { asset, .. } => asset.clone(),
            MarketplaceOperation::Bid {
                policy_id,
                asset_name_hex,
                ..
            } => asset_name_hex
                .as_ref()
                .and_then(|name| AssetId::new(policy_id.clone(), name.clone()).ok()),
        }
    }

    /// Get the policy ID being operated on (not always available for asks without asset)
    pub fn get_policy_id(&self) -> Option<&str> {
        match self {
            MarketplaceOperation::Ask { asset, .. } => asset.as_ref().map(|a| a.policy_id.as_str()),
            MarketplaceOperation::Bid { policy_id, .. } => Some(policy_id),
        }
    }

    /// Check if this is a collection-wide operation (no specific asset name)
    pub fn is_collection_operation(&self) -> bool {
        match self {
            MarketplaceOperation::Ask { asset, .. } => asset.is_none(),
            MarketplaceOperation::Bid { asset_name_hex, .. } => asset_name_hex.is_none(),
        }
    }

    /// Extract payment distribution (address, amount pairs) from the operation
    pub fn get_payment_distribution(&self) -> Vec<(String, u64)> {
        match self {
            MarketplaceOperation::Ask { targets, .. } => targets
                .iter()
                .filter_map(|target| {
                    if let OperationPayload::Lovelace { amount } = &target.payload {
                        Some((target.target.clone(), *amount))
                    } else {
                        None
                    }
                })
                .collect(),
            MarketplaceOperation::Bid { .. } => {
                // Bids don't typically have payment distributions (single payer)
                Vec::new()
            }
        }
    }
}

/// A locked target represents an asset with associated pricing/operation data
#[derive(Debug, Clone)]
pub struct LockedTarget {
    /// Target identifier (could be asset ID, address, etc.)
    pub target: String,
    /// The operation payload (pricing, token info, etc.)
    pub payload: OperationPayload,
}

// Global schema loader instance (shared across all parser instances for optimal performance)
static GLOBAL_SCHEMA_LOADER: Lazy<Mutex<crate::toml_schema::TomlSchemaLoader>> =
    Lazy::new(|| Mutex::new(crate::toml_schema::TomlSchemaLoader::new()));

/// Marketplace-specific datum parser
#[derive(Debug)]
pub struct MarketplaceDatumParser {
    /// Marketplace type for this parser
    marketplace: MarketplaceType,
}

/// Re-export MarketplaceType from address-registry (canonical definition)
pub use address_registry::MarketplaceType;

impl MarketplaceDatumParser {
    /// Create a new parser for a specific marketplace
    pub fn new(marketplace: MarketplaceType) -> Self {
        Self { marketplace }
    }

    /// Parse using TOML schema with global shared loader
    fn parse_with_toml_schema(
        &self,
        cbor_bytes: &[u8],
        schema_name: &str,
    ) -> Result<MarketplaceOperation> {
        // Use global schema loader for optimal caching
        let mut loader = GLOBAL_SCHEMA_LOADER.lock().map_err(|e| {
            DatumParsingError::SchemaValidation(format!(
                "Failed to acquire schema loader lock: {e}"
            ))
        })?;

        if let Some(schema) = loader.get_schema(schema_name) {
            debug!("Using TOML schema: {}", schema_name);
            let extractor = crate::toml_schema::CborExtractor::new(schema);
            extractor.extract_marketplace_operation(cbor_bytes)
        } else {
            debug!(
                "TOML schema '{}' not found, falling back to legacy parsing",
                schema_name
            );
            // Fall back to the old JSON-based approach
            drop(loader); // Release the lock before calling legacy approach
            self.parse_with_legacy_approach(cbor_bytes)
        }
    }

    /// Legacy parsing approach (fallback)
    fn parse_with_legacy_approach(&self, cbor_bytes: &[u8]) -> Result<MarketplaceOperation> {
        use pallas_primitives::alonzo::PlutusData;

        // First decode CBOR into PlutusData
        let plutus_data: PlutusData = pallas_codec::minicbor::decode(cbor_bytes)
            .map_err(|e| DatumParsingError::CborDecode(format!("Failed to decode CBOR: {e}")))?;

        debug!("Parsing with legacy approach: {:?}", plutus_data);

        // Convert PlutusData to JSON for processing with existing logic
        let json_value = self.plutus_data_to_json(&plutus_data)?;

        debug!(
            "Converted to JSON: {}",
            serde_json::to_string_pretty(&json_value).unwrap_or_else(|_| "invalid".to_string())
        );

        // Use existing JPG.store v3 price extraction logic
        if let Some(total_price) = utils::extract_jpg_store_v3_price_from_json(&json_value) {
            debug!(
                "Extracted price using legacy approach: {} lovelace",
                total_price
            );

            Err(DatumParsingError::SchemaValidation(format!(
                "Legacy parser extracted price ({total_price} lovelace) but cannot identify asset"
            )))
        } else {
            debug!("Could not extract price using legacy approach");
            Err(DatumParsingError::SchemaValidation(
                "No price found using legacy approach".to_string(),
            ))
        }
    }

    /// Parse CBOR datum bytes into a marketplace operation
    pub fn parse_cbor(&self, cbor_bytes: &[u8]) -> Result<MarketplaceOperation> {
        // First validate that we can decode the CBOR as PlutusData
        use pallas_primitives::alonzo::PlutusData;
        let _cbor_value: PlutusData = pallas_codec::minicbor::decode(cbor_bytes)
            .map_err(|e| DatumParsingError::CborDecode(format!("Failed to decode CBOR: {e}")))?;

        // Parse based on marketplace type
        match self.marketplace {
            MarketplaceType::JpgStoreV1 => self.parse_jpg_store_v1(cbor_bytes),
            MarketplaceType::JpgStoreV2 => self.parse_jpg_store_v2(cbor_bytes),
            MarketplaceType::JpgStoreV3 => self.parse_jpg_store_v3(cbor_bytes),
            MarketplaceType::JpgStoreV4 => self.parse_jpg_store_v4(cbor_bytes),
            MarketplaceType::Wayup => self.parse_wayup(cbor_bytes),
            MarketplaceType::Unknown => Err(DatumParsingError::SchemaValidation(
                "Cannot parse unknown marketplace type".to_string(),
            )),
        }
    }

    /// Parse JPG.store V1 datum (tries both ask and bid schemas)
    fn parse_jpg_store_v1(&self, cbor_bytes: &[u8]) -> Result<MarketplaceOperation> {
        debug!("JPG.store V1: Trying bid schema first");

        // Try bid schema first (more common for offer cancellations)
        if let Ok(result) = self.parse_with_toml_schema(cbor_bytes, "jpg_store_v1_bid") {
            debug!("JPG.store V1: Bid schema succeeded");
            return Ok(result);
        }

        debug!("JPG.store V1: Bid schema failed, trying ask schema");

        // Fallback to ask schema (for listings)
        self.parse_with_toml_schema(cbor_bytes, "jpg_store_v1_ask")
    }

    /// Parse JPG.store V2 datum (tries both ask and bid schemas)
    fn parse_jpg_store_v2(&self, cbor_bytes: &[u8]) -> Result<MarketplaceOperation> {
        // Try bid schema first (more common for offer cancellations)
        if let Ok(result) = self.parse_with_toml_schema(cbor_bytes, "jpg_store_v2_bid") {
            return Ok(result);
        }

        // Fallback to ask schema (now supports both full listings and listing updates)
        self.parse_with_toml_schema(cbor_bytes, "jpg_store_v2_ask")
    }

    /// Parse JPG.store V3 datum using TOML schema (tries both ask and bid schemas)
    fn parse_jpg_store_v3(&self, cbor_bytes: &[u8]) -> Result<MarketplaceOperation> {
        // Try bid schema first (more common for offer cancellations)
        if let Ok(result) = self.parse_with_toml_schema(cbor_bytes, "jpg_store_v3_bid") {
            return Ok(result);
        }

        // Fallback to ask schema (for listings)
        self.parse_with_toml_schema(cbor_bytes, "jpg_store_v3_ask")
    }

    /// Parse JPG.store V4 datum — simplified format with asset ID + seller credentials, no price
    ///
    /// Datum structure:
    /// ```text
    /// Constructor(0) [
    ///   Constructor(0) [ policy_id: Bytes(28), asset_name: Bytes ],  // asset
    ///   Constructor(0) [ payment_cred: Bytes(28), staking_cred: Bytes(28) ]  // seller
    /// ]
    /// ```
    fn parse_jpg_store_v4(&self, cbor_bytes: &[u8]) -> Result<MarketplaceOperation> {
        // Try the TOML schema first
        if let Ok(result) = self.parse_with_toml_schema(cbor_bytes, "jpg_store_v4_ask") {
            return Ok(result);
        }

        // Direct CBOR fallback for the simple V4 structure
        use pallas_primitives::alonzo::PlutusData;

        let plutus_data: PlutusData = pallas_codec::minicbor::decode(cbor_bytes)
            .map_err(|e| DatumParsingError::CborDecode(format!("Failed to decode CBOR: {e}")))?;

        if let PlutusData::Constr(outer) = &plutus_data {
            if outer.fields.len() >= 2 {
                // Field 0: asset identifier Constructor(0) [ policy_id, asset_name ]
                if let PlutusData::Constr(asset_constr) = &outer.fields[0] {
                    if asset_constr.fields.len() >= 2 {
                        let policy_id = match &asset_constr.fields[0] {
                            PlutusData::BoundedBytes(b) => hex::encode(b.as_slice()),
                            _ => {
                                return Err(DatumParsingError::SchemaValidation(
                                    "V4: expected bytes for policy_id".to_string(),
                                ))
                            }
                        };
                        let asset_name = match &asset_constr.fields[1] {
                            PlutusData::BoundedBytes(b) => hex::encode(b.as_slice()),
                            _ => {
                                return Err(DatumParsingError::SchemaValidation(
                                    "V4: expected bytes for asset_name".to_string(),
                                ))
                            }
                        };

                        debug!("JPG.store V4: parsed asset {policy_id}.{asset_name}");

                        // V4 has no price in the datum — return Ask with zero-price target
                        // so the sales classifier knows to use ADA flow analysis
                        let asset = AssetId::new(policy_id, asset_name).map_err(|e| {
                            DatumParsingError::SchemaValidation(format!("Invalid asset: {e}"))
                        })?;
                        return Ok(MarketplaceOperation::Ask {
                            asset: Some(asset),
                            targets: vec![],
                        });
                    }
                }
            }
        }

        Err(DatumParsingError::SchemaValidation(
            "V4: datum does not match expected structure".to_string(),
        ))
    }

    /// Parse Wayup datum
    fn parse_wayup(&self, cbor_bytes: &[u8]) -> Result<MarketplaceOperation> {
        // Use dedicated Wayup TOML schema
        self.parse_with_toml_schema(cbor_bytes, "wayup_ask")
    }

    /// Convert PlutusData to JSON for compatibility with existing parsing logic
    #[allow(clippy::only_used_in_recursion)]
    fn plutus_data_to_json(
        &self,
        plutus_data: &pallas_primitives::alonzo::PlutusData,
    ) -> Result<serde_json::Value> {
        use pallas_primitives::alonzo::PlutusData;

        match plutus_data {
            PlutusData::Constr(constr) => {
                let mut obj = serde_json::Map::new();
                obj.insert(
                    "constructor".to_string(),
                    serde_json::Value::Number(constr.tag.into()),
                );

                let fields: Result<Vec<_>> = constr
                    .fields
                    .iter()
                    .map(|field| self.plutus_data_to_json(field))
                    .collect();

                obj.insert("fields".to_string(), serde_json::Value::Array(fields?));
                Ok(serde_json::Value::Object(obj))
            }
            PlutusData::Map(map) => {
                let mut json_map = serde_json::Map::new();
                let mut map_array = Vec::new();

                for pair in map.iter() {
                    let key_json = self.plutus_data_to_json(&pair.0)?;
                    let value_json = self.plutus_data_to_json(&pair.1)?;

                    map_array.push(serde_json::json!({
                        "k": key_json,
                        "v": value_json
                    }));
                }

                json_map.insert("map".to_string(), serde_json::Value::Array(map_array));
                Ok(serde_json::Value::Object(json_map))
            }
            PlutusData::Array(arr) => {
                // Check if this is a list structure (JPG.store v3 uses these)
                let items: Result<Vec<_>> = arr
                    .iter()
                    .map(|item| self.plutus_data_to_json(item))
                    .collect();

                let mut obj = serde_json::Map::new();
                obj.insert("list".to_string(), serde_json::Value::Array(items?));
                Ok(serde_json::Value::Object(obj))
            }
            PlutusData::BigInt(big_int) => {
                // Convert to string to handle large integers
                Ok(serde_json::json!({
                    "int": format!("{:?}", big_int)
                }))
            }
            PlutusData::BoundedBytes(bytes) => Ok(serde_json::json!({
                "bytes": hex::encode(bytes.as_slice())
            })),
        }
    }
}

/// Utility functions for datum parsing
pub mod utils {
    use super::*;

    /// Extract price from datum using marketplace-agnostic approach
    /// This is a lightweight version of the existing extract_price_from_datum logic
    pub fn extract_price_from_datum_json(json: &serde_json::Value) -> Option<u64> {
        // Try JPG.store v3 specific extraction first
        if let Some(price) = extract_jpg_store_v3_price_from_json(json) {
            return Some(price);
        }

        // Add other marketplace-specific extraction logic here
        // TODO: Add Wayup, other marketplaces

        // Fallback to generic price extraction
        extract_generic_price_from_json(json)
    }

    /// Extract price from JPG.store V3 contract datum structure
    /// The datum contains: { payouts: [{ address, amount_lovelace }, ...], owner }
    /// The total price is the sum of all payout amounts
    pub fn extract_jpg_store_v3_price_from_json(json: &serde_json::Value) -> Option<u64> {
        find_jpg_store_v3_payouts(json, 0)
    }

    /// Recursively search for JPG.store v3 payouts structure and sum amounts
    fn find_jpg_store_v3_payouts(value: &serde_json::Value, depth: u8) -> Option<u64> {
        if depth > 10 {
            return None; // Prevent infinite recursion
        }

        match value {
            serde_json::Value::Object(map) => {
                // Look for "list" field containing payouts
                if let Some(serde_json::Value::Array(items)) = map.get("list") {
                    let mut total_price = 0u64;

                    debug!("Found JPG.store v3 payouts list with {} items", items.len());

                    for item in items {
                        if let Some(payout_amount) = extract_payout_amount(item) {
                            debug!("Extracted payout amount: {} lovelace", payout_amount);
                            total_price = total_price.saturating_add(payout_amount);
                        }
                    }

                    if total_price > 0 {
                        debug!(
                            "Total JPG.store v3 price extracted: {} lovelace (₳{:.2})",
                            total_price,
                            total_price as f64 / 1_000_000.0
                        );
                        return Some(total_price);
                    }
                }

                // Recursively search in all values
                for val in map.values() {
                    if let Some(price) = find_jpg_store_v3_payouts(val, depth + 1) {
                        return Some(price);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for val in arr {
                    if let Some(price) = find_jpg_store_v3_payouts(val, depth + 1) {
                        return Some(price);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Extract amount_lovelace from a single payout object
    fn extract_payout_amount(payout: &serde_json::Value) -> Option<u64> {
        debug!(
            "Analyzing payout: {}",
            serde_json::to_string(payout).unwrap_or_else(|_| "invalid json".to_string())
        );

        if let serde_json::Value::Object(payout_obj) = payout {
            // Look for constructor pattern with fields containing address and amount
            if let Some(serde_json::Value::Array(fields)) = payout_obj.get("fields") {
                debug!("Found payout fields array with {} items", fields.len());
                if fields.len() >= 2 {
                    // Second field should be amount_lovelace
                    if let Some(amount_field) = fields.get(1) {
                        debug!(
                            "Examining amount field: {}",
                            serde_json::to_string(amount_field)
                                .unwrap_or_else(|_| "invalid json".to_string())
                        );

                        // Handle complex nested JPG.store v3 structure
                        if let Some(serde_json::Value::Array(map_entries)) = amount_field.get("map")
                        {
                            for entry in map_entries {
                                if let serde_json::Value::Object(entry_obj) = entry {
                                    if let Some(v_obj) = entry_obj.get("v") {
                                        if let Some(serde_json::Value::Array(value_fields)) =
                                            v_obj.get("fields")
                                        {
                                            if value_fields.len() >= 2 {
                                                if let Some(final_map) = value_fields.get(1) {
                                                    if let Some(serde_json::Value::Array(
                                                        final_entries,
                                                    )) = final_map.get("map")
                                                    {
                                                        for final_entry in final_entries {
                                                            if let Some(final_v) =
                                                                final_entry.get("v")
                                                            {
                                                                if let Some(amount_val) =
                                                                    final_v.get("int")
                                                                {
                                                                    if let Some(amount) =
                                                                        amount_val.as_u64()
                                                                    {
                                                                        debug!("Successfully extracted payout amount: {} lovelace", amount);
                                                                        return Some(amount);
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
                            }
                        }

                        // Try direct int access first (fallback for simpler structures)
                        if let Some(amount) = amount_field.get("int") {
                            if let Some(amount_val) = amount.as_u64() {
                                debug!("Found direct int amount: {} lovelace", amount_val);
                                return Some(amount_val);
                            }
                        }
                        // Try as direct integer
                        if let Some(amount_val) = amount_field.as_u64() {
                            debug!("Found direct u64 amount: {} lovelace", amount_val);
                            return Some(amount_val);
                        }
                    }
                }
            }
        }
        debug!("No amount found in payout");
        None
    }

    /// Generic price extraction as fallback
    pub fn extract_generic_price_from_json(json: &serde_json::Value) -> Option<u64> {
        // Try to find any integer values that could represent prices (in lovelace)
        // Look for integers in the range of 1 ADA to 1000 ADA (reasonable price range)
        const MIN_PRICE: u64 = 1_000_000; // 1 ADA
        const MAX_PRICE: u64 = 1_000_000_000; // 1000 ADA

        extract_price_candidates(json, 0, MIN_PRICE, MAX_PRICE)
    }

    /// Recursively search for price candidates in JSON structure
    fn extract_price_candidates(
        value: &serde_json::Value,
        depth: u8,
        min_price: u64,
        max_price: u64,
    ) -> Option<u64> {
        if depth > 10 {
            return None; // Prevent infinite recursion
        }

        match value {
            serde_json::Value::Object(map) => {
                // Look for "int" fields that could contain prices
                if let Some(int_val) = map.get("int") {
                    if let Some(price) = int_val.as_str().and_then(|s| s.parse::<u64>().ok()) {
                        if price >= min_price && price <= max_price {
                            debug!("Found potential price candidate: {} lovelace", price);
                            return Some(price);
                        }
                    }
                    if let Some(price) = int_val.as_u64() {
                        if price >= min_price && price <= max_price {
                            debug!("Found potential price candidate: {} lovelace", price);
                            return Some(price);
                        }
                    }
                }

                // Recursively search in all values
                for val in map.values() {
                    if let Some(price) =
                        extract_price_candidates(val, depth + 1, min_price, max_price)
                    {
                        return Some(price);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for val in arr {
                    if let Some(price) =
                        extract_price_candidates(val, depth + 1, min_price, max_price)
                    {
                        return Some(price);
                    }
                }
            }
            serde_json::Value::Number(num) => {
                if let Some(price) = num.as_u64() {
                    if price >= min_price && price <= max_price {
                        debug!("Found potential price candidate: {} lovelace", price);
                        return Some(price);
                    }
                }
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = MarketplaceDatumParser::new(MarketplaceType::JpgStoreV3);
        assert_eq!(parser.marketplace, MarketplaceType::JpgStoreV3);
    }

    #[test]
    fn test_jpg_store_v4_ask_datum() {
        // Real datum from JPG.store V4 listing tx 6ade3937324e2be6d56cb39e97e7f0d0e8a924bc84ef26dc7951a982aae86828
        // Asset: ClayMates131 (policy 2309519e53dfedd2e1143dd54eae5e6236ff85e6ce1e42d2b738fb53)
        let datum_hex = "d8799fd8799f581c2309519e53dfedd2e1143dd54eae5e6236ff85e6ce1e42d2b738fb534c436c61794d61746573313331ffd8799f581ce42614770497a6766916c714924c29ad4a7e23d52bffd697cb94b839581c12596381573f1ad2de6d72e585fc93aa0496bc211bbab7726b5eeda4ffff";
        let cbor_bytes = hex::decode(datum_hex).expect("Valid hex");

        let parser = MarketplaceDatumParser::new(MarketplaceType::JpgStoreV4);
        let operation = parser
            .parse_cbor(&cbor_bytes)
            .expect("Should parse V4 datum");

        match &operation {
            MarketplaceOperation::Ask { asset, targets } => {
                let asset = asset.as_ref().expect("V4 should have asset");
                assert_eq!(
                    asset.policy_id,
                    "2309519e53dfedd2e1143dd54eae5e6236ff85e6ce1e42d2b738fb53"
                );
                assert_eq!(asset.asset_name_hex, "436c61794d61746573313331");
                // V4 has no price in datum — targets should be empty
                assert!(targets.is_empty(), "V4 datum should have no price targets");
                // total_price should be 0 (no price in datum)
                assert_eq!(operation.total_price_lovelace(), Some(0));
            }
            _ => panic!("Expected Ask operation for V4 listing"),
        }
    }

    #[test]
    fn test_kwic_price_update_datum_structure() {
        // Datum from the failing kwic_price_update test - this is a JPG.store V2 listing update
        let datum_hex = "d8799f9fd8799fd8799fd8799f581c71e8f145d1a99038961dc7e24ffc8624e2da7940901130f4f434c5feffd8799fd8799fd8799f581ca74eae0128c72faa7e6d9b4429784f9796b774fe606d42677684a2beffffffff1a005f5e10ffd8799fd8799fd8799f581cfa83bf30f05b0848474e1210f07208825437ec34b80eb55afa273325ffd8799fd8799fd8799f581c0ce6f57bb20e0b8e72f4b13eebe9da11cd47a2e736640ebba3ce1116ffffffff1a06edd590ffff581cfa83bf30f05b0848474e1210f07208825437ec34b80eb55afa273325ff";
        let cbor_bytes = hex::decode(datum_hex).expect("Valid hex");

        // This should be parsed as JPG.store V2 with our new listing update schema
        let parser = MarketplaceDatumParser::new(MarketplaceType::JpgStoreV2);

        // This datum contains valid pricing data but no identifiable policy ID/asset.
        // Parse should succeed with asset=None and valid pricing targets.
        let result = parser
            .parse_cbor(&cbor_bytes)
            .expect("Should parse pricing even without asset");
        assert!(
            result.get_asset().is_none(),
            "Asset should be None when not identifiable"
        );
        assert!(
            result.total_price_lovelace().unwrap() > 0,
            "Should still extract pricing"
        );
    }
}
