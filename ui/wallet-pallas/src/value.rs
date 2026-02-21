//! Value and balance decoding utilities
//!
//! Decode CBOR-encoded Cardano values (lovelace + multi-assets) from CIP-30 wallet API.

use crate::PallasError;
use cardano_assets::AssetId;
use pallas_codec::minicbor;
use pallas_primitives::conway::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Decoded wallet balance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalance {
    /// Total lovelace (ADA = lovelace / 1_000_000)
    pub lovelace: u64,
    /// Multi-assets grouped by policy ID
    /// Key: policy ID (hex), Value: map of asset name (hex) to quantity
    pub assets: HashMap<String, HashMap<String, u64>>,
}

impl WalletBalance {
    /// Get the ADA amount (lovelace / 1_000_000)
    pub fn ada(&self) -> f64 {
        self.lovelace as f64 / 1_000_000.0
    }

    /// Get total number of distinct native tokens
    pub fn token_count(&self) -> usize {
        self.assets.values().map(|m| m.len()).sum()
    }

    /// Get total number of distinct policies
    pub fn policy_count(&self) -> usize {
        self.assets.len()
    }
}

/// Decode a CBOR-encoded Value from the wallet's getBalance response
///
/// The Value type in Cardano can be either:
/// - A simple integer (just lovelace)
/// - A tuple of (lovelace, multi-asset map)
pub fn decode_balance(balance_hex: &str) -> Result<WalletBalance, PallasError> {
    let bytes = hex::decode(balance_hex)?;

    let value: Value =
        minicbor::decode(&bytes).map_err(|e| PallasError::CborDecode(e.to_string()))?;

    Ok(value_to_balance(value))
}

/// Convert a pallas Value to our WalletBalance struct
fn value_to_balance(value: Value) -> WalletBalance {
    match value {
        Value::Coin(lovelace) => WalletBalance {
            lovelace,
            assets: HashMap::new(),
        },
        Value::Multiasset(lovelace, multi_assets) => {
            let mut assets: HashMap<String, HashMap<String, u64>> = HashMap::new();

            for (policy_id, asset_map) in multi_assets.iter() {
                let policy_hex = hex::encode(policy_id.as_ref());
                let mut policy_assets: HashMap<String, u64> = HashMap::new();

                for (asset_name, quantity) in asset_map.iter() {
                    let asset_bytes: &[u8] = asset_name.as_ref();
                    let asset_name_hex = hex::encode(asset_bytes);
                    policy_assets.insert(asset_name_hex, u64::from(*quantity));
                }

                assets.insert(policy_hex, policy_assets);
            }

            WalletBalance { lovelace, assets }
        }
    }
}

/// A single native token
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToken {
    /// Policy ID (hex)
    pub policy_id: String,
    /// Asset name (hex)
    pub asset_name_hex: String,
    /// Asset name as UTF-8 string (if valid)
    pub asset_name: Option<String>,
    /// Quantity
    pub quantity: u64,
}

impl NativeToken {
    /// Get the full asset ID as an `AssetId` type
    pub fn asset_id(&self) -> AssetId {
        AssetId::new_unchecked(self.policy_id.clone(), self.asset_name_hex.clone())
    }

    /// Check if this token looks like an NFT (quantity = 1)
    pub fn is_likely_nft(&self) -> bool {
        self.quantity == 1
    }

    /// Get display name with CIP-67 prefix stripped and PascalCase split
    ///
    /// This provides a more human-readable name by:
    /// 1. Stripping CIP-67 prefixes (e.g., (100), (222), (333))
    /// 2. Splitting PascalCase into separate words
    /// 3. Falling back to truncated hex if not valid UTF-8
    pub fn display_name(&self) -> String {
        // Try to parse as AssetId and strip CIP-67 prefix
        if let Ok(asset_id) = AssetId::new(self.policy_id.clone(), self.asset_name_hex.clone()) {
            let stripped = asset_id.strip_cip67();
            let name = stripped.asset_name();

            if !name.is_empty() && name != self.asset_name_hex {
                // Successfully decoded to UTF-8, now split PascalCase
                return split_pascal_case(&name);
            }
        }

        // Fall back to original asset_name if available
        if let Some(ref name) = self.asset_name {
            return split_pascal_case(name);
        }

        // Final fallback: truncated hex
        if self.asset_name_hex.len() > 16 {
            format!("{}...", &self.asset_name_hex[..16])
        } else if self.asset_name_hex.is_empty() {
            "(unnamed)".to_string()
        } else {
            self.asset_name_hex.clone()
        }
    }
}

