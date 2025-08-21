use cardano_assets::AssetV2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;


/// Listing information for an asset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    /// Asset being listed
    pub asset: AssetV2,
    /// Price in lovelace
    pub price: u64,
    /// Marketplace where it's listed
    pub marketplace: String,
    /// Listing transaction hash
    pub tx_hash: Option<String>,
}

/// Floor price information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorPrice {
    /// Lowest price in lovelace
    pub price: u64,
    /// Number of assets at floor price
    pub count: u32,
    /// Sample assets at floor price (up to 5)
    pub sample_assets: Vec<AssetV2>,
    /// Marketplace distribution at floor price
    pub marketplace_distribution: HashMap<String, u32>,
}

/// Trait filter for queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitFilter {
    pub filters: HashMap<String, Vec<String>>,
}

impl TraitFilter {
    pub fn new() -> Self {
        Self {
            filters: HashMap::new(),
        }
    }

    pub fn add_trait(mut self, trait_name: String, values: Vec<String>) -> Self {
        self.filters.insert(trait_name, values);
        self
    }

    pub fn add_single_trait(mut self, trait_name: String, value: String) -> Self {
        self.filters.insert(trait_name, vec![value]);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }
}

impl Default for TraitFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Paginated results for asset queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetPage {
    /// Assets in this page
    pub assets: Vec<AssetV2>,
    /// Listings for assets (if requested)
    pub listings: Vec<Listing>,
    /// Pagination cursor for next page
    pub next_cursor: Option<String>,
    /// Total count if available
    pub total_count: Option<u32>,
}

/// Query options for asset retrieval
#[derive(Debug, Clone, Default)]
pub struct AssetQuery {
    /// Maximum number of assets to return
    pub limit: Option<u32>,
    /// Pagination cursor
    pub cursor: Option<String>,
    /// Include only listed assets
    pub listed_only: bool,
    /// Trait filters
    pub trait_filter: Option<TraitFilter>,
    /// Sort order
    pub sort_by: SortBy,
}

/// Sort options for asset queries
#[derive(Debug, Clone, Default)]
pub enum SortBy {
    #[default]
    PriceAsc,
    PriceDesc,
    RarityAsc,
    RarityDesc,
    RecentlyListed,
}

/// Error types for marketplace operations
#[derive(Debug, Clone)]
pub enum MarketplaceError {
    /// API communication error
    ApiError(String),
    /// Invalid policy ID format
    InvalidPolicyId(String),
    /// Collection not found
    CollectionNotFound(String),
    /// No listings found
    NoListingsFound,
    /// Rate limit exceeded
    RateLimited,
    /// Authentication required
    AuthRequired,
    /// Unknown error
    Unknown(String),
}

impl std::fmt::Display for MarketplaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketplaceError::ApiError(msg) => write!(f, "API error: {}", msg),
            MarketplaceError::InvalidPolicyId(id) => write!(f, "Invalid policy ID: {}", id),
            MarketplaceError::CollectionNotFound(id) => write!(f, "Collection not found: {}", id),
            MarketplaceError::NoListingsFound => write!(f, "No listings found"),
            MarketplaceError::RateLimited => write!(f, "Rate limit exceeded"),
            MarketplaceError::AuthRequired => write!(f, "Authentication required"),
            MarketplaceError::Unknown(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for MarketplaceError {}

/// Result type for marketplace operations
pub type Result<T> = std::result::Result<T, MarketplaceError>;
