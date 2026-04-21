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
//! # async fn example() -> Result<(), ownership_client::Error> {
//! let client = OwnershipClient::new("https://ownership.cnft.dev");
//! let bundle = client.get_bundle("policy_id_hex", "stake1...").await?;
//! # Ok(())
//! # }
//! ```

mod types;

pub use types::*;

use http_client::HttpClient;
pub use http_client::HttpError;

/// Unified error type for ownership client operations.
#[derive(Debug)]
pub enum Error {
    Http(HttpError),
    Json(serde_json::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "{e}"),
            Error::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<HttpError> for Error {
    fn from(e: HttpError) -> Self {
        Error::Http(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

/// Client for the collection-ownership service.
///
/// Wraps the HTTP API with typed request/response pairs.
/// Can target either a direct URL (e.g. `https://ownership.cnft.dev`)
/// or be used behind a service binding via the base URL.
#[derive(Clone)]
pub struct OwnershipClient {
    base_url: String,
    client: HttpClient,
}

impl OwnershipClient {
    /// Create a new client targeting the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: HttpClient::new(),
        }
    }

    /// Create a client with a debug token for admin endpoints.
    pub fn with_debug_token(base_url: impl Into<String>, token: &str) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: HttpClient::new().with_header("X-Debug-Token", token),
        }
    }

    /// Create a client with a custom `HttpClient` (e.g. with auth headers).
    pub fn with_client(base_url: impl Into<String>, client: HttpClient) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client,
        }
    }

    // ========================================================================
    // Public Query API
    // ========================================================================

    /// Get the ownership bundle for a stake address within a policy.
    pub async fn get_bundle(
        &self,
        policy_id: &str,
        stake: &str,
    ) -> Result<BundleResponse, HttpError> {
        let url = format!("{}/api/bundle/{policy_id}?stake={stake}", self.base_url);
        self.client.get(&url).await
    }

    /// Get a paginated bundle with optional filters.
    pub async fn get_bundle_page(
        &self,
        policy_id: &str,
        params: &BundleParams<'_>,
    ) -> Result<BundleResponse, HttpError> {
        let url = self.build_bundle_url(policy_id, params);
        self.client.get(&url).await
    }

    /// Fetch all bundle entries, automatically paginating.
    pub async fn get_bundle_all(
        &self,
        policy_id: &str,
        stake: Option<&str>,
        page_size: u32,
        trait_bits: &[usize],
    ) -> Result<BundleResponse, HttpError> {
        let mut all_entries = Vec::new();

        // Fetch first page
        let params = BundleParams {
            stake,
            limit: Some(page_size),
            cursor: None,
            trait_bits: trait_bits.to_vec(),
            ..Default::default()
        };
        let first = self.get_bundle_page(policy_id, &params).await?;
        let cache_generation = first.cache_generation;
        let schema_version = first.schema_version;
        let bitmap_size = first.bitmap_size;
        let asset_count = first.asset_count;
        all_entries.extend(first.entries);

        // Paginate remaining
        let mut cursor = first.next_cursor;
        while let Some(c) = cursor.take() {
            let params = BundleParams {
                stake,
                limit: Some(page_size),
                cursor: Some(&c),
                trait_bits: trait_bits.to_vec(),
                ..Default::default()
            };
            let resp = self.get_bundle_page(policy_id, &params).await?;
            cursor = resp.next_cursor;
            all_entries.extend(resp.entries);
        }

        Ok(BundleResponse {
            cache_generation,
            schema_version,
            bitmap_size,
            asset_count,
            entries: all_entries,
            next_cursor: None,
        })
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
        let url = format!("{}/api/resolve-fingerprint?fp={fingerprint}", self.base_url);
        match self.client.get::<AssetIdentity>(&url).await {
            Ok(identity) => Ok(Some(identity)),
            Err(HttpError::HttpStatus {
                status_code: 404, ..
            }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // ========================================================================
    // Admin API (requires debug token)
    // ========================================================================

    /// List all tracked policies.
    pub async fn list_policies(&self) -> Result<PolicyListResponse, HttpError> {
        let url = format!("{}/admin/policies", self.base_url);
        self.client.get(&url).await
    }

    /// Get sync status for a policy.
    pub async fn get_sync_status(&self, policy_id: &str) -> Result<SyncStatusResponse, HttpError> {
        let url = format!("{}/api/status/{policy_id}", self.base_url);
        self.client.get(&url).await
    }

    /// Trigger an immediate sync for a policy.
    pub async fn trigger_sync(&self, policy_id: &str) -> Result<String, HttpError> {
        let url = format!("{}/admin/policies/{policy_id}/sync", self.base_url);
        self.client.post::<(), String>(&url, &()).await
    }

    /// Update a policy (enable/disable, sync interval, label).
    pub async fn update_policy(
        &self,
        policy_id: &str,
        update: &PolicyUpdate,
    ) -> Result<String, HttpError> {
        let url = format!("{}/admin/policies/{policy_id}", self.base_url);
        self.client.patch(&url, update).await
    }

    /// Delete a policy — resets the DO and removes the D1 row.
    pub async fn delete_policy(&self, policy_id: &str) -> Result<String, HttpError> {
        let url = format!("{}/admin/policies/{policy_id}", self.base_url);
        self.client.delete(&url).await
    }

    /// Run validation checks across all policies.
    pub async fn validate(&self) -> Result<ValidateResponse, HttpError> {
        let url = format!("{}/admin/validate", self.base_url);
        self.client.get(&url).await
    }

    /// Get the visual style guide for a collection.
    pub async fn get_visual_guide(
        &self,
        policy_id: &str,
    ) -> Result<VisualGuideResponse, HttpError> {
        let url = format!("{}/admin/visual-analysis/{policy_id}/guide", self.base_url);
        self.client.get(&url).await
    }

    /// Get the narrative style guide for a collection.
    pub async fn get_narrative_guide(
        &self,
        policy_id: &str,
    ) -> Result<NarrativeGuideResponse, HttpError> {
        let url = format!(
            "{}/admin/visual-analysis/{policy_id}/narrative",
            self.base_url
        );
        self.client.get(&url).await
    }

    /// Get a visual profile for a specific asset.
    pub async fn get_asset_profile(
        &self,
        policy_id: &str,
        asset_hex: &str,
    ) -> Result<VisualProfile, HttpError> {
        let url = format!(
            "{}/admin/visual-analysis/{policy_id}/profile/{asset_hex}",
            self.base_url
        );
        self.client.get(&url).await
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    fn build_bundle_url(&self, policy_id: &str, params: &BundleParams<'_>) -> String {
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

        url.trim_end_matches(['&', '?']).to_string()
    }
}
