//! Wire types for the OpenSea v2 responses this client reads.
//!
//! Every field below is taken from a **captured response**, not from
//! documentation: the files in `tests/fixtures/` are the verbatim bodies of the
//! requests named on each type, and `tests/fixtures.rs` asserts against them. A
//! field absent from every capture is absent here rather than guessed at.
//!
//! `Option` marks two different things, and the doc comment on each field says
//! which: a field whose `null` appears in a capture, and a field that is
//! non-null in every capture but has a real absent case (an unlisted collection
//! has no floor price; a contract can be unnamed). The second group is `Option`
//! deliberately, so one odd item cannot fail a page of thousands.

use serde::{Deserialize, Serialize};

/// An OpenSea chain slug — `"robinhood"`, `"ethereum"`, `"base"`.
///
/// A newtype over the slug rather than an enum, because the set is open: the
/// live `/chains` answer carries thirty of them and OpenSea adds more. It is the
/// same call `chains::EvmChain` makes with its `Id` variant for an EIP-155 id it
/// has not named yet.
///
/// Serialised as the bare string — `"chain": "robinhood"` — not as a nested
/// struct, which is the shape a derived single-field enum would have written.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChainSlug(String);

impl ChainSlug {
    /// `robinhood` — Robinhood Chain, chain id 4663. Verified live: this slug is
    /// what the chain's own collections are indexed under.
    pub const ROBINHOOD: &'static str = "robinhood";
    /// `ethereum`.
    pub const ETHEREUM: &'static str = "ethereum";

    pub fn new(slug: impl Into<String>) -> Self {
        Self(slug.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ChainSlug {
    fn from(slug: &str) -> Self {
        Self(slug.to_owned())
    }
}

impl From<String> for ChainSlug {
    fn from(slug: String) -> Self {
        Self(slug)
    }
}

impl std::fmt::Display for ChainSlug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ============================================================================
// GET /chains
// ============================================================================

/// One entry of `GET /chains` — fixture `chains.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainInfo {
    pub chain: ChainSlug,
    pub name: String,
    pub symbol: String,
    pub supports_swaps: bool,
    pub block_explorer: String,
    pub block_explorer_url: String,
}

/// The `{"chains": […]}` envelope. [`crate::OpenseaClient::chains`] returns the
/// vector directly; this exists so the wire shape has a name.
#[derive(Debug, Deserialize)]
pub(crate) struct ChainList {
    pub chains: Vec<ChainInfo>,
}

// ============================================================================
// GET /collections, GET /collections/{slug}
// ============================================================================

/// A contract address paired with the chain it lives on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractRef {
    pub address: String,
    pub chain: ChainSlug,
}

/// A collection as `GET /collections` returns it — fixture
/// `collections_robinhood_page.json`.
///
/// This is also the shared half of [`CollectionDetails`], which composes it with
/// `#[serde(flatten)]`: the detail response is this shape plus more, and both
/// captures agree on every field here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionSummary {
    /// The slug — `"heritage-hood"` — which is the key every other endpoint takes.
    pub collection: String,
    pub name: String,
    /// Empty string in the captures, never absent.
    pub description: String,
    pub image_url: Option<String>,
    /// `null` in every list entry captured, while the detail response for the
    /// same collection carries a URL — the two endpoints genuinely differ here,
    /// so a caller who needs the banner has to take the detail response.
    pub banner_image_url: Option<String>,
    pub owner: String,
    pub safelist_status: String,
    /// Empty string in the captures, never absent.
    pub category: String,
    pub is_disabled: bool,
    pub is_nsfw: bool,
    pub trait_offers_enabled: bool,
    pub collection_offers_enabled: bool,
    pub opensea_url: String,
    /// Empty string in the captures, never absent.
    pub project_url: String,
    /// Empty string in the captures, never absent.
    pub wiki_url: String,
    /// Empty string in the captures, never absent.
    pub discord_url: String,
    /// Empty string in the captures, never absent.
    pub telegram_url: String,
    pub twitter_username: Option<String>,
    /// Empty string in the captures, never absent.
    pub instagram_username: String,
    pub contracts: Vec<ContractRef>,
}

/// One page of `GET /collections?chain=…`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionPage {
    pub collections: Vec<CollectionSummary>,
    /// The cursor for the following page, or `None` on the last one. Pass it to
    /// [`crate::OpenseaClient::collections_page`] unchanged; the client encodes it.
    pub next: Option<String>,
}

/// A fee OpenSea records for a collection, as a percentage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionFee {
    /// Percent, not a fraction: the capture carries `1.0` for a 1% fee.
    pub fee: f64,
    pub recipient: String,
    pub required: bool,
}

/// The currency a collection's listings or offers are denominated in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PricingCurrency {
    /// `"USDG"` on Robinhood Chain.
    pub symbol: String,
    pub address: String,
    pub chain: ChainSlug,
    pub image: String,
    /// `"Global Dollar"`.
    pub name: String,
    /// Token decimals — `6` for USDG.
    pub decimals: u32,
    /// A decimal **string**, as OpenSea sends prices. Kept as text rather than
    /// parsed to `f64` so the crate does not choose a precision on your behalf.
    pub eth_price: String,
    /// A decimal string, on the same reasoning as [`Self::eth_price`].
    pub usd_price: String,
}

