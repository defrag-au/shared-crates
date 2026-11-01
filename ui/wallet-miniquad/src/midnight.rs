//! Midnight wallets over the miniquad plugin protocol — the counterpart of
//! [`crate::evm`] and [`crate::bridge`]'s CIP-30 surface, and of the DApp
//! connector's own `ConnectedAPI`.
//!
//! Wallets install an `InitialAPI` on `window.midnight`, keyed by a uuid they
//! generate; [`list_wallets`] reads that object. Every call that waits returns a
//! [`ReqId`]; [`poll`] it each frame, exactly as the other two bridges work.
//!
//! The surface is deliberately the smallest one a link needs: connect to
//! mainnet and read the unshielded address ([`connect`]), and sign a text
//! message ([`sign_message`]). The message goes over as text, not hex — the
//! **wallet** prepends `midnight_signed_message:<byte length>:` and hashes the
//! result before signing, so pre-hashing here would produce a signature the
//! server refuses.
//!
//! The page loads [`crate::MIDNIGHT_PLUGIN_JS`] (`midnight.js`) beside
//! `wallet.js`. Native builds get stubs: no wallets, and every request fails
//! with "web-only".

use serde::{Deserialize, Serialize};

use crate::bridge::{FailureKind, PollResultDetailed, ReqId};

/// The network this bridge connects to. The connector specification defines the
/// id ("The network id of mainnet is `mainnet`"), and a link is stored as
/// `midnight:mainnet`, so a wallet that can only offer another network cannot
/// be linked here.
pub const MAINNET_NETWORK_ID: &str = "mainnet";

/// One wallet installed on `window.midnight`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MidnightWallet {
    /// The key the wallet installed itself under: the handle to connect it by.
    /// A UUIDv4 the wallet generated, so it is compared, never parsed.
    pub id: String,
    pub name: String,
    /// Reverse-DNS name (`com.example.wallet`): the stable way to recognise a
    /// wallet across versions — and the only way, since the id is fresh each
    /// install. Empty when the wallet gave none.
    #[serde(default)]
    pub rdns: String,
    /// The `@midnightntwrk/dapp-connector-api` version the wallet implements.
    /// Carried for display and diagnosis: the plugin has already refused any
    /// version this bridge does not speak, so a wallet that lists is one
    /// [`connect`] can be asked something.
    #[serde(default, rename = "apiVersion")]
    pub api_version: String,
}

/// A `Signature` as the wallet's `signData` returns it.
///
/// The strings are the connector's own encoding of the bytes: 64 bytes of
/// `r ‖ s` for either scheme, and a verifying key of 32 bytes (BIP-340 x-only)
/// or 33 (SEC1 compressed), hex-encoded. A wallet that sends another encoding is
/// refused by the server with a 400 — it cannot link the wrong account.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Signature {
    /// 64-byte `r ‖ s`, hex.
    pub signature: String,
    /// The verifying key, hex.
    #[serde(rename = "verifyingKey")]
    pub verifying_key: String,
    /// `schnorr_bip340`, `ecdsa_secp256k1_sha256`, or `None` — which the
    /// specification defines as `schnorr_bip340`.
    #[serde(default)]
    pub scheme: Option<String>,
}

/// The JSON `midnight_sign_message` reads.
#[derive(Serialize)]
struct SignRequest<'a> {
    address: &'a str,
    message: &'a str,
}

/// What `midnight_sign_message` is sent for `message` by `address`.
fn sign_request_json(address: &str, message: &str) -> String {
    serde_json::to_string(&SignRequest { address, message }).unwrap_or_default()
}

