pub mod koios_account_utxos;
pub mod koios_assets;
pub mod koios_cip68;
pub mod koios_evaluate;
pub mod koios_params;
mod koios_serde;
pub mod koios_transaction;
pub mod koios_utils;
pub mod koios_utxos;

use cardano_assets::{
    Asset, AssetMetadata, AssetMetadata68, AssetWithId, ExtractedCid, MetadataKind, NftPurpose,
    PolicyAssetSample, PolicyClassification, asset_from_metadata_value,
};
use http_client::{HttpClient, HttpError};
use koios_account_utxos::TxRecord;
pub use koios_evaluate::KoiosRedeemerBudget;
pub use koios_params::KoiosProtocolParams;
use koios_transaction::KoiosTransaction;
pub use koios_utxos::{KoiosAccountAsset, KoiosInlineDatum, KoiosUtxo, KoiosUtxoAsset, UtxoAmount};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, time::Duration};
use tracing::{error, info, warn};
use worker_stack::worker::{self, Env, RouteContext};

const BASE_URL: &str = "https://api.koios.rest/api/v1";

/// Koios API base URL for a Cardano network. The **same API key works across
/// all environments** — only the host differs. Unknown/mainnet fall back to the
/// mainnet endpoint.
pub fn koios_base_url(network: &str) -> String {
    match network {
        "cardano:preprod" | "cardano:testnet" => "https://preprod.koios.rest/api/v1".to_string(),
        "cardano:preview" => "https://preview.koios.rest/api/v1".to_string(),
        _ => BASE_URL.to_string(),
    }
}

/// The `KOIOS_API_KEY` bearer token, if one is configured.
///
/// Memoised in the isolate (see [`API_KEY_MEMO`]): a tx build constructs a
/// client several times, and each construction was a Secrets Store round trip.
async fn api_key(env: &Env) -> Option<String> {
    let now = worker::Date::now().as_millis();
    let memoised = API_KEY_MEMO.with(|memo| {
        memo.borrow()
            .as_ref()
            .filter(|(_, read_at)| now.saturating_sub(*read_at) < API_KEY_MEMO_TTL_MS)
            .map(|(key, _)| key.clone())
    });
    if memoised.is_some() {
        return memoised;
    }

    match worker_utils::secrets::get_secret(env, "KOIOS_API_KEY").await {
        Ok(key) if !key.is_empty() => {
            API_KEY_MEMO.with(|memo| *memo.borrow_mut() = Some((key.clone(), now)));
            Some(key)
        }
        _ => None,
    }
}

/// Koios caps a single response page at 1000 rows; paginated reads walk
/// `offset` in these increments until a short page signals the end.
const KOIOS_PAGE_LIMIT: u32 = 1000;

/// Longest one Koios request may take, response body included, before it fails.
///
/// A round trip measures ~1s, and a full 1000-row page or a script evaluation
/// lands in low single-digit seconds — so this only fires on a hung upstream,
/// which otherwise held a tx build open with no end.
const KOIOS_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Attempts per Koios request: the first, plus retries of a transient refusal.
const KOIOS_ATTEMPTS: u32 = 3;

/// The longest `Retry-After` honoured. Asked to wait longer, give up at once:
/// a build someone is watching should fail fast rather than stall.
const KOIOS_MAX_RETRY_WAIT: Duration = Duration::from_secs(5);

/// What a failed Koios request calls for.
enum Recovery {
    /// A transient refusal — rate limit or gateway — so try again after this long.
    RetryAfter(Duration),
    /// Retrying cannot help, or the attempts are spent.
    GiveUp,
}

impl Recovery {
    /// Decide for the failure of attempt number `attempt` (counting from 1).
    ///
    /// Only 429 and gateway statuses are retried. A timeout is NOT: its
    /// [`KOIOS_REQUEST_TIMEOUT`] is already spent, and a second one would
    /// double the wait for an upstream that is not answering. A 500 is not
    /// either — Koios returns it for queries that fail the same way every time.
    fn for_failure(error: &HttpError, attempt: u32) -> Self {
        if attempt >= KOIOS_ATTEMPTS {
            return Self::GiveUp;
        }
        if !matches!(failure_status(error), Some(429 | 502 | 503 | 504)) {
            return Self::GiveUp;
        }
        match error.retry_after_seconds().map(Duration::from_secs) {
            Some(wait) if wait > KOIOS_MAX_RETRY_WAIT => Self::GiveUp,
            Some(wait) => Self::RetryAfter(wait),
            None => Self::RetryAfter(Duration::from_millis(500 * u64::from(attempt))),
        }
    }
}

/// The HTTP status a failed request carried, in whichever shape the transport
/// reported it: the detailed POST keeps it, wasm's plain GET folds it into a
/// message, and native's plain GET leaves it on the reqwest error.
fn failure_status(error: &HttpError) -> Option<u16> {
    match error {
        HttpError::HttpStatus { status_code, .. } => Some(*status_code),
        HttpError::Custom(msg) => msg
            .strip_prefix("HTTP request failed with status: ")
            .and_then(|status| status.parse().ok()),
        #[cfg(not(target_arch = "wasm32"))]
        HttpError::Reqwest(e) => e.status().map(|status| status.as_u16()),
        _ => None,
    }
}

/// Map a transport failure onto [`KoiosError`], keeping a non-2xx status (and
/// its body, where the transport kept one) as [`KoiosError::KoiosResponse`].
fn koios_error(error: HttpError) -> KoiosError {
    match error {
        HttpError::HttpStatus {
            status_code, body, ..
        } => {
            error!("Koios API error: {status_code} {body}");
            KoiosError::KoiosResponse {
                status: status_code,
                body,
            }
        }
        HttpError::Custom(msg) if msg.starts_with("HTTP request failed with status:") => {
            let status_str = msg.replace("HTTP request failed with status: ", "");
            let status = status_str.parse::<u16>().unwrap_or(500);
            error!("Koios API error: {status} {msg}");
            KoiosError::KoiosResponse { status, body: msg }
        }
        other => KoiosError::Http(other),
    }
}

