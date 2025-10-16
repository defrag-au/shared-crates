#![allow(clippy::match_like_matches_macro)]

use async_stream::stream;
use cardano_assets::{
    Asset, AssetMetadata, AssetMetadata68, AssetWithId, MetadataKind, NftPurpose,
};
use chrono::Utc;
use futures_core::stream::Stream;
use http_client::HttpClient;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::{error::Error, fmt};
use tracing::warn;

mod test;

pub type BlockfrostAsset = serde_json::Map<String, Value>;

const BASE_URL: &str = "mainnet.gomaestro-api.org/v1";

#[derive(Debug)]
pub enum MaestroError {
    NoMetadata,
    Http(http_client::HttpError),
    /// Rate limit exceeded (429 response). Contains optional retry-after seconds from API
    RateLimit {
        retry_after: Option<u64>,
    },
    Deserialization(String),
    Unknown,
}

impl Default for MaestroError {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Error for MaestroError {}

impl fmt::Display for MaestroError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NoMetadata => write!(f, "Maestro error - no metadata"),
            Self::Http(err) => write!(f, "Maestro API error: {err:?}"),
            Self::RateLimit { retry_after } => match retry_after {
                Some(seconds) => write!(
                    f,
                    "Maestro rate limit exceeded, retry after {seconds} seconds"
                ),
                None => write!(f, "Maestro rate limit exceeded"),
            },
            Self::Deserialization(input) => write!(f, "Maestro deserialization failure: {input}"),
            Self::Unknown => write!(f, "Unknown Maestro error"),
        }
    }
}

impl From<http_client::HttpError> for MaestroError {
    fn from(value: http_client::HttpError) -> Self {
        Self::Http(value)
    }
}

