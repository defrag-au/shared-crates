//! Wallet error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("No wallet extension found")]
    NoWalletFound,

    #[error("Wallet not enabled: {0}")]
    NotEnabled(String),

    #[error("User rejected connection")]
    UserRejected,

    #[error("Wallet API error: {0}")]
    ApiError(String),

    #[error("Network mismatch: expected {expected}, got {actual}")]
    NetworkMismatch { expected: String, actual: String },

    #[error("Signing failed: {0}")]
    SigningFailed(String),

    #[error("Transaction submit failed: {0}")]
    SubmitFailed(String),

    #[error("JavaScript error: {0}")]
    JsError(String),
}

impl From<wasm_bindgen::JsValue> for WalletError {
    fn from(value: wasm_bindgen::JsValue) -> Self {
        let msg = value.as_string().unwrap_or_else(|| format!("{value:?}"));
        WalletError::JsError(msg)
    }
}
