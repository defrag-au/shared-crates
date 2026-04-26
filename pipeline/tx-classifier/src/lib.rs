#![allow(ambiguous_glob_reexports)]

pub use pipeline_types::{AssetId, OperationPayload, PricedAsset};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;
#[cfg(feature = "indexers")]
use tracing::info;

// Re-export types from pipeline crates
pub use address_registry::*;
pub use transactions::{MintOperation, RawTxData, TxDatum, TxInput, TxOutput};
#[cfg(feature = "indexers")]
pub mod indexers;
mod insights;
pub mod mints;
pub mod patterns;
pub mod rules;
pub mod summary;
pub mod txdata_ext;
pub mod utxo_analysis;

#[cfg(all(test, feature = "indexers"))]
mod tests;

#[cfg(feature = "indexers")]
pub use indexers::*;
pub use patterns::*;
pub use rules::*;
pub use summary::*;

pub use txdata_ext::*;

/// Returns true if the given Shelley-style address has a *script* payment credential.
pub fn is_script_address(addr: &str) -> bool {
    try_is_script_address(addr).unwrap_or_default()
}

fn try_is_script_address(addr: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let address = pallas_addresses::Address::from_bech32(addr)
        .map_err(|e| format!("Failed to parse address: {e}"))?;
    let bytes = address.to_vec();
    let header = bytes.first().ok_or("invalid address: payload empty")?;
    let addr_type = header >> 4;
    // Script-payment types per CIP-19: 1 = base script, 3 = base script+script,
    //    5 = pointer script, 7 = enterprise script
    Ok(matches!(addr_type, 1 | 3 | 5 | 7))
}
pub use utxo_analysis::*;

// OperationPayload is already available via the use statement above

/// Main error type for transaction classification
#[derive(Error, Debug)]
pub enum TxClassifierError {
    #[cfg(feature = "indexers")]
    #[error("Maestro API error: {0}")]
    Maestro(#[from] ::maestro::MaestroError),

    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("Invalid transaction hash: {0}")]
    InvalidTxHash(String),

    #[error("Classification failed: {0}")]
    ClassificationFailed(String),

    #[cfg(feature = "indexers")]
    #[error("Worker error: {0}")]
    Worker(#[from] worker::Error),
}

/// Confidence level for transaction classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Confidence {
    /// 95%+ confidence - definitive classification
    High,
    /// 70-95% confidence - strong indicators
    Medium,
    /// 40-70% confidence - some indicators
    Low,
    /// <40% confidence - uncertain classification
    Uncertain,
}

impl Confidence {
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s >= 0.95 => Self::High,
            s if s >= 0.70 => Self::Medium,
            s if s >= 0.40 => Self::Low,
            _ => Self::Uncertain,
        }
    }

    pub fn to_score(&self) -> f64 {
        match self {
            Self::High => 0.95,
            Self::Medium => 0.80,
            Self::Low => 0.55,
            Self::Uncertain => 0.25,
        }
    }
}

/// Transaction classification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxClassification {
    /// Transaction hash
    pub tx_hash: String,

    /// Transaction types that occurred in this transaction (unranked)
    pub tx_types: Vec<TxType>,

    /// Classification tags that provide additional context
    pub tags: Vec<TxTag>,

    /// Confidence level of classification
    pub confidence: Confidence,

    /// Classification score (0.0 - 1.0)
    pub score: f64,

    /// Additional context and metadata
    pub context: TxContext,

    #[serde(serialize_with = "TxClassification::serialize_filtered_assets")]
    pub assets: Vec<AssetOperation>,

    /// ADA amounts involved
    pub ada_amounts: AdaFlows,

    /// Payment distributions extracted from marketplace datums
    pub distributions: Vec<Distribution>,
}

impl TxClassification {
    fn serialize_filtered_assets<S>(
        assets: &[AssetOperation],
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let _filtered: Vec<&AssetOperation> = assets
            .iter()
            .filter(|op| !matches!(op.op_type, AssetOpType::Lock | AssetOpType::Unlock))
            .collect();

        assets.serialize(serializer)
    }
}

/// Type of payment distribution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistributionType {
    /// Royalty payment to creator/collection
    Royalty,
    /// Payment to seller
    Payment,
    /// Marketplace fee
    MarketplaceFee,
    /// Payment to project treasury from mint operations
    ProjectTreasury,
}

/// Payment distribution from marketplace transactions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Distribution {
    /// Recipient address
    pub to: String,
    /// Amount in lovelace
    pub amount_lovelace: u64,
    /// Assets covered by this distribution (policy_id.asset_name format)
    /// None if this is a general fee not tied to specific assets
    pub assets: Option<Vec<String>>,
    /// Type of distribution
    pub distribution_type: DistributionType,
}

/// Classification tags that provide additional context beyond primary transaction types
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TxTag {
    /// Sweep transaction - multiple purchases of similar assets by same buyer
    Sweep {
        /// Number of assets purchased in the sweep
        asset_count: u32,
        /// Total value of the sweep
        total_lovelace: u64,
        /// Average price per asset
        avg_ppa_lovelace: u64,
    },

    /// Bundle sale - multiple assets from same seller in single transaction
    Bundle {
        /// Number of assets in the bundle
        asset_count: u32,
        /// Total value of the bundle
        total_lovelace: u64,
        /// Price per asset in the bundle
        per_asset_lovelace: u64,
    },

    /// Bundle discount applied (common in marketplaces)
    BundleDiscount {
        /// Percentage discount (e.g., 5 for 5%)
        discount_percent: u8,
        /// Original total before discount
        original_lovelace: u64,
        /// Final total after discount
        discounted_lovelace: u64,
    },

    /// High-value transaction (above certain threshold)
    HighValue {
        /// Total ADA value of the transaction
        total_lovelace: u64,
    },

    /// Contract interaction detected
    SmartContractInteraction {
        /// Contract purpose if known
        contract_purpose: Option<String>,
        /// Contract address
        contract_address: String,
    },
}

