pub mod koios_account_utxos;
pub mod koios_assets;
pub mod koios_evaluate;
mod koios_serde;
pub mod koios_transaction;
pub mod koios_utils;
pub mod koios_utxos;

use http_client::{HttpClient, HttpError};
use koios_account_utxos::TxRecord;
pub use koios_evaluate::KoiosRedeemerBudget;
use koios_serde::as_f64;
use koios_transaction::KoiosTransaction;
pub use koios_utxos::{KoiosAccountAsset, KoiosInlineDatum, KoiosUtxo, KoiosUtxoAsset, UtxoAmount};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};
use tracing::{error, info};
use worker_stack::worker::{self, Env, RouteContext};

const BASE_URL: &str = "https://api.koios.rest/api/v1";

/// Koios API base URL for a Cardano network. The **same API key works across
/// all environments** — only the host differs. Unknown/mainnet fall back to the
/// mainnet endpoint.
pub(crate) fn koios_base_url(network: &str) -> String {
    match network {
        "cardano:preprod" | "cardano:testnet" => "https://preprod.koios.rest/api/v1".to_string(),
        "cardano:preview" => "https://preview.koios.rest/api/v1".to_string(),
        _ => BASE_URL.to_string(),
    }
}

/// Koios caps a single response page at 1000 rows; paginated reads walk
/// `offset` in these increments until a short page signals the end.
const KOIOS_PAGE_LIMIT: u32 = 1000;

#[derive(Debug)]
pub enum KoiosError {
    Http(HttpError),
    KoiosResponse { status: u16, body: String },
    Worker(worker::Error),
}

impl fmt::Display for KoiosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KoiosError::Http(e) => write!(f, "HTTP error: {e}"),
            KoiosError::Worker(_) => write!(f, "Worker error"),
            KoiosError::KoiosResponse { status, body } => {
                write!(f, "Koios returned {status}: {body}")
            }
        }
    }
}

impl Error for KoiosError {}

impl From<worker::Error> for KoiosError {
    fn from(e: worker::Error) -> Self {
        KoiosError::Worker(e)
    }
}

