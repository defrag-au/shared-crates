//! Run a request/response service on a web worker, off the UI thread.
//!
//! # What this is for
//!
//! Cardano work in a browser is arithmetic-heavy and bursty. Evaluating a
//! Plutus script runs the same CEK machine a node runs, at roughly 7ms a
//! redeemer; a cart of thirty-odd items priced across several candidate
//! packings is seconds of it. Coin selection, CBOR decoding of a block, a
//! trait sweep over a collection — all the same shape: a pure function of its
//! inputs, expensive enough that doing it on the UI thread means a frozen tab.
//!
//! That shape is a SERVICE: a request in, a response out, progress in between.
//! It is not specific to carts, and this crate does not know what a cart is —
//! a [`Service`] is declared by the crate that owns the work, never here.
//! `cart_actions::local::CartBuild` is simply the first one.
//!
//! # What makes it reusable
//!
//! The worker artifact carries **no wasm-bindgen**. It talks to its host
//! through four raw wasm imports, so the module any host instantiates is a
//! plain `wasm32-unknown-unknown` binary with a legible ABI — bytes in, bytes
//! out. That matters here specifically: a macroquad/miniquad page can never
//! load a wasm-bindgen artifact, because miniquad's `gl.js` carries none of
//! the glue. Keeping the service half clean means one worker binary serves
//! every frontend the house builds, rather than one per runtime.
//!
//! The page half ([`Client`]) IS wasm-bindgen, because `web_sys::Worker` is.
//! The sibling that half needs for a macroquad/miniquad page is
//! [`MiniquadClient`], which speaks the same wire over the plugin protocol —
//! the ABI, the wire types and [`serve`] are unaffected by the runtime.
//!
//! # Compute only, and loudly so
//!
//! A handler may be `async`, because the code it calls usually is, but every
//! await must resolve immediately: there is no reactor here to wake a pending
//! future. [`serve`] drives each request with `now_or_never`, so a handler
//! that genuinely waits returns an error naming the problem instead of hanging
//! a thread nobody is watching.
//!
//! This is a real constraint and it is the right one. I/O belongs on the page,
//! which already has `fetch`, a wallet and a socket; what belongs here is the
//! arithmetic the page cannot afford to do itself. A service that needs to
//! fetch should be handed what it needs in its request.
//!
//! # Layout
//!
//! - [`Service`] — what a service IS: three types and a version.
//! - [`serve`] / [`Emitter`] — the worker side. Needs feature `service`.
//! - [`Client`] / [`Link`] — the page side, wasm-bindgen. Needs feature `client`.
//! - [`MiniquadClient`] / [`Link`] — the page side for miniquad. Needs feature
//!   `miniquad`.
//! - [`RUNTIME_JS`] — the worker's host runtime, for a build script to stamp.
//! - [`CLIENT_JS`] — the miniquad page's plugin, likewise.

use serde::{Deserialize, Serialize};

#[cfg(feature = "service")]
pub mod abi;
#[cfg(feature = "client")]
mod client;
#[cfg(feature = "miniquad")]
mod miniquad;

#[cfg(feature = "service")]
pub use abi::{Emitter, serve};
#[cfg(feature = "client")]
pub use client::Client;
#[cfg(feature = "miniquad")]
pub use miniquad::MiniquadClient;

/// How much the page currently trusts the worker it spawned.
///
/// A named state rather than a `ready` flag, because there are three
/// situations and a bool can only tell two of them apart — and the third is
/// the one that matters: a worker that answered, but is not the worker this
/// page was built to talk to.
#[derive(Clone, Debug)]
pub enum Link {
    /// Spawned; the handshake has not landed. Calls wait for it.
    Starting,
    /// Handshaked, right service, right protocol. Calls go straight over.
    Live,
    /// Unusable, and will not become usable. Calls must go wherever they would
    /// have gone without a worker at all.
    Rejected(String),
}

/// The host-side runtime, for a build script to write next to the artifact.
///
/// Exposed as a constant rather than shipped as a file so it cannot drift from
/// the ABI it implements: the two are compiled from the same source tree, and
/// a build that changes one carries the other with it. Same trick
/// `miniquad-platform` uses for its plugin.
#[cfg(feature = "service")]
pub const RUNTIME_JS: &str = include_str!("../js/offthread.js");

/// The page-side plugin for a miniquad host, for a build script to stamp next
/// to the artifact and load before `load(…wasm)`.
///
/// Exposed as a constant for the same reason [`RUNTIME_JS`] is: the JS and the
/// Rust imports it satisfies are compiled from one source tree, so a build that
/// changes one carries the other with it.
#[cfg(feature = "miniquad")]
pub const CLIENT_JS: &str = include_str!("../js/offthread-client.js");

/// Which shape of the exchange a worker binary speaks — BOTH directions.
///
/// Not just the request. The reply crosses the same positional codec, and the
/// first thing to break this boundary in practice was on the reply side, so a
/// version naming only the request would wave through a worker whose answers
/// cannot be read.
///
/// Bump a service's [`Service::PROTOCOL`] on any change to its request, its
/// response, its events, or anything reachable from them. postcard recovers
/// values by POSITION, so an unbumped change does not fail cleanly — it
/// decodes into a different valid value.
///
/// A newtype rather than a bare `u32` because it travels beside a request id,
/// which is also a bare number, and this boundary has already been bitten once
/// by two same-typed numbers in the wrong order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ProtocolVersion(pub u32);

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// One kind of work a worker can be asked to do.
///
/// Implemented on a marker type in a crate BOTH sides depend on, which is what
/// makes a mismatch a compile error on the near side and a version refusal on
/// the far one. The engine never looks inside any of the three types.
pub trait Service: 'static {
    /// For diagnostics, and for the handshake: spawning the wrong URL should
    /// say so, rather than failing later on a payload that will not decode.
    const NAME: &'static str;

    /// See [`ProtocolVersion`]. Bump on ANY shape change, either direction.
    const PROTOCOL: ProtocolVersion;

    /// Everything the work needs. It crosses a boundary, so it is owned data:
    /// nothing borrowed, nothing behind a handle the worker cannot follow.
    type Request: Serialize + for<'de> Deserialize<'de> + 'static;

    /// Progress, for whoever is drawing. Use `()` for a service with none.
    type Event: Serialize + for<'de> Deserialize<'de> + 'static;

    /// What the work produces.
    type Response: Serialize + for<'de> Deserialize<'de> + 'static;
}

/// One request, tagged so its reply can be found again.
#[derive(Serialize, Deserialize)]
pub struct Call<R> {
    pub id: u64,
    pub body: R,
}

/// Everything a worker says.
#[derive(Serialize, Deserialize)]
pub enum Reply<E, P> {
    /// The handler is installed, and this is what it speaks. Nothing posted
    /// before this is seen.
    ///
    /// FIRST variant deliberately. postcard writes the variant index, so a
    /// worker old enough to predate a later variant still encodes `Ready` the
    /// same way — which is what lets a mismatch be REPORTED rather than merely
    /// failing to decode.
    Ready {
        service: String,
        version: ProtocolVersion,
    },
    /// Progress. Untagged by request id on purpose: a worker handles one call
    /// at a time, and an id here would imply a concurrency this does not have.
    Event(E),
    /// The answer to [`Call::id`].
    Done { id: u64, result: Result<P, String> },
}