impl std::fmt::Display for TxTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sweep {
                asset_count,
                total_lovelace,
                ..
            } => {
                let ada_value = *total_lovelace as f64 / 1_000_000.0;
                write!(f, "Sweep ({asset_count} assets, ₳{ada_value:.2})")
            }
            Self::Bundle {
                asset_count,
                total_lovelace,
                per_asset_lovelace,
                ..
            } => {
                let bundle_ada = *total_lovelace as f64 / 1_000_000.0;
                let per_asset_ada = *per_asset_lovelace as f64 / 1_000_000.0;
                write!(f, "Bundle ({asset_count} assets, ₳{bundle_ada:.2} total, ₳{per_asset_ada:.2} each)")
            }
            Self::BundleDiscount {
                discount_percent,
                discounted_lovelace,
                ..
            } => {
                let ada_value = *discounted_lovelace as f64 / 1_000_000.0;
                write!(
                    f,
                    "Bundle Discount ({discount_percent}% off, ₳{ada_value:.2})"
                )
            }
            Self::HighValue { total_lovelace } => {
                let ada_value = *total_lovelace as f64 / 1_000_000.0;
                write!(f, "High Value (₳{ada_value:.2})")
            }
            Self::SmartContractInteraction {
                contract_purpose, ..
            } => {
                if let Some(purpose) = contract_purpose {
                    write!(f, "Smart Contract ({purpose})")
                } else {
                    write!(f, "Smart Contract")
                }
            }
        }
    }
}

/// Trait for type-based transaction type matching
pub trait TxTypeMatcher {
    fn matches(tx_type: &TxType) -> bool;
}

/// Type marker for Sale transactions
pub struct Sale;
impl TxTypeMatcher for Sale {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::Sale { .. })
    }
}

/// Type marker for OfferAccept transactions
pub struct OfferAccept;
impl TxTypeMatcher for OfferAccept {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::OfferAccept { .. })
    }
}

/// Type marker for Mint transactions
pub struct Mint;
impl TxTypeMatcher for Mint {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::Mint { .. })
    }
}

/// Type marker for Burn transactions
pub struct Burn;
impl TxTypeMatcher for Burn {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::Burn { .. })
    }
}

/// Type marker for Transfer transactions
pub struct Transfer;
impl TxTypeMatcher for Transfer {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::Transfer { .. })
    }
}

/// Type marker for Offer transactions
pub struct Offer;
impl TxTypeMatcher for Offer {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::Offer { .. })
    }
}

/// Type marker for SmartContract transactions
pub struct SmartContract;
impl TxTypeMatcher for SmartContract {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::SmartContract { .. })
    }
}

/// Type marker for MarketplaceBundle transactions
pub struct MarketplaceBundle;
impl TxTypeMatcher for MarketplaceBundle {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::MarketplaceBundle { .. })
    }
}

/// Type marker for CreateOffer transactions
pub struct CreateOffer;
impl TxTypeMatcher for CreateOffer {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::CreateOffer { .. })
    }
}

/// Type marker for OfferUpdate transactions
pub struct OfferUpdate;
impl TxTypeMatcher for OfferUpdate {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::OfferUpdate { .. })
    }
}

/// Type marker for OfferCancel transactions
pub struct OfferCancel;
impl TxTypeMatcher for OfferCancel {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::OfferCancel { .. })
    }
}

/// Type marker for AssetTransfer transactions
pub struct AssetTransfer;
impl TxTypeMatcher for AssetTransfer {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::AssetTransfer { .. })
    }
}

/// Type marker for AssetStaking transactions
pub struct AssetStaking;
impl TxTypeMatcher for AssetStaking {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::AssetStaking { .. })
    }
}

/// Type marker for AssetVesting transactions
pub struct AssetVesting;
impl TxTypeMatcher for AssetVesting {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::AssetVesting { .. })
    }
}

/// Type marker for ListingCreate transactions
pub struct ListingCreate;
impl TxTypeMatcher for ListingCreate {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::ListingCreate { .. })
    }
}

/// Type marker for ListingUpdate transactions
pub struct ListingUpdate;
impl TxTypeMatcher for ListingUpdate {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::ListingUpdate { .. })
    }
}

/// Type marker for DexSwap transactions
pub struct DexSwap;
impl TxTypeMatcher for DexSwap {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::DexSwap { .. })
    }
}

/// Type marker for DexLiquidityAdd transactions
pub struct DexLiquidityAdd;
impl TxTypeMatcher for DexLiquidityAdd {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::DexLiquidityAdd { .. })
    }
}

/// Type marker for DexLiquidityRemove transactions
pub struct DexLiquidityRemove;
impl TxTypeMatcher for DexLiquidityRemove {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::DexLiquidityRemove { .. })
    }
}

/// Type marker for Unknown transactions
pub struct Unknown;
impl TxTypeMatcher for Unknown {
    fn matches(tx_type: &TxType) -> bool {
        matches!(tx_type, TxType::Unknown)
    }
}

impl TxClassification {
    /// Get all transaction types that occurred
    pub fn types(&self) -> &[TxType] {
        &self.tx_types
    }

    /// Check if transaction has a specific type (by discriminant)
    pub fn has_type(&self, tx_type: &TxType) -> bool {
        self.tx_types
            .iter()
            .any(|t| std::mem::discriminant(t) == std::mem::discriminant(tx_type))
    }

    /// Check if transaction has any of a specific type variant
    ///
    /// # Examples
    /// ```
    /// use tx_classifier::{TxClassification, Sale, Mint};
    ///
    /// // This method is typically used on classification results
    /// // from the rule engine to check for specific transaction types
    /// # fn example(classification: &TxClassification) {
    /// // Check for any sales
    /// let has_sales = classification.has_any::<Sale>();
    ///
    /// // Check for any mints
    /// let has_mints = classification.has_any::<Mint>();
    /// # }
    /// ```
    pub fn has_any<T: TxTypeMatcher>(&self) -> bool {
        self.tx_types.iter().any(|tx_type| T::matches(tx_type))
    }

