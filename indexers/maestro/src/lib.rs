#![allow(clippy::match_like_matches_macro)]

use async_stream::stream;
use cardano_assets::{
    Asset, AssetMetadata, AssetMetadata68, AssetWithId, MetadataKind, NftPurpose,
};
use chrono::Utc;
use futures_core::stream::Stream;
use http_client::HttpClient;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::str::FromStr;
use std::{error::Error, fmt};
use tracing::warn;
use worker_stack::worker;

mod test;

pub type BlockfrostAsset = serde_json::Map<String, Value>;

const BASE_URL_MAINNET: &str = "mainnet.gomaestro-api.org/v1";
const BASE_URL_PREPROD: &str = "preprod.gomaestro-api.org/v1";

#[derive(Debug, Default)]
pub enum MaestroError {
    NoMetadata,
    Http(http_client::HttpError),
    /// Rate limit exceeded (429 response). Contains optional retry-after seconds from API
    RateLimit {
        retry_after: Option<u64>,
    },
    Deserialization(String),
    #[default]
    Unknown,
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

// Datum lookup response types
#[derive(Deserialize, Debug)]
struct DatumByHashResponse {
    data: DatumData,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DatumData {
    /// Hex-encoded CBOR bytes of the datum
    pub bytes: String,
    /// JSON representation (may be null if Maestro can't decode it)
    pub json: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
struct DatumsByHashesResponse {
    data: std::collections::HashMap<String, DatumData>,
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

#[derive(Deserialize, Clone, Debug)]
pub struct AssetAmount {
    pub unit: String, // This is policy_id + asset_name
    #[serde(with = "wasm_safe_serde::u64_required")]
    #[serde(alias = "quantity")] // Support both "amount" and "quantity"
    pub amount: u64,
}

#[derive(Deserialize, Debug)]
#[cfg(feature = "transactions")]
struct AddressUtxosResponse {
    data: Vec<AddressUtxo>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AddressUtxo {
    pub tx_hash: String,
    #[serde(deserialize_with = "deserialize_u32_or_u64")]
    pub index: u32,
    pub assets: Vec<AssetAmount>,
    pub datum: Option<serde_json::Value>,
    pub script_ref: Option<String>,
}

impl From<AddressUtxo> for cardano_assets::UtxoApi {
    fn from(utxo: AddressUtxo) -> Self {
        let mut lovelace = 0u64;
        let mut native_assets = Vec::new();

        for asset in utxo.assets {
            if asset.unit == "lovelace" {
                lovelace = asset.amount;
            } else {
                // Parse the unit field which is policy_id + asset_name_hex
                if let Ok(asset_id) = cardano_assets::AssetId::from_str(&asset.unit) {
                    native_assets.push(cardano_assets::AssetQuantity {
                        asset_id,
                        quantity: asset.amount,
                    });
                }
            }
        }

        Self {
            tx_hash: utxo.tx_hash,
            output_index: utxo.index,
            lovelace,
            assets: native_assets,
        }
    }
}

fn deserialize_u32_or_u64<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let value: serde_json::Value = Deserialize::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                u32::try_from(u).map_err(|_| Error::custom("number too large for u32"))
            } else {
                Err(Error::custom("expected unsigned integer"))
            }
        }
        serde_json::Value::String(s) => s
            .parse::<u32>()
            .map_err(|_| Error::custom("failed to parse string as u32")),
        _ => Err(Error::custom("expected number or string")),
    }
}

// Helper types for nested Maestro response structures
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AdaLovelace {
    pub ada: AdaAmount,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AdaAmount {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub lovelace: u64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ByteSize {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub bytes: u64,
}

// Protocol parameters for fee calculation and transaction building
// Only deserialize the fields we actually need for transaction building
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ProtocolParameters {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub min_fee_coefficient: u64,
    pub min_fee_constant: AdaLovelace,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub min_utxo_deposit_coefficient: u64,
}

#[derive(Deserialize, Debug)]
struct ProtocolParametersResponse {
    data: ProtocolParameters,
}

// Transaction manager structures for tracking transaction state
#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug, Clone)]
pub struct TransactionState {
    pub tx_hash: String,
    pub status: TxStatus,
    pub confirmations: u32,
}

#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TxStatus {
    Pending,
    Confirmed,
    Failed,
}

#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug)]
struct TransactionStateResponse {
    #[serde(flatten)]
    data: TransactionState,
}

#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug)]
pub struct TransactionHistory {
    pub transactions: Vec<TransactionState>,
    pub page: u32,
    pub total_count: u32,
}

#[cfg(feature = "transactions")]
#[derive(Deserialize, Debug)]
struct TransactionHistoryResponse {
    #[serde(flatten)]
    data: TransactionHistory,
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
    #[allow(dead_code)] // read in submit_transaction behind cfg(feature = "transactions")
    api_key: String,
    base_url: String,
}

impl MaestroApi {
    /// Create a new MaestroApi instance with the provided API key and base URL
    pub fn new(api_key: String, base_url: String) -> Self {
        Self {
            client: HttpClient::new().with_header("api-key", &api_key),
            api_key: api_key.clone(),
            base_url,
        }
    }

    #[deprecated(note = "use for_env_with_network instead")]
    pub async fn for_env(env: &worker::Env) -> worker::Result<Self> {
        let api_key = worker_utils::secrets::get_secret(env, "MAESTRO_API_KEY").await?;
        Ok(Self {
            client: HttpClient::new().with_header("api-key", &api_key),
            api_key: api_key.clone(),
            base_url: BASE_URL_MAINNET.to_string(),
        })
    }

    /// Create MaestroApi from environment with network selection
    ///
    /// Selects the correct API key and base URL for the network:
    /// - Mainnet: MAESTRO_API_KEY_MAINNET with mainnet.gomaestro-api.org
    /// - Testnet: MAESTRO_API_KEY_TESTNET with preview.gomaestro-api.org
    pub async fn for_env_with_network(env: &worker::Env, network: &str) -> worker::Result<Self> {
        let (secret_name, base_url) = match network {
            "cardano:mainnet" => ("MAESTRO_API_KEY_MAINNET", BASE_URL_MAINNET),
            "cardano:testnet" | "cardano:preview" | "cardano:preprod" => {
                ("MAESTRO_API_KEY_TESTNET", BASE_URL_PREPROD)
            }
            _ => {
                return Err(worker::Error::RustError(format!(
                    "Unsupported network for Maestro: {network}"
                )))
            }
        };

        let api_key = worker_utils::secrets::get_secret(env, secret_name).await?;

        Ok(Self {
            client: HttpClient::new().with_header("api-key", &api_key),
            api_key: api_key.clone(),
            base_url: base_url.to_string(),
        })
    }

    pub async fn get_epoch(&self, target: EpochTarget) -> Result<EpochDetails, MaestroError> {
        let url = match target {
            EpochTarget::Current => format!("https://{}/epochs/current", self.base_url),
            EpochTarget::Specific(epoch) => format!("https://{}/epochs/{epoch}", self.base_url),
        };

        let response: EpochResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get current protocol parameters
    /// Essential for fee calculation, min UTxO values, and transaction building
    pub async fn get_protocol_parameters(&self) -> Result<ProtocolParameters, MaestroError> {
        let url = format!("https://{}/protocol-parameters", self.base_url);
        let response: ProtocolParametersResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get transaction details by hash
    pub async fn get_transaction(&self, tx_hash: &str) -> Result<TransactionDetails, MaestroError> {
        let url = format!("https://{}/transactions/{tx_hash}", self.base_url);
        let response: TransactionResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get transaction UTXOs (inputs and outputs)
    pub async fn get_transaction_utxos(
        &self,
        tx_hash: &str,
    ) -> Result<TransactionUtxos, MaestroError> {
        let url = format!("https://{}/transactions/{tx_hash}/utxos", self.base_url);
        let response: TransactionUtxosResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get transaction CBOR data (for faster parsing)
    /// Returns hex-encoded CBOR that can be parsed directly with pallas
    pub async fn get_transaction_cbor(&self, tx_hash: &str) -> Result<String, MaestroError> {
        let url = format!("https://{}/transactions/{tx_hash}/cbor", self.base_url);
        let response: TransactionCborResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get a single datum by its hash
    /// Returns CBOR bytes and optional JSON representation
    pub async fn get_datum_by_hash(&self, datum_hash: &str) -> Result<DatumData, MaestroError> {
        let url = format!("https://{}/datums/{datum_hash}", self.base_url);
        let response: DatumByHashResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get multiple datums by their hashes in a single request
    /// Returns a map of datum_hash -> DatumData
    pub async fn get_datums_by_hashes(
        &self,
        datum_hashes: &[&str],
    ) -> Result<std::collections::HashMap<String, DatumData>, MaestroError> {
        let url = format!("https://{}/datums", self.base_url);
        let body = serde_json::to_value(datum_hashes).map_err(|e| {
            MaestroError::Deserialization(format!("Failed to serialize datum hashes: {e}"))
        })?;
        let response: DatumsByHashesResponse = self.post_url(url, &body).await?;
        Ok(response.data)
    }

    /// Get complete transaction details including all inputs, outputs, collateral, scripts, etc.
    #[cfg(feature = "transactions")]
    pub async fn get_complete_transaction(
        &self,
        tx_hash: &str,
    ) -> Result<CompleteTransactionDetails, MaestroError> {
        let url = format!("https://{}/transactions/{tx_hash}", self.base_url);
        let response: CompleteTransactionResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get UTxOs at a specific address (for wallet operations and transaction building)
    #[cfg(feature = "transactions")]
    pub async fn get_address_utxos(&self, address: &str) -> Result<Vec<AddressUtxo>, MaestroError> {
        let url = format!("https://{}/addresses/{address}/utxos", self.base_url);
        let response: AddressUtxosResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Submit a signed transaction to the blockchain
    /// Returns the transaction hash on success (202 Accepted)
    ///
    /// Note: This uses worker::Fetch directly because we need to send raw CBOR bytes
    /// with application/cbor content-type, which the http-client doesn't support
    #[cfg(feature = "transactions")]
    pub async fn submit_transaction(&self, tx_cbor_hex: &str) -> Result<String, MaestroError> {
        use worker_stack::worker;

        let url = format!("https://{}/txmanager", self.base_url);

        // Decode hex to bytes for CBOR submission
        let tx_bytes = hex::decode(tx_cbor_hex)
            .map_err(|e| MaestroError::Deserialization(format!("Invalid hex: {e}")))?;

        // Use the API key from our struct
        let api_key = &self.api_key;

        // Create request with CBOR body using worker's Fetch API
        let headers = worker::Headers::new();
        headers
            .set("Content-Type", "application/cbor")
            .map_err(|e| {
                MaestroError::Http(http_client::HttpError::Custom(format!(
                    "Failed to set header: {e:?}"
                )))
            })?;
        headers.set("api-key", api_key).map_err(|e| {
            MaestroError::Http(http_client::HttpError::Custom(format!(
                "Failed to set header: {e:?}"
            )))
        })?;

        let mut init = worker::RequestInit::new();
        init.method = worker::Method::Post;
        init.headers = headers;
        init.body = Some(tx_bytes.into());

        let request = worker::Request::new_with_init(&url, &init).map_err(|e| {
            MaestroError::Http(http_client::HttpError::Custom(format!(
                "Failed to create request: {e:?}"
            )))
        })?;

        let mut response = worker::Fetch::Request(request).send().await.map_err(|e| {
            MaestroError::Http(http_client::HttpError::Custom(format!(
                "Fetch failed: {e:?}"
            )))
        })?;

        // Check for 202 Accepted status
        if response.status_code() != 202 {
            let error_text = response.text().await.unwrap_or_default();
            return Err(MaestroError::Http(http_client::HttpError::Custom(format!(
                "Transaction submission failed (status {}): {}",
                response.status_code(),
                error_text
            ))));
        }

        // Response body is the transaction hash as plain text
        let tx_hash = response.text().await.map_err(|e| {
            MaestroError::Http(http_client::HttpError::Custom(format!(
                "Failed to read response: {e:?}"
            )))
        })?;

        Ok(tx_hash.trim().to_string())
    }

    /// Get the current state of a submitted transaction
    /// Returns pending, confirmed, or failed status with confirmation count
    #[cfg(feature = "transactions")]
    pub async fn get_transaction_state(
        &self,
        tx_hash: &str,
    ) -> Result<TransactionState, MaestroError> {
        let url = format!("https://{}/txmanager/{tx_hash}/state", self.base_url);
        let response: TransactionStateResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Get transaction history from the transaction manager
    /// Returns paginated list of submitted transactions with their states
    #[cfg(feature = "transactions")]
    pub async fn get_transaction_history(
        &self,
        page: Option<u32>,
        count: Option<u32>,
    ) -> Result<TransactionHistory, MaestroError> {
        let mut query_params = Vec::new();

        if let Some(p) = page {
            query_params.push(format!("page={p}"));
        }

        if let Some(c) = count {
            query_params.push(format!("count={c}"));
        }

        let query_string = if query_params.is_empty() {
            String::new()
        } else {
            format!("?{}", query_params.join("&"))
        };

        let url = format!("https://{}/txmanager/history{query_string}", self.base_url);
        let response: TransactionHistoryResponse = self.get_url(url).await?;
        Ok(response.data)
    }

    /// Resolve a payment address to its associated stake key
    pub async fn resolve_address_to_stake_key(
        &self,
        address: &str,
    ) -> Result<Option<String>, MaestroError> {
        let url = format!("https://{}/addresses/{address}/decode", self.base_url);

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

        let url = format!(
            "https://{}/accounts/{stake_address}/assets{querystring}",
            self.base_url
        );

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
        let url = format!("https://{}/assets/{policy_id}{id}", self.base_url);
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
        self.get_asset_page_with_count(policy_id, cursor, None)
            .await
    }

    pub async fn get_asset_page_with_count(
        &self,
        policy_id: &str,
        cursor: &Option<String>,
        count: Option<u32>,
    ) -> Result<(Vec<AssetWithId>, Option<String>), MaestroError> {
        let page = self.get_assets(policy_id, cursor.clone(), count).await?;
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
            while let Ok(page) = self.get_assets(policy_id, cursor, None).await {
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
        count: Option<u32>,
    ) -> Result<PolicyAssetsResponse, MaestroError> {
        let mut query_params = Vec::new();

        if let Some(c) = cursor {
            query_params.push(format!("cursor={c}"));
        }

        if let Some(n) = count {
            query_params.push(format!("count={n}"));
        }

        let querystring = if query_params.is_empty() {
            String::new()
        } else {
            format!("?{}", query_params.join("&"))
        };

        let url = format!(
            "https://{}/policy/{policy_id}/assets{querystring}",
            self.base_url
        );

        self.get_url(url).await
    }

    async fn get_accounts(
        &self,
        policy_id: &str,
        cursor: Option<String>,
    ) -> Result<PolicyAccountsResponse, MaestroError> {
        let querystring = cursor.map_or(String::new(), |c| format!("cursor={c}"));
        let url = format!(
            "https://{}/policy/{policy_id}/accounts?count=100&{querystring}",
            self.base_url
        );

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
            "https://{}/policy/{policy_id}{asset_id}/accounts?count=100&{querystring}",
            self.base_url
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
        use http_client::HttpMethod;

        let mut attempt = 0;

        loop {
            // Request text with details to get raw body, headers, and perform custom retry logic
            let response_details = self
                .client
                .request_text_with_details(HttpMethod::GET, url, None::<&()>)
                .await?;

            match response_details.status_code {
                429 => {
                    // Parse Retry-After header using helper method
                    let retry_after = response_details.retry_after_seconds();

                    if attempt >= max_retries {
                        warn!("Max retries ({max_retries}) exceeded for URL: {url}, giving up");
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
                        "Rate limited on attempt {} for URL: {url}, retrying after {delay_ms}ms",
                        attempt + 1
                    );

                    // Use worker_utils::sleep for Cloudflare Workers compatibility
                    worker_utils::sleep::sleep(delay_ms as i32).await;
                    attempt += 1;
                    continue;
                }
                status if (200..300).contains(&status) => {
                    // Success - parse response body (data field contains the String)
                    let cleaned = strip_control_chars(&response_details.data);
                    return serde_json::from_str(&cleaned).map_err(|e| {
                        MaestroError::Deserialization(format!(
                            "deserialization failure for url: {url}, error: {e}, body: {}",
                            &cleaned[..cleaned.len().min(500)]
                        ))
                    });
                }
                _ => {
                    // Other HTTP errors - propagate immediately
                    return Err(MaestroError::Http(http_client::HttpError::Custom(format!(
                        "HTTP error {}: {}",
                        response_details.status_code, response_details.data
                    ))));
                }
            }
        }
    }

    async fn post_url<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: String,
        body: &B,
    ) -> Result<T, MaestroError> {
        use http_client::HttpMethod;

        let response_details = self
            .client
            .request_text_with_details(HttpMethod::POST, &url, Some(body))
            .await?;

        match response_details.status_code {
            status if (200..300).contains(&status) => {
                let cleaned = strip_control_chars(&response_details.data);
                serde_json::from_str(&cleaned).map_err(|e| {
                    MaestroError::Deserialization(format!(
                        "deserialization failure for url: {url}, error: {e}, body: {}",
                        &cleaned[..cleaned.len().min(500)]
                    ))
                })
            }
            429 => {
                let retry_after = response_details.retry_after_seconds();
                Err(MaestroError::RateLimit { retry_after })
            }
            _ => Err(MaestroError::Http(http_client::HttpError::Custom(format!(
                "HTTP error {}: {}",
                response_details.status_code, response_details.data
            )))),
        }
    }
}

pub fn deserialize_u64_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    match Value::deserialize(deserializer)? {
        Value::Number(num) => num
            .as_u64()
            .ok_or_else(|| Error::custom("number out of u64 range")),
        Value::String(s) => s
            .parse::<u64>()
            .map_err(|_| Error::custom("failed to parse string as u64")),
        _ => Err(Error::custom("expected number or string")),
    }
}

pub fn serialize_u64_string<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_u64(*value)
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