/// Wait out a retry delay on whichever runtime this build targets.
async fn pause(wait: Duration) {
    #[cfg(target_arch = "wasm32")]
    worker::Delay::from(wait).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(wait).await;
}

/// How long this isolate reuses a `KOIOS_API_KEY` it has read.
///
/// Long enough that a request constructing several clients reads the Secrets
/// Store once; short enough that a rotated key is picked up without a deploy.
const API_KEY_MEMO_TTL_MS: u64 = 5 * 60 * 1000;

thread_local! {
    /// The last `KOIOS_API_KEY` this isolate read, and when.
    ///
    /// Only a non-empty key is ever stored. A failed or empty lookup is retried
    /// on the next construction, so one transient Secrets Store error cannot
    /// pin a whole isolate to the keyless free tier.
    static API_KEY_MEMO: std::cell::RefCell<Option<(String, u64)>> =
        const { std::cell::RefCell::new(None) };
}

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

/// One asset's mint/burn history (`POST /asset_history`).
#[derive(Serialize, Debug, Clone)]
pub struct AssetHistoryRequest {
    #[serde(rename = "_asset_policy")]
    pub policy: String,
    #[serde(rename = "_asset_name")]
    pub asset_name_hex: String,
}

/// Transaction metadata only (`POST /tx_metadata`).
///
/// Narrower than `/tx_info`, which gates metadata behind a flag
/// [`get_tx_details`](KoiosApi::get_tx_details) does not set and returns
/// inputs, scripts and assets besides.
#[derive(Serialize, Debug, Clone)]
pub struct TxMetadataRequest {
    #[serde(rename = "_tx_hashes")]
    pub hashes: Vec<String>,
}

/// A row from `POST /asset_history`.
#[derive(Deserialize, Debug, Clone)]
pub struct KoiosAssetHistory {
    #[serde(default)]
    pub policy_id: Option<String>,
    #[serde(default)]
    pub asset_name: Option<String>,
    #[serde(default)]
    pub minting_txs: Vec<KoiosMintEvent>,
}

/// One mint or burn of an asset.
#[derive(Deserialize, Debug, Clone)]
pub struct KoiosMintEvent {
    pub tx_hash: String,
    #[serde(default)]
    pub block_time: Option<u64>,
    /// Signed, and **serialised as a STRING** by Koios: `"1"` for a mint,
    /// `"-1"` for a burn. Parse rather than assume a JSON number.
    #[serde(default)]
    pub quantity: Option<String>,
}

impl KoiosMintEvent {
    /// `true` when this event added supply rather than removing it.
    pub fn is_mint(&self) -> bool {
        self.quantity
            .as_deref()
            .and_then(|q| q.parse::<i64>().ok())
            .is_some_and(|q| q > 0)
    }
}