    /// Get all transaction types of a specific variant
    ///
    /// # Examples
    /// ```
    /// use tx_classifier::{TxClassification, Sale, Mint};
    ///
    /// // This method is typically used on classification results
    /// // from the rule engine to filter specific transaction types
    /// # fn example(classification: &TxClassification) {
    /// // Get all sales
    /// let sales = classification.get_by::<Sale>();
    ///
    /// // Get all mints
    /// let mints = classification.get_by::<Mint>();
    /// # }
    /// ```
    pub fn get_by<T: TxTypeMatcher>(&self) -> Vec<&TxType> {
        self.tx_types
            .iter()
            .filter(|tx_type| T::matches(tx_type))
            .collect()
    }

    /// Get total sale volume from all sales in the transaction
    pub fn total_sale_volume(&self) -> u64 {
        self.get_by::<Sale>()
            .iter()
            .filter_map(|tx_type| {
                if let TxType::Sale { breakdown, .. } = tx_type {
                    Some(breakdown.total_lovelace)
                } else {
                    None
                }
            })
            .sum()
    }

    /// Check if the transaction is empty (no types detected)
    pub fn is_empty(&self) -> bool {
        self.tx_types.is_empty()
    }

    /// Extract payment distributions from marketplace datums
    ///
    /// Returns a map of marketplace address -> payment distributions
    /// This is useful for analyzing royalty splits, seller payments, and marketplace fees.
    pub fn extract_marketplace_payment_distributions(
        &self,
    ) -> std::collections::HashMap<String, Vec<(String, u64)>> {
        let mut distributions = std::collections::HashMap::new();

        for asset_op in &self.assets {
            if let crate::AssetOperation {
                input: Some(input_utxo),
                input_datum: Some(datum),
                ..
            } = asset_op
            {
                // Check if this looks like a marketplace address
                if crate::registry::MarketplacePurpose::from_address(&input_utxo.address).is_some()
                {
                    // Use schema-driven payment extraction
                    if let Some(payments) =
                        crate::patterns::sales::try_schema_driven_payment_extraction(
                            datum,
                            &input_utxo.address,
                        )
                    {
                        distributions.insert(input_utxo.address.clone(), payments);
                    }
                }
            }
        }

        distributions
    }

    /// Get the total royalty amounts across all marketplace transactions
    ///
    /// Returns (total_royalties, total_seller_payments, royalty_percentage)
    pub fn calculate_royalty_breakdown(&self) -> Option<(u64, u64, f64)> {
        let distributions = self.extract_marketplace_payment_distributions();
        if distributions.is_empty() {
            return None;
        }

        let mut total_royalties = 0u64;
        let mut total_seller_payments = 0u64;

        for payments in distributions.values() {
            if payments.len() >= 2 {
                // Sort by amount to identify royalties (smaller) vs seller payment (larger)
                let mut sorted_payments = payments.clone();
                sorted_payments.sort_by_key(|(_, amount)| *amount);

                // Smallest payment is typically royalties
                total_royalties += sorted_payments[0].1;
                // Largest payment is typically seller
                total_seller_payments += sorted_payments[sorted_payments.len() - 1].1;
            }
        }

        let total_sale_value = total_royalties + total_seller_payments;
        if total_sale_value > 0 {
            let royalty_percentage = (total_royalties as f64 / total_sale_value as f64) * 100.0;
            Some((total_royalties, total_seller_payments, royalty_percentage))
        } else {
            None
        }
    }

    /// Get the actual seller addresses from marketplace transactions
    ///
    /// This analyzes payment flows to identify human sellers rather than marketplace contracts
    pub fn extract_actual_sellers(&self) -> Vec<String> {
        let distributions = self.extract_marketplace_payment_distributions();
        let mut sellers = Vec::new();

        for payments in distributions.values() {
            if let Some((seller_address, _)) = payments.iter().max_by_key(|(_, amount)| *amount) {
                if !sellers.contains(seller_address) {
                    sellers.push(seller_address.clone());
                }
            }
        }

        sellers
    }

    /// Check if the transaction is unknown/unclassified
    pub fn is_unknown(&self) -> bool {
        self.tx_types.len() == 1 && matches!(self.tx_types[0], TxType::Unknown)
    }

    /// Get a human-readable summary of what happened in this transaction
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "Empty transaction".to_string();
        }

        if self.is_unknown() {
            return "Unknown transaction type".to_string();
        }

        let type_names: Vec<String> = self
            .tx_types
            .iter()
            .map(|t| match t {
                TxType::Sale { .. } => "Sale".to_string(),
                TxType::OfferAccept { .. } => "Offer Accept".to_string(),
                TxType::Mint { .. } => "Mint".to_string(),
                TxType::Burn { .. } => "Burn".to_string(),
                TxType::Transfer { .. } => "Transfer".to_string(),
                TxType::AssetTransfer { .. } => "Asset Transfer".to_string(),
                TxType::Offer { .. } => "Offer".to_string(),
                TxType::CreateOffer { .. } => "Create Offer".to_string(),
                TxType::OfferUpdate { .. } => "Offer Update".to_string(),
                TxType::OfferCancel { .. } => "Offer Cancel".to_string(),
                TxType::SmartContract { .. } => "Smart Contract".to_string(),
                TxType::MarketplaceBundle { .. } => "Marketplace Bundle".to_string(),
                TxType::AssetStaking { .. } => "Asset Staking".to_string(),
                TxType::AssetVesting { .. } => "Asset Vesting".to_string(),
                TxType::ListingCreate { .. } => "Create Listing".to_string(),
                TxType::ListingUpdate { .. } => "Update Listing".to_string(),
                TxType::Unlisting { .. } => "Unlisting".to_string(),
                TxType::DexSwap { .. } => "DEX Swap".to_string(),
                TxType::DexLiquidityAdd { .. } => "DEX Liquidity Add".to_string(),
                TxType::DexLiquidityRemove { .. } => "DEX Liquidity Remove".to_string(),
                TxType::Unknown => "Unknown".to_string(),
            })
            .collect();

        let types_summary = match type_names.len() {
            1 => type_names[0].clone(),
            2 => format!("{} and {}", type_names[0], type_names[1]),
            _ => {
                let last = type_names.last().unwrap();
                let others = &type_names[..type_names.len() - 1];
                format!("{}, and {}", others.join(", "), last)
            }
        };

        // Add tags if present
        if !self.tags.is_empty() {
            let tag_names: Vec<String> = self.tags.iter().map(|t| t.to_string()).collect();
            format!("{} [{}]", types_summary, tag_names.join(", "))
        } else {
            types_summary
        }
    }
}

