//! EVM wallets over the miniquad plugin protocol — the counterpart of
//! [`crate::bridge`]'s CIP-30 surface, and of `wallet-core`'s wasm-bindgen
//! `evm` module.
//!
//! Wallets are found through EIP-6963 announcements ([`list_wallets`]), each
//! carrying its own provider, so the wallet the reader picks is the one that is
//! asked — not whichever extension won `window.ethereum`. Every call that waits
//! returns a [`ReqId`]; [`poll`] it each frame, exactly as the CIP-30 bridge
//! works. [`changes`] reports what the connected wallet said on its own since
//! the last call: an account switch, a chain switch, or a disconnection.
//!
//! The page loads [`crate::EVM_PLUGIN_JS`] (`evm.js`) beside `wallet.js`. Native
//! builds get stubs: no wallets, and every request fails with "web-only".

use serde::{Deserialize, Serialize};

use crate::bridge::{FailureKind, PollResultDetailed, ReqId};

/// A chain a host wants the wallet on, and how to add it to a wallet that
/// does not know it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTarget {
    pub chain_id: u64,
    /// The EIP-3085 details offered when the wallet answers "unrecognised
    /// chain". `None`: ask to switch, never to add.
    pub add: Option<AddChain>,
}

/// EIP-3085 `wallet_addEthereumChain`'s parameters, less the chain id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddChain {
    pub chain_name: String,
    pub rpc_urls: Vec<String>,
    pub native_currency: NativeCurrency,
    pub block_explorer_urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeCurrency {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

/// The JSON `evm_switch_chain` reads.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchRequest<'a> {
    chain_id: String,
    add: Option<&'a AddChain>,
}

impl ChainTarget {
    /// The request the plugin reads: the id as EIP-1193's `0x…` hex.
    fn request_json(&self) -> String {
        serde_json::to_string(&SwitchRequest {
            chain_id: format!("0x{:x}", self.chain_id),
            add: self.add.as_ref(),
        })
        .unwrap_or_default()
    }
}

/// One EVM wallet that announced itself under EIP-6963.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EvmWallet {
    /// The announcement's `uuid`: the handle to connect it by. Opaque — the
    /// spec sets no format, so it is compared, never parsed.
    pub id: String,
    pub name: String,
    /// Reverse-DNS name (`io.rabby`): the stable way to recognise a wallet
    /// across versions. Empty when the wallet gave none.
    #[serde(default)]
    pub rdns: String,
}

/// What the connected wallet reported on its own since the last [`changes`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct EvmChanges {
    /// `accountsChanged`: the accounts now permitted. Empty means the reader
    /// disconnected this site in the wallet, or locked it.
    pub accounts: Option<Vec<String>>,
    /// `chainChanged`: the new `0x…` chain id.
    pub chain: Option<String>,
}

/// An EIP-155 chain id from `eth_chainId`'s `0x…` hex. `None` for anything
/// else, rather than a guess.
pub fn parse_chain_id(hex: &str) -> Option<u64> {
    let digits = hex
        .trim()
        .strip_prefix("0x")
        .or_else(|| hex.trim().strip_prefix("0X"))?;
    u64::from_str_radix(digits, 16).ok()
}

