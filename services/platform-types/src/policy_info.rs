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
    #[serde(default)]
    pub verified_by: Vec<VerificationSource>,
    #[serde(default)]
    pub tags: Vec<PolicyTag>,
}

/// Request body for `POST /api/policies` — batch resolve multiple subjects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPolicyRequest {
    pub subjects: Vec<String>,
}

/// Response for `POST /api/policies` — map of subject string to resolved policy.
/// Only successfully resolved subjects are included in the map.
pub type BatchPolicyResponse = HashMap<String, ResolvedPolicy>;
