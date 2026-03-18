use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Classification of a Cardano native token's fungibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    /// Non-fungible token (quantity = 1, unique asset name).
    Nft,
    /// Fungible token (quantity > 1, interchangeable units).
    Ft,
    /// Rich/semi-fungible token (CIP-68 with quantity semantics).
    Rft,
    /// Token type could not be determined.
    Unknown,
}

impl TokenType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Nft => "NFT",
            Self::Ft => "FT",
            Self::Rft => "RFT",
            Self::Unknown => "Token",
        }
    }
}

/// Source that verified a policy's identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSource {
    /// Cardano Token Registry (CIP-26).
    TokenRegistry,
    /// Our internal collection-ownership service.
    CollectionOwnership,
    /// jpg.store marketplace.
    JpgStore,
    /// WayUp marketplace.
    Wayup,
}

impl VerificationSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::TokenRegistry => "Token Registry",
            Self::CollectionOwnership => "cnft.dev",
            Self::JpgStore => "jpg.store",
            Self::Wayup => "WayUp",
        }
    }
}

/// Warning/safety tag applied to a policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyTag {
    /// Known rugpull.
    Rugpull,
    /// Policy key leaked or compromised.
    Compromised,
    /// Project shut down or abandoned.
    Deprecated,
    /// No verification source confirms identity.
    Unverified,
    /// Known counterfeit/copy of another project.
    Copymint,
}

impl PolicyTag {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rugpull => "Rugpull",
            Self::Compromised => "Compromised",
            Self::Deprecated => "Deprecated",
            Self::Unverified => "Unverified",
            Self::Copymint => "Copymint",
        }
    }

    /// Whether this tag represents a safety-critical warning.
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::Rugpull | Self::Compromised | Self::Copymint)
    }
}

/// Pre-resolved metadata for a Cardano minting policy.
///
/// This is what a [`PolicyResolver`] returns — a lightweight summary
/// that any UI or business logic can use without needing async access
/// to collection APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPolicy {
    /// Human-readable name (e.g., "SpaceBudz", "HOSKY", "Black Flag").
    pub name: String,
    /// Primary token type minted under this policy.
    pub token_type: TokenType,
    /// Icon URL (token registry logo, collection image from marketplace).
    /// UI layer handles loading; `None` = show color swatch fallback.
    pub icon_url: Option<String>,
    /// Sources that have verified this policy's identity.
    /// Multiple sources = higher confidence. Empty = unverified.
    #[serde(default)]
    pub verified_by: Vec<VerificationSource>,
    /// Warning/safety tags. Empty = no known issues.
    #[serde(default)]
    pub tags: Vec<PolicyTag>,
}

impl ResolvedPolicy {
    /// Whether any verification source has confirmed this policy's identity.
    pub fn is_verified(&self) -> bool {
        !self.verified_by.is_empty()
    }

    /// Whether this policy has any safety-critical warning tags.
    pub fn has_warnings(&self) -> bool {
        self.tags.iter().any(|t| t.is_warning())
    }
}

/// Synchronous policy metadata resolver.
///
/// Implementations are pre-populated (e.g., from an async API call or
/// static config) before being passed to UI code. This keeps the trait
/// sync-friendly for immediate-mode rendering (egui) and WASM targets.
///
/// The resolver is keyed by policy_id (56-char hex string).
pub trait PolicyResolver {
    /// Look up metadata for a policy ID.
    /// Returns `None` if the policy is unknown to this resolver.
    fn resolve(&self, policy_id: &str) -> Option<&ResolvedPolicy>;
}

/// A resolver that always returns `None`.
///
/// Use as the default when no collection data has been loaded.
/// Widgets degrade gracefully (e.g., show full policy ID, no badges).
pub struct NullResolver;

impl PolicyResolver for NullResolver {
    fn resolve(&self, _policy_id: &str) -> Option<&ResolvedPolicy> {
        None
    }
}

/// A pre-populated in-memory resolver backed by a `HashMap`.
///
/// Build this from async API responses, then pass as `&dyn PolicyResolver`
/// to widgets and rendering code.
#[derive(Debug, Clone, Default)]
pub struct HashMapResolver {
    policies: HashMap<String, ResolvedPolicy>,
}

