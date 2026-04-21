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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

impl BundleEntry {
    /// Get the raw bitmap hex string, if trait data is in bitmap form.
    pub fn trait_bitmap(&self) -> Option<&str> {
        match &self.trait_data {
            Some(TraitData::Bitmap(hex)) => Some(hex),
            _ => None,
        }
    }

    /// Get decoded traits, if trait data is in decoded form.
    pub fn traits_decoded(&self) -> Option<&HashMap<String, Vec<String>>> {
        match &self.trait_data {
            Some(TraitData::Decoded(map)) => Some(map),
            _ => None,
        }
    }
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

// ============================================================================
// Sync Status
// ============================================================================

/// Response for `GET /api/status/{policy_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusResponse {
    pub phase: String,
    pub sync_sources: Vec<String>,
    pub asset_count: u32,
    pub holder_count: u32,
    #[serde(default)]
    pub last_synced_at: Option<u64>,
    #[serde(default)]
    pub last_sync_source: Option<String>,
    #[serde(default)]
    pub cache_generation: Option<u64>,
    #[serde(default)]
    pub last_alarm_at: Option<u64>,
    #[serde(default)]
    pub disabled_reason: Option<String>,
}

// ============================================================================
// Admin Types
// ============================================================================

/// A tracked policy entry from the admin API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub policy_id: String,
    pub label: String,
    pub enabled: i32,
    pub asset_count: i64,
    pub holder_count: i64,
    pub sync_interval_secs: i64,
    #[serde(default)]
    pub last_synced_at: Option<i64>,
    #[serde(default)]
    pub disabled_reason: Option<String>,
}

/// Response for `GET /admin/policies`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyListResponse {
    pub policies: Vec<PolicyEntry>,
}

/// Partial update for a policy (PATCH body).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PolicyUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_interval_secs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Response for `GET /admin/validate`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateResponse {
    pub total_policies: usize,
    pub healthy: usize,
    pub issues: Vec<PolicyIssue>,
}

/// A policy with one or more failed validation checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyIssue {
    pub policy_id: String,
    pub label: String,
    pub checks: Vec<FailedCheck>,
}

/// A single failed validation check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedCheck {
    pub check: String,
    pub severity: String,
    pub detail: String,
}

// ============================================================================
// Visual Analysis
// ============================================================================

/// Visual style guide for a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualGuide {
    pub art_style: String,
    pub color_palette: serde_json::Value,
    pub subject_form: String,
    pub composition: String,
    pub motifs: Vec<String>,
    pub summary: String,
}

/// Response for `GET /admin/visual-analysis/{policy_id}/guide`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualGuideResponse {
    pub guide: VisualGuide,
    pub sample_count: u32,
    pub model_used: String,
    pub updated_at: i64,
}

/// Narrative style guide for a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeStyleGuide {
    pub world_tone: String,
    pub narrative_voice: String,
    pub character_archetypes: Vec<serde_json::Value>,
    pub vocabulary_palette: Vec<String>,
    pub recurring_tensions: Vec<String>,
    pub world_premise: String,
}

/// Response for `GET /admin/visual-analysis/{policy_id}/narrative`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeGuideResponse {
    pub guide: NarrativeStyleGuide,
    pub model_used: String,
    pub updated_at: i64,
}

/// Visual profile for a single asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualProfile {
    pub description: String,
    pub distinctive_features: Vec<String>,
    pub character_read: String,
    pub color_signature: Vec<String>,
    pub alt_text: String,
}
