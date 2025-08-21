use crate::types::*;
use anvil_api::{AnvilClient, CollectionAssetsRequest};
use cardano_assets::{AssetV2, CollectionDetails};
use std::collections::HashMap;
use tracing::debug;

/// Marketplace abstraction client that normalizes data across marketplaces
pub struct MarketplaceClient {
    anvil_client: AnvilClient,
}

impl MarketplaceClient {
    /// Create a new marketplace client
    pub fn new() -> Self {
        Self {
            anvil_client: AnvilClient::new(),
        }
    }

    /// Create client with environment configuration
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANVIL_API_KEY").map_err(|_| MarketplaceError::AuthRequired)?;

        Ok(Self {
            anvil_client: AnvilClient::new().with_api_key(&api_key),
        })
    }

    /// Create client with API key
    pub fn with_api_key(api_key: &str) -> Self {
        Self {
            anvil_client: AnvilClient::new().with_api_key(api_key),
        }
    }

    /// Get collection details for a policy ID
    /// This normalizes and deduplicates collection metadata from asset responses
    pub async fn get_collection_details(&self, policy_id: &str) -> Result<CollectionDetails> {
        debug!("Fetching collection details for policy_id: {}", policy_id);

        // Get a small sample of assets to extract collection metadata
        let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(1));

        let response = self
            .anvil_client
            .get_collection_assets(&request)
            .await
            .map_err(|e| {
                MarketplaceError::ApiError(format!("Failed to fetch collection assets: {}", e))
            })?;

        if response.results.is_empty() {
            return Err(MarketplaceError::CollectionNotFound(policy_id.to_string()));
        }

        // Extract collection details from the first asset
        let first_asset = &response.results[0];

        first_asset
            .collection
            .clone()
            .ok_or(MarketplaceError::CollectionNotFound(policy_id.to_string()))
    }

    /// Get floor price listing for a policy ID
    pub async fn get_floor_price(&self, policy_id: &str) -> Result<FloorPrice> {
        debug!("Fetching floor price for policy_id: {}", policy_id);

        let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(50));

        let response = self
            .anvil_client
            .get_collection_assets(&request)
            .await
            .map_err(|e| MarketplaceError::ApiError(format!("Failed to fetch assets: {}", e)))?;

        if response.results.is_empty() {
            return Err(MarketplaceError::NoListingsFound);
        }

        // Find the lowest price
        let floor_price = response
            .results
            .iter()
            .filter_map(|asset| asset.listing.as_ref().map(|l| l.price))
            .min()
            .ok_or(MarketplaceError::NoListingsFound)?;

        // Count assets at floor price and collect samples
        let mut floor_assets = Vec::new();
        let mut marketplace_distribution: HashMap<String, u32> = HashMap::new();

        for asset in &response.results {
            if let Some(listing) = &asset.listing {
                if listing.price == floor_price {
                    // Convert to normalized asset
                    let normalized_asset = self.convert_to_normalized_asset(asset)?;
                    floor_assets.push(normalized_asset);

                    // Track marketplace distribution
                    let marketplace = listing.marketplace.to_string();
                    *marketplace_distribution.entry(marketplace).or_insert(0) += 1;
                }
            }
        }

        // Take up to 5 sample assets
        let sample_assets = floor_assets.into_iter().take(5).collect();
        let count = marketplace_distribution.values().sum();

        Ok(FloorPrice {
            price: floor_price,
            count,
            sample_assets,
            marketplace_distribution,
        })
    }

    /// Get floor price listing based on trait filters
    pub async fn get_floor_price_filtered(
        &self,
        policy_id: &str,
        trait_filter: &TraitFilter,
    ) -> Result<FloorPrice> {
        debug!(
            "Fetching filtered floor price for policy_id: {} with filters: {:?}",
            policy_id, trait_filter
        );

        // Start with basic request
        let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(100));

        // TODO: Implement trait filtering in anvil-api when supported
        // For now, we'll fetch assets and filter client-side
        let response = self
            .anvil_client
            .get_collection_assets(&request)
            .await
            .map_err(|e| MarketplaceError::ApiError(format!("Failed to fetch assets: {}", e)))?;

        if response.results.is_empty() {
            return Err(MarketplaceError::NoListingsFound);
        }

        // Filter assets by traits
        let filtered_assets: Vec<_> = response
            .results
            .iter()
            .filter(|asset| self.matches_trait_filter(asset, trait_filter))
            .collect();

        if filtered_assets.is_empty() {
            return Err(MarketplaceError::NoListingsFound);
        }

        // Find the lowest price among filtered assets
        let floor_price = filtered_assets
            .iter()
            .filter_map(|asset| asset.listing.as_ref().map(|l| l.price))
            .min()
            .ok_or(MarketplaceError::NoListingsFound)?;

        // Count assets at floor price and collect samples
        let mut floor_assets = Vec::new();
        let mut marketplace_distribution: HashMap<String, u32> = HashMap::new();

        for asset in &filtered_assets {
            if let Some(listing) = &asset.listing {
                if listing.price == floor_price {
                    let normalized_asset = self.convert_to_normalized_asset(asset)?;
                    floor_assets.push(normalized_asset);

                    let marketplace = listing.marketplace.to_string();
                    *marketplace_distribution.entry(marketplace).or_insert(0) += 1;
                }
            }
        }

        let sample_assets = floor_assets.into_iter().take(5).collect();
        let count = marketplace_distribution.values().sum();

        Ok(FloorPrice {
            price: floor_price,
            count,
            sample_assets,
            marketplace_distribution,
        })
    }

    /// Convert anvil asset to normalized asset format
    fn convert_to_normalized_asset(&self, anvil_asset: &anvil_api::Asset) -> Result<AssetV2> {
        use cardano_assets::Traits;
        
        // Convert HashMap<String, String> to Traits format
        let mut traits = Traits::new();
        for (key, value) in &anvil_asset.attributes {
            traits.insert_single(key.clone(), value.clone());
        }

        // Ensure image is never None - use empty string as fallback
        let image = anvil_asset.image.clone().unwrap_or_default();

        Ok(AssetV2::new(
            anvil_asset.unit.clone(),
            anvil_asset.name.clone(),
            image,
            traits,
            anvil_asset.rarity,
            vec![], // No tags from anvil API currently
        ))
    }

    /// Check if an asset matches the given trait filter
    fn matches_trait_filter(&self, asset: &anvil_api::Asset, filter: &TraitFilter) -> bool {
        if filter.is_empty() {
            return true;
        }

        // Use attributes directly
        let asset_traits = &asset.attributes;

        // Check if asset matches all required traits
        for (trait_name, required_values) in &filter.filters {
            if let Some(asset_value) = asset_traits.get(trait_name) {
                if !required_values.contains(asset_value) {
                    return false;
                }
            } else {
                // Asset doesn't have this trait
                return false;
            }
        }

        true
    }
}

impl Default for MarketplaceClient {
    fn default() -> Self {
        Self::new()
    }
}