/// The listing and offer currencies of a collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PricingCurrencies {
    pub listing_currency: PricingCurrency,
    pub offer_currency: PricingCurrency,
}

/// `GET /collections/{slug}` — fixture `collection_heritage_hood.json`.
///
/// The fields [`CollectionSummary`] does not carry are the ones only this
/// endpoint returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionDetails {
    #[serde(flatten)]
    pub summary: CollectionSummary,
    pub editors: Vec<String>,
    pub fees: Vec<CollectionFee>,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub total_supply: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub unique_item_count: u64,
    /// `"2026-09-26"` — a date, not a timestamp.
    pub created_date: String,
    pub pricing_currencies: PricingCurrencies,
}

// ============================================================================
// GET /collections/{slug}/stats
// ============================================================================

/// `GET /collections/{slug}/stats` — fixture
/// `collection_heritage_hood_stats.json`.
///
/// The two-level shape is OpenSea's: a lifetime `total` and a fixed set of
/// `intervals` (one day, seven day, thirty day).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionStats {
    pub total: StatsTotal,
    pub intervals: Vec<StatsInterval>,
}

/// Lifetime figures for a collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsTotal {
    /// Denominated in [`Self::volume_symbol`] — ETH in the capture.
    pub volume: f64,
    pub volume_symbol: String,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub sales: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub num_owners: u64,
    /// Not `null` in the captures; `Option` because a collection with no listings
    /// has no floor, and that must not fail a whole census.
    pub floor_price: Option<f64>,
    /// `"USDG"` in the capture — note this can differ from
    /// [`Self::volume_symbol`], because the two measure different things.
    pub floor_price_symbol: String,
}

/// One bucket of the `intervals` array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsInterval {
    /// `"one_day"`, `"seven_day"`, `"thirty_day"` in the captures. A `String`
    /// rather than an enum because the set is not closed by anything verified here.
    pub interval: String,
    pub volume: f64,
    pub volume_symbol: String,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub sales: u64,
}

// ============================================================================
// GET /chain/{chain}/contract/{address}
// ============================================================================

/// `GET /chain/{chain}/contract/{address}` — fixture `contract_stonkbrokers.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub address: String,
    pub chain: ChainSlug,
    /// The slug of the collection this contract belongs to. It is *not* always the
    /// human-readable slug: the capture gives `stonkbrokers-434284142` for a
    /// contract whose OpenSea URL slug is `stonkbrokers`.
    pub collection: String,
    /// `"erc721"` or `"erc1155"` — lowercase, as the API sends it.
    pub contract_standard: String,
    /// `Some("StonkBrokers")` in the capture. `Option` because a contract can be
    /// unnamed, and one of those should not fail a page of thousands.
    pub name: Option<String>,
}

// ============================================================================
// GET /chain/{chain}/contract/{address}/nfts
// ============================================================================

/// One trait of an NFT, as OpenSea indexes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trait {
    pub trait_type: String,
    pub display_type: Option<String>,
    pub max_value: Option<String>,
    /// Always a string in the capture, including for the numeric-looking values.
    pub value: String,
}

/// One NFT of `GET /chain/{chain}/contract/{address}/nfts` — fixture
/// `contract_stonkbrokers_nfts_page.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nft {
    /// A string, not an integer: token ids here are big and not always numeric.
    pub identifier: String,
    pub collection: String,
    pub contract: String,
    pub token_standard: String,
    /// Present in the capture. `Option` because unnamed tokens are ordinary.
    pub name: Option<String>,
    pub description: String,
    pub image_url: String,
    pub display_image_url: String,
    pub display_animation_url: Option<String>,
    pub metadata_url: Option<String>,
    pub opensea_url: String,
    /// `"2026-09-26T23:54:57.810195"` — naive local time, no offset, as sent.
    pub updated_at: String,
    pub is_disabled: bool,
    pub is_nsfw: bool,
    pub original_image_url: Option<String>,
    pub original_animation_url: Option<String>,
    pub traits: Vec<Trait>,
    /// `null` for every token in the capture.
    pub estimated_value_usd: Option<f64>,
    /// `null` for every token in the capture; an ERC-721 has no decimals.
    pub decimals: Option<u32>,
}

/// One page of `GET /chain/{chain}/contract/{address}/nfts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NftPage {
    pub nfts: Vec<Nft>,
    /// The cursor for the following page, or `None` on the last one.
    pub next: Option<String>,
}

// ============================================================================
// Errors
// ============================================================================

/// The body OpenSea sends with a refusal: `{"errors":["…"]}`.
#[derive(Debug, Deserialize)]
pub(crate) struct ApiErrorBody {
    pub errors: Vec<String>,
}
