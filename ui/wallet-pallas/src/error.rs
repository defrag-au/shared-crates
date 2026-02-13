//! Error types for wallet-pallas operations

use thiserror::Error;

/// Errors that can occur during pallas-based wallet operations
#[derive(Debug, Error)]
pub enum PallasError {
    /// Invalid hex string
    #[error("Invalid hex: {0}")]
    InvalidHex(String),

    /// Failed to decode address
    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    /// Failed to decode CBOR
    #[error("CBOR decode error: {0}")]
    CborDecode(String),

    /// Invalid signature format
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    /// Signature verification failed
    #[error("Signature verification failed")]
    VerificationFailed,

    /// Invalid public key
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),

    /// Transaction parsing failed
    #[error("Transaction parse error: {0}")]
    TransactionParse(String),

    /// Unsupported address type
    #[error("Unsupported address type: {0}")]
    UnsupportedAddressType(String),
}

impl From<hex::FromHexError> for PallasError {
    fn from(e: hex::FromHexError) -> Self {
        PallasError::InvalidHex(e.to_string())
    }
}