/// Split a PascalCase or camelCase string into space-separated words
///
/// Examples:
/// - "UnsignedAlgorithms" -> "Unsigned Algorithms"
/// - "BlackFlag1234" -> "Black Flag 1234"
/// - "NFTProject" -> "NFT Project"
/// - "Already Spaced" -> "Already Spaced"
fn split_pascal_case(s: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }

    let mut result = String::with_capacity(s.len() + 10);
    let chars: Vec<char> = s.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if i > 0 {
            let prev = chars[i - 1];
            let is_boundary =
                // Lowercase followed by uppercase: "aB" -> "a B"
                (prev.is_lowercase() && c.is_uppercase()) ||
                // Letter followed by digit: "a1" -> "a 1"
                (prev.is_alphabetic() && c.is_ascii_digit() && (i + 1 >= chars.len() || !chars[i + 1].is_ascii_digit() || !prev.is_ascii_digit())) ||
                // Digit followed by letter: "1a" -> "1 a"
                (prev.is_ascii_digit() && c.is_alphabetic()) ||
                // Uppercase followed by uppercase then lowercase: "ABc" -> "A Bc"
                (prev.is_uppercase() && c.is_uppercase() && i + 1 < chars.len() && chars[i + 1].is_lowercase());

            if is_boundary && !prev.is_whitespace() {
                result.push(' ');
            }
        }
        result.push(c);
    }

    result
}

/// A policy group containing multiple assets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGroup {
    /// Policy ID (56 hex chars)
    pub policy_id: String,
    /// Short display version of policy ID
    pub policy_id_short: String,
    /// All tokens under this policy
    pub tokens: Vec<NativeToken>,
    /// Whether this policy likely contains NFTs
    pub is_likely_nft_policy: bool,
    /// Count of tokens that appear to be NFTs (quantity = 1)
    pub nft_count: usize,
    /// Count of tokens that appear to be fungible (quantity > 1)
    pub fungible_count: usize,
}

impl PolicyGroup {
    /// Create a new policy group from tokens
    pub fn new(policy_id: String, tokens: Vec<NativeToken>) -> Self {
        let nft_count = tokens.iter().filter(|t| t.quantity == 1).count();
        let fungible_count = tokens.len() - nft_count;

        // A policy is likely NFT if majority of tokens have quantity 1
        let is_likely_nft_policy =
            nft_count > fungible_count || (nft_count > 0 && fungible_count == 0);

        let policy_id_short = if policy_id.len() >= 16 {
            format!(
                "{}...{}",
                &policy_id[..8],
                &policy_id[policy_id.len() - 8..]
            )
        } else {
            policy_id.clone()
        };

        Self {
            policy_id,
            policy_id_short,
            tokens,
            is_likely_nft_policy,
            nft_count,
            fungible_count,
        }
    }

    /// Get only NFT tokens (quantity = 1)
    pub fn nfts(&self) -> Vec<&NativeToken> {
        self.tokens.iter().filter(|t| t.quantity == 1).collect()
    }

    /// Get only fungible tokens (quantity > 1)
    pub fn fungibles(&self) -> Vec<&NativeToken> {
        self.tokens.iter().filter(|t| t.quantity > 1).collect()
    }

    /// Total token count
    pub fn total_count(&self) -> usize {
        self.tokens.len()
    }
}