/// Calculate net lovelace movement for an address in a transaction
/// Returns positive value if address is a net recipient, negative if net sender
pub fn get_net_cost(raw_tx_data: &RawTxData, address: &str) -> i64 {
    // Calculate total lovelace received by this address
    let total_received: u64 = raw_tx_data
        .outputs
        .iter()
        .filter(|output| output.address == address)
        .map(|output| output.amount_lovelace)
        .sum();

    // Calculate total lovelace spent by this address
    let total_spent: u64 = raw_tx_data
        .inputs
        .iter()
        .filter(|input| input.address == address)
        .map(|input| input.amount_lovelace)
        .sum();

    // Net cost = spent - received (positive means net cost, negative means net gain)
    total_spent as i64 - total_received as i64
}

/// Types of minting operations
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MintType {
    /// Standard CIP-25 mint (single token per NFT)
    Cip25,
    /// CIP-68 mint (reference + user token pair)
    Cip68,
    /// Fungible token mint
    Fungible,
    /// Unknown/other mint type
    Unknown,
}

/// Direction of asset staking operations
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StakeDirection {
    /// Assets going TO staking contract (being staked)
    Stake,
    /// Assets coming FROM staking contract (being unstaked)
    Unstake,
}

/// Style of vesting contract
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VestStyle {
    /// Individual vesting via Shield Vest
    Shield,
    /// Social/collective vesting via CrowdLock
    CrowdLock,
}

impl VestStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            VestStyle::Shield => "shield",
            VestStyle::CrowdLock => "crowdlock",
        }
    }
}

impl std::fmt::Display for VestStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Direction of asset vesting operations
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VestingDirection {
    /// Assets being locked into a vesting contract (time-locked)
    Lock,
    /// Assets being released from a vesting contract (after schedule)
    Unlock,
}

/// Simplified sale information focusing on the key public data
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SaleBreakdown {
    /// Total sale price - the amount paid for the NFT
    pub total_lovelace: u64,
}

impl std::fmt::Display for SaleBreakdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ada_amount = self.total_lovelace as f64 / 1_000_000.0;
        write!(f, "₳{ada_amount:.2}")
    }
}

/// Types of transactions that can be classified
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TxType {
    /// Asset minting transaction
    Mint {
        assets: Vec<AssetId>, // Primary minted assets (UserNfts for CIP-68)
        #[serde(skip_serializing_if = "Vec::is_empty")]
        reference_assets: Vec<AssetId>, // Reference NFTs for CIP-68 (not serialized if empty)
        total_lovelace: Option<u64>, // Total lovelace spent on minting (excluding tx fee)
        minter: String,       // Address receiving the primary assets
        mint_type: MintType,  // Type of mint operation
    },

    /// Asset burning transaction
    Burn { policy_id: String, asset_count: u32 },

    /// Asset sale transaction with complete fee breakdown
    Sale {
        asset: PricedAsset,
        breakdown: SaleBreakdown,
        seller: String,
        buyer: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        marketplace: Option<String>,
    },

    /// Offer acceptance - seller accepts buyer's previously made offer
    OfferAccept {
        asset: AssetId,
        offer_lovelace: u64,
        seller: String,
        buyer: String,
    },

    /// Complex marketplace operation (multiple actions in one TX)
    MarketplaceBundle {
        primary_operation: String, // "purchase", "listing", "unlisting", etc.
        operations: Vec<String>,   // All operations that occurred
        asset_count: u32,
        total_lovelace: Option<u64>,
    },

    /// Offer/bid transaction
    Offer {
        asset: Option<AssetId>, // None for collection offers
        offer_lovelace: u64,
        bidder: String,
    },

    /// Create multiple offers transaction
    CreateOffer {
        policy_id: String,                  // Target policy for the offers
        encoded_asset_name: Option<String>, // Specific asset within policy (None for collection offers)
        offer_count: u32,                   // Number of offers created
        offer_lovelace: u64,                // Lovelace amount per offer
        total_lovelace: u64,                // Total lovelace committed to offers
        bidder: String,                     // Address creating the offers
        marketplace: String,                // Marketplace contract address
    },

    /// Update existing offer transaction
    OfferUpdate {
        policy_id: String,                  // Target policy for the offer
        encoded_asset_name: Option<String>, // Specific asset within policy (None for collection offers)
        original_amount: u64,               // Original offer amount in lovelace
        updated_amount: u64,                // Updated offer amount in lovelace
        delta_amount: i64,                  // Change in offer amount (can be negative)
        bidder: String,                     // Address updating the offer
        marketplace: String,                // Marketplace contract address
        offer_index: u32,                   // Index to distinguish multiple offers in same TX
    },

    /// Cancel existing offers transaction
    OfferCancel {
        policy_id: String,                  // Target policy for the cancelled offers
        encoded_asset_name: Option<String>, // Specific asset within policy (None for collection offers)
        offer_count: u32,                   // Number of offers cancelled
        total_cancelled_lovelace: u64,      // Total lovelace released from cancelled offers
        bidder: String,                     // Address cancelling the offers
        marketplace: String,                // Marketplace contract address
    },

    /// Simple transfer
    Transfer { assets: Vec<AssetId> },

    /// Asset transfer between addresses (not involving script addresses)
    AssetTransfer {
        assets: Vec<AssetId>, // Assets being transferred
        sender: String,       // Address sending the assets
        receiver: String,     // Address receiving the assets
    },

    /// Smart contract interaction
    SmartContract {
        contract_address: String,
        operation: String,
    },

    /// Asset staking/unstaking transaction
    AssetStaking {
        assets: Vec<AssetId>,
        direction: StakeDirection,
        staker_address: String,
        staking_contract: String,
        staking_label: String,
    },

    /// Asset vesting (lock/unlock) transaction
    AssetVesting {
        assets: Vec<AssetId>,
        direction: VestingDirection,
        vest_style: VestStyle,
        owner_address: String,
        vesting_contract: String,
        vesting_label: String,
    },

    /// Create marketplace listing transaction
    ListingCreate {
        assets: Vec<PricedAsset>,
        total_listing_count: u32,
        seller: String,
        marketplace: String,
    },

    /// Update marketplace listing price transaction
    ListingUpdate {
        assets: Vec<PricedAsset>,
        total_listing_count: u32,
        seller: String,
        marketplace: String,
    },

    /// Remove marketplace listing transaction
    Unlisting {
        assets: Vec<AssetId>,
        total_unlisting_count: u32,
        seller: String,
        marketplace: String,
    },

    /// DEX swap transaction
    DexSwap {
        /// DEX platform performing the swap
        dex_platform: String,
        /// Asset being sold (from perspective of swapper)
        asset_in: OperationPayload,
        /// Asset being bought (from perspective of swapper)
        asset_out: OperationPayload,
        /// Address performing the swap
        swapper: String,
    },

    /// DEX liquidity provision transaction
    DexLiquidityAdd {
        /// DEX platform where liquidity is added
        dex_platform: String,
        /// First asset in the liquidity pair
        asset_a: AssetId,
        /// Amount of first asset provided
        amount_a: u64,
        /// Second asset in the liquidity pair
        asset_b: AssetId,
        /// Amount of second asset provided
        amount_b: u64,
        /// Address providing liquidity
        provider: String,
    },

    /// DEX liquidity removal transaction
    DexLiquidityRemove {
        /// DEX platform where liquidity is removed
        dex_platform: String,
        /// First asset in the liquidity pair
        asset_a: AssetId,
        /// Amount of first asset withdrawn
        amount_a: u64,
        /// Second asset in the liquidity pair
        asset_b: AssetId,
        /// Amount of second asset withdrawn
        amount_b: u64,
        /// Address removing liquidity
        provider: String,
    },

    /// Unknown/unclassified transaction
    Unknown,
}