/// A row from `POST /tx_metadata`.
#[derive(Deserialize, Debug, Clone)]
pub struct KoiosTxMetadata {
    pub tx_hash: String,
    /// Keyed by metadata label as a string — `"721"`, `"777"`, …
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Batch confirmation check (`POST /tx_status`) — one request for many tx
/// hashes, far lighter than `/tx_info`.
#[derive(Serialize, Debug, Clone)]
pub struct TxStatusRequest {
    #[serde(rename = "_tx_hashes")]
    pub hashes: Vec<String>,
}

/// One row of `/tx_status`. `num_confirmations` is `null` (→ `None`) until the
/// tx is in a block, then the depth in blocks.
#[derive(Deserialize, Debug, Clone)]
pub struct TxStatus {
    pub tx_hash: String,
    pub num_confirmations: Option<u64>,
}

/// The two fields that identify a UTxO, and nothing else — the non-extended
/// `/address_utxos` row as [`KoiosApi::get_address_utxo_refs`] reads it.
/// Deliberately narrow: deserialising the full row would pull in the asset
/// list this exists to avoid.
#[derive(Deserialize, Debug, Clone)]
pub struct KoiosUtxoRef {
    pub tx_hash: String,
    pub tx_index: u32,
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

#[derive(Serialize, Debug, Clone)]
pub struct UtxoRefsRequest {
    /// UTxO references in the form `"tx_hash#index"`.
    #[serde(rename = "_utxo_refs")]
    pub utxo_refs: Vec<String>,
    #[serde(rename = "_extended")]
    pub extended: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct DatumInfoRequest {
    #[serde(rename = "_datum_hashes")]
    pub datum_hashes: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct AssetUtxosRequest {
    /// `[[policy_id, asset_name_hex], …]` — Koios takes the pair as a
    /// two-element array, not a concatenated id.
    #[serde(rename = "_asset_list")]
    pub asset_list: Vec<[String; 2]>,
    #[serde(rename = "_extended")]
    pub extended: bool,
}

#[derive(Debug, Serialize)]
pub struct TxCborRequest {
    #[serde(rename = "_tx_hashes")]
    pub tx_hashes: Vec<String>,
}

/// A whole transaction as CBOR (`POST /tx_cbor`).
///
/// Needed where a *decoded* view will not do: recovering a hash-only datum
/// means hashing the preimage's ORIGINAL bytes, which a decode/re-encode does
/// not reproduce. It also carries the transaction's auxiliary data, which is
/// where jpg.store publishes listing datums.
#[derive(Debug, Deserialize)]
pub struct KoiosTxCbor {
    pub tx_hash: String,
    /// Full transaction CBOR, hex.
    #[serde(default)]
    pub cbor: Option<String>,
}

/// A datum resolved by hash (`POST /datum_info`) — the CBOR preimage plus its
/// Plutus-JSON rendering.
#[derive(Debug, Clone, Deserialize)]
pub struct KoiosDatumInfo {
    /// Koios wires this as `datum_hash` (matching the `_datum_hashes`
    /// request key), *not* `hash` — deserialising it as `hash` fails the
    /// whole batch with `missing field \`hash\``. The alias keeps the bare
    /// `hash` spelling accepted in case a mirror/proxy emits it.
    #[serde(rename = "datum_hash", alias = "hash")]
    pub hash: String,
    /// Raw datum CBOR (hex), if the node holds the preimage.
    #[serde(default)]
    pub bytes: Option<String>,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
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
    /// Koios sends this as a decimal STRING (`"10000000000"`), not a number.
    /// It was previously read through `as_f64`, which silently loses precision
    /// past 2^53 — invisible on an NFT collection where every supply is 1, and
    /// wrong the first time a real fungible passes through.
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub total_supply: u64,
    pub mint_cnt: u64,
    pub burn_cnt: u64,
    pub creation_time: u64,
    pub token_registry_metadata: Option<TokenMetadata>,
    /// The minting transaction's label-721 payload, verbatim:
    /// `{"721": {"<policy>": {"<name>": {...}}}}`. This is the CIP-25
    /// metadata every image and trait derives from. `None` for assets
    /// minted without it (CIP-68, or bare fungibles).
    #[serde(default)]
    pub minting_tx_metadata: Option<serde_json::Value>,
    /// CIP-68 datum-derived metadata, where the reference token carries it.
    #[serde(default)]
    pub cip68_metadata: Option<serde_json::Value>,
}

impl KoiosAssetInfo {
    /// This asset's own CIP-25 record: `minting_tx_metadata["721"][policy][name]`.
    ///
    /// CIP-25 v1 keys the name as UTF-8 and v2 as hex; both are tried, hex
    /// first because it is unambiguous.
    pub fn cip25_record(&self) -> Option<&serde_json::Value> {
        cip25_record_in(
            self.minting_tx_metadata.as_ref()?,
            &self.policy_id,
            &self.asset_name,
        )
    }

    /// This asset's CIP-68 metadata map, decoded out of the Plutus detailed
    /// JSON Koios serves in `cip68_metadata`.
    ///
    /// Plain JSON, not the typed form: trait extraction reads the original
    /// document so a shape [`AssetMetadata`] cannot match still yields
    /// traits.
    #[must_use]
    pub fn cip68_record(&self) -> Option<serde_json::Value> {
        koios_cip68::cip68_metadata_value(self.cip68_metadata.as_ref()?)
    }

    /// This asset's CIP-68 metadata as typed [`AssetMetadata68`], carrying the
    /// purpose implied by the CIP-67 label Koios keyed the datum under.
    #[must_use]
    pub fn cip68_metadata68(&self) -> Option<AssetMetadata68> {
        koios_cip68::decode_cip68_metadata(self.cip68_metadata.as_ref()?)
    }

    /// Which metadata standard this asset actually carries.
    ///
    /// CIP-25 wins when both are present: an asset minted with a `721`
    /// payload is a CIP-25 asset that also happens to have a datum (ADA
    /// Handle does exactly this), and the stored kind has to keep saying so.
    #[must_use]
    pub fn metadata_kind(&self) -> MetadataKind {
        if self.cip25_record().is_some() {
            return MetadataKind::Cip25;
        }
        match self.cip68_metadata68() {
            Some(cip68) => MetadataKind::Cip68(cip68.purpose),
            None => MetadataKind::Unknown,
        }
    }

    /// Flatten this asset's metadata to the shared [`Asset`] — name, image,
    /// media type and traits.
    ///
    /// CIP-68 is preferred over CIP-25 because the datum is the live
    /// declaration: a dynamic NFT's current image is there, while the `721`
    /// payload is frozen at mint. Both go through the v2
    /// [`asset_from_metadata_value`] extractor over the raw document, so
    /// results match every other path that reads metadata.
    #[must_use]
    pub fn to_asset(&self) -> Option<Asset> {
        if let Some(cip68) = self.cip68_record()
            && let Ok(asset) = asset_from_metadata_value(cip68)
        {
            return Some(asset);
        }
        if let Some(cip25) = self.cip25_record()
            && let Ok(asset) = asset_from_metadata_value(cip25.clone())
        {
            return Some(asset);
        }
        None
    }

    /// Every IPFS CID (headline image plus each `files[]` entry) this asset's
    /// metadata references. Flattening to [`Asset`] keeps only the headline
    /// image, so callers that need the full media set read it here.
    #[must_use]
    pub fn extract_cids(&self) -> Vec<ExtractedCid> {
        if let Some(cip68) = self.cip68_metadata68() {
            return cip68.extract_cids();
        }
        if let Some(cip25) = self
            .cip25_record()
            .and_then(|v| serde_json::from_value::<AssetMetadata>(v.clone()).ok())
        {
            return cip25.extract_cids();
        }
        Vec::new()
    }

    /// Unix seconds this asset was first minted, for a stored mint timestamp.
    #[must_use]
    pub fn mint_timestamp(&self) -> u64 {
        self.creation_time
    }

    /// This asset as an importable [`AssetWithId`], or `None` if it does not
    /// belong in a collection import — a reference token, or an asset with no
    /// readable metadata.
    ///
    /// The extracted CID set is captured here because flattening to [`Asset`]
    /// keeps only the headline image, and `files[]` media is gone after that.
    #[must_use]
    pub fn as_asset_with_id(&self) -> Option<AssetWithId> {
        if !self.should_import() {
            return None;
        }
        let asset = self.to_asset()?;
        Some(AssetWithId::new(
            self.asset_name.clone(),
            asset,
            self.extract_cids(),
        ))
    }

    /// Whether this asset belongs in a collection import.
    ///
    /// CIP-25 assets do; of the CIP-68 pair only the `222` user token does —
    /// the `100` reference token is the metadata carrier, not a holdable
    /// item, and importing it double-counts the collection.
    #[must_use]
    pub fn should_import(&self) -> bool {
        if self.cip25_record().is_some() {
            return true;
        }
        matches!(
            self.cip68_metadata68(),
            Some(AssetMetadata68 {
                purpose: NftPurpose::UserNft,
                ..
            })
        )
    }
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

/// Row from `GET /policy_asset_info` — a policy's assets with their mint
/// metadata attached.
///
/// This is the *heavy* per-policy listing: every row carries its full `721`
/// document. Reach for [`KoiosPolicyAssetName`] when all you need is "what
/// lives under this policy"; use this when the answer depends on what the
/// metadata says, as policy classification does.
#[derive(Debug, Serialize, Deserialize)]
pub struct KoiosPolicyAssetInfo {
    /// Hex-encoded asset name; null/absent for the empty name.
    pub asset_name: Option<String>,
    /// Koios serialises large supplies as strings; wasm-safe-serde
    /// accepts both string and integer forms.
    #[serde(default, with = "wasm_safe_serde::u64_option")]
    pub total_supply: Option<u64>,
    /// The minting transaction's label-721 payload. Absent for CIP-68 assets
    /// and bare fungibles.
    #[serde(default)]
    pub minting_tx_metadata: Option<serde_json::Value>,
}

impl KoiosPolicyAssetInfo {
    /// Reduce this row to the evidence [`classify_policy`] weighs.
    ///
    /// `policy_id` is needed to navigate the `721` envelope, which this
    /// endpoint's rows do not carry.
    #[must_use]
    pub fn as_policy_sample(&self, policy_id: &str) -> PolicyAssetSample {
        let name_hex = self.asset_name.clone().unwrap_or_default();
        let has_fungible_signals = self
            .minting_tx_metadata
            .as_ref()
            .and_then(|meta| cip25_record_in(meta, policy_id, &name_hex))
            .and_then(|record| serde_json::from_value::<AssetMetadata>(record.clone()).ok())
            .is_some_and(|meta| meta.has_fungible_signals());

        PolicyAssetSample {
            name_hex,
            has_fungible_signals,
            total_supply: self.total_supply,
        }
    }
}

/// One asset's CIP-25 record inside a `721` payload.
///
/// CIP-25 v1 keys the asset name as UTF-8 and v2 as hex; both are tried, hex
/// first because it is unambiguous.
fn cip25_record_in<'a>(
    minting_tx_metadata: &'a serde_json::Value,
    policy_id: &str,
    asset_name_hex: &str,
) -> Option<&'a serde_json::Value> {
    let by_policy = minting_tx_metadata.get("721")?.get(policy_id)?;
    if let Some(record) = by_policy.get(asset_name_hex) {
        return Some(record);
    }
    let utf8 = hex::decode(asset_name_hex)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())?;
    by_policy.get(&utf8)
}

/// Row from `GET /policy_asset_addresses` — one (asset, holder) pair for a
/// policy.
///
/// Koios inlines `stake_address`, so a holder roll-up needs no second
/// address→stake resolution pass. It is absent for an enterprise address
/// (no staking part) and for script addresses that carry none.
#[derive(Debug, Serialize, Deserialize)]
pub struct KoiosPolicyAssetAddress {
    /// Hex-encoded asset name; null/absent for the empty name.
    pub asset_name: Option<String>,
    pub payment_address: String,
    pub stake_address: Option<String>,
    /// Koios serialises quantities as strings.
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub quantity: u64,
}

/// Row from `GET /policy_asset_list` — the cheapest per-policy asset
/// listing there is: name and supply, no metadata.
///
/// Use this to walk *which* assets a policy holds; follow with
/// [`KoiosApi::get_policy_assets`] for the ones whose metadata you need.
/// [`KoiosApi::get_policy_asset_info`] returns the same window with every
/// row's full `minting_tx_metadata` attached, which on a 10k-asset
/// collection is megabytes a name-only caller throws away.
#[derive(Debug, Serialize, Deserialize)]
pub struct KoiosPolicyAssetName {
    /// Hex-encoded asset name; null/absent for the empty name.
    pub asset_name: Option<String>,
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
            client: HttpClient::new().with_timeout(KOIOS_REQUEST_TIMEOUT),
            base_url: BASE_URL.to_string(),
        }
    }
}

