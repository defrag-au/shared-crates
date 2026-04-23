//! Cardanoscan API client.
//!
//! Provides typed access to the Cardanoscan REST API for Cardano blockchain data.
//! Primary use case: resolving CIP-14 asset fingerprints to (policy_id, asset_name).
//!
//! API docs: <https://docs.cardanoscan.io>

use reqwest::Client;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://api.cardanoscan.io/api/v1";

/// Error type for Cardanoscan API operations.
#[derive(Debug)]
pub enum Error {
    Request(String),
    Http { status: u16, body: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Request(e) => write!(f, "Cardanoscan request error: {e}"),
            Error::Http { status, body } => write!(f, "Cardanoscan HTTP {status}: {body}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Request(e.to_string())
    }
}

/// Cardanoscan API client.
#[derive(Clone)]
pub struct CardanoscanClient {
    client: Client,
    api_key: String,
}

impl CardanoscanClient {
    /// Create a new client with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, Error> {
        let resp = self
            .client
            .get(path)
            .header("apiKey", &self.api_key)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Http { status, body });
        }
        Ok(resp.json().await?)
    }

    // ========================================================================
    // Asset endpoints
    // ========================================================================

    /// Get asset details by CIP-14 fingerprint (`asset1...`).
    ///
    /// Returns policy_id, asset_name, total supply, and mint info.
    pub async fn get_asset_by_fingerprint(&self, fingerprint: &str) -> Result<AssetInfo, Error> {
        let url = format!("{BASE_URL}/asset?fingerprint={fingerprint}");
        self.get(&url).await
    }

    /// Get asset details by asset ID (policy_id + asset_name hex concatenated).
    pub async fn get_asset_by_id(&self, asset_id: &str) -> Result<AssetInfo, Error> {
        let url = format!("{BASE_URL}/asset?assetId={asset_id}");
        self.get(&url).await
    }

    /// Get assets by policy ID.
    pub async fn get_assets_by_policy(
        &self,
        policy_id: &str,
        page: u32,
        limit: u32,
    ) -> Result<AssetListResponse, Error> {
        let url = format!(
            "{BASE_URL}/asset/list/bypolicyid?policyId={policy_id}&pageNo={page}&limit={limit}"
        );
        self.get(&url).await
    }

    /// Get asset holders by policy ID.
    pub async fn get_holders_by_policy(
        &self,
        policy_id: &str,
        page: u32,
        limit: u32,
    ) -> Result<HolderListResponse, Error> {
        let url = format!(
            "{BASE_URL}/asset/holders/bypolicyid?policyId={policy_id}&pageNo={page}&limit={limit}"
        );
        self.get(&url).await
    }
}

// ============================================================================
// Response types
// ============================================================================

/// Asset information returned by the asset endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetInfo {
    pub policy_id: String,
    pub asset_name: String,
    /// Raw fingerprint bytes as hex (NOT bech32 — use `fingerprint_bech32()` for `asset1...` format)
    pub fingerprint: String,
    /// Concatenation of policy_id + asset_name hex
    pub asset_id: String,
    pub total_supply: String,
    pub tx_count: u64,
    pub minted_on: Option<String>,
}

/// Paginated list of assets under a policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetListResponse {
    pub assets: Vec<AssetInfo>,
    pub count: u64,
    pub page_no: u32,
    pub limit: u32,
}

/// Asset holder information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HolderInfo {
    pub address: String,
    pub policy_id: String,
    pub asset_name: String,
    pub fingerprint: String,
    pub asset_id: String,
    pub balance: String,
}

/// Paginated list of asset holders.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HolderListResponse {
    pub holders: Vec<HolderInfo>,
    pub count: u64,
    pub page_no: u32,
    pub limit: u32,
}