impl std::fmt::Display for TxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sale {
                asset,
                breakdown,
                buyer,
                ..
            } => write!(f, "{asset} sold for {breakdown} to {buyer}"),
            Self::OfferAccept {
                asset,
                offer_lovelace,
                buyer,
                ..
            } => {
                let ada_amount = *offer_lovelace as f64 / 1_000_000.0;
                let name = asset.asset_name();
                write!(f, "{name} offer accepted for ₳{ada_amount:.2} to {buyer}")
            }
            Self::Mint {
                assets,
                total_lovelace,
                minter,
                mint_type,
                ..
            } => {
                // Now assets contains only the primary assets (UserNfts for CIP-68), so count directly
                let count_display = assets.len();
                let unit = if count_display == 1 {
                    "asset"
                } else {
                    "assets"
                };

                let mint_type_str = match mint_type {
                    MintType::Cip25 => "CIP-25",
                    MintType::Cip68 => "CIP-68",
                    MintType::Fungible => "fungible",
                    MintType::Unknown => "",
                };
                let type_part = if mint_type_str.is_empty() {
                    "".to_string()
                } else {
                    format!(" ({mint_type_str})")
                };

                match total_lovelace {
                    Some(cost) => {
                        let ada_amount = *cost as f64 / 1_000_000.0;
                        write!(
                            f,
                            "{count_display} {unit} minted{type_part} for ₳{ada_amount:.2} by {minter}"
                        )
                    }
                    None => write!(f, "{count_display} {unit} minted{type_part} by {minter}"),
                }
            }
            Self::CreateOffer {
                offer_count,
                offer_lovelace,
                policy_id,
                encoded_asset_name,
                bidder,
                ..
            } => {
                let ada_per_offer = *offer_lovelace as f64 / 1_000_000.0;
                let offer_unit = if *offer_count == 1 { "offer" } else { "offers" };
                let target = match encoded_asset_name {
                    Some(name) => format!("asset {policy_id}.{name}"),
                    None => format!("policy {policy_id}"),
                };
                write!(
                    f,
                    "{offer_count} {offer_unit} created (₳{ada_per_offer:.0} each) on {target} by {bidder}"
                )
            }
            Self::OfferUpdate {
                original_amount,
                updated_amount,
                delta_amount,
                policy_id,
                encoded_asset_name,
                bidder,
                ..
            } => {
                let original_ada = *original_amount as f64 / 1_000_000.0;
                let updated_ada = *updated_amount as f64 / 1_000_000.0;
                let delta_ada = *delta_amount as f64 / 1_000_000.0;
                let delta_sign = if *delta_amount >= 0 { "+" } else { "" }; // Negative already includes the sign

                let asset_part = match encoded_asset_name {
                    Some(name) => format!(" for asset {name}"),
                    None => String::new(), // Collection offer
                };

                write!(
                    f,
                    "Offer updated from ₳{original_ada:.0} to ₳{updated_ada:.0} ({delta_sign}₳{delta_ada:.0}) on policy {policy_id}{asset_part} by {bidder}"
                )
            }
            Self::OfferCancel {
                policy_id,
                encoded_asset_name,
                offer_count,
                bidder,
                ..
            } => {
                let offer_unit = if *offer_count == 1 { "offer" } else { "offers" };

                let asset_part = match encoded_asset_name {
                    Some(name) => format!(" for asset {name}"),
                    None => String::new(), // Collection offer
                };

                write!(
                    f,
                    "{offer_count} {offer_unit} cancelled on policy {policy_id}{asset_part} by {bidder}"
                )
            }
            Self::AssetTransfer {
                assets,
                sender,
                receiver,
                ..
            } => {
                let count = assets.len();
                let asset_unit = if count == 1 { "asset" } else { "assets" };
                write!(
                    f,
                    "{count} {asset_unit} transfer from {sender} to {receiver}"
                )
            }
            Self::Burn {
                policy_id,
                asset_count,
            } => {
                let asset_unit = if *asset_count == 1 { "asset" } else { "assets" };
                write!(
                    f,
                    "{asset_count} {asset_unit} burned from policy {policy_id}"
                )
            }
            Self::MarketplaceBundle {
                primary_operation,
                asset_count,
                total_lovelace,
                ..
            } => {
                let asset_unit = if *asset_count == 1 { "asset" } else { "assets" };
                match total_lovelace {
                    Some(total) => {
                        let ada_amount = *total as f64 / 1_000_000.0;
                        write!(f, "{primary_operation} bundle: {asset_count} {asset_unit} for ₳{ada_amount:.2}")
                    }
                    None => write!(f, "{primary_operation} bundle: {asset_count} {asset_unit}"),
                }
            }
            Self::Offer {
                asset,
                offer_lovelace,
                bidder,
            } => {
                let ada_amount = *offer_lovelace as f64 / 1_000_000.0;
                match asset {
                    Some(asset) => {
                        let name = asset.asset_name();
                        write!(f, "Offer of ₳{ada_amount:.2} for {name} by {bidder}")
                    }
                    None => write!(f, "Collection offer of ₳{ada_amount:.2} by {bidder}"),
                }
            }
            Self::Transfer { assets } => {
                let count = assets.len();
                let asset_unit = if count == 1 { "asset" } else { "assets" };
                write!(f, "{count} {asset_unit} transferred")
            }
            Self::SmartContract {
                contract_address,
                operation,
            } => {
                write!(f, "Smart contract {operation} at {contract_address}")
            }
            Self::AssetStaking {
                assets,
                direction,
                staker_address,
                staking_label,
                ..
            } => {
                let count = assets.len();
                let asset_unit = if count == 1 { "asset" } else { "assets" };
                let action = match direction {
                    StakeDirection::Stake => "staked",
                    StakeDirection::Unstake => "unstaked",
                };
                write!(
                    f,
                    "{count} {asset_unit} {action} by {staker_address} in {staking_label}"
                )
            }
            Self::AssetVesting {
                assets,
                direction,
                vest_style,
                owner_address,
                ..
            } => {
                let count = assets.len();
                let asset_unit = if count == 1 { "asset" } else { "assets" };
                let action = match direction {
                    VestingDirection::Lock => "locked",
                    VestingDirection::Unlock => "unlocked",
                };
                write!(
                    f,
                    "{count} {asset_unit} {action} via {vest_style} vesting by {owner_address}"
                )
            }
            Self::ListingCreate {
                assets,
                total_listing_count,
                seller,
                ..
            } => {
                let asset_unit = if *total_listing_count == 1 {
                    "asset"
                } else {
                    "assets"
                };

                // Calculate pricing information from PricedAssets
                let prices: Vec<u64> = assets
                    .iter()
                    .filter_map(|asset| asset.price_lovelace)
                    .collect();

                if prices.is_empty() {
                    write!(f, "{total_listing_count} {asset_unit} listed by {seller}")
                } else {
                    let total_value: u64 = prices.iter().sum();
                    let ada_total = total_value as f64 / 1_000_000.0;

                    // Check if all prices are the same
                    let all_same_price = prices.iter().all(|&p| p == prices[0]);

                    if all_same_price && prices.len() > 1 {
                        let ada_per = prices[0] as f64 / 1_000_000.0;
                        write!(f, "{total_listing_count} {asset_unit} listed by {seller} at ₳{ada_per:.1} each (₳{ada_total:.1} total)")
                    } else {
                        write!(f, "{total_listing_count} {asset_unit} listed by {seller} for ₳{ada_total:.1} total")
                    }
                }
            }
            Self::ListingUpdate {
                assets,
                total_listing_count,
                seller,
                ..
            } => {
                let asset_unit = if *total_listing_count == 1 {
                    "asset"
                } else {
                    "assets"
                };

                // Calculate pricing information from PricedAssets
                let new_prices: Vec<u64> = assets
                    .iter()
                    .filter_map(|asset| asset.price_lovelace)
                    .collect();

                let price_deltas: Vec<i64> = assets
                    .iter()
                    .filter_map(|asset| asset.delta_lovelace)
                    .collect();

                if !new_prices.is_empty() && !price_deltas.is_empty() {
                    // We have both new prices and deltas - show the change
                    let total_new: u64 = new_prices.iter().sum();
                    let total_delta: i64 = price_deltas.iter().sum();
                    let total_old = (total_new as i64 - total_delta) as u64;

                    let old_ada = total_old as f64 / 1_000_000.0;
                    let new_ada = total_new as f64 / 1_000_000.0;

                    // Check if all prices are the same
                    let all_same_new_price = new_prices.iter().all(|&p| p == new_prices[0]);
                    let all_same_delta = price_deltas.iter().all(|&d| d == price_deltas[0]);

                    if all_same_new_price && all_same_delta && new_prices.len() > 1 {
                        let new_ada_per = new_prices[0] as f64 / 1_000_000.0;
                        let old_ada_per =
                            (new_prices[0] as i64 - price_deltas[0]) as f64 / 1_000_000.0;
                        write!(f, "{total_listing_count} {asset_unit} repriced by {seller} from ₳{old_ada_per:.1} to ₳{new_ada_per:.1} each (₳{new_ada:.1})")
                    } else {
                        write!(f, "{total_listing_count} {asset_unit} repriced by {seller} from ₳{old_ada:.1} to ₳{new_ada:.1}")
                    }
                } else if !new_prices.is_empty() {
                    // We only have new prices - could be new prices or old prices without delta
                    let total_new: u64 = new_prices.iter().sum();
                    let new_ada = total_new as f64 / 1_000_000.0;

                    // Check if this appears to be partial pricing (old price without new price)
                    // This happens when delta is None but price is available
                    let has_any_deltas = assets.iter().any(|asset| asset.delta_lovelace.is_some());

                    if has_any_deltas {
                        // Normal case: we have new pricing
                        write!(f, "{total_listing_count} {asset_unit} updated by {seller} to ₳{new_ada:.1} total")
                    } else {
                        // Partial case: we likely have old pricing but new pricing failed to extract
                        write!(f, "{total_listing_count} {asset_unit} repriced by {seller} from ₳{new_ada:.1} (new price unknown)")
                    }
                } else {
                    // No pricing info available
                    write!(
                        f,
                        "{total_listing_count} {asset_unit} listing updated by {seller}"
                    )
                }
            }
            Self::Unlisting {
                assets,
                total_unlisting_count,
                seller,
                ..
            } => {
                let asset_unit = if *total_unlisting_count == 1 {
                    "asset"
                } else {
                    "assets"
                };

                if assets.len() == 1 {
                    write!(f, "{} unlisted by {seller}", assets[0].asset_name())
                } else {
                    write!(
                        f,
                        "{total_unlisting_count} {asset_unit} unlisted by {seller}"
                    )
                }
            }
            Self::DexSwap {
                dex_platform,
                asset_in,
                asset_out,
                swapper,
            } => {
                // Format asset displays based on OperationPayload type
                let in_display = match asset_in {
                    OperationPayload::Lovelace { amount } => {
                        let ada_amount = *amount as f64 / 1_000_000.0;
                        format!("₳{ada_amount:.2}")
                    }
                    OperationPayload::NativeToken {
                        policy_id: _,
                        encoded_name,
                        amount,
                    } => {
                        // Try to decode asset name
                        let asset_name = if encoded_name.is_empty() {
                            String::new()
                        } else {
                            match hex::decode(encoded_name) {
                                Ok(bytes) => match String::from_utf8(bytes) {
                                    Ok(decoded) => decoded,
                                    Err(_) => encoded_name.clone(),
                                },
                                Err(_) => encoded_name.clone(),
                            }
                        };
                        format!("{amount} {asset_name}")
                    }
                };

                let out_display = match asset_out {
                    OperationPayload::Lovelace { amount } => {
                        let ada_amount = *amount as f64 / 1_000_000.0;
                        format!("₳{ada_amount:.2}")
                    }
                    OperationPayload::NativeToken {
                        policy_id: _,
                        encoded_name,
                        amount,
                    } => {
                        // Try to decode asset name
                        let asset_name = if encoded_name.is_empty() {
                            String::new()
                        } else {
                            match hex::decode(encoded_name) {
                                Ok(bytes) => match String::from_utf8(bytes) {
                                    Ok(decoded) => decoded,
                                    Err(_) => encoded_name.clone(),
                                },
                                Err(_) => encoded_name.clone(),
                            }
                        };
                        format!("{amount} {asset_name}")
                    }
                };

                write!(
                    f,
                    "DEX swap on {dex_platform}: {in_display} → {out_display} by {swapper}"
                )
            }
            Self::DexLiquidityAdd {
                dex_platform,
                asset_a,
                amount_a,
                asset_b,
                amount_b,
                provider,
            } => {
                let a_display = if asset_a.policy_id.is_empty() {
                    let ada_amount = *amount_a as f64 / 1_000_000.0;
                    format!("₳{ada_amount:.2}")
                } else {
                    format!("{amount_a} {}", asset_a.asset_name())
                };

                let b_display = if asset_b.policy_id.is_empty() {
                    let ada_amount = *amount_b as f64 / 1_000_000.0;
                    format!("₳{ada_amount:.2}")
                } else {
                    format!("{amount_b} {}", asset_b.asset_name())
                };

                write!(
                    f,
                    "Liquidity added to {dex_platform}: {a_display} + {b_display} by {provider}"
                )
            }
            Self::DexLiquidityRemove {
                dex_platform,
                asset_a,
                amount_a,
                asset_b,
                amount_b,
                provider,
            } => {
                let a_display = if asset_a.policy_id.is_empty() {
                    let ada_amount = *amount_a as f64 / 1_000_000.0;
                    format!("₳{ada_amount:.2}")
                } else {
                    format!("{amount_a} {}", asset_a.asset_name())
                };

                let b_display = if asset_b.policy_id.is_empty() {
                    let ada_amount = *amount_b as f64 / 1_000_000.0;
                    format!("₳{ada_amount:.2}")
                } else {
                    format!("{amount_b} {}", asset_b.asset_name())
                };

                write!(f, "Liquidity removed from {dex_platform}: {a_display} + {b_display} by {provider}")
            }
            Self::Unknown => write!(f, "Unknown transaction type"),
        }
    }
}