impl KoiosApi {
    /// Build a client for a base URL, with an optional bearer token.
    ///
    /// The other constructors read the key from a Cloudflare Secrets Store and
    /// so are only usable inside a worker. This one takes the token directly,
    /// which is what a CLI or test harness needs — `KoiosApi` was otherwise
    /// unconstructible outside the worker runtime despite the crate itself
    /// building natively.
    ///
    /// Pass `None` for the keyless free tier.
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        let client = match api_key {
            Some(key) if !key.is_empty() => HttpClient::with_bearer_token(key),
            _ => HttpClient::new(),
        };
        Self {
            client: client.with_timeout(KOIOS_REQUEST_TIMEOUT),
            base_url: base_url.into(),
        }
    }

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
        let client = match api_key(env).await {
            Some(key) => HttpClient::with_bearer_token(key),
            None => HttpClient::new(),
        };
        Ok(Self {
            client: client.with_timeout(KOIOS_REQUEST_TIMEOUT),
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

    /// Batch confirmation depths for many tx hashes in one request
    /// (`POST /tx_status`). A hash absent from the response, or present with
    /// `num_confirmations == None`, is not yet on-chain.
    pub async fn get_tx_status(&self, hashes: &[String]) -> Result<Vec<TxStatus>, KoiosError> {
        let url = format!("{}/tx_status", self.base_url);
        self.post_json(
            &url,
            &TxStatusRequest {
                hashes: hashes.to_vec(),
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

    /// One asset's full mint/burn history (`POST /asset_history`), newest
    /// first.
    ///
    /// Needed whenever "which declaration is current" matters, because
    /// [`get_asset_info`](Self::get_asset_info) collapses an asset's history to
    /// a single `minting_tx_hash` and its `creation_time` can come from a
    /// *different* mint than that hash — so it is not a sound basis for
    /// "the latest mint" even when it happens to give it.
    pub async fn get_asset_history(
        &self,
        policy_id: &str,
        asset_name_hex: &str,
    ) -> Result<Option<KoiosAssetHistory>, KoiosError> {
        let url = format!("{}/asset_history", self.base_url);
        let mut rows: Vec<KoiosAssetHistory> = self
            .post_json(
                &url,
                &AssetHistoryRequest {
                    policy: policy_id.to_string(),
                    asset_name_hex: asset_name_hex.to_string(),
                },
            )
            .await?;
        Ok(rows.pop())
    }

    /// Metadata for transactions, by label (`POST /tx_metadata`).
    ///
    /// Separate from [`get_tx_details`](Self::get_tx_details), which asks
    /// `/tx_info` for inputs, scripts and assets but never sets `_metadata` —
    /// so its `metadata` field comes back empty.
    pub async fn get_tx_metadata(
        &self,
        hashes: &[String],
    ) -> Result<Vec<KoiosTxMetadata>, KoiosError> {
        let url = format!("{}/tx_metadata", self.base_url);
        self.post_json(
            &url,
            &TxMetadataRequest {
                hashes: hashes.to_vec(),
            },
        )
        .await
    }

    /// One asset's `POST /asset_info` row, or `None` if Koios has never seen
    /// it. The row carries the minting metadata, so this is the one call an
    /// image resolver needs on a network without an assets database.
    pub async fn get_asset_info(
        &self,
        policy_id: &str,
        asset_name_hex: &str,
    ) -> Result<Option<KoiosAssetInfo>, KoiosError> {
        let mut rows = self
            .get_policy_assets(&[(policy_id.to_string(), asset_name_hex.to_string())])
            .await?;
        Ok(rows.pop())
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

    /// Just the `(tx_hash, tx_index)` of an address's UTxOs
    /// (`POST /address_utxos`, `_extended=false`).
    ///
    /// For callers that only need to know WHICH UTxOs exist — a liveness or
    /// spent-check — rather than what is in them.
    ///
    /// `_extended=true` makes Koios inline every output's full asset list,
    /// and on a UTxO-rich wallet that dominates the whole request. Measured
    /// 2026-09-18 against a 944-UTxO wallet: **~10.5s and 1.3 MB extended,
    /// ~2.3s non-extended**, for a caller that then discarded everything
    /// except these two fields. That one call was most of the wall-clock in
    /// building a marketplace cancel.
    ///
    /// Prefer this over [`Self::get_address_utxos`] unless you actually read
    /// the value or asset list.
    pub async fn get_address_utxo_refs(
        &self,
        address: &str,
    ) -> Result<Vec<(String, u32)>, KoiosError> {
        let url = format!("{}/address_utxos", self.base_url);
        let rows: Vec<KoiosUtxoRef> = self
            .post_paginated(
                &url,
                &AddressUtxosRequest {
                    addresses: vec![address.to_string()],
                    extended: false,
                },
            )
            .await?;
        Ok(rows.into_iter().map(|r| (r.tx_hash, r.tx_index)).collect())
    }

    /// Resolve a set of UTxO references (`"tx_hash#index"`) to their full
    /// extended UTxOs (`POST /utxo_info`, `_extended=true`) — the direct
    /// equivalent of Maestro's `/transactions/outputs`. Only *unspent* UTxOs
    /// are returned; spent references are omitted.
    pub async fn get_utxo_info(&self, utxo_refs: &[String]) -> Result<Vec<KoiosUtxo>, KoiosError> {
        if utxo_refs.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/utxo_info", self.base_url);
        self.post_paginated(
            &url,
            &UtxoRefsRequest {
                utxo_refs: utxo_refs.to_vec(),
                extended: true,
            },
        )
        .await
    }

    /// Every unspent UTxO currently holding one of `assets`
    /// (`POST /asset_utxos`, `_extended=true`).
    ///
    /// The lookup a state-NFT-identified contract needs: an AMM pool, a
    /// launchpad pool or any other singleton whose UTxO moves on every
    /// interaction is found by the token that follows it, never by a UTxO
    /// reference somebody cached. Assets are `(policy_id, asset_name_hex)`.
    pub async fn get_asset_utxos(
        &self,
        assets: &[(String, String)],
    ) -> Result<Vec<KoiosUtxo>, KoiosError> {
        if assets.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/asset_utxos", self.base_url);
        self.post_paginated(
            &url,
            &AssetUtxosRequest {
                asset_list: assets
                    .iter()
                    .map(|(policy, name)| [policy.clone(), name.clone()])
                    .collect(),
                extended: true,
            },
        )
        .await
    }

    /// Resolve datums by hash (`POST /datum_info`) — returns the CBOR preimage
    /// (`bytes`) for each hash the node holds. Used to witness hash-kind datums.
    pub async fn get_datum_info(
        &self,
        datum_hashes: &[String],
    ) -> Result<Vec<KoiosDatumInfo>, KoiosError> {
        if datum_hashes.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/datum_info", self.base_url);
        self.post_paginated(
            &url,
            &DatumInfoRequest {
                datum_hashes: datum_hashes.to_vec(),
            },
        )
        .await
    }

    /// Whole transactions as CBOR (`POST /tx_cbor`).
    pub async fn get_tx_cbor(&self, tx_hashes: &[String]) -> Result<Vec<KoiosTxCbor>, KoiosError> {
        if tx_hashes.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/tx_cbor", self.base_url);
        self.post_paginated(
            &url,
            &TxCborRequest {
                tx_hashes: tx_hashes.to_vec(),
            },
        )
        .await
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

    /// A policy's assets with name, supply and mint metadata
    /// (`GET /policy_asset_info`), one page.
    ///
    /// `limits` of `None` takes Koios's default page (capped at 1000);
    /// fungible-token callers wanting the primary asset sort by supply and
    /// take the head.
    pub async fn get_policy_asset_info(
        &self,
        policy_id: &str,
        limits: Option<KoiosLimits>,
    ) -> Result<Vec<KoiosPolicyAssetInfo>, KoiosError> {
        let url = format!(
            "{}/policy_asset_info?_asset_policy={policy_id}",
            self.base_url
        );
        let options = limits.map(QueryOptions::from);
        self.get_json_with_options(&url, options.as_ref()).await
    }

    /// Decide whether a policy is an NFT collection, a currency or a set of
    /// editions, from the first `sample_size` assets minted under it.
    ///
    /// One request. The sample is a prefix of the policy's assets, not a
    /// random draw, so a collection that mints its currency first could in
    /// principle read as a currency — which is why
    /// [`cardano_assets::classify_policy`] settles on CIP-67 labels wherever
    /// they exist and only falls back to supply heuristics when nothing
    /// declared itself.
    pub async fn classify_policy(
        &self,
        policy_id: &str,
        sample_size: u32,
    ) -> Result<PolicyClassification, KoiosError> {
        let rows = self
            .get_policy_asset_info(policy_id, Some(KoiosLimits::only(sample_size)))
            .await?;
        let samples: Vec<PolicyAssetSample> = rows
            .iter()
            .map(|row| row.as_policy_sample(policy_id))
            .collect();
        Ok(cardano_assets::classify_policy(&samples))
    }

    /// Every asset under a policy, with full metadata, walking the pages.
    ///
    /// Two requests per page: the asset names in the window, then their
    /// metadata. Listing and metadata are separate Koios routes and there is
    /// no single route that pages full `asset_info` rows by policy. The
    /// policy's own empty-named row (the CIP-27 royalty token) is not an asset
    /// and is dropped before the metadata request.
    ///
    /// ⚠️ Unbounded in the size of the policy, and each row carries its whole
    /// metadata document — a 10k collection is ten listing requests, ten
    /// metadata requests and megabytes of JSON. Fine for a one-shot bootstrap;
    /// page it yourself if you are on a request or wall-clock budget.
    pub async fn get_all_policy_assets(
        &self,
        policy_id: &str,
    ) -> Result<Vec<KoiosAssetInfo>, KoiosError> {
        let mut all = Vec::new();
        let mut offset = 0u32;

        loop {
            // The listing page size bounds the walk, so ask the listing how
            // many rows it had rather than inferring from the metadata rows,
            // which are fewer whenever a page holds the royalty token.
            let names = self
                .get_policy_asset_names(policy_id, KoiosLimits::new(KOIOS_PAGE_LIMIT, Some(offset)))
                .await?;
            let listed = names.len() as u32;

            let wanted: Vec<(String, String)> = names
                .into_iter()
                .filter_map(|row| row.asset_name)
                .filter(|name| !name.is_empty())
                .map(|name| (policy_id.to_string(), name))
                .collect();

            if !wanted.is_empty() {
                all.extend(self.get_policy_assets(&wanted).await?);
            }

            if listed < KOIOS_PAGE_LIMIT {
                break;
            }
            offset += KOIOS_PAGE_LIMIT;
        }

        Ok(all)
    }

    /// One page of a policy's asset names (`GET /policy_asset_list`).
    ///
    /// Name and supply only — see [`KoiosPolicyAssetName`] for why this is
    /// the right call to walk a large policy with.
    pub async fn get_policy_asset_names(
        &self,
        policy_id: &str,
        limits: KoiosLimits,
    ) -> Result<Vec<KoiosPolicyAssetName>, KoiosError> {
        let url = format!(
            "{}/policy_asset_list?_asset_policy={policy_id}",
            self.base_url
        );
        self.get_json_with_options(&url, Some(&QueryOptions::from(limits)))
            .await
    }

    /// Every (asset, holder) pair under a policy
    /// (`GET /policy_asset_addresses`), paging until exhausted.
    ///
    /// Each row carries the holder's `stake_address` inline, so a holder
    /// roll-up is one call — not a listing followed by an address→stake
    /// resolution pass.
    ///
    /// ⚠️ This is unbounded in the size of the policy: a 10k-asset
    /// collection held across 4k wallets is 10k rows, ten pages. Callers on
    /// a request budget should page it themselves via
    /// [`Self::get_policy_asset_addresses_page`].
    pub async fn get_policy_asset_addresses(
        &self,
        policy_id: &str,
    ) -> Result<Vec<KoiosPolicyAssetAddress>, KoiosError> {
        let mut all = Vec::new();
        let mut offset = 0u32;

        loop {
            let page = self
                .get_policy_asset_addresses_page(
                    policy_id,
                    KoiosLimits::new(KOIOS_PAGE_LIMIT, Some(offset)),
                )
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

    /// One page of [`Self::get_policy_asset_addresses`].
    pub async fn get_policy_asset_addresses_page(
        &self,
        policy_id: &str,
        limits: KoiosLimits,
    ) -> Result<Vec<KoiosPolicyAssetAddress>, KoiosError> {
        let url = format!(
            "{}/policy_asset_addresses?_asset_policy={policy_id}",
            self.base_url
        );
        self.get_json_with_options(&url, Some(&QueryOptions::from(limits)))
            .await
    }

    pub async fn get_policy_asset_mints(
        &self,
        policy_id: &str,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<PolicyAssetMint>, KoiosError> {
        let url = format!(
            "{}/policy_asset_mints?_asset_policy={policy_id}",
            self.base_url
        );
        self.get_json_with_options(&url, options).await
    }

    /// Get policy asset mints as raw JSON to explore available fields
    pub async fn get_policy_asset_mints_raw(
        &self,
        policy_id: &str,
        options: Option<&QueryOptions>,
    ) -> Result<serde_json::Value, KoiosError> {
        let url = format!(
            "{}/policy_asset_mints?_asset_policy={policy_id}",
            self.base_url
        );
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

        let mut attempt = 1;
        loop {
            match self.client.get::<R>(&final_url).await {
                Ok(result) => return Ok(result),
                Err(e) => match Recovery::for_failure(&e, attempt) {
                    Recovery::RetryAfter(wait) => {
                        warn!(
                            "Koios refused attempt {attempt} of {final_url} ({e}); retrying in {}ms",
                            wait.as_millis()
                        );
                        pause(wait).await;
                        attempt += 1;
                    }
                    Recovery::GiveUp => return Err(koios_error(e)),
                },
            }
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

        // The detailed request keeps the response body on a non-2xx status.
        // Koios's Ogmios passthrough answers a failed evaluation with a 400
        // whose body names the reason (a script that refused, a body the
        // ledger could not decode); a bare status code throws that away. It
        // also keeps the headers, so a 429's `Retry-After` is honoured.
        let mut attempt = 1;
        loop {
            match self
                .client
                .post_with_details::<T, R>(&final_url, body)
                .await
            {
                Ok(details) => return Ok(details.data),
                Err(e) => match Recovery::for_failure(&e, attempt) {
                    Recovery::RetryAfter(wait) => {
                        warn!(
                            "Koios refused attempt {attempt} of {final_url} ({e}); retrying in {}ms",
                            wait.as_millis()
                        );
                        pause(wait).await;
                        attempt += 1;
                    }
                    Recovery::GiveUp => return Err(koios_error(e)),
                },
            }
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

    fn refused(status_code: u16, retry_after: Option<&str>) -> HttpError {
        HttpError::HttpStatus {
            status_code,
            headers: retry_after
                .map(|secs| [("retry-after".to_string(), secs.to_string())].into())
                .unwrap_or_default(),
            body: String::new(),
        }
    }

    /// A rate limit is waited out for exactly as long as Koios asks.
    #[test]
    fn test_recovery_honours_retry_after() {
        assert!(matches!(
            Recovery::for_failure(&refused(429, Some("2")), 1),
            Recovery::RetryAfter(wait) if wait == Duration::from_secs(2)
        ));
    }

    /// Asked to wait longer than a watched build should stall, fail now.
    #[test]
    fn test_recovery_gives_up_on_a_long_retry_after() {
        assert!(matches!(
            Recovery::for_failure(&refused(429, Some("60")), 1),
            Recovery::GiveUp
        ));
    }

    /// With no `Retry-After`, a gateway failure backs off further each attempt.
    #[test]
    fn test_recovery_backs_off_without_retry_after() {
        assert!(matches!(
            Recovery::for_failure(&refused(503, None), 1),
            Recovery::RetryAfter(wait) if wait == Duration::from_millis(500)
        ));
        assert!(matches!(
            Recovery::for_failure(&refused(503, None), 2),
            Recovery::RetryAfter(wait) if wait == Duration::from_millis(1000)
        ));
    }

    /// The last attempt is final, whatever the failure.
    #[test]
    fn test_recovery_stops_after_the_last_attempt() {
        assert!(matches!(
            Recovery::for_failure(&refused(429, Some("1")), KOIOS_ATTEMPTS),
            Recovery::GiveUp
        ));
    }

    /// A refused evaluation (400) and a deterministic query failure (500) come
    /// back the same way every time, so they surface at once with their body.
    #[test]
    fn test_recovery_does_not_retry_a_deterministic_failure() {
        assert!(matches!(
            Recovery::for_failure(&refused(400, None), 1),
            Recovery::GiveUp
        ));
        assert!(matches!(
            Recovery::for_failure(&refused(500, None), 1),
            Recovery::GiveUp
        ));
    }

    /// A timeout has already spent its whole budget; a second would double it.
    #[test]
    fn test_recovery_does_not_retry_a_timeout() {
        let timed_out = HttpError::Timeout {
            after: KOIOS_REQUEST_TIMEOUT,
        };
        assert!(matches!(
            Recovery::for_failure(&timed_out, 1),
            Recovery::GiveUp
        ));
    }

    /// wasm's plain GET reports the status only inside its message.
    #[test]
    fn test_recovery_reads_a_status_folded_into_a_message() {
        let gateway = HttpError::Custom("HTTP request failed with status: 502".to_string());
        assert!(matches!(
            Recovery::for_failure(&gateway, 1),
            Recovery::RetryAfter(_)
        ));
    }

    /// `/datum_info` rows key the hash as `datum_hash`. Captured from a
    /// live response — the previous `hash` spelling failed the whole
    /// batch with `missing field \`hash\``, which surfaced downstream as
    /// an unbuildable cancel-offer cart.
    #[test]
    fn test_datum_info_deserialises_koios_wire_shape() {
        let body = r#"[
          {"datum_hash":"14420b67fb7c9ef762b9658d88108a6f76b4af2a72f66dbf828d2bb0adb2b015",
           "creation_tx_hash":"73c1524c4a0e0c921d7ac4b4388cb5a1e6b1757b515e4189df788e28953fce98",
           "bytes":"d8799f581ccba51a2e5bff",
           "value":{"constructor":0}}
        ]"#;
        let rows: Vec<KoiosDatumInfo> = serde_json::from_str(body).expect("datum_info wire shape");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].hash,
            "14420b67fb7c9ef762b9658d88108a6f76b4af2a72f66dbf828d2bb0adb2b015"
        );
        assert_eq!(rows[0].bytes.as_deref(), Some("d8799f581ccba51a2e5bff"));
    }

    /// The bare `hash` spelling stays accepted via alias, so a mirror or
    /// proxy emitting the older shape doesn't break resolution.
    #[test]
    fn test_datum_info_accepts_bare_hash_alias() {
        let body =
            r#"[{"hash":"ec3131073f6c9c0ad76835c886fc467a796f50343b1630e5d50dc92e87c772d2"}]"#;
        let rows: Vec<KoiosDatumInfo> = serde_json::from_str(body).expect("bare hash alias");
        assert_eq!(
            rows[0].hash,
            "ec3131073f6c9c0ad76835c886fc467a796f50343b1630e5d50dc92e87c772d2"
        );
        assert!(rows[0].bytes.is_none());
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

    /// Live `POST /asset_info` responses, captured 2026-09-20.
    ///
    /// `cip25` is Boss Cat Rocket Club #9717 — the asset whose lookup was
    /// failing when Maestro's gateway went dark, and a policy whose `721`
    /// payload carries the whole mint batch, so selecting the right record
    /// out of it is the thing being tested.
    ///
    /// `cip68` is the ADA Handle `$thiya` user token: a datum-backed asset
    /// that *also* carries a `721` payload, which is the one case where the
    /// two standards disagree about what an asset is.
    fn asset_info(fixture: &str) -> KoiosAssetInfo {
        serde_json::from_str::<Vec<KoiosAssetInfo>>(fixture)
            .expect("fixture decodes")
            .pop()
            .expect("fixture has a row")
    }

    #[test]
    fn cip25_asset_resolves_its_own_record_out_of_the_mint_payload() {
        let info = asset_info(test_case!("cip25_asset_info.json"));
        let asset = info.to_asset().expect("resolves");

        assert_eq!(asset.name, "Boss Cat Rocket Club #9717");
        assert_eq!(
            asset.image,
            "ipfs://QmUdkbDDaeu9dkZ3CGpqCUkdoTFRadhhwYWzLuFvodHhhY"
        );
        // The neighbouring assets in the same `721` payload have different
        // traits; picking a sibling's record would still "resolve".
        assert_eq!(
            asset.traits.get("Fur").map(Vec::as_slice),
            Some(["Cyborg".to_owned()].as_slice())
        );
        assert_eq!(info.metadata_kind(), MetadataKind::Cip25);
        assert!(info.should_import());
    }

    #[test]
    fn cip68_asset_resolves_from_its_datum() {
        let info = asset_info(test_case!("cip68_asset_info.json"));
        let asset = info.to_asset().expect("resolves");

        assert_eq!(asset.name, "$thiya");
        assert_eq!(
            asset.image,
            "ipfs://zb2rhbpf2ov4KQA9S1dW9rGbZzHRoWqucSmgUdLUP77fNVXCu"
        );
        assert_eq!(asset.media_type.as_deref(), Some("image/jpeg"));

        let typed = info.cip68_metadata68().expect("typed metadata");
        assert_eq!(typed.purpose, NftPurpose::UserNft);
        assert!(info.should_import());
    }

    /// This asset carries both standards, and they disagree. CIP-25 wins the
    /// stored kind — matching the Maestro-era classification the `asset` DB
    /// rows were written under, so the cutover does not re-key them.
    #[test]
    fn a_dual_standard_asset_is_still_classified_cip25() {
        let info = asset_info(test_case!("cip68_asset_info.json"));
        assert!(info.cip25_record().is_some());
        assert!(info.cip68_record().is_some());
        assert_eq!(info.metadata_kind(), MetadataKind::Cip25);
    }

    /// A live `GET /policy_asset_addresses` page, captured 2026-09-20: three
    /// rows at staked addresses and two at a script address with no staking
    /// part. Both the string-encoded `quantity` and the null `stake_address`
    /// are wire shapes a typed struct gets wrong by default.
    #[test]
    fn test_deserialize_policy_asset_addresses() {
        let rows: Vec<KoiosPolicyAssetAddress> =
            serde_json::from_str(test_case!("policy_asset_addresses.json"))
                .expect("fixture decodes");

        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].quantity, 1);
        assert_eq!(
            rows[0].stake_address.as_deref(),
            Some("stake1uxqh9rn76n8nynsnyvf4ulndjv0srcc8jtvumut3989cqmgjt49h6")
        );
        // A script address holds these two; there is no stake credential to
        // key them by.
        assert!(rows[3].stake_address.is_none());
        assert!(rows[4].stake_address.is_none());
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
                assert_eq!(
                    jpg_movement.from_address,
                    "addr1q88nlawx6kkrpxkzuvreak9tq6y3chdfu29uhqv0yhe436xx0t0gdpe5aullxhvze42uhkf90zpm907jydk8g6x4z9sqzt0w5s"
                );
                assert_eq!(
                    jpg_movement.to_address,
                    "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdkrxp0e2m4utglc7hmzkuta3e2td72cdjq9m9xlfn6rz8vq86l65l"
                );
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
