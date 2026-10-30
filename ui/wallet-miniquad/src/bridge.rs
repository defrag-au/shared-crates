//! CIP-30 surface over the miniquad plugin protocol.
//!
//! Each async call returns a [`ReqId`] immediately; the actual promise
//! result lands in a JS-side map keyed by the id. Rust calls [`poll`]
//! each frame until [`PollResult`] flips off `Pending`. One promise →
//! one ReqId; `poll` consumes the result (removes the entry from the
//! JS map) when it returns `Ok` or `Err`.

use serde::Deserialize;

/// Opaque handle to an in-flight wallet request. JS-side increments a
/// counter and stores the promise's eventual resolution in a Map keyed
/// by this id; Rust polls until status flips from `pending`.
///
/// `allow(dead_code)`: on native the inner `i32` is constructed (the
/// stub returns `ReqId(0)`) but never consumed — the native `poll`
/// stub ignores it. The wasm path uses it via the FFI calls.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ReqId(pub i32);

/// One CIP-30 wallet announced via `window.cardano.<key>`.
#[derive(Debug, Clone, Deserialize)]
pub struct Provider {
    /// Identifier on `window.cardano.<key>` — e.g. "eternl", "nami".
    pub key: String,
    /// User-facing display name — wallet's own `name` field if present,
    /// otherwise falls back to `key`.
    pub name: String,
    #[serde(default)]
    pub version: String,
}

/// Outcome of polling a request. `Pending` means try again next frame.
///
/// The `kind` is dropped here so existing consumers keep their shape. A host
/// that needs to tell a user's refusal from a wallet fault calls
/// [`poll_detailed`] instead.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum PollResult {
    Pending,
    Ok { data: String },
    Err { data: String },
}

/// Whether an error was the reader declining or the wallet failing.
///
/// CIP-30 has no single "declined" code (`APIError.Refused` is -3; code 2 is
/// `UserDeclined` for a signature and `Failure` for a send), so the plugin
/// classifies on the unambiguous code plus decline wording. The distinction is
/// the one a UI needs: a decline returns to the picker, a fault is shown.
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FailureKind {
    /// The wallet, the node, or the bridge failed. Show it.
    #[default]
    Failed,
    /// The reader closed or refused the prompt. Not an error to shout about.
    Declined,
}

/// The typed form of [`PollResult`], with the failure kind kept.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum PollResultDetailed {
    Pending,
    Ok {
        data: String,
    },
    Err {
        data: String,
        #[serde(default)]
        kind: FailureKind,
    },
}

/// CIP-30 `signData` result — COSE_Sign1 signature + COSE_Key public
/// key, both hex-encoded. The pair is what a verifier (e.g.
/// `wallet-pallas::verify_data_signature` on a backend Worker) feeds
/// into Cardano-aware verification to confirm the user controls the
/// signing key.
#[derive(Debug, Clone, Deserialize)]
pub struct DataSignature {
    pub signature: String,
    pub key: String,
}

