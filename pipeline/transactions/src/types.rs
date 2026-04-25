//! Core transaction types and structures
//!
//! This module contains all the fundamental data structures used to represent
//! Cardano transactions in a unified format across different data sources.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Transaction datum information stored in different formats
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TxDatum {
    /// Datum referenced by hash only
    Hash { hash: String },
    /// Datum with raw CBOR bytes (and computed hash)
    Bytes {
        hash: String,
        bytes: String, // Hex-encoded CBOR bytes
    },
    /// Datum with decoded JSON representation (and optional bytes + computed hash)
    Json {
        hash: String,
        json: serde_json::Value,
        bytes: Option<String>, // Hex-encoded CBOR bytes if available
    },
}

impl TxDatum {
    /// Get the hash of this datum
    pub fn hash(&self) -> &str {
        match self {
            TxDatum::Hash { hash } => hash,
            TxDatum::Bytes { hash, .. } => hash,
            TxDatum::Json { hash, .. } => hash,
        }
    }

    /// Get the CBOR bytes if available
    pub fn bytes(&self) -> Option<&str> {
        match self {
            TxDatum::Hash { .. } => None,
            TxDatum::Bytes { bytes, .. } => Some(bytes),
            TxDatum::Json { bytes, .. } => bytes.as_deref(),
        }
    }

    /// Get the JSON representation if available
    pub fn json(&self) -> Option<&serde_json::Value> {
        match self {
            TxDatum::Hash { .. } => None,
            TxDatum::Bytes { .. } => None,
            TxDatum::Json { json, .. } => Some(json),
        }
    }

    /// Check if this datum has CBOR bytes available
    pub fn has_bytes(&self) -> bool {
        self.bytes().is_some()
    }

    /// Decode CBOR bytes to extract structured PlutusData using Pallas
    pub fn decode_cbor(&self) -> Result<pallas_primitives::alonzo::PlutusData, String> {
        use pallas_codec::minicbor::{Decode, Decoder};

        let bytes_str = self.bytes().ok_or("No CBOR bytes available")?;
        let bytes = hex::decode(bytes_str).map_err(|e| format!("Invalid hex: {e}"))?;

        let mut decoder = Decoder::new(&bytes);
        pallas_primitives::alonzo::PlutusData::decode(&mut decoder, &mut ())
            .map_err(|e| format!("CBOR decode error: {e}"))
    }

    /// Try to extract price information from sales script datums
    /// Looks for lovelace amounts that could represent sale prices
    pub fn extract_potential_prices(&self) -> Vec<u64> {
        if let Ok(cbor_value) = self.decode_cbor() {
            Self::find_lovelace_amounts(&cbor_value)
        } else {
            Vec::new()
        }
    }

    /// Extract all potential monetary values including fees and smaller amounts
    /// Looks for any integer that could represent lovelace (including fees/royalties)
    pub fn extract_all_monetary_values(&self) -> Vec<u64> {
        if let Ok(cbor_value) = self.decode_cbor() {
            Self::find_all_monetary_amounts(&cbor_value)
        } else {
            Vec::new()
        }
    }

    /// Recursively search PlutusData structure for potential lovelace amounts
    /// Looks for integers in typical price ranges (1 ADA to 10,000 ADA)
    fn find_lovelace_amounts(value: &pallas_primitives::alonzo::PlutusData) -> Vec<u64> {
        let mut amounts = Vec::new();

        match value {
            pallas_primitives::alonzo::PlutusData::BigInt(big_int) => {
                match big_int {
                    pallas_primitives::alonzo::BigInt::Int(int) => {
                        let val: i128 = (*int).into();
                        // Look for positive integers in reasonable price range
                        if (1_000_000..=10_000_000_000).contains(&val) {
                            // >= 1 ADA to 10,000 ADA
                            amounts.push(val as u64);
                        }
                    }
                    pallas_primitives::alonzo::BigInt::BigUInt(bytes)
                    | pallas_primitives::alonzo::BigInt::BigNInt(bytes) => {
                        // Handle big integers - simplified conversion
                        if bytes.len() <= 8 {
                            let mut num_bytes = [0u8; 8];
                            num_bytes[8 - bytes.len()..].copy_from_slice(bytes);
                            let val = u64::from_be_bytes(num_bytes);
                            if (1_000_000..=10_000_000_000).contains(&val) {
                                amounts.push(val);
                            }
                        }
                    }
                }
            }
            pallas_primitives::alonzo::PlutusData::Array(arr) => {
                for item in arr.iter() {
                    amounts.extend(Self::find_lovelace_amounts(item));
                }
            }
            pallas_primitives::alonzo::PlutusData::Map(map) => {
                for (_, v) in map.iter() {
                    amounts.extend(Self::find_lovelace_amounts(v));
                }
            }
            pallas_primitives::alonzo::PlutusData::Constr(constr) => {
                // Search constructor arguments
                for field in constr.fields.iter() {
                    amounts.extend(Self::find_lovelace_amounts(field));
                }
            }
            _ => {}
        }

        amounts
    }

