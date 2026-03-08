use serde::{Deserialize, Serialize};

/// Token type being minted.
///
/// Determines quantity constraints and CIP-67 prefix for CIP-68 mints.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    /// Non-fungible token (quantity must be 1)
    /// CIP-67: user = 222 (000de140), reference = 100 (000643b0)
    #[default]
    Nft,

    /// Rich fungible token / semi-fungible (quantity > 1 allowed)
    /// CIP-67: user = 444 (001bc280), reference = 100 (000643b0)
    Rft,

    /// Cardano native token (fungible, any quantity)
    /// CIP-67: user = 333 (0014df10), reference = 100 (000643b0)
    Fungible,
}

impl TokenType {
    /// Get the CIP-67 user token prefix for CIP-68 mints.
    pub fn user_prefix(&self) -> &'static str {
        match self {
            TokenType::Nft => "000de140",      // (222)
            TokenType::Rft => "001bc280",      // (444)
            TokenType::Fungible => "0014df10", // (333)
        }
    }

    /// Get the CIP-67 reference token prefix (same for all types).
    pub fn reference_prefix(&self) -> &'static str {
        "000643b0" // (100)
    }

    /// Validate quantity for this token type.
    pub fn validate_quantity(&self, quantity: u64) -> Result<(), String> {
        match self {
            TokenType::Nft if quantity != 1 => Err("NFT quantity must be exactly 1".to_string()),
            TokenType::Rft | TokenType::Fungible if quantity == 0 => {
                Err("Token quantity must be at least 1".to_string())
            }
            _ => Ok(()),
        }
    }
}

/// Metadata standard tag for simple serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataStandardTag {
    /// CIP-25: Metadata in transaction auxiliary data (default)
    #[default]
    Cip25,

    /// CIP-68: Metadata in reference token inline datum
    Cip68,
}

impl MetadataStandardTag {
    /// Check if this is CIP-68.
    pub fn is_cip68(&self) -> bool {
        matches!(self, MetadataStandardTag::Cip68)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_type_prefixes() {
        assert_eq!(TokenType::Nft.user_prefix(), "000de140");
        assert_eq!(TokenType::Rft.user_prefix(), "001bc280");
        assert_eq!(TokenType::Fungible.user_prefix(), "0014df10");

        // Reference prefix is same for all
        assert_eq!(TokenType::Nft.reference_prefix(), "000643b0");
        assert_eq!(TokenType::Rft.reference_prefix(), "000643b0");
        assert_eq!(TokenType::Fungible.reference_prefix(), "000643b0");
    }

    #[test]
    fn test_nft_quantity_validation() {
        assert!(TokenType::Nft.validate_quantity(1).is_ok());
        assert!(TokenType::Nft.validate_quantity(0).is_err());
        assert!(TokenType::Nft.validate_quantity(2).is_err());
    }

    #[test]
    fn test_rft_quantity_validation() {
        assert!(TokenType::Rft.validate_quantity(1).is_ok());
        assert!(TokenType::Rft.validate_quantity(100).is_ok());
        assert!(TokenType::Rft.validate_quantity(0).is_err());
    }

    #[test]
    fn test_fungible_quantity_validation() {
        assert!(TokenType::Fungible.validate_quantity(1).is_ok());
        assert!(TokenType::Fungible.validate_quantity(1_000_000).is_ok());
        assert!(TokenType::Fungible.validate_quantity(0).is_err());
    }

    #[test]
    fn test_metadata_standard_default() {
        let standard: MetadataStandardTag = Default::default();
        assert!(!standard.is_cip68());
    }
}
