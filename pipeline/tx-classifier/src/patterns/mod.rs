//! Transaction pattern detection modules
//!
//! This module contains specialized pattern detectors for different types of Cardano transactions.
//! Each sub-module focuses on a specific transaction type for better maintainability.

pub mod dex;
pub mod extractors;
pub mod listings;
pub mod marketplace_classification;
pub mod marketplace_pricing;
pub mod mints;
pub mod offers;
pub mod sales;
pub mod transfers;
pub mod utils;

// Re-export main types and functions for backward compatibility
pub use dex::*;
pub use extractors::*;
pub use listings::*;
pub use marketplace_classification::*;
pub use marketplace_pricing::*;
pub use mints::*;
pub use offers::*;
pub use sales::*;
pub use transfers::*;
pub use utils::*;

use crate::TxType;
use crate::*;
use once_cell::sync::Lazy;
use serde_json::Value;

/// Main pattern detection result containing all detected transaction types
#[derive(Debug)]
pub struct PatternDetectionResult {
    pub transactions: Vec<(TxType, f64)>,
}

/// Individual transaction pattern with detection function
#[derive(Debug)]
pub struct TransactionPattern {
    pub name: String,
    pub description: String,
    pub detect_fn: fn(&PatternContext) -> PatternDetectionResult,
    pub confidence_threshold: f64,
}

/// Context for pattern detection containing transaction data and metadata
/// Contains ALL operations with classification preserved - patterns should filter based on classification
#[derive(Debug)]
pub struct PatternContext<'a> {
    /// ALL asset operations with classification preserved for proper datum indexing
    /// Patterns should filter to genuine operations using the classification field
    pub asset_operations: &'a [AssetOperation],
    pub metadata: &'a Option<Value>,
    pub scripts: &'a [String],
    pub addresses: Vec<String>,
    pub raw_tx_data: &'a RawTxData,
}

/// Registry of all transaction patterns
static PATTERNS: Lazy<Vec<TransactionPattern>> = Lazy::new(|| {
    vec![
        // Mint patterns
        TransactionPattern {
            name: "utxo_mint_rule".to_string(),
            description: "UTXO-based mint detection: assets in outputs but not inputs + ADA cost calculation (PRIMARY MINT DETECTOR)".to_string(),
            detect_fn: detect_utxo_mint_wrapper,
            confidence_threshold: 0.7,
        },
        // Offer patterns - NEW ARCHITECTURE
        TransactionPattern {
            name: "asset_based_create_offer_rule".to_string(),
            description:
                "Create multiple offers on JPG.store using asset operations with enrichment"
                    .to_string(),
            detect_fn: detect_create_offer_with_enrichment_wrapper,
            confidence_threshold: 0.7,
        },
        TransactionPattern {
            name: "offer_update_rule".to_string(),
            description: "Detect offer updates based on asset operations".to_string(),
            detect_fn: detect_offer_update_wrapper,
            confidence_threshold: 0.8,
        },
        TransactionPattern {
            name: "offer_cancel_rule".to_string(),
            description: "Detect offer cancellations based on asset operations".to_string(),
            detect_fn: detect_offer_cancel_wrapper,
            confidence_threshold: 0.8,
        },
        TransactionPattern {
            name: "offer_accepts_rule".to_string(),
            description: "Detect offer acceptances based on asset operations".to_string(),
            detect_fn: detect_offer_accepts_wrapper,
            confidence_threshold: 0.9,
        },
        // Sales patterns
        TransactionPattern {
            name: "sale_rule".to_string(),
            description: "Detect sales based on asset and ADA flows".to_string(),
            detect_fn: detect_sales_wrapper,
            confidence_threshold: 0.8,
        },
        // Listing patterns
        TransactionPattern {
            name: "listing_create_rule".to_string(),
            description: "Detect listing creation based on asset operations".to_string(),
            detect_fn: detect_listing_create_wrapper,
            confidence_threshold: 0.8,
        },
        TransactionPattern {
            name: "listing_update_rule".to_string(),
            description: "Detect listing updates based on asset operations".to_string(),
            detect_fn: detect_listing_update_wrapper,
            confidence_threshold: 0.8,
        },
        TransactionPattern {
            name: "unlisting_rule".to_string(),
            description: "Detect unlisting based on asset operations".to_string(),
            detect_fn: detect_unlisting_wrapper,
            confidence_threshold: 0.8,
        },
        // Transfer patterns
        TransactionPattern {
            name: "transfer_rule".to_string(),
            description: "Detect asset transfers, burns, staking, and smart contract interactions".to_string(),
            detect_fn: detect_transfers_wrapper,
            confidence_threshold: 0.5,
        },
        // DEX patterns
        TransactionPattern {
            name: "dex_swap_rule".to_string(),
            description: "Detect DEX swap transactions (asset trading)".to_string(),
            detect_fn: detect_dex_swap_wrapper,
            confidence_threshold: 0.8,
        },
        TransactionPattern {
            name: "dex_liquidity_add_rule".to_string(),
            description: "Detect DEX liquidity provision transactions".to_string(),
            detect_fn: detect_dex_liquidity_add_wrapper,
            confidence_threshold: 0.7,
        },
        TransactionPattern {
            name: "dex_liquidity_remove_rule".to_string(),
            description: "Detect DEX liquidity removal transactions".to_string(),
            detect_fn: detect_dex_liquidity_remove_wrapper,
            confidence_threshold: 0.7,
        },
    ]
});

/// Main pattern detection function - analyzes transaction context and returns detected patterns
pub fn detect_patterns(context: &PatternContext) -> PatternDetectionResult {
    let mut all_transactions = Vec::new();

    for pattern in PATTERNS.iter() {
        let result = (pattern.detect_fn)(context);
        for (tx_type, confidence) in result.transactions {
            if confidence >= pattern.confidence_threshold {
                all_transactions.push((tx_type, confidence));
            }
        }
    }

    // Post-process: collapse CreateOffer + OfferCancel pairs into OfferUpdate
    // when the same bidder creates and cancels offers for the same policy in one tx
    offers::collapse_offer_deltas(&mut all_transactions);

    PatternDetectionResult {
        transactions: all_transactions,
    }
}