impl WalletBalance {
    /// Get all `AssetId`s held under a given policy ID
    pub fn asset_ids_for_policy(&self, policy_id: &str) -> Vec<AssetId> {
        self.assets
            .get(policy_id)
            .map(|asset_map| {
                asset_map
                    .keys()
                    .map(|asset_name_hex| {
                        AssetId::new_unchecked(policy_id.to_string(), asset_name_hex.clone())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all native tokens as a flat list
    pub fn tokens(&self) -> Vec<NativeToken> {
        let mut tokens = Vec::new();

        for (policy_id, asset_map) in &self.assets {
            for (asset_name_hex, quantity) in asset_map {
                let asset_name = hex::decode(asset_name_hex)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok());

                tokens.push(NativeToken {
                    policy_id: policy_id.clone(),
                    asset_name_hex: asset_name_hex.clone(),
                    asset_name,
                    quantity: *quantity,
                });
            }
        }

        tokens
    }

    /// Get assets grouped by policy ID
    pub fn policy_groups(&self) -> Vec<PolicyGroup> {
        let mut groups: Vec<PolicyGroup> = self
            .assets
            .iter()
            .map(|(policy_id, asset_map)| {
                let tokens: Vec<NativeToken> = asset_map
                    .iter()
                    .map(|(asset_name_hex, quantity)| {
                        let asset_name = hex::decode(asset_name_hex)
                            .ok()
                            .and_then(|bytes| String::from_utf8(bytes).ok());

                        NativeToken {
                            policy_id: policy_id.clone(),
                            asset_name_hex: asset_name_hex.clone(),
                            asset_name,
                            quantity: *quantity,
                        }
                    })
                    .collect();

                PolicyGroup::new(policy_id.clone(), tokens)
            })
            .collect();

        // Sort by NFT count descending (NFT policies first), then by total count
        groups.sort_by(|a, b| {
            b.is_likely_nft_policy
                .cmp(&a.is_likely_nft_policy)
                .then_with(|| b.nft_count.cmp(&a.nft_count))
                .then_with(|| b.total_count().cmp(&a.total_count()))
        });

        groups
    }

    /// Get only policy groups that likely contain NFTs
    pub fn nft_policy_groups(&self) -> Vec<PolicyGroup> {
        self.policy_groups()
            .into_iter()
            .filter(|g| g.is_likely_nft_policy)
            .collect()
    }

    /// Get total count of likely NFTs across all policies
    pub fn nft_count(&self) -> usize {
        self.tokens().iter().filter(|t| t.quantity == 1).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_pascal_case_basic() {
        assert_eq!(
            split_pascal_case("UnsignedAlgorithms"),
            "Unsigned Algorithms"
        );
        assert_eq!(split_pascal_case("BlackFlag"), "Black Flag");
        assert_eq!(split_pascal_case("HelloWorld"), "Hello World");
    }

    #[test]
    fn test_split_pascal_case_with_numbers() {
        assert_eq!(split_pascal_case("BlackFlag1234"), "Black Flag 1234");
        assert_eq!(split_pascal_case("Pirate1086"), "Pirate 1086");
        assert_eq!(split_pascal_case("NFT123Collection"), "NFT 123 Collection");
    }

    #[test]
    fn test_split_pascal_case_acronyms() {
        assert_eq!(split_pascal_case("NFTProject"), "NFT Project");
        assert_eq!(split_pascal_case("HTTPServer"), "HTTP Server");
        assert_eq!(split_pascal_case("IOError"), "IO Error");
    }

    #[test]
    fn test_split_pascal_case_already_spaced() {
        assert_eq!(split_pascal_case("Already Spaced"), "Already Spaced");
        assert_eq!(split_pascal_case("hello world"), "hello world");
    }

    #[test]
    fn test_split_pascal_case_single_word() {
        assert_eq!(split_pascal_case("Hello"), "Hello");
        assert_eq!(split_pascal_case("ALLCAPS"), "ALLCAPS");
        assert_eq!(split_pascal_case("lowercase"), "lowercase");
    }

    #[test]
    fn test_split_pascal_case_empty() {
        assert_eq!(split_pascal_case(""), "");
    }

    #[test]
    fn test_native_token_display_name() {
        // Standard token
        let token = NativeToken {
            policy_id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
            asset_name_hex: "50697261746531303836".to_string(), // "Pirate1086"
            asset_name: Some("Pirate1086".to_string()),
            quantity: 1,
        };
        assert_eq!(token.display_name(), "Pirate 1086");
    }
}