/// Transaction context and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxContext {
    /// Block height
    pub block_height: Option<u64>,

    /// Block timestamp
    pub timestamp: Option<u64>,

    /// Transaction fee in lovelace
    pub fee: Option<u64>,

    /// Transaction size in bytes
    pub size: Option<u32>,

    /// Metadata attached to transaction
    pub metadata: Option<serde_json::Value>,

    /// Script interactions detected
    pub scripts: Vec<String>,

    /// Additional classification notes
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TxUtxo {
    pub address: String,
    pub idx: u32,
}

impl TxUtxo {
    pub fn get_marketplace_purpose(&self) -> Option<MarketplacePurpose> {
        MarketplacePurpose::from_address(&self.address)
    }
}

/// Asset operations in a transaction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetOperation {
    pub payload: OperationPayload,
    pub op_type: AssetOpType,
    #[serde(
        rename = "sender",
        serialize_with = "AssetOperation::serialize_utxo_address"
    )]
    pub input: Option<TxUtxo>,
    #[serde(
        rename = "receiver",
        serialize_with = "AssetOperation::serialize_utxo_address"
    )]
    pub output: Option<TxUtxo>,
    #[serde(skip_serializing)]
    pub input_datum: Option<TxDatum>,
    #[serde(skip_serializing)]
    pub output_datum: Option<TxDatum>,
    /// Classification for UTXO analysis (genuine vs housekeeping)
    #[serde(skip_serializing)]
    pub classification: OperationClassification,
}