impl From<MaestroError> for worker::Error {
    fn from(value: MaestroError) -> Self {
        worker::Error::RustError(value.to_string())
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct AssetStandards {
    pub cip25_metadata: Option<AssetMetadata>,
    pub cip68_metadata: Option<AssetMetadata68>,
}

impl TryFrom<AssetStandards> for Asset {
    type Error = MaestroError;
    fn try_from(value: AssetStandards) -> Result<Self, Self::Error> {
        tracing::debug!("TryFrom<AssetStandards> for Asset:");
        tracing::debug!(
            "  cip68_metadata present: {}",
            value.cip68_metadata.is_some()
        );
        tracing::debug!(
            "  cip25_metadata present: {}",
            value.cip25_metadata.is_some()
        );

        if let Some(ref cip25) = value.cip25_metadata {
            tracing::debug!("  cip25_metadata content: {:#?}", cip25);
        }

        match (value.cip68_metadata, value.cip25_metadata) {
            (Some(cip68), _) => {
                tracing::debug!("  Using cip68 metadata");
                Ok(Asset::from(cip68.metadata))
            }
            (_, Some(metadata)) => {
                tracing::debug!("  Using cip25 metadata");
                let asset = Asset::from(metadata);
                tracing::debug!(
                    "  Converted to asset: name='{}', image='{}', traits={:#?}",
                    asset.name,
                    asset.image,
                    asset.traits
                );
                Ok(asset)
            }
            (_, _) => {
                tracing::debug!("  No metadata found!");
                Err(MaestroError::NoMetadata)
            }
        }
    }
}

impl From<AssetStandards> for MetadataKind {
    fn from(value: AssetStandards) -> Self {
        match (value.get_nft_purpose(), value.cip25_metadata) {
            (Some(purpose), _) => Self::Cip68(purpose),
            (None, Some(_)) => Self::Cip25,
            _ => Self::Unknown,
        }
    }
}

impl AssetStandards {
    #[must_use]
    pub fn get_nft_purpose(&self) -> Option<NftPurpose> {
        match (&self.cip25_metadata, &self.cip68_metadata) {
            (Some(_), _) => None,
            (_, Some(cip68)) => Some(cip68.purpose.clone()),
            _ => None,
        }
    }

    #[must_use]
    pub fn should_import(&self) -> bool {
        match (&self.cip25_metadata, &self.cip68_metadata) {
            (Some(_), _) => true,
            (
                _,
                Some(AssetMetadata68 {
                    purpose: NftPurpose::UserNft,
                    ..
                }),
            ) => true,
            _ => false,
        }
    }
}

#[derive(Deserialize, Debug)]
struct AssetResult {
    asset_name: String,
    asset_standards: AssetStandards,
}

#[derive(Deserialize, Debug)]
struct AssetAccountsResponse {
    data: Vec<AccountQuantity>,
    next_cursor: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct AccountQuantity {
    pub account: String,
    pub amount: u32,
}

#[derive(Deserialize, Debug)]
struct PolicyAccountsResponse {
    data: Vec<PolicyAssetOwner>,
    next_cursor: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct PolicyAssetOwner {
    pub account: String,
    pub assets: Vec<PolicyAsset>,
}

#[derive(Deserialize, Debug)]
pub struct PolicyAsset {
    pub name: String,
    pub amount: u32,
}

#[derive(Deserialize, Debug)]
struct PolicyAssetsResponse {
    data: Vec<AssetResult>,
    next_cursor: Option<String>,
}

impl PolicyAssetsResponse {
    pub fn get_importable_nfts(&self) -> Vec<&AssetResult> {
        self.data
            .iter()
            .filter(|r| r.asset_standards.should_import())
            .collect()
    }
}

#[derive(Deserialize, Debug)]
struct AssetInfoResponse {
    data: DetailedAssetInfo,
}

#[derive(Deserialize, Debug)]
pub struct DetailedAssetInfo {
    pub asset_name: String,
    #[serde(deserialize_with = "deserialize_u64_string")]
    pub total_supply: u64,
    pub first_mint_tx: MintTx,
    pub asset_standards: AssetStandards,
}

impl TryFrom<DetailedAssetInfo> for Asset {
    type Error = MaestroError;

    fn try_from(value: DetailedAssetInfo) -> Result<Self, Self::Error> {
        Asset::try_from(value.asset_standards)
    }
}

#[derive(Deserialize, Debug)]
pub struct MintTx {
    pub tx_hash: String,
    pub slot: u64,
    #[serde(with = "maestro_date_format")]
    pub timestamp: chrono::DateTime<Utc>,
    pub amount: String,
}

// Transaction data structures for tx-classifier
#[derive(Deserialize, Debug)]
struct TransactionResponse {
    data: TransactionDetails,
}

#[derive(Deserialize, Debug)]
struct TransactionCborResponse {
    data: String, // Hex-encoded CBOR
}

#[derive(Deserialize, Debug)]
pub struct TransactionDetails {
    pub tx_hash: String,
    pub block_height: u64,
    pub slot: u64,
    #[serde(with = "maestro_date_format")]
    pub timestamp: chrono::DateTime<Utc>,
    pub fee: u64,
    pub size: u32,
    pub metadata: Option<serde_json::Value>,
    pub scripts: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct TransactionUtxosResponse {
    data: TransactionUtxos,
}

#[derive(Deserialize, Debug)]
pub struct TransactionUtxos {
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
}

#[derive(Deserialize, Debug)]
pub struct TransactionInput {
    pub address: String,
    pub tx_hash: String,
    pub output_index: u32,
    #[serde(deserialize_with = "deserialize_u64_string")]
    pub amount: u64,
    pub assets: Vec<AssetAmount>,
}

#[derive(Deserialize, Debug)]
pub struct TransactionOutput {
    pub address: String,
    #[serde(deserialize_with = "deserialize_u64_string")]
    pub amount: u64,
    pub assets: Vec<AssetAmount>,
    pub datum_hash: Option<String>,
    pub inline_datum: Option<serde_json::Value>,
    pub script_ref: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct AssetAmount {
    pub unit: String, // This is policy_id + asset_name
    #[serde(deserialize_with = "deserialize_u64_string")]
    pub quantity: u64,
}

// Address decode structures for stake key resolution
#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct AddressDecodeResponse {
    bech32: String,
    hex: String,
    network: String,
    payment_cred: PaymentCredential,
    staking_cred: Option<StakingCredential>,
}

// Account assets structures for user holdings
#[derive(Deserialize, Debug)]
#[allow(unused)]
struct AccountAssetsResponse {
    data: Vec<AssetHolding>,
    last_updated: BlockInfo,
    next_cursor: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct AssetHolding {
    pub unit: String, // This is policy_id + asset_name
    pub amount: u64,  // Try without string deserialization first
}

#[derive(Deserialize, Debug)]
pub struct BlockInfo {
    pub timestamp: String,
    pub block_hash: String,
    pub block_slot: u64,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct PaymentCredential {
    kind: String,
    bech32: String,
    hex: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct StakingCredential {
    kind: String,
    bech32: String,
    reward_address: Option<String>,
    hex: String,
    pointer: Option<serde_json::Value>,
}

pub enum EpochTarget {
    Current,
    Specific(u32),
}

#[derive(Deserialize, Debug)]
struct EpochResponse {
    data: EpochDetails,
}

#[derive(Deserialize, Debug)]
pub struct EpochDetails {
    pub epoch_no: u32,
    #[serde(deserialize_with = "deserialize_u64_string")]
    pub fees: u64,
    pub tx_count: u32,
    pub blk_count: u32,
    pub start_time: u32,
}

// Comprehensive transaction data structures (transactions feature)
#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug)]
pub struct CompleteTransactionResponse {
    pub data: CompleteTransactionDetails,
}

#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug)]
pub struct CompleteTransactionDetails {
    pub tx_hash: String,
    pub block_hash: String,
    pub block_tx_index: u32,
    pub block_height: u64,
    pub block_timestamp: u64,
    pub block_absolute_slot: u64,
    pub block_epoch: u32,
    pub inputs: Vec<CompleteTransactionInput>,
    pub outputs: Vec<CompleteTransactionOutput>,
    pub reference_inputs: Vec<CompleteTransactionInput>,
    pub collateral_inputs: Vec<CompleteTransactionInput>,
    pub collateral_return: Option<CompleteTransactionOutput>,
    pub mint: Vec<serde_json::Value>,
    pub invalid_before: Option<u64>,
    pub invalid_hereafter: Option<u64>,
    pub fee: u64,
    pub deposit: u64,
    pub certificates: serde_json::Value,
    pub withdrawals: Vec<serde_json::Value>,
    pub additional_signers: Vec<String>,
    pub scripts_executed: Vec<ScriptExecuted>,
    pub scripts_successful: bool,
    pub redeemers: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
    pub size: u32,
}

#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug)]
pub struct CompleteTransactionInput {
    pub tx_hash: String,
    pub index: u32,
    pub assets: Vec<TransactionAsset>,
    pub address: String,
    pub datum: Option<serde_json::Value>,
    pub reference_script: Option<serde_json::Value>,
}

#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug)]
pub struct CompleteTransactionOutput {
    pub tx_hash: String,
    pub index: u32,
    pub assets: Vec<TransactionAsset>,
    pub address: String,
    pub datum: Option<serde_json::Value>,
    pub reference_script: Option<serde_json::Value>,
}

#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug)]
pub struct TransactionAsset {
    pub unit: String,
    pub amount: u64,
}

#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug)]
pub struct ScriptExecuted {
    pub hash: String,
    #[serde(rename = "type")]
    pub script_type: String,
    pub bytes: String,
    pub json: Option<serde_json::Value>,
}

pub struct MaestroApi {
    client: HttpClient,
}

impl MaestroApi {
    /// Create a new MaestroApi instance with the provided API key
    pub fn new(api_key: String) -> Self {
        Self {
            client: HttpClient::new().with_header("api-key", &api_key),
        }
    }

    pub async fn for_env(env: &worker::Env) -> worker::Result<Self> {
        let api_key = worker_utils::secrets::get_secret(env, "MAESTRO_API_KEY").await?;
        Ok(Self {
            client: HttpClient::new().with_header("api-key", &api_key),
        })
    }

    pub async fn get_epoch(&self, target: EpochTarget) -> Result<EpochDetails, MaestroError> {
        let url = match target {
            EpochTarget::Current => format!("https://{BASE_URL}/epoch/current"),
            EpochTarget::Specific(epoch) => format!("https://{BASE_URL}/epoch/{epoch}"),
        };

        let response: EpochResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get transaction details by hash
    pub async fn get_transaction(&self, tx_hash: &str) -> Result<TransactionDetails, MaestroError> {
        let url = format!("https://{BASE_URL}/transactions/{tx_hash}");
        let response: TransactionResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get transaction UTXOs (inputs and outputs)
    pub async fn get_transaction_utxos(
        &self,
        tx_hash: &str,
    ) -> Result<TransactionUtxos, MaestroError> {
        let url = format!("https://{BASE_URL}/transactions/{tx_hash}/utxos");
        let response: TransactionUtxosResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get transaction CBOR data (for faster parsing)
    /// Returns hex-encoded CBOR that can be parsed directly with pallas
    pub async fn get_transaction_cbor(&self, tx_hash: &str) -> Result<String, MaestroError> {
        let url = format!("https://{BASE_URL}/transactions/{tx_hash}/cbor");
        let response: TransactionCborResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get complete transaction details including all inputs, outputs, collateral, scripts, etc.
    #[cfg(feature = "transactions")]
    pub async fn get_complete_transaction(
        &self,
        tx_hash: &str,
    ) -> Result<CompleteTransactionDetails, MaestroError> {
        let url = format!("https://{BASE_URL}/transactions/{tx_hash}");
        let response: CompleteTransactionResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Resolve a payment address to its associated stake key
    pub async fn resolve_address_to_stake_key(
        &self,
        address: &str,
    ) -> Result<Option<String>, MaestroError> {
        let url = format!("https://{BASE_URL}/addresses/{address}/decode");

        match self.get_url::<AddressDecodeResponse>(url).await {
            Ok(response) => {
                // Extract reward_address from staking_cred if it exists
                Ok(response.staking_cred.and_then(|cred| cred.reward_address))
            }
            Err(e) => {
                // Check if this is a 404 error (address not found/invalid)
                if let MaestroError::Http(ref http_err) = e {
                    // For now, treat any HTTP error as "no stake key found"
                    // In a real implementation, we'd check the specific status code
                    warn!("Address decode failed: {http_err:?}");
                    return Ok(None);
                }
                Err(e)
            }
        }
    }

    /// Get all assets held by a specific stake address/account
    pub async fn get_account_assets(
        &self,
        stake_address: &str,
    ) -> Result<Vec<AssetHolding>, MaestroError> {
        let mut all_assets = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let page = self
                .get_account_assets_page(stake_address, cursor, None, None)
                .await?;
            all_assets.extend(page.data);

            if page.next_cursor.is_none() {
                break;
            }
            cursor = page.next_cursor;
        }

        Ok(all_assets)
    }

    /// Get assets held by a specific stake address, filtered by policy
    pub async fn get_account_assets_for_policy(
        &self,
        stake_address: &str,
        policy_id: &str,
    ) -> Result<Vec<AssetHolding>, MaestroError> {
        let mut all_assets = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let page = self
                .get_account_assets_page(stake_address, cursor.clone(), Some(policy_id), None)
                .await?;
            all_assets.extend(page.data);

            if page.next_cursor.is_none() {
                break;
            }
            cursor = page.next_cursor.clone();
        }

        Ok(all_assets)
    }

    /// Get a single page of account assets
    async fn get_account_assets_page(
        &self,
        stake_address: &str,
        cursor: Option<String>,
        policy_id: Option<&str>,
        count: Option<u32>,
    ) -> Result<AccountAssetsResponse, MaestroError> {
        let mut query_params = Vec::new();

        if let Some(policy) = policy_id {
            query_params.push(format!("policy={policy}"));
        }

        if let Some(c) = cursor {
            query_params.push(format!("cursor={c}"));
        }

        let count = count.unwrap_or(100);
        query_params.push(format!("count={count}"));

        let querystring = if query_params.is_empty() {
            String::new()
        } else {
            format!("?{}", query_params.join("&"))
        };

        let url = format!("https://{BASE_URL}/accounts/{stake_address}/assets{querystring}");

        self.get_url(url).await
    }

    pub async fn get(&self, id: &str, policy_id: &str) -> Result<Asset, MaestroError> {
        self.get_detailed(id, policy_id)
            .await
            .and_then(Asset::try_from)
    }

    pub async fn get_detailed(
        &self,
        id: &str,
        policy_id: &str,
    ) -> Result<DetailedAssetInfo, MaestroError> {
        let url = format!("https://{BASE_URL}/assets/{policy_id}{id}");
        let response: AssetInfoResponse = self.get_url(url.clone()).await?;
        Ok(response.data)
    }

    pub async fn get_all_assets(&self, policy_id: &str) -> Result<Vec<AssetWithId>, MaestroError> {
        let mut output: Vec<AssetWithId> = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let (assets, next_cursor) = self.get_asset_page(policy_id, &cursor).await?;
            output.extend(assets);
            if next_cursor.is_none() {
                break;
            }

            cursor = next_cursor;
        }

        Ok(output)
    }

    pub async fn get_asset_page(
        &self,
        policy_id: &str,
        cursor: &Option<String>,
    ) -> Result<(Vec<AssetWithId>, Option<String>), MaestroError> {
        let page = self.get_assets(policy_id, cursor.clone()).await?;
        let assets: Vec<_> = page
            .get_importable_nfts()
            .iter()
            .filter_map(|d| match Asset::try_from(d.asset_standards.clone()) {
                Ok(asset) => Some(asset.with_id(&d.asset_name)),
                Err(_) => None,
            })
            .collect();

        Ok((assets, page.next_cursor))
    }

    pub async fn get_all_owners_for_policy(
        &self,
        policy_id: &str,
    ) -> Result<Vec<PolicyAssetOwner>, MaestroError> {
        let mut output: Vec<PolicyAssetOwner> = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let page = self.get_accounts(policy_id, cursor).await?;
            output.extend(page.data);
            if page.next_cursor.is_none() {
                break;
            }

            cursor = page.next_cursor;
        }

        Ok(output)
    }

    pub async fn get_all_owners_for_asset(
        &self,
        policy_id: &str,
        asset_id: &str,
    ) -> Result<Vec<AccountQuantity>, MaestroError> {
        let mut output: Vec<AccountQuantity> = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let page = self.get_asset_accounts(policy_id, asset_id, cursor).await?;
            output.extend(page.data);
            if page.next_cursor.is_none() {
                break;
            }

            cursor = page.next_cursor;
        }

        Ok(output)
    }

    #[allow(clippy::needless_lifetimes)]
    pub async fn get_asset_stream<'a>(
        &'a self,
        policy_id: &'a str,
    ) -> impl Stream<Item = Asset> + 'a {
        let mut cursor: Option<String> = None;

        stream! {
            while let Ok(page) = self.get_assets(policy_id, cursor).await {
                for data in &page.data {
                    if let Ok(result) = Asset::try_from(data.asset_standards.clone()) {
                        yield result;
                    }
                }

                if page.next_cursor.is_none() {
                    break;
                }

                cursor = page.next_cursor;
            }
        }
    }

    async fn get_assets(
        &self,
        policy_id: &str,
        cursor: Option<String>,
    ) -> Result<PolicyAssetsResponse, MaestroError> {
        let querystring = cursor.map_or(String::new(), |c| format!("cursor={c}"));
        let url = format!("https://{BASE_URL}/policy/{policy_id}/assets?{querystring}");

        self.get_url(url).await
    }

    async fn get_accounts(
        &self,
        policy_id: &str,
        cursor: Option<String>,
    ) -> Result<PolicyAccountsResponse, MaestroError> {
        let querystring = cursor.map_or(String::new(), |c| format!("cursor={c}"));
        let url = format!("https://{BASE_URL}/policy/{policy_id}/accounts?count=100&{querystring}");

        self.get_url(url).await
    }

    async fn get_asset_accounts(
        &self,
        policy_id: &str,
        asset_id: &str,
        cursor: Option<String>,
    ) -> Result<AssetAccountsResponse, MaestroError> {
        let querystring = cursor.map_or(String::new(), |c| format!("cursor={c}"));
        let url = format!(
            "https://{BASE_URL}/policy/{policy_id}{asset_id}/accounts?count=100&{querystring}"
        );

        self.get_url(url).await
    }

    // async fn get_url(&self, url: String) -> reqwest::Result<Response> {
    //     self.client
    //         .get(&url)
    //         .header(ACCEPT, "application/json")
    //         .header(CONTENT_TYPE, "application/json")
    //         .header("api-key", self.api_key.clone())
    //         .send()
    //         .await?
    //         .error_for_status()
    // }

    async fn get_url<T: serde::de::DeserializeOwned>(
        &self,
        url: String,
    ) -> Result<T, MaestroError> {
        self.get_url_with_retry(&url, 3).await
    }

    async fn get_url_with_retry<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        max_retries: u32,
    ) -> Result<T, MaestroError> {
        use http_client::{HttpMethod, ResponseDetails};

        let mut attempt = 0;

        loop {
            let response_details: ResponseDetails = self
                .client
                .request_with_response_details(HttpMethod::GET, url, None::<&()>)
                .await?;

            match response_details.status_code {
                429 => {
                    // Parse Retry-After header if present
                    let retry_after = response_details
                        .headers
                        .get("retry-after")
                        .and_then(|s| s.parse::<u64>().ok());

                    if attempt >= max_retries {
                        warn!(
                            "Max retries ({}) exceeded for URL: {}, giving up",
                            max_retries, url
                        );
                        return Err(MaestroError::RateLimit { retry_after });
                    }

                    // Calculate delay: use retry-after header or exponential backoff
                    let delay_ms = retry_after
                        .map(|seconds| seconds * 1000) // Convert to milliseconds
                        .unwrap_or_else(|| {
                            // Exponential backoff: 1s, 2s, 4s, 8s...
                            1000 * (1 << attempt)
                        });

                    warn!(
                        "Rate limited on attempt {} for URL: {}, retrying after {}ms",
                        attempt + 1,
                        url,
                        delay_ms
                    );

                    // Use worker_utils::sleep for Cloudflare Workers compatibility
                    worker_utils::sleep::sleep(delay_ms as i32).await;
                    attempt += 1;
                    continue;
                }
                status if (200..300).contains(&status) => {
                    // Success - parse response
                    let cleaned = strip_control_chars(&response_details.body);
                    return serde_json::from_str(&cleaned).map_err(|_| {
                        MaestroError::Deserialization(format!(
                            "deserialization failure for url: {url}"
                        ))
                    });
                }
                _ => {
                    // Other HTTP errors - propagate immediately
                    return Err(MaestroError::Http(http_client::HttpError::Custom(format!(
                        "HTTP error {}: {}",
                        response_details.status_code, response_details.body
                    ))));
                }
            }
        }
    }
}

pub fn deserialize_u64_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<u64>().map_err(serde::de::Error::custom)
}

mod maestro_date_format {
    use chrono::{DateTime, NaiveDateTime, Utc};
    use serde::{self, Deserialize, Deserializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    // // The signature of a serialize_with function must follow the pattern:
    // //
    // //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    // //    where
    // //        S: Serializer
    // //
    // // although it may also be generic over the input types T.
    // pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    // where
    //     S: Serializer,
    // {
    //     let s = format!("{}", date.format(FORMAT));
    //     serializer.serialize_str(&s)
    // }

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let dt = NaiveDateTime::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)?;
        Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    }
}

pub(crate) fn strip_control_chars(input: &str) -> String {
    input
        .chars()
        .filter(|&c| {
            // Retain printable characters and common whitespace
            !(c.is_control() && c != '\n' && c != '\r' && c != '\t')
        })
        .collect()
}
