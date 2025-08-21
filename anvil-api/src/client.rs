use crate::{error::AnvilError, types::*};
use http_client::HttpClient;
use tracing::debug;

const BASE_URL: &str = "https://prod.api.ada-anvil.app";

pub struct AnvilClient {
    http_client: HttpClient,
    base_url: String,
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

    pub async fn get_collection_assets(
        &self,
        request: &CollectionAssetsRequest,
    ) -> Result<CollectionAssetsResponse, AnvilError> {
        if request.policy_id.trim().is_empty() {
            return Err(AnvilError::InvalidInput(
                "Policy ID cannot be empty".to_string(),
            ));
        }

        let mut query_params = vec![("policyId", request.policy_id.as_str())];

        let limit_str;
        if let Some(limit) = &request.limit {
            limit_str = limit.to_string();
            query_params.push(("limit", &limit_str));
        }

        if let Some(cursor) = &request.cursor {
            query_params.push(("cursor", cursor.as_str()));
        }

        if let Some(sale_type) = &request.sale_type {
            let sale_type_str = match sale_type {
                SaleType::All => "all",
                SaleType::ListedOnly => "listedOnly",
                SaleType::Bundles => "bundles",
            };
            query_params.push(("saleType", sale_type_str));
        }

        if let Some(order_by) = &request.order_by {
            let order_by_str = match order_by {
                OrderBy::PriceAsc => "priceAsc",
                OrderBy::PriceDesc => "priceDesc",
            };
            query_params.push(("orderBy", order_by_str));
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

        let response = self.http_client.get::<CollectionAssetsResponse>(&url).await?;

        Ok(response)
    }
}