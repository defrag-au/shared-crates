//! TOML-based schema loader and CBOR extraction engine
//!
//! This module provides lightweight schema-driven parsing using TOML schema definitions
//! that work directly with PlutusData structures without heavy CDDL dependencies.

use crate::{DatumParsingError, Result};
use pallas_primitives::alonzo::PlutusData;
use pipeline_types::{AssetId, OperationPayload};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

/// TOML schema definition for a datum type
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TomlSchema {
    /// Schema metadata
    pub schema: SchemaMetadata,
    /// Type definitions
    pub types: HashMap<String, TypeDefinition>,
    /// Extraction instructions
    pub extraction: ExtractionMethods,
}

/// Schema metadata
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SchemaMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub root_type: String,
}

/// Type definition for a CBOR structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TypeDefinition {
    /// Type category
    #[serde(rename = "type")]
    pub type_kind: String,
    /// Description
    pub description: Option<String>,
    /// For arrays: minimum items
    pub min_items: Option<usize>,
    /// For arrays: maximum items
    pub max_items: Option<usize>,
    /// For arrays: item type
    pub item_type: Option<String>,
    /// For constructors: tag number
    pub constructor_tag: Option<u64>,
    /// For constructors: minimum fields
    pub min_fields: Option<usize>,
    /// For constructors: maximum fields
    pub max_fields: Option<usize>,
    /// For bytes: expected length
    pub byte_length: Option<usize>,
    /// For uints: value range
    pub range: Option<[u64; 2]>,
    /// Field definitions for structured types
    pub fields: Option<Vec<FieldDefinition>>,
}

/// Field definition within a type
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FieldDefinition {
    pub name: String,
    pub index: usize,
    #[serde(rename = "type")]
    pub field_type: String,
    pub description: Option<String>,
    pub required: bool,
    /// Extraction hint for this field
    pub extract_as: Option<String>,
    /// Extraction method for this field
    pub extract_method: Option<String>,
    /// For bytes: expected length
    pub byte_length: Option<usize>,
    /// For uints: value range
    pub range: Option<[u64; 2]>,
    /// For arrays: minimum items
    pub min_items: Option<usize>,
    /// For arrays: item type
    pub item_type: Option<String>,
}

/// Extraction methods and paths
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractionMethods {
    /// How to extract price from this datum
    pub price_extraction: Option<String>,
    /// Direct field name for price (if price_extraction = "direct_field")
    pub price_field: Option<String>,
    /// Path to seller address
    pub seller_path: Option<String>,
    /// Path to bidder address (for bids)
    pub bidder_path: Option<String>,
    /// Path to policy ID
    pub policy_path: Option<String>,
    /// Path to asset name
    pub asset_name_path: Option<String>,
    /// Path to expiration timestamp
    pub expiration_path: Option<String>,
    /// Fee extraction method
    pub fee_extraction: Option<String>,
    /// Direct field for fees
    pub fee_field: Option<String>,
    /// Seller extraction method
    pub seller_extraction: Option<String>,
}

/// Schema loader for TOML-based datum schemas (lazy loading)
#[derive(Debug)]
pub struct TomlSchemaLoader {
    /// Loaded schemas by name (cache)
    schemas: HashMap<String, TomlSchema>,
}

impl Default for TomlSchemaLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl TomlSchemaLoader {
    /// Create a new schema loader with lazy loading
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// Load a schema from string content
    fn load_schema_from_string(&mut self, name: &str, content: &str) -> Result<()> {
        debug!("Loading embedded TOML schema: {}", name);

        let schema: TomlSchema = toml::from_str(content).map_err(|e| {
            DatumParsingError::SchemaValidation(format!("Failed to parse TOML for {name}: {e}"))
        })?;

        debug!(
            "Loaded embedded schema: {} v{}",
            schema.schema.name, schema.schema.version
        );
        self.schemas.insert(schema.schema.name.clone(), schema);

        Ok(())
    }