/// Whether `rgba` is a whole `size`×`size` image, before it becomes a texture.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn icon_len_ok(rgba: &[u8], size: u16) -> bool {
    size > 0 && rgba.len() == usize::from(size) * usize::from(size) * 4
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::{
        ChainTarget, EvmChanges, EvmWallet, FailureKind, PollResultDetailed, ReqId, icon_len_ok,
    };
    use sapp_jsutils::JsObject;

    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn evm_list_wallets() -> JsObject;
        fn evm_connect(id: JsObject, silent: i32) -> i32;
        fn evm_chain_id() -> i32;
        fn evm_switch_chain(request: JsObject) -> i32;
        fn evm_icon(id: JsObject, size: u32) -> i32;
        fn evm_icon_pixels(req: i32) -> JsObject;
        fn evm_disconnect();
        fn evm_changes() -> JsObject;
        fn evm_cancel(req: i32);
        fn evm_poll(req: i32) -> JsObject;
    }

    fn js_to_string(js: JsObject) -> String {
        let mut s = String::new();
        js.to_string(&mut s);
        s
    }

    pub fn list_wallets() -> Vec<EvmWallet> {
        serde_json::from_str(&js_to_string(unsafe { evm_list_wallets() })).unwrap_or_default()
    }

    /// Ask wallet `id` for its accounts. `silent` uses `eth_accounts`, which
    /// never prompts: the reconnect on load, answering `Declined` when the site
    /// is not permitted. On ok, `data` is the first account.
    pub fn connect(id: &str, silent: bool) -> ReqId {
        ReqId(unsafe { evm_connect(JsObject::string(id), i32::from(silent)) })
    }

    /// `eth_chainId` of the connected wallet; on ok, `data` is `0x…` hex.
    pub fn chain_id() -> ReqId {
        ReqId(unsafe { evm_chain_id() })
    }

    /// Ask the connected wallet onto `target` (EIP-3326), adding it first
    /// (EIP-3085) when the wallet does not know it and `target.add` says how.
    /// On ok, `data` is the chain the wallet is on afterwards — read back, so
    /// a wallet that "switched" elsewhere is not taken at its word.
    pub fn switch_chain(target: &ChainTarget) -> ReqId {
        ReqId(unsafe { evm_switch_chain(JsObject::string(&target.request_json())) })
    }

    pub fn icon(id: &str, size: u16) -> ReqId {
        ReqId(unsafe { evm_icon(JsObject::string(id), u32::from(size)) })
    }

    pub fn take_icon(req: ReqId, size: u16) -> Option<Vec<u8>> {
        let mut rgba = Vec::new();
        unsafe { evm_icon_pixels(req.0) }.to_byte_buffer(&mut rgba);
        icon_len_ok(&rgba, size).then_some(rgba)
    }

    /// Let go, revoking the site's permission where the wallet supports it.
    pub fn disconnect() {
        unsafe { evm_disconnect() }
    }

    pub fn changes() -> EvmChanges {
        serde_json::from_str(&js_to_string(unsafe { evm_changes() })).unwrap_or_default()
    }

    pub fn cancel(req: ReqId) {
        unsafe { evm_cancel(req.0) }
    }

    pub fn poll(req: ReqId) -> PollResultDetailed {
        let s = js_to_string(unsafe { evm_poll(req.0) });
        serde_json::from_str(&s).unwrap_or(PollResultDetailed::Err {
            data: format!("malformed poll response: {s}"),
            kind: FailureKind::Failed,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::{ChainTarget, EvmChanges, EvmWallet, FailureKind, PollResultDetailed, ReqId};

    pub fn list_wallets() -> Vec<EvmWallet> {
        Vec::new()
    }
    pub fn connect(_id: &str, _silent: bool) -> ReqId {
        ReqId(0)
    }
    pub fn chain_id() -> ReqId {
        ReqId(0)
    }
    pub fn switch_chain(target: &ChainTarget) -> ReqId {
        // Built natively too, so the request's shape is checked off the web.
        let _ = target.request_json();
        ReqId(0)
    }
    pub fn icon(_id: &str, _size: u16) -> ReqId {
        ReqId(0)
    }
    pub fn take_icon(_req: ReqId, _size: u16) -> Option<Vec<u8>> {
        None
    }
    pub fn disconnect() {}
    pub fn changes() -> EvmChanges {
        EvmChanges::default()
    }
    pub fn cancel(_req: ReqId) {}
    pub fn poll(_req: ReqId) -> PollResultDetailed {
        PollResultDetailed::Err {
            data: "EVM wallets are web-only".into(),
            kind: FailureKind::Failed,
        }
    }
}

pub use imp::*;

/// Plugin version handshake for `evm.js`, which declares `version: 2`.
/// Bumped 1 → 2 with `switch_chain`.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn evm_crate_version() -> u32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the plugin reads: the id as `0x…` hex, and EIP-3085's own field
    /// names for the chain to add.
    #[test]
    fn a_switch_request_is_written_as_the_plugin_reads_it() {
        let target = ChainTarget {
            chain_id: 4663,
            add: Some(AddChain {
                chain_name: "Robinhood Chain".into(),
                rpc_urls: vec!["https://rpc.example".into()],
                native_currency: NativeCurrency {
                    name: "Ether".into(),
                    symbol: "ETH".into(),
                    decimals: 18,
                },
                block_explorer_urls: Vec::new(),
            }),
        };
        assert_eq!(
            target.request_json(),
            r#"{"chainId":"0x1237","add":{"chainName":"Robinhood Chain","rpcUrls":["https://rpc.example"],"nativeCurrency":{"name":"Ether","symbol":"ETH","decimals":18},"blockExplorerUrls":[]}}"#
        );
        let switch_only = ChainTarget {
            chain_id: 1,
            add: None,
        };
        assert_eq!(
            switch_only.request_json(),
            r#"{"chainId":"0x1","add":null}"#
        );
    }

    #[test]
    fn a_chain_id_reads_from_hex() {
        assert_eq!(parse_chain_id("0x1"), Some(1));
        assert_eq!(parse_chain_id("0x1237"), Some(4663));
        assert_eq!(parse_chain_id(" 0X89 "), Some(137));
        assert_eq!(
            parse_chain_id("1"),
            None,
            "eth_chainId is always 0x-prefixed"
        );
        assert_eq!(parse_chain_id("0xzz"), None);
    }

    #[test]
    fn changes_read_as_the_plugin_writes_them() {
        let none: EvmChanges = serde_json::from_str(r#"{"accounts":null,"chain":null}"#).unwrap();
        assert_eq!(none, EvmChanges::default());
        let both: EvmChanges =
            serde_json::from_str(r#"{"accounts":["0xab"],"chain":"0x1"}"#).unwrap();
        assert_eq!(both.accounts.as_deref(), Some(&["0xab".to_string()][..]));
        assert_eq!(both.chain.as_deref(), Some("0x1"));
    }

    #[test]
    fn a_wallet_without_an_rdns_still_lists() {
        let wallets: Vec<EvmWallet> =
            serde_json::from_str(r#"[{"id":"u-1","name":"Rabby"}]"#).unwrap();
        assert_eq!(wallets[0].rdns, "");
    }
}
