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
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum PollResult {
    Pending,
    Ok { data: String },
    Err { data: String },
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

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::{PollResult, Provider, ReqId};
    use sapp_jsutils::JsObject;

    extern "C" {
        fn wallet_list_providers() -> JsObject;
        fn wallet_connect(name: JsObject) -> i32;
        fn wallet_reward_address() -> i32;
        fn wallet_balance() -> i32;
        fn wallet_disconnect();
        fn wallet_sign_data(addr: JsObject, payload_hex: JsObject) -> i32;
        fn wallet_poll(req_id: i32) -> JsObject;
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

    pub fn reward_address() -> ReqId {
        ReqId(unsafe { wallet_reward_address() })
    }

    /// CIP-30 getBalance — returns hex-encoded CBOR Value (lovelace +
    /// optional multiasset). Decode via [`crate::balance::extract_handle`]
    /// to surface the user's $handle.
    pub fn balance() -> ReqId {
        ReqId(unsafe { wallet_balance() })
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

    pub fn poll(id: ReqId) -> PollResult {
        let s = js_to_string(unsafe { wallet_poll(id.0) });
        serde_json::from_str(&s).unwrap_or(PollResult::Err {
            data: format!("malformed poll response: {s}"),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::{PollResult, Provider, ReqId};

    pub fn list_providers() -> Vec<Provider> {
        Vec::new()
    }
    pub fn connect(_name: &str) -> ReqId {
        ReqId(0)
    }
    pub fn reward_address() -> ReqId {
        ReqId(0)
    }
    pub fn balance() -> ReqId {
        ReqId(0)
    }
    pub fn disconnect() {}
    pub fn sign_data(_addr: &str, _payload_hex: &str) -> ReqId {
        ReqId(0)
    }
    pub fn poll(_id: ReqId) -> PollResult {
        PollResult::Err {
            data: "wallet auth is web-only".into(),
        }
    }
}

pub use imp::*;