    /// Get a schema by name (loads on-demand)
    pub fn get_schema(&mut self, name: &str) -> Option<&TomlSchema> {
        // Check if already loaded
        if self.schemas.contains_key(name) {
            return self.schemas.get(name);
        }

        // Try to load the specific schema
        if let Some(content) = self.get_embedded_schema_content(name) {
            if let Ok(()) = self.load_schema_from_string(name, content) {
                debug!("Lazy loaded schema: {}", name);
                return self.schemas.get(name);
            } else {
                warn!("Failed to lazy load schema: {}", name);
            }
        }

        None
    }

    /// Get embedded schema content by name (without loading into memory)
    fn get_embedded_schema_content(&self, name: &str) -> Option<&'static str> {
        match name {
            "jpg_store_v1_ask" => Some(include_str!("../projects/jpg-store-v1/ask.toml")),
            "jpg_store_v1_bid" => Some(include_str!("../projects/jpg-store-v1/bid.toml")),
            "jpg_store_v2_ask" => Some(include_str!("../projects/jpg-store-v2/ask.toml")),
            "jpg_store_v2_bid" => Some(include_str!("../projects/jpg-store-v2/bid.toml")),
            "jpg_store_v3_ask" => Some(include_str!("../projects/jpg-store-v3/ask.toml")),
            "jpg_store_v3_bid" => Some(include_str!("../projects/jpg-store-v3/bid.toml")),
            "jpg_store_v3_fee" => Some(include_str!("../projects/jpg-store-v3/fee.toml")),
            "jpg_store_v4_ask" => Some(include_str!("../projects/jpg-store-v4/ask.toml")),
            "wayup_ask" => Some(include_str!("../projects/wayup/ask.toml")),
            _ => None,
        }
    }

    /// List all loaded schema names
    pub fn list_schemas(&self) -> Vec<&str> {
        self.schemas.keys().map(|s| s.as_str()).collect()
    }

    /// List all available schema names (including unloaded)
    pub fn list_available_schemas(&self) -> Vec<&'static str> {
        vec![
            "jpg_store_v1_ask",
            "jpg_store_v1_bid",
            "jpg_store_v2_ask",
            "jpg_store_v2_bid",
            "jpg_store_v3_ask",
            "jpg_store_v3_bid",
            "jpg_store_v3_fee",
            "jpg_store_v4_ask",
            "wayup_ask",
        ]
    }
}

/// CBOR extraction engine using TOML schemas
#[derive(Debug)]
pub struct CborExtractor<'a> {
    schema: &'a TomlSchema,
}

impl<'a> CborExtractor<'a> {
    /// Create a new extractor for a schema
    pub fn new(schema: &'a TomlSchema) -> Self {
        Self { schema }
    }

    /// Extract marketplace operation from CBOR bytes using the schema
    pub fn extract_marketplace_operation(
        &self,
        cbor_bytes: &[u8],
    ) -> Result<super::marketplace_parsers::MarketplaceOperation> {
        // Decode CBOR into PlutusData
        let plutus_data: PlutusData = pallas_codec::minicbor::decode(cbor_bytes)
            .map_err(|e| DatumParsingError::CborDecode(format!("Failed to decode CBOR: {e}")))?;

        debug!(
            "Extracting from PlutusData using schema: {}",
            self.schema.schema.name
        );

        // Get the root type definition
        let root_type = self
            .schema
            .types
            .get(&self.schema.schema.root_type)
            .ok_or_else(|| {
                DatumParsingError::SchemaValidation(format!(
                    "Root type '{}' not found in schema",
                    self.schema.schema.root_type
                ))
            })?;

        // Validate and extract based on schema
        self.extract_operation_from_plutus_data(&plutus_data, root_type)
    }

    /// Extract operation from PlutusData using type definition
    fn extract_operation_from_plutus_data(
        &self,
        data: &PlutusData,
        type_def: &TypeDefinition,
    ) -> Result<super::marketplace_parsers::MarketplaceOperation> {
        match type_def.type_kind.as_str() {
            "array" => self.extract_from_array(data, type_def),
            "constructor" => self.extract_from_constructor(data, type_def),
            _ => Err(DatumParsingError::SchemaValidation(format!(
                "Unsupported root type: {}",
                type_def.type_kind
            ))),
        }
    }

