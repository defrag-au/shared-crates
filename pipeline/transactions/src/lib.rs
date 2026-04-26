//! Core transaction data structures for Cardano pipeline processing
//!
//! This crate provides the fundamental data structures used throughout the
//! Cardano transaction processing pipeline, including transaction inputs,
//! outputs, mint operations, and metadata structures.

use serde::{Deserialize, Serialize};

mod asset;
mod screening;
pub mod types;

pub use asset::*;
pub use screening::*;
pub use types::*;

/// Raw transaction data structure containing all transaction components
///
/// This is the unified transaction format used throughout the pipeline,
/// regardless of the original data source (Blockfrost, Maestro, CBOR, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTxData {
    pub tx_hash: String,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub collateral_inputs: Vec<TxInput>,
    pub collateral_outputs: Vec<TxOutput>,
    #[serde(default)]
    pub reference_inputs: Vec<TxInput>, // Reference inputs (read-only, not consumed)
    #[serde(default)]
    pub mint: Vec<MintOperation>, // Structured mint/burn data
    pub metadata: Option<serde_json::Value>,
    #[serde(with = "wasm_safe_serde::u64_option")]
    pub fee: Option<u64>,
    #[serde(with = "wasm_safe_serde::u64_option")]
    pub block_height: Option<u64>,
    #[serde(with = "wasm_safe_serde::u64_option")]
    pub timestamp: Option<u64>,
    pub size: Option<u32>,
    pub scripts: Vec<String>,
    #[serde(default)]
    pub redeemers: Option<serde_json::Value>, // Raw redeemer data from Maestro
}

impl RawTxData {
    /// Create a new empty RawTxData with the given transaction hash
    pub fn new(tx_hash: String) -> Self {
        Self {
            tx_hash,
            inputs: Vec::new(),
            outputs: Vec::new(),
            collateral_inputs: Vec::new(),
            collateral_outputs: Vec::new(),
            reference_inputs: Vec::new(),
            mint: Vec::new(),
            metadata: None,
            fee: None,
            block_height: None,
            timestamp: None,
            size: None,
            scripts: Vec::new(),
            redeemers: None,
        }
    }

    /// Check if this transaction has any mint operations
    pub fn has_mints(&self) -> bool {
        !self.mint.is_empty()
    }

    /// Get all minted assets (positive amounts)
    pub fn get_minted_assets(&self) -> Vec<&MintOperation> {
        self.mint.iter().filter(|op| op.amount > 0).collect()
    }

    /// Get all burned assets (negative amounts)
    pub fn get_burned_assets(&self) -> Vec<&MintOperation> {
        self.mint.iter().filter(|op| op.amount < 0).collect()
    }

    /// Check if this transaction contains any scripts
    pub fn has_scripts(&self) -> bool {
        !self.scripts.is_empty()
    }

    /// Get the total number of inputs
    pub fn input_count(&self) -> usize {
        self.inputs.len()
    }

    /// Get the total number of outputs
    pub fn output_count(&self) -> usize {
        self.outputs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_js_safe_serialization() {
        // Test with a large fee that exceeds JavaScript's safe integer limit
        let large_fee = 12738606488933375_u64; // The problematic number from the error
        let raw_tx = RawTxData {
            tx_hash: "test".to_string(),
            reference_inputs: vec![],
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "test_address".to_string(),
                amount_lovelace: large_fee,
                assets: std::collections::HashMap::new(),
                datum: None,
                script_ref: None,
            }],
            collateral_inputs: vec![],
            collateral_outputs: vec![],
            mint: vec![],
            metadata: None,
            fee: Some(large_fee),
            block_height: Some(large_fee),
            timestamp: Some(large_fee),
            size: None,
            scripts: vec![],
            redeemers: None,
        };

        // Serialize to JSON
        let json = serde_json::to_string(&raw_tx).expect("Should serialize successfully");

        // The large numbers should be serialized as strings
        assert!(
            json.contains("\"12738606488933375\""),
            "Large numbers should be serialized as strings"
        );

        // Verify we can deserialize back
        let deserialized: RawTxData =
            serde_json::from_str(&json).expect("Should deserialize successfully");
        assert_eq!(deserialized.fee, Some(large_fee));
        assert_eq!(deserialized.outputs[0].amount_lovelace, large_fee);

        println!("Serialized JSON: {json}");
    }

    #[test]
    fn test_small_numbers_remain_numeric() {
        // Test with a small number that should remain numeric
        let small_amount = 1_000_000_u64; // 1 ADA
        let raw_tx = RawTxData {
            tx_hash: "test".to_string(),
            inputs: vec![],
            reference_inputs: vec![],
            outputs: vec![TxOutput {
                address: "test_address".to_string(),
                amount_lovelace: small_amount,
                assets: std::collections::HashMap::new(),
                datum: None,
                script_ref: None,
            }],
            collateral_inputs: vec![],
            collateral_outputs: vec![],
            mint: vec![],
            metadata: None,
            fee: Some(small_amount),
            block_height: Some(small_amount),
            timestamp: Some(small_amount),
            size: None,
            scripts: vec![],
            redeemers: None,
        };

        // Serialize to JSON
        let json = serde_json::to_string(&raw_tx).expect("Should serialize successfully");

        // Small numbers should remain as numbers, not strings
        assert!(
            json.contains("1000000"),
            "Small numbers should remain numeric"
        );
        assert!(
            !json.contains("\"1000000\""),
            "Small numbers should not be stringified"
        );

        println!("Serialized JSON: {json}");
    }
}