    /// Recursively search PlutusData structure for ALL potential monetary amounts
    /// Includes smaller amounts that could represent fees, royalties, or other costs
    fn find_all_monetary_amounts(value: &pallas_primitives::alonzo::PlutusData) -> Vec<u64> {
        let mut amounts = Vec::new();
        match value {
            pallas_primitives::alonzo::PlutusData::BigInt(big_int) => {
                match big_int {
                    pallas_primitives::alonzo::BigInt::Int(int) => {
                        let val: i128 = (*int).into();
                        // Look for positive integers in extended range to catch fees
                        if (100_000..=10_000_000_000).contains(&val) {
                            // >= 0.1 ADA to 10,000 ADA (catch smaller fees)
                            amounts.push(val as u64);
                        }
                    }
                    pallas_primitives::alonzo::BigInt::BigUInt(bytes)
                    | pallas_primitives::alonzo::BigInt::BigNInt(bytes) => {
                        // Handle big integers - simplified conversion
                        if bytes.len() <= 8 {
                            let mut num_bytes = [0u8; 8];
                            num_bytes[8 - bytes.len()..].copy_from_slice(bytes);
                            let val = u64::from_be_bytes(num_bytes);
                            if (100_000..=10_000_000_000).contains(&val) {
                                amounts.push(val);
                            }
                        }
                    }
                }
            }
            pallas_primitives::alonzo::PlutusData::Array(arr) => {
                for item in arr.iter() {
                    amounts.extend(Self::find_all_monetary_amounts(item));
                }
            }
            pallas_primitives::alonzo::PlutusData::Map(map) => {
                for (_, v) in map.iter() {
                    amounts.extend(Self::find_all_monetary_amounts(v));
                }
            }
            pallas_primitives::alonzo::PlutusData::Constr(constr) => {
                // Search constructor arguments
                for field in constr.fields.iter() {
                    amounts.extend(Self::find_all_monetary_amounts(field));
                }
            }
            _ => {}
        }
        amounts
    }

    /// Extract potential policy IDs from CBOR structure
    /// Policy IDs are 56-character hex strings (28 bytes)
    pub fn extract_policy_ids(&self) -> Vec<String> {
        if let Ok(cbor_value) = self.decode_cbor() {
            Self::find_policy_ids(&cbor_value)
        } else {
            Vec::new()
        }
    }

    /// Extract policy IDs and their associated asset names from CBOR structure
    /// Returns tuples of (policy_id, encoded_asset_name) where asset_name is None for collection offers
    pub fn extract_policy_assets(&self) -> Vec<(String, Option<String>)> {
        if let Ok(cbor_value) = self.decode_cbor() {
            Self::find_policy_assets(&cbor_value)
        } else {
            Vec::new()
        }
    }

    /// Recursively search PlutusData structure for potential policy IDs
    /// Looks for 56-character hex strings that could be policy IDs
    fn find_policy_ids(value: &pallas_primitives::alonzo::PlutusData) -> Vec<String> {
        let mut policy_ids = Vec::new();
        match value {
            pallas_primitives::alonzo::PlutusData::BoundedBytes(bytes)
                // Policy IDs are 28 bytes (56 hex characters)
                if bytes.len() == 28 =>
            {
                let hex_string = hex::encode(bytes.as_slice());
                if hex_string.len() == 56 && hex_string.chars().all(|c| c.is_ascii_hexdigit()) {
                    policy_ids.push(hex_string);
                }
            }
            pallas_primitives::alonzo::PlutusData::Array(arr) => {
                for item in arr.iter() {
                    policy_ids.extend(Self::find_policy_ids(item));
                }
            }
            pallas_primitives::alonzo::PlutusData::Map(map) => {
                for (_, v) in map.iter() {
                    policy_ids.extend(Self::find_policy_ids(v));
                }
            }
            pallas_primitives::alonzo::PlutusData::Constr(constr) => {
                // Recursively search constructor fields
                for field in constr.fields.iter() {
                    policy_ids.extend(Self::find_policy_ids(field));
                }
            }
            _ => {}
        }

        policy_ids
    }