    /// Extract from array-based datum
    fn extract_from_array(
        &self,
        data: &PlutusData,
        type_def: &TypeDefinition,
    ) -> Result<super::marketplace_parsers::MarketplaceOperation> {
        if let PlutusData::Array(arr) = data {
            // Validate array bounds
            if let Some(min_items) = type_def.min_items {
                if arr.len() < min_items {
                    return Err(DatumParsingError::SchemaValidation(format!(
                        "Array too short: {} < {}",
                        arr.len(),
                        min_items
                    )));
                }
            }
            if let Some(max_items) = type_def.max_items {
                if arr.len() > max_items {
                    return Err(DatumParsingError::SchemaValidation(format!(
                        "Array too long: {} > {}",
                        arr.len(),
                        max_items
                    )));
                }
            }

            debug!("Processing array with {} items", arr.len());

            // Extract price using schema instructions
            let price = self.extract_price_from_array(arr, type_def)?;

            // Determine if this is an Ask or Bid based on schema name or extraction methods
            if self.schema.schema.name.contains("bid") {
                // Extract bid information
                let policy_id = self.extract_policy_id_from_array(arr, type_def)?;
                let asset_name = self.extract_asset_name_from_array(arr, type_def);

                Ok(super::marketplace_parsers::MarketplaceOperation::Bid {
                    policy_id,
                    asset_name_hex: asset_name,
                    offer_lovelace: price,
                })
            } else {
                // Extract ask information
                let asset = self.extract_asset_from_array(arr, type_def).ok();
                let targets = self.extract_targets_from_array(arr, type_def, price)?;

                Ok(super::marketplace_parsers::MarketplaceOperation::Ask { asset, targets })
            }
        } else {
            Err(DatumParsingError::SchemaValidation(
                "Expected array for array type".to_string(),
            ))
        }
    }

    /// Extract from constructor-based datum
    fn extract_from_constructor(
        &self,
        data: &PlutusData,
        type_def: &TypeDefinition,
    ) -> Result<super::marketplace_parsers::MarketplaceOperation> {
        if let PlutusData::Constr(constr) = data {
            // Validate constructor tag if specified
            if let Some(expected_tag) = type_def.constructor_tag {
                if constr.tag != expected_tag {
                    return Err(DatumParsingError::SchemaValidation(format!(
                        "Wrong constructor tag: {} != {}",
                        constr.tag, expected_tag
                    )));
                }
            }

            // Validate field count
            if let Some(min_fields) = type_def.min_fields {
                if constr.fields.len() < min_fields {
                    return Err(DatumParsingError::SchemaValidation(format!(
                        "Too few fields: {} < {}",
                        constr.fields.len(),
                        min_fields
                    )));
                }
            }
            if let Some(max_fields) = type_def.max_fields {
                if constr.fields.len() > max_fields {
                    return Err(DatumParsingError::SchemaValidation(format!(
                        "Too many fields: {} > {}",
                        constr.fields.len(),
                        max_fields
                    )));
                }
            }

            debug!(
                "Processing constructor with tag {} and {} fields",
                constr.tag,
                constr.fields.len()
            );

            // For constructor, treat fields as array for extraction
            self.extract_from_array(&PlutusData::Array(constr.fields.clone()), type_def)
        } else {
            Err(DatumParsingError::SchemaValidation(
                "Expected constructor for constructor type".to_string(),
            ))
        }
    }

    /// Extract price from array using schema extraction methods
    fn extract_price_from_array(
        &self,
        arr: &[PlutusData],
        type_def: &TypeDefinition,
    ) -> Result<u64> {
        match self.schema.extraction.price_extraction.as_deref() {
            Some("direct_field") => {
                if let Some(field_name) = &self.schema.extraction.price_field {
                    self.extract_direct_field_value(arr, type_def, field_name)
                } else {
                    Err(DatumParsingError::SchemaValidation(
                        "price_field not specified for direct_field extraction".to_string(),
                    ))
                }
            }
            Some("sum_payout_amounts") => self.extract_sum_of_payouts(arr, type_def),
            Some("sum_wayup_amounts") => self.extract_sum_of_wayup_payouts(arr, type_def),
            _ => {
                // Fallback: try to find any reasonable uint value
                self.extract_fallback_price(arr)
            }
        }
    }