/// Whether `rgba` is a whole `size`×`size` RGBA image. The one check between
/// the plugin's buffer and a texture: a short buffer would make
/// `Texture2D::from_rgba8` read past its end.
///
/// `allow(dead_code)` on native, where the stub has no buffer to check.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn icon_len_ok(rgba: &[u8], size: u16) -> bool {
    size > 0 && rgba.len() == usize::from(size) * usize::from(size) * 4
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::{FailureKind, PollResult, PollResultDetailed, Provider, ReqId};
    use sapp_jsutils::JsObject;

    // `unsafe extern` is edition-2024 syntax — the edition made explicit what
    // was already implicit. Same ABI, same behaviour.
    unsafe extern "C" {
        fn wallet_list_providers() -> JsObject;
        fn wallet_connect(name: JsObject) -> i32;
        fn wallet_refresh(name: JsObject) -> i32;
        fn wallet_network_id() -> i32;
        fn wallet_reward_address() -> i32;
        fn wallet_balance() -> i32;
        fn wallet_utxos() -> i32;
        fn wallet_change_address() -> i32;
        fn wallet_sign_tx(tx_hex: JsObject, partial_sign: i32) -> i32;
        fn wallet_submit_tx(tx_hex: JsObject) -> i32;
        fn wallet_disconnect();
        fn wallet_sign_data(addr: JsObject, payload_hex: JsObject) -> i32;
        fn wallet_cancel(req_id: i32);
        fn wallet_poll(req_id: i32) -> JsObject;
        fn wallet_icon(key: JsObject, size: u32) -> i32;
        fn wallet_icon_pixels(req_id: i32) -> JsObject;
    }

    fn js_to_string(js: JsObject) -> String {
        let mut s = String::new();
        js.to_string(&mut s);
        s
    }

    pub fn list_providers() -> Vec<Provider> {
        let s = js_to_string(unsafe { wallet_list_providers() });
        serde_json::from_str(&s).unwrap_or_default()
    }

    pub fn connect(name: &str) -> ReqId {
        ReqId(unsafe { wallet_connect(JsObject::string(name)) })
    }

    /// A fresh handle from wallet `name`, never prompting: `isEnabled()`, then
    /// `enable()` only if the site is still allowed. A CIP-30 handle belongs
    /// to the account selected when it was enabled, so this is how a switch
    /// made in the extension is seen. On ok, every later read answers for the
    /// account selected now; a site no longer allowed fails `Declined` and
    /// keeps the old handle.
    pub fn refresh(name: &str) -> ReqId {
        ReqId(unsafe { wallet_refresh(JsObject::string(name)) })
    }

    /// CIP-30 `getNetworkId`: 0 = testnet, 1 = mainnet.
    pub fn network_id() -> ReqId {
        ReqId(unsafe { wallet_network_id() })
    }

    pub fn reward_address() -> ReqId {
        ReqId(unsafe { wallet_reward_address() })
    }

    /// CIP-30 getBalance — returns hex-encoded CBOR Value (lovelace +
    /// optional multiasset). Decode via [`crate::balance::extract_handle`]
    /// to surface the user's $handle.
    pub fn balance() -> ReqId {
        ReqId(unsafe { wallet_balance() })
    }

    /// CIP-30 getUtxos — the wallet's spendable UTxOs. On the resolved
    /// poll, `data` is a JSON array of hex-encoded CBOR
    /// `TransactionUnspentOutput` strings (empty array when the wallet
    /// holds none). Decode with [`crate::parse_utxos`], then each entry
    /// via `wallet_pallas::decode_utxo`.
    pub fn utxos() -> ReqId {
        ReqId(unsafe { wallet_utxos() })
    }

    /// CIP-30 getChangeAddress. On the resolved poll, `data` is the
    /// hex-encoded change address.
    pub fn change_address() -> ReqId {
        ReqId(unsafe { wallet_change_address() })
    }

    /// CIP-30 signTx. `tx_hex` is the unsigned tx CBOR (hex);
    /// `partial_sign` false means the wallet must fully witness the tx.
    /// On the resolved poll, `data` is the witness-set hex — assemble it
    /// onto the tx via `wallet_pallas::assemble_signed_tx` before
    /// [`submit_tx`].
    pub fn sign_tx(tx_hex: &str, partial_sign: bool) -> ReqId {
        ReqId(unsafe { wallet_sign_tx(JsObject::string(tx_hex), partial_sign as i32) })
    }

    /// CIP-30 submitTx. `tx_hex` is the fully-signed tx CBOR (hex). On
    /// the resolved poll, `data` is the tx hash.
    pub fn submit_tx(tx_hex: &str) -> ReqId {
        ReqId(unsafe { wallet_submit_tx(JsObject::string(tx_hex)) })
    }

    /// Drop the cached `api` reference on the JS side. The wallet's
    /// permission grant stays intact — next `connect` is silent (no
    /// popup). Use the OS-level "disconnect dapp" controls if you
    /// want to fully revoke.
    pub fn disconnect() {
        unsafe { wallet_disconnect() }
    }

    /// CIP-30 signData. `addr` is hex (stake or payment address).
    /// `payload_hex` is hex-encoded bytes the wallet will wrap in a
    /// COSE_Sign1 envelope and sign with the corresponding key.
    pub fn sign_data(addr: &str, payload_hex: &str) -> ReqId {
        ReqId(unsafe { wallet_sign_data(JsObject::string(addr), JsObject::string(payload_hex)) })
    }

    /// Ask for the icon wallet `key` announces, rasterised to `size`×`size`
    /// RGBA. Polls like any request; once it resolves `Ok`, take the pixels
    /// with [`take_icon`]. A wallet with no icon resolves `Err`.
    pub fn icon(key: &str, size: u16) -> ReqId {
        ReqId(unsafe { wallet_icon(JsObject::string(key), u32::from(size)) })
    }

    /// The pixels of a resolved [`icon`] request — `size * size * 4` bytes of
    /// RGBA, rows top to bottom — or `None` when there are none of that
    /// length. Taken once.
    pub fn take_icon(id: ReqId, size: u16) -> Option<Vec<u8>> {
        let mut rgba = Vec::new();
        unsafe { wallet_icon_pixels(id.0) }.to_byte_buffer(&mut rgba);
        super::icon_len_ok(&rgba, size).then_some(rgba)
    }

    /// Abandon a request. A promise that resolves later has nowhere to land.
    pub fn cancel(id: ReqId) {
        unsafe { wallet_cancel(id.0) }
    }

    pub fn poll(id: ReqId) -> PollResult {
        match poll_detailed(id) {
            PollResultDetailed::Pending => PollResult::Pending,
            PollResultDetailed::Ok { data } => PollResult::Ok { data },
            PollResultDetailed::Err { data, .. } => PollResult::Err { data },
        }
    }

    /// Poll and keep the failure kind. One promise → one result: calling both
    /// this and [`poll`] for the same id consumes it once.
    pub fn poll_detailed(id: ReqId) -> PollResultDetailed {
        let s = js_to_string(unsafe { wallet_poll(id.0) });
        serde_json::from_str(&s).unwrap_or(PollResultDetailed::Err {
            data: format!("malformed poll response: {s}"),
            kind: FailureKind::Failed,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::{FailureKind, PollResult, PollResultDetailed, Provider, ReqId};

    pub fn list_providers() -> Vec<Provider> {
        Vec::new()
    }
    pub fn connect(_name: &str) -> ReqId {
        ReqId(0)
    }
    pub fn refresh(_name: &str) -> ReqId {
        ReqId(0)
    }
    pub fn network_id() -> ReqId {
        ReqId(0)
    }
    pub fn reward_address() -> ReqId {
        ReqId(0)
    }
    pub fn balance() -> ReqId {
        ReqId(0)
    }
    pub fn utxos() -> ReqId {
        ReqId(0)
    }
    pub fn change_address() -> ReqId {
        ReqId(0)
    }
    pub fn sign_tx(_tx_hex: &str, _partial_sign: bool) -> ReqId {
        ReqId(0)
    }
    pub fn submit_tx(_tx_hex: &str) -> ReqId {
        ReqId(0)
    }
    pub fn disconnect() {}
    pub fn sign_data(_addr: &str, _payload_hex: &str) -> ReqId {
        ReqId(0)
    }
    pub fn icon(_key: &str, _size: u16) -> ReqId {
        ReqId(0)
    }
    pub fn take_icon(_id: ReqId, _size: u16) -> Option<Vec<u8>> {
        None
    }
    pub fn cancel(_id: ReqId) {}
    pub fn poll(_id: ReqId) -> PollResult {
        PollResult::Err {
            data: "wallet auth is web-only".into(),
        }
    }
    pub fn poll_detailed(_id: ReqId) -> PollResultDetailed {
        PollResultDetailed::Err {
            data: "wallet auth is web-only".into(),
            kind: FailureKind::Failed,
        }
    }
}

pub use imp::*;

#[cfg(test)]
mod tests {
    use super::icon_len_ok;

    #[test]
    fn only_a_whole_image_passes() {
        assert!(icon_len_ok(&[0; 2 * 2 * 4], 2));
        assert!(!icon_len_ok(&[0; 2 * 2 * 4 - 1], 2));
        assert!(
            !icon_len_ok(&[], 2),
            "a wallet with no pixels is not an icon"
        );
        assert!(!icon_len_ok(&[], 0));
    }
}