impl HashMapResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from an existing map.
    pub fn from_map(policies: HashMap<String, ResolvedPolicy>) -> Self {
        Self { policies }
    }

    /// Insert or update a single policy.
    pub fn insert(&mut self, policy_id: String, resolved: ResolvedPolicy) {
        self.policies.insert(policy_id, resolved);
    }

    /// Insert a simple name-only entry (convenience for the common case).
    /// Sets `token_type: Unknown`, no icon, no verification, no tags.
    pub fn insert_name(&mut self, policy_id: String, name: String) {
        self.policies.insert(
            policy_id,
            ResolvedPolicy {
                name,
                token_type: TokenType::Unknown,
                icon_url: None,
                verified_by: vec![],
                tags: vec![],
            },
        );
    }

    pub fn len(&self) -> usize {
        self.policies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }
}

impl PolicyResolver for HashMapResolver {
    fn resolve(&self, policy_id: &str) -> Option<&ResolvedPolicy> {
        self.policies.get(policy_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_resolver_returns_none() {
        let resolver = NullResolver;
        assert!(resolver.resolve("abc123").is_none());
    }

    #[test]
    fn test_hashmap_resolver_insert_and_resolve() {
        let mut resolver = HashMapResolver::new();
        resolver.insert(
            "policy_a".to_string(),
            ResolvedPolicy {
                name: "SpaceBudz".to_string(),
                token_type: TokenType::Nft,
                icon_url: Some("https://example.com/icon.png".to_string()),
                verified_by: vec![VerificationSource::JpgStore],
                tags: vec![],
            },
        );
        let resolved = resolver.resolve("policy_a").unwrap();
        assert_eq!(resolved.name, "SpaceBudz");
        assert_eq!(resolved.token_type, TokenType::Nft);
        assert!(resolved.is_verified());
        assert!(!resolved.has_warnings());
    }

    #[test]
    fn test_hashmap_resolver_insert_name() {
        let mut resolver = HashMapResolver::new();
        resolver.insert_name("policy_b".to_string(), "HOSKY".to_string());
        let resolved = resolver.resolve("policy_b").unwrap();
        assert_eq!(resolved.name, "HOSKY");
        assert_eq!(resolved.token_type, TokenType::Unknown);
        assert!(!resolved.is_verified());
    }

    #[test]
    fn test_hashmap_resolver_unknown_returns_none() {
        let resolver = HashMapResolver::new();
        assert!(resolver.resolve("unknown").is_none());
    }

    #[test]
    fn test_policy_tag_warnings() {
        assert!(PolicyTag::Rugpull.is_warning());
        assert!(PolicyTag::Compromised.is_warning());
        assert!(PolicyTag::Copymint.is_warning());
        assert!(!PolicyTag::Deprecated.is_warning());
        assert!(!PolicyTag::Unverified.is_warning());
    }

    #[test]
    fn test_resolved_policy_has_warnings() {
        let policy = ResolvedPolicy {
            name: "Scam Project".to_string(),
            token_type: TokenType::Nft,
            icon_url: None,
            verified_by: vec![],
            tags: vec![PolicyTag::Rugpull],
        };
        assert!(policy.has_warnings());

        let clean = ResolvedPolicy {
            name: "Good Project".to_string(),
            token_type: TokenType::Nft,
            icon_url: None,
            verified_by: vec![VerificationSource::TokenRegistry],
            tags: vec![],
        };
        assert!(!clean.has_warnings());
    }

    #[test]
    fn test_token_type_serde_roundtrip() {
        let original = TokenType::Nft;
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, r#""nft""#);
        let parsed: TokenType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_resolved_policy_serde_roundtrip() {
        let original = ResolvedPolicy {
            name: "Test".to_string(),
            token_type: TokenType::Ft,
            icon_url: None,
            verified_by: vec![
                VerificationSource::TokenRegistry,
                VerificationSource::JpgStore,
            ],
            tags: vec![PolicyTag::Deprecated],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ResolvedPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test");
        assert_eq!(parsed.token_type, TokenType::Ft);
        assert_eq!(parsed.verified_by.len(), 2);
        assert_eq!(parsed.tags.len(), 1);
    }

    #[test]
    fn test_resolved_policy_serde_defaults() {
        // Deserialize without optional fields — verified_by and tags default to empty
        let json = r#"{"name":"Minimal","token_type":"unknown","icon_url":null}"#;
        let parsed: ResolvedPolicy = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.name, "Minimal");
        assert!(parsed.verified_by.is_empty());
        assert!(parsed.tags.is_empty());
    }

    #[test]
    fn test_dyn_policy_resolver() {
        // Verify the trait is object-safe
        let mut resolver = HashMapResolver::new();
        resolver.insert_name("pid".to_string(), "Test".to_string());
        let dyn_resolver: &dyn PolicyResolver = &resolver;
        assert!(dyn_resolver.resolve("pid").is_some());
        assert!(dyn_resolver.resolve("other").is_none());
    }
}