    /// Extract value from a specific field by name
    fn extract_direct_field_value(
        &self,
        arr: &[PlutusData],
        type_def: &TypeDefinition,
        field_name: &str,
    ) -> Result<u64> {
        if let Some(fields) = &type_def.fields {
            for field in fields {
                if field.name == *field_name && field.index < arr.len() {
                    return self.extract_uint_value(&arr[field.index]);
                }
            }
        }
        Err(DatumParsingError::SchemaValidation(format!(
            "Field '{field_name}' not found"
        )))
    }

    /// Extract and sum payout amounts (JPG.store style)
    fn extract_sum_of_payouts(&self, arr: &[PlutusData], type_def: &TypeDefinition) -> Result<u64> {
        // Find the payouts field
        if let Some(fields) = &type_def.fields {
            for field in fields {
                if field.extract_method.as_deref() == Some("sum_payout_amounts")
                    && field.index < arr.len()
                {
                    return self.sum_array_amounts(&arr[field.index]);
                }
            }
        }
        Err(DatumParsingError::SchemaValidation(
            "No payouts field found for sum_payout_amounts".to_string(),
        ))
    }

    /// Extract and sum Wayup-style payout amounts
    fn extract_sum_of_wayup_payouts(
        &self,
        arr: &[PlutusData],
        type_def: &TypeDefinition,
    ) -> Result<u64> {
        // Find the payment targets field for Wayup
        if let Some(fields) = &type_def.fields {
            for field in fields {
                if field.extract_method.as_deref() == Some("sum_wayup_amounts")
                    && field.index < arr.len()
                {
                    debug!("Found Wayup payment targets field at index {}", field.index);
                    return self.sum_wayup_amounts(&arr[field.index]);
                }
            }
        }
        Err(DatumParsingError::SchemaValidation(
            "No payment targets field found for sum_wayup_amounts".to_string(),
        ))
    }

    /// Sum amounts in an array of payout structures (JPG.store style)
    fn sum_array_amounts(&self, data: &PlutusData) -> Result<u64> {
        if let PlutusData::Array(payouts) = data {
            let mut total = 0u64;

            for payout in payouts.iter() {
                // Try different payout structures:

                // 1. Simple array with amount at index 1 (original JPG.store style)
                if let PlutusData::Array(payout_fields) = payout {
                    if payout_fields.len() >= 2 {
                        if let Ok(amount) = self.extract_uint_value(&payout_fields[1]) {
                            debug!("Found amount in array at index 1: {}", amount);
                            total = total.saturating_add(amount);
                            continue;
                        }
                    }
                }

                // 2. Constructor-based structure (JPG.store V2 style) - recursively search for amounts
                if let Some(amount) = self.find_amount_in_payout_recursive(payout, 0) {
                    debug!("Found amount in constructor structure: {}", amount);
                    total = total.saturating_add(amount);
                }
            }

            if total > 0 {
                debug!("Total extracted from payouts: {} lovelace", total);
                Ok(total)
            } else {
                Err(DatumParsingError::SchemaValidation(
                    "No valid amounts found in payouts".to_string(),
                ))
            }
        } else {
            Err(DatumParsingError::SchemaValidation(
                "Expected array for payouts".to_string(),
            ))
        }
    }

