use crate::{error::AnvilError, types::*};
use http_client::HttpClient;
use tracing::debug;

const BASE_URL: &str = "https://prod.api.ada-anvil.app";

pub struct AnvilClient {
    http_client: HttpClient,
    base_url: String,
}

impl Default for AnvilClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AnvilClient {
    pub fn new() -> Self {
        Self {
            http_client: HttpClient::new().with_user_agent("anvil-api-client/0.1.0"),
            base_url: BASE_URL.to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.to_string();
        self
    }

    pub fn with_api_key(self, api_key: &str) -> Self {
        Self {
            http_client: self.http_client.with_header("X-Api-Key", api_key),
            base_url: self.base_url,
        }
    }

    /// Get collection details by extracting metadata from a sample asset
    /// This is a convenience method that fetches a single asset to get collection metadata
    pub async fn get_collection_details(
        &self,
        policy_id: &str,
    ) -> Result<cardano_assets::CollectionDetails, AnvilError> {
        debug!("Fetching collection details for policy_id: {}", policy_id);

        // Get a single asset to extract collection metadata
        let request = CollectionAssetsRequest::new(policy_id).with_limit(1);

        let response = self.get_collection_assets(&request).await?;

        if response.results.is_empty() {
            return Err(AnvilError::InvalidInput(format!(
                "No assets found for policy ID: {}",
                policy_id
            )));
        }

        // Extract collection details from the first asset
        let first_asset = &response.results[0];

        first_asset
            .collection
            .clone()
            .ok_or_else(|| {
                AnvilError::InvalidInput(format!(
                    "No collection details found for policy ID: {}",
                    policy_id
                ))
            })
    }

    /// Get floor assets - returns the cheapest listed assets in price ascending order
    /// This is a convenience method for getting floor price listings
    pub async fn get_floor(
        &self,
        policy_id: &str,
        count: u32,
    ) -> Result<Vec<Asset>, AnvilError> {
        debug!("Fetching {} floor assets for policy_id: {}", count, policy_id);

        let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(count))
            .with_order_by(OrderBy::PriceAsc);

        let response = self.get_collection_assets(&request).await?;
        
        Ok(response.results)
    }

    pub async fn get_collection_assets(
        &self,
        request: &CollectionAssetsRequest,
    ) -> Result<CollectionAssetsResponse, AnvilError> {
        if request.policy_id.trim().is_empty() {
            return Err(AnvilError::InvalidInput(
                "Policy ID cannot be empty".to_string(),
            ));
        }

        // Collect all string values first to avoid borrow checker issues
        let limit_str = request.limit.as_ref().map(|l| l.to_string());
        let min_price_str = request.min_price.as_ref().map(|p| p.to_string());
        let max_price_str = request.max_price.as_ref().map(|p| p.to_string());
        let min_rarity_str = request.min_rarity.as_ref().map(|r| r.to_string());
        let max_rarity_str = request.max_rarity.as_ref().map(|r| r.to_string());
        let properties_json = request
            .properties
            .as_ref()
            .filter(|p| !p.is_empty())
            .map(|p| serde_json::to_string(p))
            .transpose()
            .map_err(|e| {
                AnvilError::InvalidInput(format!("Failed to serialize properties: {}", e))
            })?;

        // Build query parameters
        let mut query_params = vec![("policyId", request.policy_id.as_str())];

        if let Some(ref limit_str) = limit_str {
            query_params.push(("limit", limit_str.as_str()));
        }

        if let Some(cursor) = &request.cursor {
            query_params.push(("cursor", cursor.as_str()));
        }

        if let Some(ref min_price_str) = min_price_str {
            query_params.push(("minPrice", min_price_str.as_str()));
        }

        if let Some(ref max_price_str) = max_price_str {
            query_params.push(("maxPrice", max_price_str.as_str()));
        }

        if let Some(ref min_rarity_str) = min_rarity_str {
            query_params.push(("minRarity", min_rarity_str.as_str()));
        }

        if let Some(ref max_rarity_str) = max_rarity_str {
            query_params.push(("maxRarity", max_rarity_str.as_str()));
        }

        if let Some(order_by) = &request.order_by {
            let order_by_str = match order_by {
                OrderBy::PriceAsc => "priceAsc",
                OrderBy::PriceDesc => "priceDesc",
                OrderBy::NameAsc => "nameAsc",
                OrderBy::IdxAsc => "idxAsc",
                OrderBy::RecentlyListed => "recentlyListed",
                OrderBy::RarityAsc => "rarityAsc",
                OrderBy::RecentlyMinted => "recentlyMinted",
            };
            query_params.push(("orderBy", order_by_str));
        }

        if let Some(term) = &request.term {
            query_params.push(("term", term.as_str()));
        }

        if let Some(listing_type) = &request.listing_type {
            let listing_type_str = match listing_type {
                ListingType::JpgStore => "jpgstore",
                ListingType::Wayup => "wayup",
                ListingType::SpaceBudz => "spacebudz",
            };
            query_params.push(("listingType", listing_type_str));
        }

        if let Some(sale_type) = &request.sale_type {
            let sale_type_str = match sale_type {
                SaleType::All => "all",
                SaleType::ListedOnly => "listedOnly",
                SaleType::Bundles => "bundles",
            };
            query_params.push(("saleType", sale_type_str));
        }

        if let Some(ref properties_json) = properties_json {
            query_params.push(("properties", properties_json.as_str()));
        }

        let query_string = query_params
            .iter()
            .map(|(key, value)| format!("{}={}", key, urlencoding::encode(value)))
            .collect::<Vec<_>>()
            .join("&");

        let url = format!(
            "{}/marketplace/api/get-collection-assets?{}",
            self.base_url, query_string
        );

        debug!("Making request to: {}", url);

        let response = self
            .http_client
            .get::<CollectionAssetsResponse>(&url)
            .await?;

        Ok(response)
    }
}
