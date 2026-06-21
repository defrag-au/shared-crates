//! Wire types for the policy-info service.
//!
//! Used by both `policy-info-client` (HTTP-facing consumers) and
//! `cnft.dev-workers/services/policy-info` (the service implementation,
//! which re-exports these from its local `types` module).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use cardano_assets::TokenType;

/// Source that verified a policy's identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSource {
    TokenRegistry,
    CollectionOwnership,
    JpgStore,
    Wayup,
}

/// Warning/safety tag applied to a policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyTag {
    Rugpull,
    Compromised,
    Deprecated,
    Unverified,
    Copymint,
}

/// Pre-resolved metadata for a Cardano minting policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPolicy {
    pub name: String,
    pub token_type: TokenType,
    pub icon_url: Option<String>,
    /// Fungible-token decimal places (CF Token Registry `decimals`).
    /// Defaults to `0` — the Cardano convention for NFTs and tokens with no
    /// registry entry (raw base units). Consumers divide on-chain amounts by
    /// `10^decimals` to render human-scaled values (e.g. `$ANGELS`).
    #[serde(default)]
    pub decimals: u8,
    #[serde(default)]
    pub verified_by: Vec<VerificationSource>,
    #[serde(default)]
    pub tags: Vec<PolicyTag>,
}

/// One token search result row (`GET /api/policy/search?q=`).
///
/// A flattened, render-ready projection of a [`ResolvedPolicy`] paired with
/// the registry `subject` it was found under. The frontend search landing
/// renders these directly (icon + name + verified/rug chips) and navigates
/// to `/{policy_id}/` when one is chosen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySearchResult {
    /// Full registry subject — a bare 56-hex `policy_id` (NFT policies) or
    /// `policy_id + asset_name_hex` (fungibles). The canonical lookup key.
    pub subject: String,
    /// Bare 56-hex policy id (the leading 56 chars of `subject`). The
    /// holder dashboard is per-policy, so navigation targets this.
    pub policy_id: String,
    /// Display name (registry name / ticker).
    pub name: String,
    /// Token icon, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// Fungible-token decimals (CF registry); `0` for NFTs / unknown.
    #[serde(default)]
    pub decimals: u8,
    /// True when at least one source verified the policy's identity.
    #[serde(default)]
    pub verified: bool,
    /// Safety tags (rug / compromised / copymint / …) — a trust signal.
    #[serde(default)]
    pub tags: Vec<PolicyTag>,
}

/// Response for `GET /api/policy/search?q=` — ranked search results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicySearchResponse {
    pub results: Vec<PolicySearchResult>,
}

/// Request body for `POST /api/policies` — batch resolve multiple subjects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPolicyRequest {
    pub subjects: Vec<String>,
}

/// Response for `POST /api/policies` — map of subject string to resolved policy.
/// Only successfully resolved subjects are included in the map.
pub type BatchPolicyResponse = HashMap<String, ResolvedPolicy>;