    /// Sum amounts in Wayup-style constructor-based payout structures
    fn sum_wayup_amounts(&self, data: &PlutusData) -> Result<u64> {
        if let PlutusData::Array(payouts) = data {
            let mut total = 0u64;

            for payout in payouts.iter() {
                // Wayup style: constructor with fields[1] containing the amount
                if let PlutusData::Constr(payout_constr) = payout {
                    if payout_constr.fields.len() >= 2 {
                        if let Ok(amount) = self.extract_uint_value(&payout_constr.fields[1]) {
                            debug!("Extracted Wayup payout amount: {} lovelace", amount);
                            total = total.saturating_add(amount);
                        }
                    }
                }
            }

            if total > 0 {
                debug!("Total Wayup price extracted: {} lovelace", total);
                Ok(total)
            } else {
                Err(DatumParsingError::SchemaValidation(
                    "No valid Wayup amounts found".to_string(),
                ))
            }
        } else {
            Err(DatumParsingError::SchemaValidation(
                "Expected array for Wayup payouts".to_string(),
            ))
        }
    }

    /// Fallback price extraction - recursively find any reasonable uint
    fn extract_fallback_price(&self, arr: &[PlutusData]) -> Result<u64> {
        let mut candidate_prices = Vec::new();

        // Recursively search for all uint values in the datum
        for item in arr {
            self.collect_uint_values_recursive(item, &mut candidate_prices, 0);
        }

        // Sort by value and find the largest reasonable price
        candidate_prices.sort_by(|a, b| b.cmp(a)); // Sort descending

        for price in candidate_prices {
            // Check if it's in a reasonable price range (0.1 ADA to 1000 ADA)
            if (100_000..=1_000_000_000).contains(&price) {
                debug!("Found fallback price: {} lovelace", price);
                return Ok(price);
            }
        }

        Err(DatumParsingError::SchemaValidation(
            "No reasonable price value found".to_string(),
        ))
    }

    /// Recursively collect all uint values from PlutusData
    fn collect_uint_values_recursive(&self, data: &PlutusData, values: &mut Vec<u64>, depth: u8) {
        if depth > 15 {
            return; // Prevent infinite recursion
        }

        // Try to extract uint value from current item
        if let Ok(value) = self.extract_uint_value(data) {
            values.push(value);
        }

        // Recursively search nested structures
        match data {
            PlutusData::Constr(constr) => {
                for field in constr.fields.iter() {
                    self.collect_uint_values_recursive(field, values, depth + 1);
                }
            }
            PlutusData::Array(arr) => {
                for item in arr.iter() {
                    self.collect_uint_values_recursive(item, values, depth + 1);
                }
            }
            PlutusData::Map(map) => {
                for (key, value) in map.iter() {
                    self.collect_uint_values_recursive(key, values, depth + 1);
                    self.collect_uint_values_recursive(value, values, depth + 1);
                }
            }
            _ => {}
        }
    }

    /// Extract uint value from PlutusData
    fn extract_uint_value(&self, data: &PlutusData) -> Result<u64> {
        match data {
            PlutusData::BigInt(big_int) => {
                // Try to convert BigInt to u64
                match big_int {
                    pallas_primitives::alonzo::BigInt::Int(i) => {
                        if *i >= 0.into() {
                            Ok(i128::from(*i) as u64)
                        } else {
                            Err(DatumParsingError::SchemaValidation(
                                "Negative integer found".to_string(),
                            ))
                        }
                    }
                    pallas_primitives::alonzo::BigInt::BigUInt(bytes) => {
                        // Convert bytes to u64 if possible
                        if bytes.len() <= 8 {
                            let mut value = 0u64;
                            for byte in bytes.iter() {
                                value = (value << 8) | (*byte as u64);
                            }
                            Ok(value)
                        } else {
                            Err(DatumParsingError::SchemaValidation(
                                "Integer too large for u64".to_string(),
                            ))
                        }
                    }
                    pallas_primitives::alonzo::BigInt::BigNInt(_) => {
                        Err(DatumParsingError::SchemaValidation(
                            "Negative big integer found".to_string(),
                        ))
                    }
                }
            }
            _ => Err(DatumParsingError::SchemaValidation(
                "Expected integer".to_string(),
            )),
        }
    }

