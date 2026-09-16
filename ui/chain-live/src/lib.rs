//! `chain-live` — the chain's heartbeat, and this frontend's own transactions
//! riding it, dropped into any egui frontend.
//!
//! Everything a surface needs to feel live without lying about the chain:
//!
//! - **A feed.** A ui-flow socket to a heartbeat host (the augminted gateway
//!   today), folded into a [`chain_heartbeat::Heartbeat`]. A dropped socket
//!   reads as offline, never as a chain that stopped producing.
//! - **Your transactions.** The host says when it submitted one; the crate
//!   tracks it until it lands, and places it on the block it landed in.
//! - **Confirmation on block arrival.** With [`Confirmation::Polled`] the crate
//!   asks the host's [`TxStatusSource`] once per block, a jittered few seconds
//!   after it, and only while something is waiting. Hosts that already learn of
//!   landings (a live delta) use [`Confirmation::Reported`] and call
//!   [`ChainLive::landed`] themselves; with
//!   [`Confirmation::reported_with_blocks`] the crate looks up which block each
//!   reported landing is in, so it still rides its bar.
//! - **The widgets.** [`ChainLive::pulse`] for a header, [`ChainLive::train`]
//!   for a drawer or panel footer. Both draw nothing until the feed has
//!   delivered, so an unreachable host, or one following another network,
//!   leaves no train rather than a mysterious "offline" or a wrong one.
//!
//! ```ignore
//! struct MyStatus;
//! impl TxStatusSource for MyStatus {
//!     fn landed(&self, tx_hashes: Vec<String>) -> LocalBoxFuture<'static, Result<Vec<TxLanding>, String>> {
//!         Box::pin(async move { api::tx_status(tx_hashes).await })
//!     }
//! }
//!
//! let mut chain = ChainLive::new(ChainLiveConfig {
//!     network: Network::Mainnet,
//!     feed: FeedEndpoint::Blocks, // blocks-mainnet.hodlcroft.com
//!     confirmation: Confirmation::polled(MyStatus),
//! });
//!
//! // Every frame, before drawing:
//! for event in chain.tick(ui.ctx()) {
//!     let ChainLiveEvent::Landed(landing) = event;
//!     // move the host's own state on
//! }
//! chain.pulse(ui);               // in the header
//! chain.train(ui);               // pinned under the cart
//!
//! // When the cart submits:
//! chain.submitted(tx_hash, "Your swap");
//! ```
//!
//! The pure part — what is waiting, when to ask, what a response means — is
//! [`tracker`]'s and tested natively. The socket, clock and task spawning are
//! browser-side and only meaningful on `wasm32`.

mod feed;
mod live;
mod tracker;

// The tracker moved to `chain-heartbeat` (renderer-free, so macroquad can host
// it too). Re-exported under the names consumers already use — `TrackEvent` is
// this crate's `ChainLiveEvent` — so nothing downstream changes.
pub use chain_heartbeat::{
    Network, PollTiming, TrackEvent as ChainLiveEvent, TrackedTx, TxLanding, TxProgress,
};
pub use feed::{FeedEndpoint, FeedLink};
pub use live::{CONFIRMATION_TIMING, ChainLive, ChainLiveConfig, Confirmation, TxStatusSource};