impl AssetOperation {
    pub fn seller(&self) -> String {
        self.input.clone().map_or("unknown".into(), |s| s.address)
    }

    pub fn buyer(&self) -> String {
        self.output.clone().map_or("unknown".into(), |s| s.address)
    }

    /// Get the policy ID if this is a native token operation
    pub fn policy_id(&self) -> Option<&String> {
        match &self.payload {
            OperationPayload::NativeToken { policy_id, .. } => Some(policy_id),
            OperationPayload::Lovelace { .. } => None,
        }
    }

    /// Get the encoded asset name if this is a native token operation
    pub fn asset_name(&self) -> Option<&String> {
        match &self.payload {
            OperationPayload::NativeToken { encoded_name, .. } => Some(encoded_name),
            OperationPayload::Lovelace { .. } => None,
        }
    }

    /// Get the amount (works for both native tokens and lovelace)
    pub fn amount(&self) -> u64 {
        match &self.payload {
            OperationPayload::NativeToken { amount, .. } => *amount,
            OperationPayload::Lovelace { amount } => *amount,
        }
    }

    /// Get the full asset ID for native tokens (policy_id + encoded_name)
    pub fn asset_id(&self) -> Option<String> {
        match &self.payload {
            OperationPayload::NativeToken {
                policy_id,
                encoded_name,
                ..
            } => Some(format!("{policy_id}{encoded_name}")),
            OperationPayload::Lovelace { .. } => None,
        }
    }

