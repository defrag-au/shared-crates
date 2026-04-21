//! Typed HTTP client for the collection-ownership service.
//!
//! Provides request/response types and a client for querying ownership data,
//! rarity ranks, trait bitmaps, and CIP-14 fingerprint resolution.
//!
//! # Usage
//!
//! ```no_run
//! use ownership_client::OwnershipClient;
//!
//! let client = OwnershipClient::new("https://ownership.cnft.dev");
//! let bundle = client.get_bundle("policy_id_hex", "stake1...").await?;
//! ```

mod types;

pub use types::*;

use http_client::{HttpClient, HttpError};

/// Client for the collection-ownership service.
///
/// Wraps the HTTP API with typed request/response pairs.
/// Can target either a direct URL (e.g. `https://ownership.cnft.dev`)
/// or be used behind a service binding via the base URL.
pub struct OwnershipClient {
    base_url: String,
    client: HttpClient,
}

impl OwnershipClient {
    /// Create a new client targeting the given base URL.
    ///
    /// # Example
    /// ```no_run
    /// let client = OwnershipClient::new("https://ownership.cnft.dev");
    /// ```
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: HttpClient::new(),
        }
    }

    /// Create a client with a custom `HttpClient` (e.g. with auth headers).
    pub fn with_client(base_url: impl Into<String>, client: HttpClient) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client,
        }
    }

    /// Get the ownership bundle for a stake address within a policy.
    ///
    /// Returns all assets owned by the stake address, with rarity ranks and
    /// optional trait data. Supports pagination via cursor.
    pub async fn get_bundle(
        &self,
        policy_id: &str,
        stake: &str,
    ) -> Result<BundleResponse, HttpError> {
        let url = format!(
            "{}/api/bundle/{policy_id}?stake={stake}",
            self.base_url
        );
        self.client.get(&url).await
    }

    /// Get a paginated bundle with optional trait decoding.
    pub async fn get_bundle_page(
        &self,
        policy_id: &str,
        params: &BundleParams<'_>,
    ) -> Result<BundleResponse, HttpError> {
        let mut url = format!("{}/api/bundle/{policy_id}?", self.base_url);

        if let Some(stake) = params.stake {
            url.push_str(&format!("stake={stake}&"));
        }
        if let Some(limit) = params.limit {
            url.push_str(&format!("limit={limit}&"));
        }
        if let Some(cursor) = params.cursor {
            url.push_str(&format!("cursor={cursor}&"));
        }
        if params.decode_traits {
            url.push_str("traits=decoded&");
        }
        if !params.trait_bits.is_empty() {
            let bits: Vec<String> = params.trait_bits.iter().map(|b| b.to_string()).collect();
            url.push_str(&format!("trait_bits={}&", bits.join(",")));
        }

        // Remove trailing & or ?
        let url = url.trim_end_matches(['&', '?']).to_string();
        self.client.get(&url).await
    }

    /// Point query: who owns this asset?
    pub async fn get_owner(
        &self,
        policy_id: &str,
        asset_name_hex: &str,
    ) -> Result<OwnerResponse, HttpError> {
        let url = format!(
            "{}/api/owner/{policy_id}?asset={asset_name_hex}",
            self.base_url
        );
        self.client.get(&url).await
    }

    /// Ownership check: does this stake own this asset?
    pub async fn check_ownership(
        &self,
        policy_id: &str,
        asset_name_hex: &str,
        stake: &str,
    ) -> Result<CheckResponse, HttpError> {
        let url = format!(
            "{}/api/check/{policy_id}?asset={asset_name_hex}&stake={stake}",
            self.base_url
        );
        self.client.get(&url).await
    }

    /// Get collection stats (asset count, holder count, last updated).
    pub async fn get_stats(&self, policy_id: &str) -> Result<StatsResponse, HttpError> {
        let url = format!("{}/api/stats/{policy_id}", self.base_url);
        self.client.get(&url).await
    }

    /// Get ownership changes since a sequence number (polling pattern).
    pub async fn get_changes(
        &self,
        policy_id: &str,
        since: u64,
        limit: u32,
    ) -> Result<ChangesFeedResponse, HttpError> {
        let url = format!(
            "{}/api/changes/{policy_id}?since={since}&limit={limit}",
            self.base_url
        );
        self.client.get(&url).await
    }

    /// Get the most recent N changes (for initial load).
    pub async fn get_latest_changes(
        &self,
        policy_id: &str,
        limit: u32,
    ) -> Result<ChangesFeedResponse, HttpError> {
        let url = format!(
            "{}/api/changes/{policy_id}?latest=true&limit={limit}",
            self.base_url
        );
        self.client.get(&url).await
    }

    /// Get the trait bitmap schema for decoding bitmaps.
    pub async fn get_trait_schema(
        &self,
        policy_id: &str,
    ) -> Result<TraitSchemaResponse, HttpError> {
        let url = format!("{}/api/trait-schema/{policy_id}", self.base_url);
        self.client.get(&url).await
    }

    /// Resolve a CIP-14 fingerprint to (policy_id, asset_name_hex).
    ///
    /// Returns `None` (via 404) if the fingerprint is not in the registry.
    pub async fn resolve_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<Option<AssetIdentity>, HttpError> {
        let url = format!(
            "{}/api/resolve-fingerprint?fp={fingerprint}",
            self.base_url
        );
        match self.client.get::<AssetIdentity>(&url).await {
            Ok(identity) => Ok(Some(identity)),
            Err(HttpError::HttpStatus { status_code: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
