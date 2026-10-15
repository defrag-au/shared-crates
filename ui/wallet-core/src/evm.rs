//! EIP-1193 — `window.ethereum`, the EVM counterpart to [`crate::cip30`].
//!
//! Enough of the injected provider to **link** an EVM account: detect it, read
//! its accounts, and `personal_sign` a challenge. Nothing here reads balances or
//! signs transactions — the platform's EVM ownership comes from chain state
//! (collection-ownership's EVM reconcile), never from the wallet, so a wallet
//! that is not installed is a missed link, not a missed reading.
//!
//! ## Why `personal_sign` takes the message, not hex
//!
//! EIP-191 signs *bytes*, and the provider decides how to read its first
//! parameter: a string that looks like hex is hex-decoded, anything else is
//! UTF-8. This estate's challenge messages are ASCII, so leaving that choice to
//! the provider is where "signed something subtly different" starts.
//! [`EvmProvider::personal_sign`] hex-encodes the UTF-8 bytes itself, so the
//! wallet signs exactly the bytes the server reconstructs — the same rule the
//! light link page follows in JavaScript.

use wasm_bindgen::JsValue;

/// The injected provider, as detected at [`EvmProvider::detect`].
///
/// Holds the JS object itself: EIP-1193 has no stable identity beyond it, and
/// re-reading `window.ethereum` at call time would hand a different object to a
/// caller that had already checked it was there.
#[derive(Clone)]
pub struct EvmProvider {
    provider: JsValue,
}

/// Why a provider call did not produce an answer.
///
/// `Refused` covers both a malformed request and the reader closing the dialog
/// — EIP-1193 does not distinguish them in a way a caller can act on
/// differently, and the wallet's own `message` is more useful to show than any
/// classification this could invent.
#[derive(Debug, Clone, thiserror::Error)]
pub enum EvmError {
    #[error("no EVM provider (window.ethereum) is installed in this browser")]
    Unavailable,
    #[error("{0}")]
    Refused(String),
    #[error("the provider answered with something this bridge cannot read: {0}")]
    Malformed(String),
}

impl EvmProvider {
    /// The browser's provider, when it has one.
    ///
    /// Filters on the actual EIP-1193 contract (`request`), not on
    /// `window.ethereum` merely existing: several wallets install a namespace
    /// object for their own helpers, and older shims exposed only `send`.
    pub fn detect() -> Option<Self> {
        Some(Self {
            provider: evm_provider_js()?,
        })
    }

    /// Accounts the provider is already willing to name. Never prompts.
    pub async fn accounts(&self) -> Result<Vec<String>, EvmError> {
        self.account_list("eth_accounts").await
    }

    /// Accounts, *asking* the reader to connect if the wallet is locked or this
    /// origin is not yet permitted.
    pub async fn request_accounts(&self) -> Result<Vec<String>, EvmError> {
        self.account_list("eth_requestAccounts").await
    }

    /// Sign `message` (UTF-8) with `address`, returning the 65-byte `r || s || v`
    /// hex signature.
    pub async fn personal_sign(&self, message: &str, address: &str) -> Result<String, EvmError> {
        let value = evm_personal_sign_js(&self.provider, &to_hex(message.as_bytes()), address)
            .await
            .map_err(refused)?;
        value
            .as_string()
            .ok_or_else(|| EvmError::Malformed("personal_sign returned a non-string".into()))
    }

    async fn account_list(&self, method: &str) -> Result<Vec<String>, EvmError> {
        let value = evm_request_js(&self.provider, method, &js_sys::Array::new())
            .await
            .map_err(refused)?;
        // Guarded, because `js_sys::Array::from` on a bare string char-splits
        // it into single-character "accounts" — the same trap `cip30`'s
        // `sign_txs` documents, and here it would be a wallet that appears to
        // hold 42 one-character accounts.
        if !value.is_array() {
            return Err(EvmError::Malformed(format!(
                "{method} did not return a list"
            )));
        }
        let array = js_sys::Array::from(&value);
        Ok(array.iter().filter_map(|v| v.as_string()).collect())
    }
}

/// `0x…` of the UTF-8 bytes — the form `personal_sign` hex-decodes.
fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("0x");
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// An EIP-1193 rejection: an object with a `message`, a bare string, or
/// something unnameable.
fn refused(value: JsValue) -> EvmError {
    let message = js_sys::Reflect::get(&value, &JsValue::from_str("message"))
        .ok()
        .and_then(|m| m.as_string())
        .or_else(|| value.as_string())
        .unwrap_or_else(|| format!("{value:?}"));
    EvmError::Refused(message)
}

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export function evmProvider() {
    const provider = (typeof window !== 'undefined') ? window.ethereum : undefined;
    if (!provider || typeof provider.request !== 'function') return undefined;
    return provider;
}

export async function evmRequest(provider, method, params) {
    return await provider.request({ method, params });
}

export async function evmPersonalSign(provider, messageHex, address) {
    // [message, address] is the EIP-1193 order. MetaMask's pre-2020 releases
    // wanted it the other way round; those versions are long out of service.
    return await provider.request({ method: 'personal_sign', params: [messageHex, address] });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = evmProvider)]
    fn evm_provider_js() -> Option<JsValue>;

    #[wasm_bindgen(js_name = evmRequest, catch)]
    async fn evm_request_js(
        provider: &JsValue,
        method: &str,
        params: &JsValue,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = evmPersonalSign, catch)]
    async fn evm_personal_sign_js(
        provider: &JsValue,
        message_hex: &str,
        address: &str,
    ) -> Result<JsValue, JsValue>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The encoding `personal_sign` hex-decodes. Lower-case because every
    /// signature-adjacent string this estate handles is, and a mixed-case one
    /// is one more thing a comparison could be wrong about.
    #[test]
    fn to_hex_is_0x_prefixed_lower_case() {
        assert_eq!(to_hex(b""), "0x");
        assert_eq!(to_hex(b"\x01\xab\xff"), "0x01abff");
    }

    /// Byte length, not character length — the challenge messages are ASCII
    /// today, and this is the property that keeps a non-ASCII one honest.
    #[test]
    fn to_hex_encodes_bytes_not_characters() {
        assert_eq!(to_hex("é".as_bytes()), "0xc3a9");
    }
}
