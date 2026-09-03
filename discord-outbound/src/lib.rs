//! Posting and editing Discord messages, from anywhere.
//!
//! One message model ([`discord_message::MessageBody`]), two verbs
//! ([`DiscordOutbound::send`] and [`DiscordOutbound::edit`]), and one HTTP
//! request method per platform. See
//! `augminted-bots/docs/DISCORD_OUTBOUND_CONSOLIDATION_DESIGN.md` for what this
//! replaced: two independent clients, fifteen send/edit entry points, three
//! multipart implementations and three Components V2 renderers.
//!
//! # Reads are not here
//!
//! Guild members, roles, audit logs, command registration and permission sync
//! all stay where they are. Folding them in "while we're in here" is precisely
//! how the file this replaces reached 1,942 lines.
//!
//! # Choosing a feature
//!
//! `reqwest` and `gloo` name **HTTP stacks, not targets**. `reqwest` compiles
//! for `wasm32-unknown-unknown` and is what augminted-bots' workers already
//! use; `gloo` is what cnft.dev-workers' workers use. Pick whichever the rest
//! of the crate already links, not whichever sounds like the platform.

use thiserror::Error;

#[cfg(feature = "native")]
mod native;
#[cfg(feature = "wasm")]
mod wasm;

#[cfg(feature = "wasm")]
use worker_stack::worker;

pub mod multipart;
pub mod ratelimit;
mod send;
mod target;
pub mod types;

#[cfg(feature = "native")]
pub use native::*;
#[cfg(feature = "wasm")]
pub use wasm::*;

pub use send::*;
pub use target::*;
pub use types::*;

pub mod compat;

pub(crate) const BASE_URL: &str = discord_message::BASE_URL;

#[derive(Error, Debug)]
pub enum DiscordError {
    #[error("Request failed: {0}")]
    Request(String),

    #[error("Serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Rate limited: retry after {retry_after:.2}s (global: {global})")]
    RateLimited { retry_after: f64, global: bool },

    /// Cloudflare error 1015 — Discord's *edge* refusing the caller's egress
    /// IP, not a Discord per-route bucket. A distinct variant because it needs
    /// a different answer than backoff, and because matching on it beats
    /// matching on a message string. See `ratelimit::is_cloudflare_block`.
    #[error("Blocked by Cloudflare (error 1015) — an edge block, not a Discord rate limit")]
    CloudflareBlocked,

    #[error("Invalid attachment: {0}")]
    InvalidAttachment(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[cfg(feature = "native")]
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[cfg(feature = "wasm")]
    #[error("Gloo error: {0}")]
    Gloo(String),

    #[cfg(feature = "wasm")]
    #[error("Worker error: {0}")]
    Worker(#[from] worker::Error),
}
