//! Classification of Cardano native tokens by fungibility.

use serde::{Deserialize, Serialize};

/// Classification of a Cardano native token's fungibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    /// Non-fungible token (unique, supply = 1)
    Nft,
    /// Fungible token (divisible, high supply)
    Ft,
    /// Rich/semi-fungible token (multiple editions, 1 < supply < millions)
    Rft,
    /// Could not determine
    Unknown,
}

impl std::fmt::Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenType::Nft => write!(f, "nft"),
            TokenType::Ft => write!(f, "ft"),
            TokenType::Rft => write!(f, "rft"),
            TokenType::Unknown => write!(f, "unknown"),
        }
    }
}
