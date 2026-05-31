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
//! - [`reward_address`] — `getRewardAddresses()` (hex)
//! - [`balance`] — `getBalance()` (hex-encoded CBOR Value)
//! - [`sign_data`] — CIP-30 `signData` (hex payload → COSE_Sign1 hex)
//! - [`disconnect`] — drops the cached JS-side api reference
//! - [`poll`] / [`PollResult`] — call each frame on the [`ReqId`] returned
//!   by the async calls above; promises resolve once and `poll` returns
//!   the payload once
//! - [`hex_to_bech32`] — address hex → `stake1…` / `addr1…` via pallas
//! - [`balance::extract_handle`] — walk the balance CBOR for an ADA Handle
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

pub use bridge::*;

/// JavaScript source for the wallet plugin. Consumers materialise this
/// into their HTML shell directory (typically via `build.rs`).
pub const PLUGIN_JS: &str = include_str!("../js/wallet.js");

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

/// `true` only on WASM builds where the wallet bridge can actually
/// reach `window.cardano`. Native consumers use this to short-circuit
/// flows that would otherwise step through phantom polling.
pub const AVAILABLE: bool = cfg!(target_arch = "wasm32");

/// Plugin version handshake. miniquad's `gl.js` looks for an export
/// named `<plugin>_crate_version` and compares it to the plugin's JS
/// `version` field. `wallet.js` declares `version: 1`, so we return 1.
///
/// Only emitted on wasm32 — on native the symbol is meaningless.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
extern "C" fn wallet_crate_version() -> u32 {
    1
}
