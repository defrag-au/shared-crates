//! Wallet error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("No wallet extension found")]
    NoWalletFound,

    #[error("Wallet not enabled: {0}")]
    NotEnabled(String),

    #[error("Rejected by user")]
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

impl WalletError {
    /// True for a user-cancelled prompt (connect / sign / submit). Lets the
    /// UI treat a deliberate decline as a no-op rather than a hard failure
    /// (no scary error toast).
    pub fn is_user_rejected(&self) -> bool {
        matches!(self, WalletError::UserRejected)
    }
}

impl From<wasm_bindgen::JsValue> for WalletError {
    fn from(value: wasm_bindgen::JsValue) -> Self {
        let (code, message) = extract_js_error(&value);

        // Detect a user-cancelled prompt. Code is ambiguous across CIP-30
        // error types (e.g. `2` = TxSignError.UserDeclined but also
        // TxSendError.Failure), so key on the unambiguous APIError.Refused
        // (-3) plus decline wording in the message. Deliberately does NOT
        // match a bare "refused" (TxSendError.Refused = the NODE rejecting a
        // tx, not the user).
        let lower = message.to_ascii_lowercase();
        let declined = code == Some(-3.0)
            || lower.contains("declined")
            || lower.contains("user cancel")
            || lower.contains("cancelled by")
            || lower.contains("canceled by")
            || lower.contains("rejected by user")
            || lower.contains("user rejected");
        if declined {
            return WalletError::UserRejected;
        }

        // Otherwise preserve the real, human-readable message (prefixed with
        // the CIP-30 code when present) instead of an opaque `JsValue(...)`.
        match code {
            Some(c) => WalletError::JsError(format!("[{}] {message}", c as i64)),
            None => WalletError::JsError(message),
        }
    }
}

/// Pull a `(code, message)` out of whatever a wallet throws. CIP-30 errors
/// are objects `{ code: number, info: string }`; wallets/relays also throw
/// plain `Error` objects (`.message`), Maestro-style `{ code, message }`,
/// or bare strings. `JsValue::as_string()` is `None` for all the object
/// shapes, which is why the old impl produced `JsValue(Object(...))`.
fn extract_js_error(value: &wasm_bindgen::JsValue) -> (Option<f64>, String) {
    use wasm_bindgen::JsValue;

    if let Some(s) = value.as_string() {
        return (None, s);
    }

    let get_str = |key: &str| -> Option<String> {
        js_sys::Reflect::get(value, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
    };
    let code = js_sys::Reflect::get(value, &JsValue::from_str("code"))
        .ok()
        .and_then(|v| v.as_f64());

    // CIP-30 uses `info`; Error/Maestro use `message`.
    let message = get_str("info")
        .or_else(|| get_str("message"))
        .unwrap_or_else(|| format!("{value:?}"));

    (code, message)
}
