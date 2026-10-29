//! Cardano CIP-30 wallet bridge for miniquad WASM apps.
//!
//! The miniquad runtime (`gl.js`) does *not* use `wasm-bindgen`, so the
//! `wallet-core` / `wallet-leptos` path doesn't apply. This crate
//! implements the CIP-30 surface we need on top of miniquad's plugin
//! protocol (`sapp_jsutils::JsObject`) instead — same CIP-30 spec,
//! different bridge.
//!
//! ## Surface
//!
//! - [`list_providers`] / [`Provider`] — `window.cardano.<key>` discovery
//! - [`connect`] — `enable()` (wallet popup)
//! - [`network_id`] — `getNetworkId()` (`0` testnet, `1` mainnet)
//! - [`reward_address`] — `getRewardAddresses()` (hex)
//! - [`balance`] — `getBalance()` (hex-encoded CBOR Value)
//! - [`utxos`] — `getUtxos()` (JSON array of hex CBOR UTxOs; see
//!   [`parse_utxos`])
//! - [`change_address`] — `getChangeAddress()` (hex)
//! - [`sign_tx`] — CIP-30 `signTx` (unsigned tx hex → witness-set hex)
//! - [`submit_tx`] — CIP-30 `submitTx` (signed tx hex → tx hash)
//! - [`sign_data`] — CIP-30 `signData` (hex payload → COSE_Sign1 hex)
//! - [`disconnect`] — drops the cached JS-side api reference
//! - [`poll`] / [`PollResult`] — call each frame on the [`ReqId`] returned
//!   by the async calls above; promises resolve once and `poll` returns
//!   the payload once
//! - [`poll_detailed`] / [`PollResultDetailed`] — the same poll with the
//!   [`FailureKind`] kept, so a host can tell a reader's refusal from a wallet
//!   fault
//! - [`cancel`] — drop an in-flight request the host has abandoned, so the
//!   JS-side map cannot grow without bound
//! - [`icon`] / [`take_icon`] — the wallet's announced CIP-30 `icon`,
//!   rasterised by the browser to a square of RGBA, so an SVG icon needs no
//!   decoder on this side
//! - [`hex_to_bech32`] — address hex → `stake1…` / `addr1…` via pallas
//! - [`stake_address_bech32`] — the stake address a payment address delegates to
//! - [`balance::extract_handle`] — walk the balance CBOR for an ADA Handle
//! - [`evm`] — EVM wallets: EIP-6963 discovery and the EIP-1193 provider,
//!   in their own plugin ([`EVM_PLUGIN_JS`])
//!
//! ## Wiring it into a consumer
//!
//! Add this crate + [`miniquad_platform`] as deps. Stamp the JS plugin
//! files onto disk via `build.rs`:
//!
//! ```ignore
//! fs::write("web/wallet.js", wallet_miniquad::PLUGIN_JS)?;
//! fs::write("web/platform.js", miniquad_platform::PLUGIN_JS)?;
//! ```
//!
//! Load both in your HTML shell *after* `gl.js` and *before* the
//! `load("…wasm")` call:
//!
//! ```html
//! <script src="gl.js"></script>
//! <script src="sapp_jsutils.js"></script>
//! <script src="platform.js"></script>
//! <script src="wallet.js"></script>
//! <script>load("yourapp.wasm");</script>
//! ```
//!
//! Native builds get stubs — every async function returns
//! `ReqId(0)` and `poll` returns an error indicating wallet auth is
//! web-only, so the surface compiles identically across targets and
//! consumers can `cfg`-gate or call uniformly.

pub mod balance;
pub mod bridge;
pub mod evm;

pub use bridge::*;

/// CIP-30 `getNetworkId` for mainnet. The only network the route worker's
/// Splash and LumpPad venues deploy to.
pub const MAINNET_NETWORK_ID: u8 = 1;

/// The stake address a payment address delegates to, in bech32, when it has a
/// stake credential. `None` for an enterprise address, a script-stake address
/// pallas cannot name, or anything that is not a Shelley address.
///
/// The same rule `wallet-pallas`'s wrapper implements; kept here so a
/// miniquad host can derive the stake identity from the change address it
/// already fetched, without another wallet round trip or a second crate.
pub fn stake_address_bech32(hex: &str) -> Option<String> {
    use pallas_addresses::{Address, ScriptHash, StakeAddress, StakeKeyHash, StakePayload};

    match Address::from_hex(hex).ok()? {
        Address::Shelley(addr) => {
            let payload = match addr.delegation() {
                pallas_addresses::ShelleyDelegationPart::Key(hash) => {
                    StakePayload::Stake(StakeKeyHash::from(hash.as_ref()))
                }
                pallas_addresses::ShelleyDelegationPart::Script(hash) => {
                    StakePayload::Script(ScriptHash::from(hash.as_ref()))
                }
                _ => return None,
            };
            StakeAddress::new(addr.network(), payload).to_bech32().ok()
        }
        // A bare reward address IS a stake address — see `wallet-pallas`'s
        // wrapper for why this arm matters to `getRewardAddresses()`.
        Address::Stake(addr) => addr.to_bech32().ok(),
        _ => None,
    }
}