    /// Recursively search PlutusData structure for policy IDs and their associated asset names
    /// Based on the actual CBOR pattern: Map -> PolicyID(key) -> Constructor -> Map -> AssetName(key)
    fn find_policy_assets(
        value: &pallas_primitives::alonzo::PlutusData,
    ) -> Vec<(String, Option<String>)> {
        let mut policy_assets = Vec::new();

        match value {
            pallas_primitives::alonzo::PlutusData::Map(map) => {
                for (key, val) in map.iter() {
                    // Check if the key is a policy ID (28 bytes = 56 hex chars)
                    if let pallas_primitives::alonzo::PlutusData::BoundedBytes(key_bytes) = key {
                        if key_bytes.len() == 28 {
                            let policy_id = hex::encode(key_bytes.as_slice());
                            // Extract asset names from the constructor structure
                            let asset_names = Self::extract_asset_names_from_constructor(val);

                            if asset_names.is_empty() {
                                // Collection offer - no specific asset
                                policy_assets.push((policy_id, None));
                            } else {
                                // Specific asset offers
                                for asset_name in asset_names {
                                    policy_assets.push((policy_id.clone(), Some(asset_name)));
                                }
                            }
                        }
                    }

                    // Continue recursively searching the value
                    policy_assets.extend(Self::find_policy_assets(val));
                }
            }
            pallas_primitives::alonzo::PlutusData::Array(arr) => {
                for item in arr.iter() {
                    policy_assets.extend(Self::find_policy_assets(item));
                }
            }
            pallas_primitives::alonzo::PlutusData::Constr(constr) => {
                // Search constructor fields
                for field in constr.fields.iter() {
                    policy_assets.extend(Self::find_policy_assets(field));
                }
            }
            _ => {}
        }

        policy_assets
    }

    /// Extract asset names from Plutus constructor structure
    /// Expects: Constructor -> Fields[Map] where Map contains AssetName(key) -> Quantity(value)
    fn extract_asset_names_from_constructor(
        value: &pallas_primitives::alonzo::PlutusData,
    ) -> Vec<String> {
        let mut asset_names = Vec::new();

        match value {
            // Plutus constructor with fields
            pallas_primitives::alonzo::PlutusData::Constr(constr) => {
                // Look inside the constructor fields
                for field in constr.fields.iter() {
                    asset_names.extend(Self::extract_asset_names_from_constructor(field));
                }
            }
            // Array of fields
            pallas_primitives::alonzo::PlutusData::Array(arr) => {
                for item in arr.iter() {
                    asset_names.extend(Self::extract_asset_names_from_constructor(item));
                }
            }
            pallas_primitives::alonzo::PlutusData::Map(map) => {
                // Found the asset map - extract asset names as keys
                for (key, _val) in map.iter() {
                    if let pallas_primitives::alonzo::PlutusData::BoundedBytes(key_bytes) = key {
                        // Asset names are variable length byte arrays
                        let hex_string = hex::encode(key_bytes.as_slice());
                        asset_names.push(hex_string);
                    }
                }
            }
            _ => {}
        }

        asset_names
    }
}

/// Transaction input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub address: String,
    pub tx_hash: String,
    pub output_index: u32,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub amount_lovelace: u64,
    #[serde(with = "wasm_safe_serde::asset_map")]
    pub assets: HashMap<String, u64>, // asset_id -> quantity
    pub datum: Option<TxDatum>,
}

/// Transaction output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub address: String,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub amount_lovelace: u64,
    #[serde(with = "wasm_safe_serde::asset_map")]
    pub assets: HashMap<String, u64>, // asset_id -> quantity
    pub datum: Option<TxDatum>,
    pub script_ref: Option<String>,
}

/// Mint operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintOperation {
    pub unit: String, // policy_id + asset_name
    #[serde(with = "wasm_safe_serde::i64")]
    pub amount: i64, // Can be negative for burns
}

impl MintOperation {
    /// Extract the policy ID from the unit (first 56 characters)
    pub fn policy_id(&self) -> String {
        if self.unit.len() >= 56 {
            self.unit[..56].to_string()
        } else {
            self.unit.clone()
        }
    }

    /// Extract the asset name from the unit (characters after position 56)
    pub fn asset_name(&self) -> String {
        if self.unit.len() > 56 {
            self.unit[56..].to_string()
        } else {
            String::new()
        }
    }

    /// Check if this is a mint operation (positive amount)
    pub fn is_mint(&self) -> bool {
        self.amount > 0
    }

    /// Check if this is a burn operation (negative amount)
    pub fn is_burn(&self) -> bool {
        self.amount < 0
    }
}
