//! Runtime-agnostic Ouroboros node-to-node client.
//!
//! Frames the multiplexer and speaks the four mini-protocols a chain follower
//! needs: handshake, chain-sync, block-fetch and keep-alive. The caller brings
//! any `futures-io` byte stream (a Cloudflare `Socket` through
//! `tokio_util::compat`, a tokio `TcpStream`, an in-memory pipe in a test), so
//! nothing here depends on an async runtime, a socket API or wasm-bindgen.
//!
//! Ported from `block-tap`'s crate of the same name, with the three changes a
//! LONG-LIVED connection needs and a batch sync never exercised:
//!
//! - [`Mux::recv_any`] is **cancel-safe**. Every byte read is stored in the mux
//!   before the next suspension point, so racing a receive against a timer and
//!   dropping the loser loses nothing. That race is how keep-alive gets sent
//!   while waiting on the chain.
//! - [`keepalive`] exists. Without it a quiet connection has no way to tell a
//!   slow chain from a dead socket.
//! - The handshake declares [`handshake::DiffusionMode::InitiatorOnly`], so a
//!   relay never starts its own mini-protocols back down a connection that has
//!   nobody to answer them (their bytes would otherwise pile up unread).

pub mod blockfetch;
pub mod chainsync;
pub mod codec;
pub mod handshake;
pub mod keepalive;
pub mod mux;

pub use codec::{Point, Tip};
pub use mux::{Mux, MuxError};

/// Mini-protocol numbers on a node-to-node bearer.
pub mod protocol {
    pub const HANDSHAKE: u16 = 0;
    pub const CHAIN_SYNC: u16 = 2;
    pub const BLOCK_FETCH: u16 = 3;
    pub const KEEP_ALIVE: u16 = 8;
}

/// Network magics for the public networks.
pub mod magic {
    pub const MAINNET: u64 = 764_824_073;
    pub const PREPROD: u64 = 1;
    pub const PREVIEW: u64 = 2;
}

/// Shorthand for turning a minicbor error into the string every protocol error
/// carries. The decode errors are only ever reported, never matched on.
pub(crate) fn cbor_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