    /// Check if this is a native token operation
    pub fn is_native_token(&self) -> bool {
        matches!(self.payload, OperationPayload::NativeToken { .. })
    }

    /// Check if this is a lovelace (ADA) operation
    pub fn is_lovelace(&self) -> bool {
        matches!(self.payload, OperationPayload::Lovelace { .. })
    }

    fn serialize_utxo_address<S>(utxo: &Option<TxUtxo>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let address = utxo.as_ref().map(|u| u.address.as_str()).unwrap_or("");
        serializer.serialize_str(address)
    }
}

/// Types of asset operations
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AssetOpType {
    Mint,
    Burn,
    Transfer,
    Lock,   // Locked in smart contract
    Unlock, // Released from smart contract
}

/// Classification of asset operations for UTXO analysis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationClassification {
    /// Genuine economic activity (wheat)
    Genuine,
    /// UTXO housekeeping or uninteresting operations (chaff)
    Housekeeping,
}

pub enum OpTypeMatcher {
    Any,
    OneOf(Vec<AssetOpType>),
}

/// ADA flow analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaFlows {
    /// Total ADA input (excluding collateral)
    pub total_input: u64,

    /// Total ADA output (excluding collateral)
    pub total_output: u64,

    /// Transaction fee
    pub fee: u64,

    /// Collateral input amount
    pub collateral_input: u64,

    /// Collateral output amount (usually 0 for successful transactions)
    pub collateral_output: u64,

    /// Largest single ADA transfer (potential sale price, excluding collateral)
    pub largest_transfer: Option<u64>,

    /// Address-to-address ADA flows (excluding collateral)
    pub flows: Vec<AdaFlow>,

    /// Collateral flows (separate from main transaction flows)
    pub collateral_flows: Vec<AdaFlow>,
}

/// Individual ADA flow between addresses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaFlow {
    pub from_address: String,
    pub to_address: String,
    pub amount: u64,
}

#[cfg(feature = "indexers")]
/// Main transaction classifier — requires the `indexers` feature for data fetching.
pub struct TxClassifier {
    indexer_pool: IndexerPool,
    rule_engine: RuleEngine,
}

#[cfg(feature = "indexers")]
impl TxClassifier {
    /// Create a new transaction classifier with the given address registry.
    pub fn new(indexer_pool: IndexerPool, registry: Box<dyn AddressLookup>) -> Self {
        Self {
            indexer_pool,
            rule_engine: RuleEngine::new(registry),
        }
    }

    /// Create a classifier from environment variables using the default ecosystem registry.
    pub async fn from_env(
        env: &worker::Env,
        network: &str,
        registry: Box<dyn AddressLookup>,
    ) -> Result<Self, TxClassifierError> {
        let indexer_pool = IndexerPool::from_env(env, network).await?;
        Ok(Self::new(indexer_pool, registry))
    }

    /// Fetch raw transaction data without classification
    pub async fn get_raw_tx_data(&self, tx_hash: &str) -> Result<RawTxData, TxClassifierError> {
        if !is_valid_tx_hash(tx_hash) {
            return Err(TxClassifierError::InvalidTxHash(tx_hash.to_string()));
        }
        self.indexer_pool.get_transaction(tx_hash).await
    }

    /// Classify a transaction by hash
    pub async fn classify_transaction(
        &self,
        tx_hash: &str,
    ) -> Result<TxClassification, TxClassifierError> {
        info!("Classifying transaction: {}", tx_hash);

        // Validate transaction hash format
        if !is_valid_tx_hash(tx_hash) {
            return Err(TxClassifierError::InvalidTxHash(tx_hash.to_string()));
        }

        // Fetch transaction data from indexers
        let tx_data = self.indexer_pool.get_transaction(tx_hash).await?;

        // Run classification rules
        let mut classification = self.rule_engine.classify(&tx_data);
        classification.tx_hash = tx_hash.to_string();

        debug!("Classification result: {:?}", classification);
        Ok(classification)
    }

    /// Classify multiple transactions
    pub async fn classify_batch(
        &self,
        tx_hashes: &[String],
    ) -> Vec<Result<TxClassification, TxClassifierError>> {
        let mut results = Vec::new();

        for tx_hash in tx_hashes {
            results.push(self.classify_transaction(tx_hash).await);
        }

        results
    }
}

/// Validate transaction hash format
#[cfg(feature = "indexers")]
fn is_valid_tx_hash(tx_hash: &str) -> bool {
    tx_hash.len() == 64 && tx_hash.chars().all(|c| c.is_ascii_hexdigit())
}
