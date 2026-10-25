//! Cloudflare Realtime TURN credentials: the minting pair, the response shape,
//! and when a mint goes stale.
//!
//! TURN is the relay a symmetric NAT pair needs, and its credentials are
//! short-lived, so a client cannot carry them: something server-side mints them
//! and the browser asks for the result before it builds an `RTCPeerConnection`.
//!
//! The transport is deliberately not here. A Worker mints through `Fetch`, a
//! CLI through its own HTTP client, and a crate that did the HTTP could serve
//! only one of them. What is here is the part each caller would otherwise get
//! wrong in its own way:
//!
//! - [`TokenPair`] keeps the key id and the API token as the one value the
//!   secrets store holds, splits them once, and refuses to print the token.
//! - [`Minted`] parses the response — whose `iceServers` field is an object for
//!   one server and an array for several — and says whether its age makes it
//!   worth reusing.
//!
//! `serde` is the one dependency, and only for reading that response. A
//! hand-rolled JSON reader would be the part of this crate worth not
//! improvising — the same argument `freeice`'s `ice-check` makes about
//! MESSAGE-INTEGRITY — and every consumer already has it.

mod ice;
mod pair;

pub use ice::{IceServer, IceServersBody, MintError, MintRequest, Minted};
pub use pair::{Half, PairError, TokenPair};
