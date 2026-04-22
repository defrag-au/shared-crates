//! Typed HTTP client for the collection-ownership service.
//!
//! Uses reqwest for HTTP — works on both native and WASM targets.

mod types;

pub use types::*;

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Error type for ownership client operations.
#[derive(Debug)]
pub enum Error {
    Request(String),
    Http { status: u16, body: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Request(e) => write!(f, "HTTP request error: {e}"),
            Error::Http { status, body } => write!(f, "HTTP {status}: {body}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Request(e.to_string())
    }
}

/// Client for the collection-ownership service.
#[derive(Clone)]
pub struct OwnershipClient {
    base_url: String,
    client: Client,
}

impl OwnershipClient {
    /// Create a new client targeting the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }

    /// Create a client with a debug token for admin endpoints.
    pub fn with_debug_token(base_url: impl Into<String>, token: &str) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "X-Debug-Token",
            reqwest::header::HeaderValue::from_str(token).unwrap(),
        );
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::builder().default_headers(headers).build().unwrap(),
        }
    }

    // ── helpers ──────────────────────────────────────────────────────

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, Error> {
        let resp = self.client.get(url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Http { status, body });
        }
        Ok(resp.json().await?)
    }

    async fn post_empty(&self, url: &str) -> Result<String, Error> {
        let resp = self.client.post(url).send().await?;
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        if status >= 400 {
            return Err(Error::Http { status, body });
        }
        Ok(body)
    }

    async fn patch_json<T: Serialize>(&self, url: &str, body: &T) -> Result<String, Error> {
        let resp = self.client.patch(url).json(body).send().await?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        if status >= 400 {
            return Err(Error::Http { status, body: text });
        }
        Ok(text)
    }

    async fn delete_url(&self, url: &str) -> Result<String, Error> {
        let resp = self.client.delete(url).send().await?;
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        if status >= 400 {
            return Err(Error::Http { status, body });
        }
        Ok(body)
    }

    // ========================================================================
    // Public Query API
    // ========================================================================

    pub async fn get_bundle(
        &self,
        policy_id: &str,
        stake: &str,
    ) -> Result<BundleResponse, Error> {
        let url = format!("{}/api/bundle/{policy_id}?stake={stake}", self.base_url);
        self.get_json(&url).await
    }

    pub async fn get_bundle_page(
        &self,
        policy_id: &str,
        params: &BundleParams<'_>,
    ) -> Result<BundleResponse, Error> {
        let url = self.build_bundle_url(policy_id, params);
        self.get_json(&url).await
    }

    pub async fn get_bundle_all(
        &self,
        policy_id: &str,
        stake: Option<&str>,
        page_size: u32,
        trait_bits: &[usize],
    ) -> Result<BundleResponse, Error> {
        let mut all_entries = Vec::new();

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

    pub async fn get_owner(
        &self,
        policy_id: &str,
        asset_name_hex: &str,
    ) -> Result<OwnerResponse, Error> {
        let url = format!(
            "{}/api/owner/{policy_id}?asset={asset_name_hex}",
            self.base_url
        );
        self.get_json(&url).await
    }

    pub async fn check_ownership(
        &self,
        policy_id: &str,
        asset_name_hex: &str,
        stake: &str,
    ) -> Result<CheckResponse, Error> {
        let url = format!(
            "{}/api/check/{policy_id}?asset={asset_name_hex}&stake={stake}",
            self.base_url
        );
        self.get_json(&url).await
    }

    pub async fn get_stats(&self, policy_id: &str) -> Result<StatsResponse, Error> {
        let url = format!("{}/api/stats/{policy_id}", self.base_url);
        self.get_json(&url).await
    }

    pub async fn get_changes(
        &self,
        policy_id: &str,
        since: u64,
        limit: u32,
    ) -> Result<ChangesFeedResponse, Error> {
        let url = format!(
            "{}/api/changes/{policy_id}?since={since}&limit={limit}",
            self.base_url
        );
        self.get_json(&url).await
    }

    pub async fn get_latest_changes(
        &self,
        policy_id: &str,
        limit: u32,
    ) -> Result<ChangesFeedResponse, Error> {
        let url = format!(
            "{}/api/changes/{policy_id}?latest=true&limit={limit}",
            self.base_url
        );
        self.get_json(&url).await
    }

    pub async fn get_trait_schema(
        &self,
        policy_id: &str,
    ) -> Result<TraitSchemaResponse, Error> {
        let url = format!("{}/api/trait-schema/{policy_id}", self.base_url);
        self.get_json(&url).await
    }

    pub async fn get_collections(&self) -> Result<PolicyListResponse, Error> {
        let url = format!("{}/api/collections", self.base_url);
        self.get_json(&url).await
    }

    pub async fn resolve_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<Option<AssetIdentity>, Error> {
        let url = format!(
            "{}/api/resolve-fingerprint?fp={fingerprint}",
            self.base_url
        );
        match self.get_json::<AssetIdentity>(&url).await {
            Ok(identity) => Ok(Some(identity)),
            Err(Error::Http { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // ========================================================================
    // Admin API (requires debug token)
    // ========================================================================

    pub async fn list_policies(&self) -> Result<PolicyListResponse, Error> {
        let url = format!("{}/admin/policies", self.base_url);
        self.get_json(&url).await
    }

    pub async fn get_sync_status(&self, policy_id: &str) -> Result<SyncStatusResponse, Error> {
        let url = format!("{}/api/status/{policy_id}", self.base_url);
        self.get_json(&url).await
    }

    pub async fn trigger_sync(&self, policy_id: &str) -> Result<String, Error> {
        let url = format!("{}/admin/policies/{policy_id}/sync", self.base_url);
        self.post_empty(&url).await
    }

    pub async fn update_policy(
        &self,
        policy_id: &str,
        update: &PolicyUpdate,
    ) -> Result<String, Error> {
        let url = format!("{}/admin/policies/{policy_id}", self.base_url);
        self.patch_json(&url, update).await
    }

    pub async fn delete_policy(&self, policy_id: &str) -> Result<String, Error> {
        let url = format!("{}/admin/policies/{policy_id}", self.base_url);
        self.delete_url(&url).await
    }

    pub async fn validate(&self) -> Result<ValidateResponse, Error> {
        let url = format!("{}/admin/validate", self.base_url);
        self.get_json(&url).await
    }

    pub async fn get_visual_guide(
        &self,
        policy_id: &str,
    ) -> Result<VisualGuideResponse, Error> {
        let url = format!("{}/admin/visual-analysis/{policy_id}/guide", self.base_url);
        self.get_json(&url).await
    }

    pub async fn get_narrative_guide(
        &self,
        policy_id: &str,
    ) -> Result<NarrativeGuideResponse, Error> {
        let url = format!(
            "{}/admin/visual-analysis/{policy_id}/narrative",
            self.base_url
        );
        self.get_json(&url).await
    }

    pub async fn get_asset_profile(
        &self,
        policy_id: &str,
        asset_hex: &str,
    ) -> Result<VisualProfile, Error> {
        let url = format!(
            "{}/admin/visual-analysis/{policy_id}/profile/{asset_hex}",
            self.base_url
        );
        self.get_json(&url).await
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
