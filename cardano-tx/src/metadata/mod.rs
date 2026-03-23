//! Metadata encoding for Cardano transaction standards
//!
//! - [`cip25`] — CIP-25 auxiliary data (off-chain NFT metadata)
//! - [`cip68`] — CIP-68 inline datums (on-chain reference token metadata)
//! - [`cip67`] — CIP-67 asset name label prefixes

pub mod cip20;
pub mod cip25;
pub mod cip68;

/// CIP-67 asset name label prefixes for CIP-68 token types.
///
/// These 4-byte prefixes distinguish token roles on-chain:
/// - User tokens: held by the owner, carry no metadata
/// - Reference tokens: held at a script address, carry the inline datum with metadata
pub mod cip67 {
    /// NFT user token prefix — label 222 (`000de140`)
    pub const NFT_USER: &str = "000de140";
    /// RFT (semi-fungible) user token prefix — label 444 (`001bc280`)
    pub const RFT_USER: &str = "001bc280";
    /// FT (fungible) user token prefix — label 333 (`0014df10`)
    pub const FT_USER: &str = "0014df10";
    /// Reference token prefix — label 100 (`000643b0`), same for all token types
    pub const REFERENCE: &str = "000643b0";
}

/// Errors from metadata encoding operations.
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("Missing 721 key in metadata")]
    MissingCip25Key,
    #[error("Failed to encode: {0}")]
    EncodeError(String),
    #[error("Unsupported metadata value: {0}")]
    UnsupportedValue(String),
}