/// The address in `data`, when `data` is a sign reply.
pub fn parse_signature(data: &str) -> Option<Signature> {
    serde_json::from_str(data).ok()
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::{FailureKind, MidnightWallet, PollResultDetailed, ReqId, sign_request_json};
    use sapp_jsutils::JsObject;

    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn midnight_list_wallets() -> JsObject;
        fn midnight_connect(id: JsObject) -> i32;
        fn midnight_sign_message(request: JsObject) -> i32;
        fn midnight_disconnect();
        fn midnight_cancel(req: i32);
        fn midnight_poll(req: i32) -> JsObject;
    }

    fn js_to_string(js: JsObject) -> String {
        let mut s = String::new();
        js.to_string(&mut s);
        s
    }

    pub fn list_wallets() -> Vec<MidnightWallet> {
        serde_json::from_str(&js_to_string(unsafe { midnight_list_wallets() })).unwrap_or_default()
    }

    /// Connect wallet `id` on mainnet and read its unshielded address. On ok,
    /// `data` is that address (`mn_addr…`); a reader refusing is `Declined`, and
    /// a wallet that connected to another network is a `Failed` naming it.
    pub fn connect(id: &str) -> ReqId {
        ReqId(unsafe { midnight_connect(JsObject::string(id)) })
    }

    /// Ask the connected wallet to sign `message` for `address`, as text. On ok,
    /// `data` is the wallet's [`Signature`] as JSON — read it with
    /// [`super::parse_signature`].
    pub fn sign_message(address: &str, message: &str) -> ReqId {
        let json = sign_request_json(address, message);
        ReqId(unsafe { midnight_sign_message(JsObject::string(&json)) })
    }

    /// Forget the connected API. The connector has no revoke, so the wallet's
    /// own settings are where a grant is withdrawn.
    pub fn disconnect() {
        unsafe { midnight_disconnect() }
    }

    pub fn cancel(req: ReqId) {
        unsafe { midnight_cancel(req.0) }
    }

    pub fn poll(req: ReqId) -> PollResultDetailed {
        let s = js_to_string(unsafe { midnight_poll(req.0) });
        serde_json::from_str(&s).unwrap_or(PollResultDetailed::Err {
            data: format!("malformed poll response: {s}"),
            kind: FailureKind::Failed,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::{FailureKind, MidnightWallet, PollResultDetailed, ReqId, sign_request_json};

    pub fn list_wallets() -> Vec<MidnightWallet> {
        Vec::new()
    }
    pub fn connect(_id: &str) -> ReqId {
        ReqId(0)
    }
    pub fn sign_message(address: &str, message: &str) -> ReqId {
        // Built natively too, so the request's shape is checked off the web.
        let _ = sign_request_json(address, message);
        ReqId(0)
    }
    pub fn disconnect() {}
    pub fn cancel(_req: ReqId) {}
    pub fn poll(_req: ReqId) -> PollResultDetailed {
        PollResultDetailed::Err {
            data: "Midnight wallets are web-only".into(),
            kind: FailureKind::Failed,
        }
    }
}

pub use imp::*;

/// Plugin version handshake for `midnight.js`, which declares `version: 1`.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn midnight_crate_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the plugin reads: the address the wallet should sign for, and the
    /// message as plain text — not hex, which is what the other two chains send.
    #[test]
    fn a_sign_request_carries_the_message_as_text() {
        assert_eq!(
            sign_request_json("mn_addr1qq", "Link this wallet"),
            r#"{"address":"mn_addr1qq","message":"Link this wallet"}"#
        );
    }

    /// The wallet's own field names, and `scheme` absent when it did not name
    /// one — which the specification defines as `schnorr_bip340`.
    #[test]
    fn a_signature_reads_as_the_wallet_writes_it() {
        let absent: Signature =
            serde_json::from_str(r#"{"signature":"ef","verifyingKey":"12"}"#).unwrap();
        assert_eq!(absent.signature, "ef");
        assert_eq!(absent.verifying_key, "12");
        assert_eq!(absent.scheme, None);

        let named: Signature = serde_json::from_str(
            r#"{"signature":"ef","verifyingKey":"12","scheme":"ecdsa_secp256k1_sha256"}"#,
        )
        .unwrap();
        assert_eq!(named.scheme.as_deref(), Some("ecdsa_secp256k1_sha256"));
    }

    /// A wallet that gave no rdns still lists: the modal shows its name, and
    /// nothing reads the rdns to connect it.
    #[test]
    fn a_wallet_without_an_rdns_still_lists() {
        let wallets: Vec<MidnightWallet> =
            serde_json::from_str(r#"[{"id":"u-1","name":"1AM","apiVersion":"4.0.1"}]"#).unwrap();
        assert_eq!(wallets[0].rdns, "");
        assert_eq!(wallets[0].api_version, "4.0.1");
    }

    /// The plugin's own reply, through the parser a host uses.
    #[test]
    fn a_sign_reply_parses_or_is_none() {
        let signature = parse_signature(r#"{"signature":"ef","verifyingKey":"12","scheme":null}"#)
            .expect("a sign reply");
        assert_eq!(signature.verifying_key, "12");
        assert_eq!(parse_signature("not json"), None);
    }
}
