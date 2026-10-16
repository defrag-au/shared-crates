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

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

/// One EVM wallet the browser offers.
///
/// The counterpart of [`crate::WalletInfo`], and deliberately *not* an enum:
/// Cardano's set is closed (a known list of `window.cardano` keys), while EVM
/// wallets announce themselves under [EIP-6963] with a uuid, a name and an icon
/// — open-ended by design, because several of them otherwise have to fight over
/// `window.ethereum`.
///
/// [`uuid`](Self::uuid) is the identity to reconnect with; `rdns` is the
/// reverse-DNS name the wallet publishes (e.g. `io.rabby`), which is the stable
/// way to recognise a wallet across versions when its display name changes.
///
/// [EIP-6963]: https://eips.ethereum.org/EIPS/eip-6963
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmWalletInfo {
    /// The wallet's own identifier. [`EvmWalletId::fallback`] when the only
    /// provider available is the un-announced `window.ethereum`.
    pub id: EvmWalletId,
    /// Display name as the wallet gives it ("Rabby", "MetaMask").
    pub name: String,
    /// Data-URL icon, when the wallet supplied one.
    pub icon: Option<String>,
    /// Reverse-DNS identifier, e.g. `io.rabby`. `None` for the fallback.
    pub rdns: Option<String>,
}

/// The handle for one EVM wallet: EIP-6963's `uuid`, as the wallet gave it.
///
/// A newtype rather than a `String` because this is an **identity** — the thing
/// a host passes back to reach the provider again — and it must not be
/// mistakable for the Cardano side's `api_name`, nor for a display name.
///
/// **Not validated, and not assumed to be a UUID.** EIP-6963 says wallets
/// supply a uuid and most mint a real v4, but the spec sets no format
/// requirement, so this is an opaque token: store it, compare it, hand it back.
/// [`EvmWalletId::fallback`] is the only value this crate mints itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EvmWalletId(String);

impl EvmWalletId {
    /// The identity of a provider found only as `window.ethereum` — a wallet
    /// that does not announce, which an EIP-6963 host still has to offer.
    ///
    /// Spelled like a URL rather than a token so it cannot collide with an
    /// announced uuid, and so a log line reads as what it is.
    pub fn fallback() -> Self {
        Self("window.ethereum".to_string())
    }

    /// The raw handle — for keying, for logs, and for the wire.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is [`Self::fallback`].
    pub fn is_fallback(&self) -> bool {
        self.as_str() == "window.ethereum"
    }
}

impl From<String> for EvmWalletId {
    /// From whatever an announcement carried. Infallible on purpose: there is
    /// no syntax to get wrong.
    fn from(uuid: String) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Display for EvmWalletId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod id_tests {
    use super::EvmWalletId;

    /// The fallback exists so nothing compares against a literal, and it has to
    /// stay distinguishable from a wallet-minted id.
    #[test]
    fn the_fallback_is_a_value_of_its_own() {
        let fallback = EvmWalletId::fallback();
        assert!(fallback.is_fallback());
        assert_eq!(fallback.as_str(), "window.ethereum");

        let announced = EvmWalletId::from("c8c5f9d0-1f1a-4a8e-9c76-0a2f5f0b7d21".to_string());
        assert!(!announced.is_fallback());
        assert_ne!(announced, fallback);
    }

    /// Whatever the wallet called itself is preserved verbatim — this type is a
    /// handle, not a parser, so it must not normalise or reformat.
    #[test]
    fn the_handle_is_kept_verbatim() {
        let odd = EvmWalletId::from("Rabby! not.a.uuid".to_string());
        assert_eq!(odd.as_str(), "Rabby! not.a.uuid");
        assert_eq!(odd.to_string(), "Rabby! not.a.uuid");
    }
}

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

// EIP-6963: a wallet answers `eip6963:requestProvider` with an
// `eip6963:announceProvider` event carrying `detail.info` and `detail.provider`.
// The listener has to be in place BEFORE the request goes out, and the answers
// arrive in that same tick — the spec's event is synchronous — so this waits for
// nothing and needs no task. Duplicates are dropped by uuid, because a wallet
// may also have announced on load and must appear once.
//
// A provider that only injects `window.ethereum` and never announces is NOT
// collected: see `collect_announced_wallets` for why that is deliberate.
export function collectAnnouncedEvmWallets() {
    if (typeof window === 'undefined') return [];
    const announced = new Map();
    const remember = (event) => {
        const detail = event && event.detail;
        const info = detail && detail.info;
        if (!info || !info.uuid || !detail.provider) return;
        announced.set(info.uuid, {
            id: info.uuid,
            name: info.name,
            icon: info.icon ?? null,
            rdns: info.rdns ?? null,
        });
    };
    window.addEventListener('eip6963:announceProvider', remember);
    try {
        window.dispatchEvent(new CustomEvent('eip6963:requestProvider'));
    } finally {
        window.removeEventListener('eip6963:announceProvider', remember);
    }
    return Array.from(announced.values());
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

    #[wasm_bindgen(js_name = collectAnnouncedEvmWallets)]
    fn collect_announced_evm_wallets_js() -> JsValue;
}

/// Every EVM wallet that answered `eip6963:requestProvider`.
///
/// The request is dispatched and the answers are read in the same tick — the
/// spec's event is synchronous, so nothing here waits and nothing spawns a task,
/// which is what lets a plain widget build the list on the frame it is drawn.
///
/// **Announced wallets only.** A provider that merely injects `window.ethereum`
/// and never announces is left out, because it has no name: an announcement
/// carries one, and a picker row reading `window.ethereum` is worse than no row
/// at all. That is a decision rather than an oversight — a fallback would need a
/// name to show first, and [`EvmWalletId::fallback`] stays unused until one
/// exists.
///
/// A payload that does not parse yields an empty list rather than an error: this
/// is a list of things to *offer*, and offering none is the safe end of that.
pub fn collect_announced_wallets() -> Vec<EvmWalletInfo> {
    serde_wasm_bindgen::from_value(collect_announced_evm_wallets_js()).unwrap_or_default()
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