    /// Extract policy ID from array
    fn extract_policy_id_from_array(
        &self,
        arr: &[PlutusData],
        type_def: &TypeDefinition,
    ) -> Result<String> {
        // First try field-based extraction (for simple schemas)
        if let Some(field_name) = &self.schema.extraction.policy_path {
            if let Some(fields) = &type_def.fields {
                for field in fields {
                    if field.name == *field_name && field.index < arr.len() {
                        if let Ok(policy_id) = self.extract_bytes_value(&arr[field.index]) {
                            return Ok(policy_id);
                        }
                    }
                }
            }
        }

        // Fallback to recursive search (for complex structures like JPG.store)
        if let Some(policy_id) = self.find_policy_id_in_data(arr) {
            debug!("Found policy ID via recursive search: {}", policy_id);
            Ok(policy_id)
        } else {
            Err(DatumParsingError::SchemaValidation(
                "Policy ID not found".to_string(),
            ))
        }
    }

    /// Extract asset name from array (optional)
    fn extract_asset_name_from_array(
        &self,
        arr: &[PlutusData],
        type_def: &TypeDefinition,
    ) -> Option<String> {
        // First try field-based extraction (for simple schemas)
        if let Some(field_name) = &self.schema.extraction.asset_name_path {
            if let Some(fields) = &type_def.fields {
                for field in fields {
                    if field.name == *field_name && field.index < arr.len() {
                        if let Ok(bytes_hex) = self.extract_bytes_value(&arr[field.index]) {
                            return Some(bytes_hex);
                        }
                    }
                }
            }
        }

        // Fallback to recursive search (for complex structures like JPG.store)
        // We need the policy ID to search for asset names within that policy's map
        if let Some(policy_id) = self.find_policy_id_in_data(arr) {
            if let Some(asset_name) = self.find_asset_name_in_data(arr, &policy_id) {
                debug!("Found asset name via recursive search: {}", asset_name);
                return Some(asset_name);
            }
        }

        None
    }

    /// Extract bytes value as hex string
    fn extract_bytes_value(&self, data: &PlutusData) -> Result<String> {
        match data {
            PlutusData::BoundedBytes(bytes) => Ok(hex::encode(bytes.as_slice())),
            _ => Err(DatumParsingError::SchemaValidation(
                "Expected bytes".to_string(),
            )),
        }
    }

    /// Extract asset from array or constructor fields
    fn extract_asset_from_array(
        &self,
        arr: &[PlutusData],
        _type_def: &TypeDefinition,
    ) -> Result<AssetId> {
        // Look for policy ID in the PlutusData structure
        // For JPG.store bid datums, the policy ID appears as a map key in the fields
        if let Some(policy_id) = self.find_policy_id_in_data(arr) {
            // For collection bids, there's typically no specific asset name (empty map value)
            // For specific asset bids, there would be an asset name in the map value
            let asset_name = self
                .find_asset_name_in_data(arr, &policy_id)
                .unwrap_or_default();

            debug!(
                "Extracted asset: policy_id={}, asset_name_hex={}",
                policy_id, asset_name
            );
            AssetId::new(policy_id, asset_name)
                .map_err(|e| DatumParsingError::SchemaValidation(format!("Invalid asset: {e}")))
        } else {
            Err(DatumParsingError::SchemaValidation(
                "No policy ID found in datum".to_string(),
            ))
        }
    }

    /// Extract targets from array
    fn extract_targets_from_array(
        &self,
        _arr: &[PlutusData],
        _type_def: &TypeDefinition,
        price: u64,
    ) -> Result<Vec<super::marketplace_parsers::LockedTarget>> {
        // For now, return a single target with the extracted price
        // In a full implementation, this would extract actual target addresses from payouts
        Ok(vec![super::marketplace_parsers::LockedTarget {
            target: "extracted_seller_address".to_string(),
            payload: OperationPayload::Lovelace { amount: price },
        }])
    }

    /// Find policy ID in PlutusData structure (recursive search)
    fn find_policy_id_in_data(&self, data: &[PlutusData]) -> Option<String> {
        self.find_policy_id_recursive(data, 0)
    }

