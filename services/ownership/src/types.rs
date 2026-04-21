//! Request and response types for the collection-ownership service.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Bundle
// ============================================================================

/// Parameters for paginated bundle queries.
#[derive(Debug, Default)]
pub struct BundleParams<'a> {
    /// Filter to a single holder's assets.
    pub stake: Option<&'a str>,
    /// Max entries per page.
    pub limit: Option<u32>,
    /// Keyset cursor — fetch entries after this asset_name_hex.
    pub cursor: Option<&'a str>,
    /// Whether to return decoded traits (instead of raw bitmaps).
    pub decode_traits: bool,
    /// Filter to assets that have these trait bits set.
    pub trait_bits: Vec<usize>,
}

/// Response for `GET /api/bundle/{policy_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleResponse {
    pub cache_generation: u64,
    pub schema_version: u64,
    pub bitmap_size: u32,
    /// Total collection asset count (from cached meta).
    pub asset_count: Option<u32>,
    pub entries: Vec<BundleEntry>,
    /// Cursor for the next page. `None` if this is the last page.
    pub next_cursor: Option<String>,
}

/// A single entry in the ownership bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleEntry {
    pub asset_name_hex: String,
    /// Human-readable asset name (e.g. "Skeleton King").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub owner_stake: String,
    /// Trait data — bitmap (default) or decoded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trait_data: Option<TraitData>,
    /// Legacy cnft.tools rarity rank.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity_rank: Option<u32>,
    /// Multiple rarity rankings (statistical, IC, cnft.tools).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<RarityRanks>,
}

/// Rarity rankings computed by multiple scoring algorithms.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RarityRanks {
    /// Magic Eden statistical rarity (product of trait probabilities). Rank 1 = rarest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statistical: Option<u32>,
    /// OpenRarity information content (entropy-normalized). Rank 1 = rarest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub information_content: Option<u32>,
    /// Legacy cnft.tools rank (from upstream API). Rank 1 = rarest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cnft_tools: Option<u32>,
}

/// Trait data in either compact bitmap or decoded form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TraitData {
    /// Hex-encoded bitmap string (compact, requires schema to decode).
    Bitmap(String),
    /// Decoded trait key-value pairs.
    Decoded(HashMap<String, Vec<String>>),
}

// ============================================================================
// Owner / Check
// ============================================================================

/// Response for `GET /api/owner/{policy_id}?asset=...`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerResponse {
    pub owner_stake: Option<String>,
}

/// Response for `GET /api/check/{policy_id}?asset=...&stake=...`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResponse {
    pub owned: bool,
}

// ============================================================================
// Stats
// ============================================================================

/// Response for `GET /api/stats/{policy_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub asset_count: u32,
    pub holder_count: u32,
    pub last_updated: Option<u64>,
}

// ============================================================================
// Changes Feed
// ============================================================================

/// Response for `GET /api/changes/{policy_id}?since=...&limit=...`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesFeedResponse {
    pub results: Vec<ChangeEntry>,
    pub last_seq: u64,
    pub pending: u64,
}

/// A single ownership change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEntry {
    pub seq: u64,
    pub asset_name_hex: String,
    pub from_stake: Option<String>,
    pub to_stake: Option<String>,
    pub tx_hash: Option<String>,
    pub timestamp: u64,
}

// ============================================================================
// Trait Schema
// ============================================================================

/// Response for `GET /api/trait-schema/{policy_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitSchemaResponse {
    pub schema_version: u64,
    pub bitmap_size: u32,
    /// Mapping of "TraitName:Value" → bit position.
    pub traits: HashMap<String, usize>,
}

// ============================================================================
// Fingerprint Resolution
// ============================================================================

/// A resolved asset identity from CIP-14 fingerprint lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIdentity {
    pub policy_id: String,
    pub asset_name_hex: String,
    pub fingerprint: String,
}