/// JavaScript source for the wallet plugin. Consumers materialise this
/// into their HTML shell directory (typically via `build.rs`).
pub const PLUGIN_JS: &str = include_str!("../js/wallet.js");

/// JavaScript source for the EVM plugin ([`evm`]), stamped beside
/// [`PLUGIN_JS`] as `evm.js` by a consumer that connects EVM wallets. A
/// consumer that never calls [`evm`] imports none of it, and needs none.
pub const EVM_PLUGIN_JS: &str = include_str!("../js/evm.js");

/// Convert a CIP-30 hex address into its bech32 form. Falls back to
/// returning the raw hex when the header byte is unknown or the
/// payload doesn't round-trip.
///
/// Works for both stake (`stake1…` / `stake_test1…`) and payment
/// (`addr1…` / `addr_test1…`) addresses; pallas does the heavy
/// lifting and infers the right hrp from the header byte.
pub fn hex_to_bech32(hex: &str) -> String {
    pallas_addresses::Address::from_hex(hex)
        .and_then(|a| a.to_bech32())
        .unwrap_or_else(|_| hex.to_string())
}

/// Parse the resolved `data` payload of a [`bridge::utxos`] poll — a JSON
/// array of hex-encoded CBOR `TransactionUnspentOutput` strings. Returns
/// an empty vec on a malformed payload (the wallet returned no UTxOs, or
/// the shape drifted), so callers can treat "no spendable inputs" and
/// "couldn't parse" uniformly.
pub fn parse_utxos(data: &str) -> Vec<String> {
    serde_json::from_str(data).unwrap_or_default()
}

/// `true` only on WASM builds where the wallet bridge can actually
/// reach `window.cardano`. Native consumers use this to short-circuit
/// flows that would otherwise step through phantom polling.
pub const AVAILABLE: bool = cfg!(target_arch = "wasm32");

/// Plugin version handshake. miniquad's `gl.js` looks for an export
/// named `<plugin>_crate_version` and compares it to the plugin's JS
/// `version` field. `wallet.js` declares `version: 4`, so we return 4.
///
/// Bumped 1 → 2 when the tx-signing surface (`utxos` / `change_address`
/// / `sign_tx` / `submit_tx`) was added; bumped 2 → 3 with `network_id`,
/// `cancel`, and the typed failure `kind` on poll errors; bumped 3 → 4 with
/// `icon` and its pixels. A consumer carrying
/// a stale `wallet.js` then fails the handshake loudly rather than
/// link-erroring on the missing imports. Re-stamp [`PLUGIN_JS`] (build.rs) to
/// clear it.
///
/// Only emitted on wasm32 — on native the symbol is meaningless.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn wallet_crate_version() -> u32 {
    4
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mainnet base address (header 0x01) over a key payment hash and a key
    /// stake hash. Constructed here rather than pasted as a fixture: the
    /// headers are the spec, and a pasted address would hide a parser drift.
    fn base_address() -> String {
        format!("01{}{}", "11".repeat(28), "22".repeat(28))
    }

    #[test]
    fn a_base_address_names_its_stake_address() {
        let hex = base_address();
        assert!(hex_to_bech32(&hex).starts_with("addr1"));
        let stake = stake_address_bech32(&hex).expect("a base address has a stake part");
        assert!(stake.starts_with("stake1"), "{stake}");
    }

    #[test]
    fn an_enterprise_address_has_no_stake_address() {
        let hex = format!("61{}", "11".repeat(28));
        assert!(stake_address_bech32(&hex).is_none());
    }

    /// `getRewardAddresses()` hands over a bare reward address, and the
    /// conversion must treat it as the stake address it is.
    #[test]
    fn a_reward_address_converts_to_itself() {
        let hex = format!("e1{}", "33".repeat(28));
        let stake = stake_address_bech32(&hex).expect("a reward address is a stake address");
        assert!(stake.starts_with("stake1"), "{stake}");
        assert_eq!(hex_to_bech32(&hex), stake);
    }

    /// The wire adds a `kind` to errors. An older plugin that omits it must
    /// still parse, as a failure rather than a silent decline.
    #[test]
    fn a_poll_error_keeps_its_kind() {
        let declined: bridge::PollResultDetailed =
            serde_json::from_str(r#"{"status":"err","data":"User declined","kind":"declined"}"#)
                .unwrap();
        assert!(matches!(
            declined,
            bridge::PollResultDetailed::Err {
                kind: bridge::FailureKind::Declined,
                ..
            }
        ));

        let old: bridge::PollResultDetailed =
            serde_json::from_str(r#"{"status":"err","data":"boom"}"#).unwrap();
        assert!(matches!(
            old,
            bridge::PollResultDetailed::Err {
                kind: bridge::FailureKind::Failed,
                ..
            }
        ));

        let ok: bridge::PollResultDetailed =
            serde_json::from_str(r#"{"status":"ok","data":"1"}"#).unwrap();
        assert!(matches!(ok, bridge::PollResultDetailed::Ok { .. }));
    }
}