    /// Recursively search for policy ID in PlutusData
    #[allow(clippy::only_used_in_recursion)]
    fn find_policy_id_recursive(&self, data: &[PlutusData], depth: u8) -> Option<String> {
        if depth > 10 {
            return None; // Prevent infinite recursion
        }

        // First priority: Look for policy IDs as map keys (this is where the asset policy ID appears in JPG.store bids)
        for item in data {
            if let PlutusData::Map(map) = item {
                for (key, _value) in map.iter() {
                    if let PlutusData::BoundedBytes(bytes) = key {
                        let hex_string = hex::encode(bytes.as_slice());
                        // Policy IDs are 56 characters (28 bytes in hex)
                        if hex_string.len() == 56
                            && hex_string.chars().all(|c| c.is_ascii_hexdigit())
                        {
                            debug!("Found policy ID as map key: {}", hex_string);
                            return Some(hex_string);
                        }
                    }
                }
            }
        }

        // Second priority: Recursively search in nested structures
        for item in data {
            match item {
                PlutusData::Constr(constr) => {
                    if let Some(policy_id) =
                        self.find_policy_id_recursive(&constr.fields, depth + 1)
                    {
                        return Some(policy_id);
                    }
                }
                PlutusData::Array(arr) => {
                    if let Some(policy_id) = self.find_policy_id_recursive(arr, depth + 1) {
                        return Some(policy_id);
                    }
                }
                PlutusData::Map(map) => {
                    // Already handled above, but check nested maps
                    for (_key, value) in map.iter() {
                        if let Some(policy_id) =
                            self.find_policy_id_recursive(std::slice::from_ref(value), depth + 1)
                        {
                            return Some(policy_id);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Find asset name for a specific policy ID in PlutusData structure
    fn find_asset_name_in_data(&self, data: &[PlutusData], policy_id: &str) -> Option<String> {
        debug!("Searching for asset name for policy ID: {}", policy_id);
        // Search for the policy ID as a map key, then look at its associated value
        // for asset name information
        let result = self.find_asset_name_recursive(data, policy_id, 0);
        debug!("Asset name search result for {}: {:?}", policy_id, result);
        result
    }

    /// Recursively search for asset names within the policy ID's map value
    fn find_asset_name_recursive(
        &self,
        data: &[PlutusData],
        policy_id: &str,
        depth: u8,
    ) -> Option<String> {
        if depth > 15 {
            return None; // Prevent infinite recursion
        }

        for item in data {
            match item {
                PlutusData::Map(map) => {
                    // Look for our target policy ID as a map key
                    for (key, value) in map.iter() {
                        if let PlutusData::BoundedBytes(bytes) = key {
                            let hex_string = hex::encode(bytes.as_slice());
                            if hex_string == policy_id {
                                // Found our policy ID! Now look for asset names in the value
                                if let Some(asset_name) =
                                    self.extract_asset_name_from_policy_value(value)
                                {
                                    debug!(
                                        "Found asset name for policy {}: {}",
                                        policy_id, asset_name
                                    );
                                    return Some(asset_name);
                                }
                                // If no asset name found in this policy's value, it's a collection bid
                                debug!(
                                    "No specific asset name found for policy {} (collection bid)",
                                    policy_id
                                );
                                return None;
                            }
                        }
                    }

                    // Recursively search in map values
                    for (_key, value) in map.iter() {
                        if let Some(asset_name) = self.find_asset_name_recursive(
                            std::slice::from_ref(value),
                            policy_id,
                            depth + 1,
                        ) {
                            return Some(asset_name);
                        }
                    }
                }
                PlutusData::Constr(constr) => {
                    if let Some(asset_name) =
                        self.find_asset_name_recursive(&constr.fields, policy_id, depth + 1)
                    {
                        return Some(asset_name);
                    }
                }
                PlutusData::Array(arr) => {
                    if let Some(asset_name) =
                        self.find_asset_name_recursive(arr, policy_id, depth + 1)
                    {
                        return Some(asset_name);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Extract asset name from the value associated with a policy ID
    fn extract_asset_name_from_policy_value(&self, value: &PlutusData) -> Option<String> {
        // Look for nested map structures within the policy value
        self.find_asset_name_in_nested_value(value, 0)
    }

    /// Search for asset names in nested structures within a policy value
    #[allow(clippy::only_used_in_recursion)]
    fn find_asset_name_in_nested_value(&self, value: &PlutusData, depth: u8) -> Option<String> {
        if depth > 10 {
            return None;
        }

        match value {
            PlutusData::Map(map) => {
                // Look for non-empty asset names as map keys
                for (key, _value) in map.iter() {
                    if let PlutusData::BoundedBytes(bytes) = key {
                        let hex_string = hex::encode(bytes.as_slice());
                        // Asset names are typically 8-64 characters and not empty
                        // Also not 56 chars (policy ID length)
                        if !hex_string.is_empty()
                            && hex_string.len() != 56
                            && hex_string.len() >= 8
                            && hex_string.len() <= 64
                            && hex_string.chars().all(|c| c.is_ascii_hexdigit())
                        {
                            debug!("Found asset name in policy value: {}", hex_string);
                            return Some(hex_string);
                        }
                    }
                }

                // Recursively search in map values
                for (_key, val) in map.iter() {
                    if let Some(asset_name) = self.find_asset_name_in_nested_value(val, depth + 1) {
                        return Some(asset_name);
                    }
                }
            }
            PlutusData::Constr(constr) => {
                for field in constr.fields.iter() {
                    if let Some(asset_name) = self.find_asset_name_in_nested_value(field, depth + 1)
                    {
                        return Some(asset_name);
                    }
                }
            }
            PlutusData::Array(arr) => {
                for item in arr.iter() {
                    if let Some(asset_name) = self.find_asset_name_in_nested_value(item, depth + 1)
                    {
                        return Some(asset_name);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Recursively find amounts in nested constructor structures
    fn find_amount_in_payout_recursive(&self, data: &PlutusData, depth: u8) -> Option<u64> {
        if depth > 10 {
            return None; // Prevent infinite recursion
        }

        // Try to extract if this is a direct uint
        if let Ok(amount) = self.extract_uint_value(data) {
            // Only consider reasonable amounts (> 1000 lovelace)
            if amount > 1000 {
                return Some(amount);
            }
        }

        // Recursively search in nested structures
        match data {
            PlutusData::Constr(constr) => {
                for field in constr.fields.iter() {
                    if let Some(amount) = self.find_amount_in_payout_recursive(field, depth + 1) {
                        return Some(amount);
                    }
                }
            }
            PlutusData::Array(arr) => {
                for item in arr.iter() {
                    if let Some(amount) = self.find_amount_in_payout_recursive(item, depth + 1) {
                        return Some(amount);
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
    fn test_schema_loader_creation() {
        let mut loader = TomlSchemaLoader::new();
        // Should start empty with lazy loading
        assert_eq!(
            loader.schemas.len(),
            0,
            "Should start with no schemas loaded (lazy loading)"
        );

        // Test that we can load a schema on demand
        assert!(loader.get_schema("jpg_store_v3_ask").is_some());
        assert_eq!(
            loader.schemas.len(),
            1,
            "Should have 1 schema loaded after access"
        );
    }

    #[test]
    fn test_load_schema_from_toml() {
        let mut loader = TomlSchemaLoader::new();

        // Test that no schemas are loaded initially (lazy loading)
        let schema_names = loader.list_schemas();
        assert!(
            schema_names.is_empty(),
            "Should start with no schemas loaded"
        );

        // Test that we can get a specific schema
        let jpg_v3_schema = loader.get_schema("jpg_store_v3_ask");
        assert!(
            jpg_v3_schema.is_some(),
            "Should have JPG.store V3 ask schema"
        );

        if let Some(schema) = jpg_v3_schema {
            assert_eq!(schema.schema.name, "jpg_store_v3_ask");
            assert_eq!(schema.schema.version, "3.0");
        }

        // Now we should have one schema loaded
        let schema_names = loader.list_schemas();
        assert_eq!(
            schema_names.len(),
            1,
            "Should have 1 schema loaded after access"
        );
    }
}