impl From<KoiosError> for worker::Error {
    fn from(_: KoiosError) -> Self {
        worker::Error::BadEncoding
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct StakeAddressesRequest {
    #[serde(rename = "_stake_addresses")]
    pub stakes: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct AddressTxsRequest {
    #[serde(rename = "_addresses")]
    pub addresses: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct AccountUtxoRequest {
    #[serde(rename = "_stake_addresses")]
    pub stakes: Vec<String>,
    #[serde(rename = "_extended")]
    pub extended: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct TxInfoRequest {
    #[serde(rename = "_tx_hashes")]
    pub hashes: Vec<String>,
    #[serde(rename = "_scripts")]
    pub scripts: bool,
    #[serde(rename = "_assets")]
    pub assets: bool,
    #[serde(rename = "_inputs")]
    pub inputs: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct AssetInfoRequest {
    #[serde(rename = "_asset_list")]
    pub assets: Vec<(String, String)>,
}

#[derive(Serialize, Debug, Clone)]
pub struct AddressUtxosRequest {
    #[serde(rename = "_addresses")]
    pub addresses: Vec<String>,
    #[serde(rename = "_extended")]
    pub extended: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct CredentialUtxosRequest {
    /// Payment credentials in **hex** (28-byte key/script hash), not bech32.
    #[serde(rename = "_payment_credentials")]
    pub payment_credentials: Vec<String>,
    #[serde(rename = "_extended")]
    pub extended: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct AccountAssetsRequest {
    #[serde(rename = "_stake_addresses")]
    pub stakes: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxSummary {
    pub tx_hash: String,
    pub epoch_no: u32,
    pub block_height: u64,
    pub block_time: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KoiosStakeData {
    pub stake_address: String,
    pub addresses: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KoiosAssetInfo {
    pub policy_id: String,
    pub asset_name: String,
    pub asset_name_ascii: Option<String>,
    pub fingerprint: String,
    pub minting_tx_hash: String,
    #[serde(deserialize_with = "as_f64")]
    pub total_supply: f64,
    pub mint_cnt: u64,
    pub burn_cnt: u64,
    pub creation_time: u64,
    pub token_registry_metadata: Option<TokenMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub url: Option<String>,
    pub logo: Option<String>,
    pub name: Option<String>,
    pub ticker: Option<String>,
    pub decimals: Option<u32>,
    pub description: Option<String>,
}

/// Row from `GET /policy_asset_info` — the lightweight per-policy
/// asset listing (name + supply), used for "what lives under this
/// policy" discovery.
#[derive(Debug, Serialize, Deserialize)]
pub struct KoiosPolicyAssetInfo {
    /// Hex-encoded asset name; null/absent for the empty name.
    pub asset_name: Option<String>,
    /// Koios serialises large supplies as strings; wasm-safe-serde
    /// accepts both string and integer forms.
    #[serde(default, with = "wasm_safe_serde::u64_option")]
    pub total_supply: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyAssetMint {
    pub asset_name: String,
    pub mint_cnt: u64,
    pub burn_cnt: u64,
    pub creation_time: u64,
    pub minting_tx_hash: String,
}

// Kupo API structures
#[derive(Debug, Serialize, Deserialize)]
pub struct KupoTimestamp {
    pub slot_no: u64,
    pub header_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KupoAssetMatch {
    pub transaction_id: String,
    pub output_index: u32,
    pub address: String,
    pub value: KupoValue,
    pub datum_hash: Option<String>,
    pub script_hash: Option<String>,
    pub created_at: KupoTimestamp,
    pub spent_at: Option<KupoTimestamp>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KupoValue {
    pub coins: u64,
    #[serde(flatten)]
    pub assets: std::collections::HashMap<String, std::collections::HashMap<String, u64>>,
}

#[derive(Debug, Clone)]
pub struct KupoQueryOptions {
    pub order: Option<String>, // "most_recent_first" or "oldest_first"
    pub limit: Option<u32>,
    pub created_after: Option<u64>,  // slot number
    pub created_before: Option<u64>, // slot number
    pub spent: Option<bool>,         // filter by spent status
}

impl Default for KupoQueryOptions {
    fn default() -> Self {
        Self {
            order: Some("most_recent_first".to_string()),
            limit: None,
            created_after: None,
            created_before: None,
            spent: None,
        }
    }
}

impl KupoQueryOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_order(mut self, order: impl Into<String>) -> Self {
        self.order = Some(order.into());
        self
    }

    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn with_created_after(mut self, slot: u64) -> Self {
        self.created_after = Some(slot);
        self
    }

    pub fn with_created_before(mut self, slot: u64) -> Self {
        self.created_before = Some(slot);
        self
    }

    pub fn with_spent_filter(mut self, spent: bool) -> Self {
        self.spent = Some(spent);
        self
    }

    pub fn build_query_string(&self) -> String {
        let mut params = Vec::new();

        if let Some(ref order) = self.order {
            params.push(format!("order={order}"));
        }

        if let Some(limit) = self.limit {
            params.push(format!("limit={limit}"));
        }

        if let Some(created_after) = self.created_after {
            params.push(format!("created_after={created_after}"));
        }

        if let Some(created_before) = self.created_before {
            params.push(format!("created_before={created_before}"));
        }

        if let Some(spent) = self.spent {
            params.push(format!("spent={spent}"));
        }

        if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        }
    }
}

#[derive(Debug, Clone)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl fmt::Display for SortOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SortOrder::Asc => write!(f, "asc"),
            SortOrder::Desc => write!(f, "desc"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderBy {
    pub field: String,
    pub direction: SortOrder,
}

impl fmt::Display for OrderBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.field, self.direction)
    }
}

#[derive(Debug, Clone)]
pub enum FilterOperator {
    Eq,
    Gte,
    Lte,
    Gt,
    Lt,
}

impl fmt::Display for FilterOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilterOperator::Eq => write!(f, "eq"),
            FilterOperator::Gte => write!(f, "gte"),
            FilterOperator::Lte => write!(f, "lte"),
            FilterOperator::Gt => write!(f, "gt"),
            FilterOperator::Lt => write!(f, "lt"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Filter {
    pub field: String,
    pub operator: FilterOperator,
    pub value: String,
}

impl fmt::Display for Filter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}.{}", self.field, self.operator, self.value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KoiosLimits {
    pub limit: u32,
    pub offset: u32,
}

impl Default for KoiosLimits {
    fn default() -> Self {
        Self {
            limit: 1000,
            offset: 0,
        }
    }
}

impl KoiosLimits {
    pub fn only(limit: u32) -> Self {
        Self { limit, offset: 0 }
    }

    pub fn new(limit: u32, offset: Option<u32>) -> Self {
        Self {
            limit,
            offset: offset.unwrap_or(0),
        }
    }

    pub(crate) fn get_params(&self) -> Vec<String> {
        let mut params = vec![format!("limit={}", self.limit)];

        if self.offset != 0 {
            params.push(format!("offset={}", self.offset));
        }

        params
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueryOptions {
    pub limits: KoiosLimits,
    pub order: Option<OrderBy>,
    pub filters: Vec<Filter>,
    pub select: Option<Vec<String>>,
}

impl From<KoiosLimits> for QueryOptions {
    fn from(limits: KoiosLimits) -> Self {
        Self {
            limits,
            order: None,
            filters: vec![],
            select: None,
        }
    }
}

impl QueryOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limits(mut self, limits: KoiosLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_order(mut self, field: impl Into<String>, direction: SortOrder) -> Self {
        self.order = Some(OrderBy {
            field: field.into(),
            direction,
        });
        self
    }

    pub fn with_filter(
        mut self,
        field: impl Into<String>,
        operator: FilterOperator,
        value: impl Into<String>,
    ) -> Self {
        self.filters.push(Filter {
            field: field.into(),
            operator,
            value: value.into(),
        });
        self
    }

    pub fn with_select(mut self, fields: Vec<impl Into<String>>) -> Self {
        self.select = Some(fields.into_iter().map(|f| f.into()).collect());
        self
    }

    pub fn with_select_field(mut self, field: impl Into<String>) -> Self {
        match &mut self.select {
            Some(fields) => fields.push(field.into()),
            None => self.select = Some(vec![field.into()]),
        }
        self
    }

    pub fn build_query_string(&self) -> String {
        let mut params = Vec::new();

        // Add select fields first if specified
        if let Some(ref select_fields) = self.select {
            params.push(format!("select={}", select_fields.join(",")));
        }

        if let Some(ref order) = self.order {
            params.push(format!("order={order}"));
        }

        for filter in &self.filters {
            params.push(filter.to_string());
        }

        // Apply pagination limits last as order is important for Koios API
        params.extend(self.limits.get_params());

        format!("?{}", params.join("&"))
    }

    pub fn append_to_url(&self, base_url: &str) -> String {
        if self.is_empty() {
            return base_url.to_string();
        }

        let mut params = Vec::new();

        // Add select fields first if specified
        if let Some(ref select_fields) = self.select {
            params.push(format!("select={}", select_fields.join(",")));
        }

        if let Some(ref order) = self.order {
            params.push(format!("order={order}"));
        }

        for filter in &self.filters {
            params.push(filter.to_string());
        }

        // Apply pagination limits last as order is important for Koios API
        params.extend(self.limits.get_params());

        if params.is_empty() {
            return base_url.to_string();
        }

        let separator = if base_url.contains('?') { "&" } else { "?" };
        format!("{base_url}{separator}{}", params.join("&"))
    }

    pub fn is_empty(&self) -> bool {
        self.limits == KoiosLimits::default()
            && self.order.is_none()
            && self.filters.is_empty()
            && self.select.is_none()
    }
}

pub struct KoiosApi {
    client: HttpClient,
    base_url: String,
}

pub struct KupoApi {
    client: HttpClient,
    base_url: String,
}

impl KupoApi {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: HttpClient::new(),
            base_url: base_url.into(),
        }
    }

    pub fn for_env(env: &Env) -> worker::Result<Self> {
        let base_url = env.secret("KUPO_BASE_URL")?.to_string();
        Ok(Self::new(base_url))
    }

    /// Get asset matches for a specific policy ID pattern
    /// Pattern can be exact policy ID or policy_id.* for all assets under that policy
    pub async fn get_policy_matches(
        &self,
        policy_pattern: &str,
        options: Option<&KupoQueryOptions>,
    ) -> Result<Vec<KupoAssetMatch>, KoiosError> {
        let query_string = options
            .map(|opts| opts.build_query_string())
            .unwrap_or_default();

        let url = format!("{}/matches/{policy_pattern}{query_string}", self.base_url);

        info!("requesting data from: {}", url);

        match self.client.get::<Vec<KupoAssetMatch>>(&url).await {
            Ok(result) => Ok(result),
            Err(HttpError::Custom(msg)) if msg.starts_with("HTTP request failed with status:") => {
                // Extract status code from error message
                let status_str = msg.replace("HTTP request failed with status: ", "");
                let status = status_str.parse::<u16>().unwrap_or(500);
                error!("Kupo API error: {} {}", status, msg);
                Err(KoiosError::KoiosResponse { status, body: msg })
            }
            Err(e) => Err(KoiosError::Http(e)),
        }
    }

    /// Get recent asset mints for a policy in chronological order
    /// This solves the Koios limitation of identical creation_time values
    pub async fn get_recent_mints(
        &self,
        policy_id: &str,
        after_slot: Option<u64>,
        limit: Option<u32>,
    ) -> Result<Vec<KupoAssetMatch>, KoiosError> {
        let pattern = format!("{policy_id}.*");

        let mut options = KupoQueryOptions::new()
            .with_order("most_recent_first")
            .with_spent_filter(false); // Only unspent outputs (recent mints)

        if let Some(slot) = after_slot {
            options = options.with_created_after(slot);
        }

        if let Some(limit_val) = limit {
            options = options.with_limit(limit_val);
        }

        self.get_policy_matches(&pattern, Some(&options)).await
    }

    /// Get assets minted in a specific slot range
    pub async fn get_mints_in_slot_range(
        &self,
        policy_id: &str,
        start_slot: u64,
        end_slot: u64,
        limit: Option<u32>,
    ) -> Result<Vec<KupoAssetMatch>, KoiosError> {
        let pattern = format!("{policy_id}.*");

        let mut options = KupoQueryOptions::new()
            .with_order("most_recent_first")
            .with_created_after(start_slot)
            .with_created_before(end_slot)
            .with_spent_filter(false);

        if let Some(limit_val) = limit {
            options = options.with_limit(limit_val);
        }

        self.get_policy_matches(&pattern, Some(&options)).await
    }
}

impl Default for KoiosApi {
    fn default() -> Self {
        Self {
            client: HttpClient::new(),
            base_url: BASE_URL.to_string(),
        }
    }
}

impl KoiosApi {
    /// Build a client from a `RouteContext`, reading the optional
    /// `KOIOS_API_KEY` bearer token from the Cloudflare Secrets Store.
    pub async fn for_context(ctx: &RouteContext<()>) -> worker::Result<Self> {
        Self::for_env(&ctx.env).await
    }

    /// Build a mainnet client from the worker `Env`.
    ///
    /// Reads `KOIOS_API_KEY` via [`worker_utils::secrets::get_secret`]
    /// (Secrets Store first, then `env.secret` for local dev). When a
    /// non-empty key is present it authenticates with a bearer token
    /// (paid tier); otherwise it falls back to the keyless free tier.
    pub async fn for_env(env: &Env) -> worker::Result<Self> {
        Self::for_env_with_network(env, "cardano:mainnet").await
    }

    /// Build a client targeting a specific Cardano network. The **same
    /// `KOIOS_API_KEY` authenticates every environment** — only the host
    /// differs (mainnet → `api.koios.rest`, preprod/testnet →
    /// `preprod.koios.rest`, preview → `preview.koios.rest`).
    pub async fn for_env_with_network(env: &Env, network: &str) -> worker::Result<Self> {
        let client = match worker_utils::secrets::get_secret(env, "KOIOS_API_KEY").await {
            Ok(key) if !key.is_empty() => HttpClient::with_bearer_token(key),
            _ => HttpClient::new(),
        };
        Ok(Self {
            client,
            base_url: koios_base_url(network),
        })
    }

    pub async fn get_stake_addresses(
        &self,
        stake_address: &str,
    ) -> Result<Vec<KoiosStakeData>, KoiosError> {
        let url = format!("{}/account_addresses", self.base_url);
        self.post_json(
            &url,
            &StakeAddressesRequest {
                stakes: vec![stake_address.to_string()],
            },
        )
        .await
    }

    pub async fn get_stake_utxos(&self, stake_address: &str) -> Result<Vec<TxRecord>, KoiosError> {
        let url = format!("{}/account_utxos", self.base_url);
        self.post_json(
            &url,
            &AccountUtxoRequest {
                stakes: vec![stake_address.to_string()],
                extended: true,
            },
        )
        .await
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_transactions(
        &self,
        addresses: &[String],
    ) -> Result<Vec<TxSummary>, KoiosError> {
        let options = QueryOptions::new().with_order("block_height", SortOrder::Desc);
        self.get_transactions_with_options(addresses, Some(&options))
            .await
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_transactions_with_options(
        &self,
        addresses: &[String],
        options: Option<&QueryOptions>,
    ) -> Result<Vec<TxSummary>, KoiosError> {
        let url = format!("{}/address_txs", self.base_url);
        self.post_json_with_options(
            &url,
            &AddressTxsRequest {
                addresses: addresses.to_vec(),
            },
            options,
        )
        .await
    }

    pub async fn get_tx_details(
        &self,
        hashes: &[String],
    ) -> Result<Vec<KoiosTransaction>, KoiosError> {
        let url = format!("{}/tx_info?order=block_height.desc", self.base_url);
        self.post_json(
            &url,
            &TxInfoRequest {
                hashes: hashes.to_vec(),
                scripts: true,
                assets: true,
                inputs: true,
            },
        )
        .await
    }

    pub async fn get_policy_assets(
        &self,
        assets: &[(String, String)],
    ) -> Result<Vec<KoiosAssetInfo>, KoiosError> {
        let url = format!("{}/asset_info", self.base_url);
        self.post_json(
            &url,
            &AssetInfoRequest {
                assets: assets.to_vec(),
            },
        )
        .await
    }

    /// Extended UTxOs for one or more bech32 payment/base addresses
    /// (`POST /address_utxos`, `_extended=true`). Koios serves the whole
    /// batch in a single request; each row is tagged with its `address`.
    pub async fn get_address_utxos_batch(
        &self,
        addresses: &[String],
    ) -> Result<Vec<KoiosUtxo>, KoiosError> {
        let url = format!("{}/address_utxos", self.base_url);
        self.post_paginated(
            &url,
            &AddressUtxosRequest {
                addresses: addresses.to_vec(),
                extended: true,
            },
        )
        .await
    }

    /// Extended UTxOs for a single address (convenience over
    /// [`Self::get_address_utxos_batch`]).
    pub async fn get_address_utxos(&self, address: &str) -> Result<Vec<KoiosUtxo>, KoiosError> {
        self.get_address_utxos_batch(&[address.to_string()]).await
    }

    /// Extended UTxOs for one or more **hex** payment credentials
    /// (`POST /credential_utxos`, `_extended=true`). One request covers the
    /// whole batch; rows are tagged with their owning `address`.
    pub async fn get_credential_utxos_batch(
        &self,
        credentials: &[String],
    ) -> Result<Vec<KoiosUtxo>, KoiosError> {
        let url = format!("{}/credential_utxos", self.base_url);
        self.post_paginated(
            &url,
            &CredentialUtxosRequest {
                payment_credentials: credentials.to_vec(),
                extended: true,
            },
        )
        .await
    }

    /// Extended UTxOs for a single hex payment credential (convenience over
    /// [`Self::get_credential_utxos_batch`]).
    pub async fn get_credential_utxos(
        &self,
        credential: &str,
    ) -> Result<Vec<KoiosUtxo>, KoiosError> {
        self.get_credential_utxos_batch(&[credential.to_string()])
            .await
    }

    /// Native-asset holdings for one or more stake addresses
    /// (`POST /account_assets`). One request covers the whole batch; each
    /// row carries its `stake_address`.
    pub async fn get_account_assets_batch(
        &self,
        stake_addresses: &[String],
    ) -> Result<Vec<KoiosAccountAsset>, KoiosError> {
        let url = format!("{}/account_assets", self.base_url);
        self.post_paginated(
            &url,
            &AccountAssetsRequest {
                stakes: stake_addresses.to_vec(),
            },
        )
        .await
    }

    /// Native-asset holdings for a single stake address (convenience over
    /// [`Self::get_account_assets_batch`]).
    pub async fn get_account_assets(
        &self,
        stake_address: &str,
    ) -> Result<Vec<KoiosAccountAsset>, KoiosError> {
        self.get_account_assets_batch(&[stake_address.to_string()])
            .await
    }

    /// All assets minted under a policy with name + supply
    /// (`GET /policy_asset_info`). Returns up to Koios's page cap
    /// (1000); fungible-token callers wanting the primary asset sort
    /// by supply and take the head.
    pub async fn get_policy_asset_info(
        &self,
        policy_id: &str,
    ) -> Result<Vec<KoiosPolicyAssetInfo>, KoiosError> {
        let url = format!("{}/policy_asset_info?_asset_policy={policy_id}", self.base_url);
        self.get_json(&url).await
    }

    pub async fn get_policy_asset_mints(
        &self,
        policy_id: &str,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<PolicyAssetMint>, KoiosError> {
        let url = format!("{}/policy_asset_mints?_asset_policy={policy_id}", self.base_url);
        self.get_json_with_options(&url, options).await
    }

    /// Get policy asset mints as raw JSON to explore available fields
    pub async fn get_policy_asset_mints_raw(
        &self,
        policy_id: &str,
        options: Option<&QueryOptions>,
    ) -> Result<serde_json::Value, KoiosError> {
        let url = format!("{}/policy_asset_mints?_asset_policy={policy_id}", self.base_url);
        self.get_json_with_options(&url, options).await
    }

    pub async fn get_recent_policy_asset_mints(
        &self,
        policy_id: &str,
        after_block_time: Option<u64>,
    ) -> Result<Vec<PolicyAssetMint>, KoiosError> {
        let mut options = QueryOptions::new()
            .with_order("creation_time", SortOrder::Desc)
            .with_limits(KoiosLimits::only(50));

        if let Some(block_time) = after_block_time {
            options =
                options.with_filter("creation_time", FilterOperator::Gte, block_time.to_string());
        }

        self.get_policy_asset_mints(policy_id, Some(&options)).await
    }

    pub async fn get_json<R: DeserializeOwned>(&self, url: &str) -> Result<R, KoiosError> {
        self.get_json_with_options(url, None).await
    }

    pub async fn get_json_with_options<R: DeserializeOwned>(
        &self,
        url: &str,
        options: Option<&QueryOptions>,
    ) -> Result<R, KoiosError> {
        let final_url = match options {
            Some(opts) => opts.append_to_url(url),
            None => url.to_string(),
        };

        info!("requesting data from: {}", final_url);

        match self.client.get::<R>(&final_url).await {
            Ok(result) => Ok(result),
            Err(HttpError::Custom(msg)) if msg.starts_with("HTTP request failed with status:") => {
                // Extract status code from error message
                let status_str = msg.replace("HTTP request failed with status: ", "");
                let status = status_str.parse::<u16>().unwrap_or(500);
                error!("Koios API error: {} {}", status, msg);
                Err(KoiosError::KoiosResponse { status, body: msg })
            }
            Err(e) => Err(KoiosError::Http(e)),
        }
    }

    pub async fn post_json<T: Serialize + std::fmt::Debug, R: DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<R, KoiosError> {
        self.post_json_with_options(url, body, None).await
    }

    pub async fn post_json_with_options<T: Serialize + std::fmt::Debug, R: DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
        options: Option<&QueryOptions>,
    ) -> Result<R, KoiosError> {
        let final_url = match options {
            Some(opts) => opts.append_to_url(url),
            None => url.to_string(),
        };

        info!(
            "requesting data from: {}, data: {}",
            final_url,
            serde_json::to_string(body).unwrap()
        );

        match self.client.post::<T, R>(&final_url, body).await {
            Ok(result) => Ok(result),
            Err(HttpError::Custom(msg)) if msg.starts_with("HTTP request failed with status:") => {
                // Extract status code from error message
                let status_str = msg.replace("HTTP request failed with status: ", "");
                let status = status_str.parse::<u16>().unwrap_or(500);
                error!("Koios API error: {} {}", status, msg);
                Err(KoiosError::KoiosResponse { status, body: msg })
            }
            Err(e) => Err(KoiosError::Http(e)),
        }
    }

    /// POST a request that returns a row array, walking Koios's offset/limit
    /// pagination until a short page is returned. Koios caps a page at
    /// [`KOIOS_PAGE_LIMIT`] rows, so any endpoint that can return more than
    /// that (e.g. UTxOs for a busy credential/address, or a whale's asset
    /// list) must page to stay exhaustive — matching Maestro's cursor loop.
    async fn post_paginated<T, R>(&self, url: &str, body: &T) -> Result<Vec<R>, KoiosError>
    where
        T: Serialize + std::fmt::Debug,
        R: DeserializeOwned,
    {
        let mut all = Vec::new();
        let mut offset = 0u32;

        loop {
            let options = QueryOptions::from(KoiosLimits::new(KOIOS_PAGE_LIMIT, Some(offset)));
            let page: Vec<R> = self
                .post_json_with_options(url, body, Some(&options))
                .await?;
            let page_len = page.len() as u32;
            all.extend(page);

            if page_len < KOIOS_PAGE_LIMIT {
                break;
            }
            offset += KOIOS_PAGE_LIMIT;
        }

        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::assertions_on_constants)]

    //! Koios API Test Rate Limiting
    //!
    //! These tests make real Koios API calls and must respect rate limits:
    //! - Koios allows limited requests per second for free tier
    //! - Tests include delays to prevent 429 rate limit errors
    //! - Run tests sequentially to avoid conflicts: `cargo test --lib -- --test-threads=1`

    use std::time::Duration;
    use test_utils::test_case;
    use tokio::time::sleep;
    use tracing::Level;

    use crate::koios_account_utxos::TxRecord;
    use crate::koios_transaction::KoiosTransaction;
    use crate::koios_utils::trace_asset_movements;

    use super::*;

    // Rate limiting helper for tests
    async fn rate_limit_delay() {
        // Koios free tier has rate limits, wait 1 second between requests
        sleep(Duration::from_millis(1000)).await;
    }

    #[test]
    fn test_query_options_empty() {
        let options = QueryOptions::new();
        assert!(options.is_empty());
        assert_eq!(options.build_query_string(), "?limit=1000");
        assert_eq!(
            options.append_to_url("https://api.example.com/test"),
            "https://api.example.com/test"
        );
    }

    #[test]
    fn test_query_options_with_pagination() {
        let options = QueryOptions::new().with_limits(KoiosLimits::new(50, Some(100)));

        assert!(!options.is_empty());
        assert_eq!(options.build_query_string(), "?limit=50&offset=100");
        assert_eq!(
            options.append_to_url("https://api.example.com/test"),
            "https://api.example.com/test?limit=50&offset=100"
        );
    }

    #[test]
    fn test_query_options_with_ordering() {
        let options = QueryOptions::new()
            .with_order("block_height", SortOrder::Desc)
            .with_limits(KoiosLimits::new(25, None));

        assert_eq!(
            options.build_query_string(),
            "?order=block_height.desc&limit=25"
        );
    }

    #[test]
    fn test_query_options_with_filters() {
        let options = QueryOptions::new()
            .with_filter("creation_time", FilterOperator::Gte, "1640000000")
            .with_filter("amount", FilterOperator::Lt, "1000000");

        assert_eq!(
            options.build_query_string(),
            "?creation_time=gte.1640000000&amount=lt.1000000&limit=1000"
        );
    }

    #[test]
    fn test_query_options_append_to_existing_url() {
        let options = QueryOptions::new()
            .with_limits(KoiosLimits::new(10, None))
            .with_order("timestamp", SortOrder::Asc);

        // Test appending to URL that already has query parameters
        assert_eq!(
            options.append_to_url("https://api.example.com/test?existing=param"),
            "https://api.example.com/test?existing=param&order=timestamp.asc&limit=10"
        );

        // Test appending to URL without query parameters
        assert_eq!(
            options.append_to_url("https://api.example.com/test"),
            "https://api.example.com/test?order=timestamp.asc&limit=10"
        );
    }

    #[test]
    fn test_sort_order_display() {
        assert_eq!(SortOrder::Asc.to_string(), "asc");
        assert_eq!(SortOrder::Desc.to_string(), "desc");
    }

    #[test]
    fn test_filter_operator_display() {
        assert_eq!(FilterOperator::Eq.to_string(), "eq");
        assert_eq!(FilterOperator::Gte.to_string(), "gte");
        assert_eq!(FilterOperator::Lte.to_string(), "lte");
        assert_eq!(FilterOperator::Gt.to_string(), "gt");
        assert_eq!(FilterOperator::Lt.to_string(), "lt");
    }

    #[test]
    fn test_query_options_with_select_fields() {
        let options = QueryOptions::new()
            .with_select(vec!["asset_name", "creation_time", "minting_tx_hash"])
            .with_limits(KoiosLimits::new(10, None));

        assert_eq!(
            options.build_query_string(),
            "?select=asset_name,creation_time,minting_tx_hash&limit=10"
        );
    }

    #[test]
    fn test_query_options_with_additional_select_field() {
        let options = QueryOptions::new()
            .with_select_field("asset_name")
            .with_select_field("tx_timestamp")
            .with_order("asset_name", SortOrder::Asc)
            .with_limits(KoiosLimits::new(5, None));

        assert_eq!(
            options.build_query_string(),
            "?select=asset_name,tx_timestamp&order=asset_name.asc&limit=5"
        );
    }

    #[tokio::test]
    #[ignore = "hits live Koios API, flaky due to 429 rate limits"]
    async fn test_explore_policy_asset_mints_fields() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        // Add delay to respect Koios rate limits
        rate_limit_delay().await;

        let koios = KoiosApi::default();
        let policy_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

        // Now test with all available fields and try to get transaction details
        let options = QueryOptions::new()
            .with_select(vec![
                "asset_name",
                "creation_time",
                "minting_tx_hash",
                "fingerprint",
                "mint_cnt",
                "total_supply",
            ])
            .with_order("creation_time", SortOrder::Desc)
            .with_limits(KoiosLimits::new(3, None));

        match koios
            .get_policy_asset_mints_raw(policy_id, Some(&options))
            .await
        {
            Ok(raw_data) => {
                println!(
                    "Raw policy asset mints response: {}",
                    serde_json::to_string_pretty(&raw_data).unwrap()
                );
            }
            Err(err) => {
                println!("Policy asset mints exploration failed (this might be expected): {err:?}");
                // This might fail if some fields don't exist, which is fine for exploration
            }
        }
    }

    #[test]
    fn test_deserialize_txs() {
        match serde_json::from_str::<Vec<TxSummary>>(test_case!("address_txs.json")) {
            Ok(_) => assert!(true, "decoded successfully"),
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_transaction() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        match serde_json::from_str::<Vec<KoiosTransaction>>(test_case!("sample_tx_details.json")) {
            Ok(_) => {
                assert!(true, "decoded successfully");
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_utxos() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        match serde_json::from_str::<Vec<TxRecord>>(test_case!("account_utxos.json")) {
            Ok(_) => {
                assert!(true, "decoded successfully");
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_asset_movements() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        match serde_json::from_str::<Vec<KoiosTransaction>>(test_case!("sample_tx_details.json")) {
            Ok(tx) => {
                let movements = trace_asset_movements(tx.first().unwrap());
                assert_eq!(movements.len(), 1);

                let jpg_movement = movements.first().unwrap();
                assert_eq!(
                    jpg_movement.policy_id,
                    "681b5d0383ac3b457e1bcc453223c90ccef26b234328f45fa10fd276"
                );
                assert_eq!(jpg_movement.asset_name, "4a5047");
                assert_eq!(jpg_movement.quantity, 1000000000.0);
                assert_eq!(jpg_movement.from_address, "addr1q88nlawx6kkrpxkzuvreak9tq6y3chdfu29uhqv0yhe436xx0t0gdpe5aullxhvze42uhkf90zpm907jydk8g6x4z9sqzt0w5s");
                assert_eq!(jpg_movement.to_address, "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdkrxp0e2m4utglc7hmzkuta3e2td72cdjq9m9xlfn6rz8vq86l65l");
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }
    #[test]
    fn test_deserialize_transaction_multi() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        match serde_json::from_str::<Vec<KoiosTransaction>>(test_case!(
            "sample_tx_details_multi.json"
        )) {
            Ok(_) => {
                assert!(true, "decoded successfully");
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[tokio::test]
    #[ignore = "hits live Koios API, flaky due to 429 rate limits"]
    async fn test_get_stake_addresses() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        // Add delay to respect Koios rate limits
        rate_limit_delay().await;

        let stake_address: &str = "stake1u8pnqhu4d67950u0ta3tw97cu49kl9vxeqzajn05eap3rkqlm34ns";
        let koios = KoiosApi::default();

        match koios.get_stake_addresses(stake_address).await {
            Ok(addresses) => {
                assert!(!addresses.is_empty(), "Should fetch at least one address");
            }
            Err(err) => {
                println!("encountered fetching txs error: {err:?}");
                panic!("failed txfetch");
            }
        }
    }

    #[tokio::test]
    #[ignore = "hits live Koios API, flaky due to 429 rate limits"]
    async fn test_get_stake_utxos() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        // Add delay to respect Koios rate limits
        rate_limit_delay().await;

        let stake_address: &str = "stake1u8pnqhu4d67950u0ta3tw97cu49kl9vxeqzajn05eap3rkqlm34ns";
        let koios = KoiosApi::default();

        match koios.get_stake_utxos(stake_address).await {
            Ok(records) => {
                assert!(!records.is_empty(), "Should fetch at least one address");
            }
            Err(err) => {
                println!("encountered fetching txs error: {err:?}");
                panic!("failed txfetch");
            }
        }
    }

    #[tokio::test]
    #[ignore = "hits live Koios API, flaky due to 429 rate limits"]
    async fn test_tx_info_request() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        // Add delay to respect Koios rate limits
        rate_limit_delay().await;

        let shield_addresses = vec!["addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdkrxp0e2m4utglc7hmzkuta3e2td72cdjq9m9xlfn6rz8vq86l65l".to_string()];
        let koios = KoiosApi::default();

        match koios.get_transactions(&shield_addresses).await {
            Ok(txs) => {
                assert!(!txs.is_empty(), "Should fetch at least one transaction");
            }
            Err(err) => {
                println!("encountered fetching txs error: {err:?}");
                panic!("failed txfetch");
            }
        }
    }

    #[tokio::test]
    #[ignore = "hits live Koios API, flaky due to 429 rate limits"]
    async fn test_tx_details_request() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        // Add delay to respect Koios rate limits
        rate_limit_delay().await;

        let koios = KoiosApi::default();

        match koios
            .get_tx_details(&Vec::from([
                "b44c5ba55ec165f1bfb9b670d981193952add6c44865b3f6bedd59156aea1187".to_string(),
            ]))
            .await
        {
            Ok(txs) => {
                assert!(!txs.is_empty(), "Should fetch at least one transaction");
            }
            Err(err) => {
                println!("encountered fetching txs error: {err:?}");
                panic!("failed txfetch");
            }
        }
    }

    #[ignore = "hits live Koios API, flaky due to 429 rate limits"]
    #[tokio::test]
    async fn test_tx_details_request_multi() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        let hashes: Vec<_> = Vec::from([
            "71d5ff4d509b18caf49bd713c1125093cc1300e84daafe141dec2af4b9893eb9",
            "177ce384facd84d26096b0e1d042d8ec5414670adc96716ac6e4990702c1b1cb",
            "6597e543e7a3105364b9c9974853a4d5f0f496afc607958e77f479275c08fd27",
            "b65e59b5f05fa37820befec9d8b6c48d14bbf5a92ad4e202a61a78bd31c383b0",
            "881eb87f6565d105ccf4924ea8f531c9bc7f5ccede1b2e359e1b7d2c34709c8f",
            "1298c9fa9084063d6b041ddbe0d1c4e038c185cf54ff91fcf96df3e54e7abccd",
            "6ef903a421dece9537ea0ae7dc17f5a1ce1b3ebfde9c74c6eead798a2be4e435",
            "5d63758a56797efaf7dfbaa68f273386986c596c8913f05e272fa4e523cfaf5b",
            "acaddb61bf5cd5bae9a8c19751da33335c20a9ad55bf13d46599332601352c7c",
            "2c2e0d86310e88e5944cdde4a5f296203d4b31b22ae28aecdad84c88095b7cea",
            "47906775396ba729caf99bb91c725aa50df6e2e848068fa5ecdadf6a7e2558db",
            "1811682d0962db602a66923b8f363e54f451370e098f59fb24d12753072e9e59",
            "69a619f9bd8ea3ff35f9921842b35cd6c44cd52e169e285d6a11b97d884cb907",
            "df32cf1dddd1b9ecbeeabbf2094b3579ae439815477efcb65673283b61d804a1",
            "8e519756ba31857acf929558ad3245152da52b8b0139890195cdf00523fd1764",
            "256f224727d7cb3a1dfab0dc73cfc3fc11c757391ce13b03d391554564cac175",
            "43d4b2307849b2b685a16d29cdf74dcda6a28433c80f88a1eee9189fdac20af9",
            "ba742975248fd4e590574d48d4d5b747ea8067b7ca25185fa9179dd35503d386",
            "e28900468f9cf2842300665db846ab1871c565b9a586f4503f43838c7d633727",
            "67d77e955779752524ea651269c370dc6c55437a5ab351aac97d7ca3521173e8",
            "8bf99ffab88df7ac12bc59129732c5cae10abfbb0ca2e75700c577aaf2dfd128",
            "5d132e56c3a66072ccdaaa7e8be18df45a0bb59267a62bc384cb74702eecfeda",
            "8f36ef1ec3cddfd19c9679a83c1fff17d27fbee6b57b6d065c99aa16bf34bef2",
            "84aa6d14845f0e80f38eb2184ec821080cfcd384b30d2d86adf48c7b336aaacc",
            "b44c5ba55ec165f1bfb9b670d981193952add6c44865b3f6bedd59156aea1187",
        ])
        .iter()
        .map(|s| (*s).to_string())
        .collect();

        let koios = KoiosApi::default();

        match koios.get_tx_details(&hashes).await {
            Ok(txs) => {
                assert_eq!(txs.len(), hashes.len());
            }
            Err(err) => {
                println!("encountered fetching txs error: {err:?}");
                panic!("failed txfetch");
            }
        }
    }

    #[tokio::test]
    #[ignore = "hits live Koios API, flaky due to 429 rate limits"]
    async fn test_get_policy_details() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        // Add delay to respect Koios rate limits
        rate_limit_delay().await;

        let koios = KoiosApi::default();
        let assets: Vec<(String, String)> = Vec::from([
            (
                "ac015c38917f306a84748c2d646bed90bdd64421c592163e60702d73".into(),
                "5453555255".into(),
            ),
            (
                "067cac6082f8661b6e14909b40590120bf0bf02c21f5d07ee03d0e02".into(),
                "534e654c".into(),
            ),
            (
                "3afa6ffa22caa93b78182cf3da6bbd28cf4964f92d17da3d9e44a1ae".into(),
                "4b52414b454e".into(),
            ),
        ]);

        match koios.get_policy_assets(&assets).await {
            Ok(results) => {
                assert_eq!(results.len(), assets.len());
            }
            Err(err) => {
                println!("encountered fetching txs error: {err:?}");
                panic!("failed txfetch");
            }
        }
    }

    #[tokio::test]
    #[ignore = "hits live Koios API, flaky due to 429 rate limits"]
    async fn test_get_policy_asset_mints() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        // Add delay to respect Koios rate limits
        rate_limit_delay().await;

        let koios = KoiosApi::default();
        // Using a well-known policy ID that should have mint data
        let policy_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

        match koios
            .get_policy_asset_mints(policy_id, Some(&KoiosLimits::only(100).into()))
            .await
        {
            Ok(mints) => {
                assert_eq!(mints.len(), 100);
            }
            Err(err) => {
                panic!("failed to find assets for policy_id {policy_id}: {err:?}");
            }
        }
    }

    #[tokio::test]
    #[ignore = "hits live Koios API, flaky due to 429 rate limits"]
    async fn test_recent_get_policy_asset_mints() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        // Add delay to respect Koios rate limits
        rate_limit_delay().await;

        let koios = KoiosApi::default();
        // Using a well-known policy ID that should have mint data
        let policy_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

        match koios.get_recent_policy_asset_mints(policy_id, None).await {
            Ok(mints) => {
                // Just verify we can parse the response without errors
                // The actual data depends on the policy having mints
                println!("Found {} mint records: {:?}", mints.len(), mints);
            }
            Err(err) => {
                panic!("failed to find assets for policy_id {policy_id}: {err:?}");
            }
        }
    }
}
