//! Wire protocol for services that advertise Discord commands to Augie.
//!
//! A **plugin** is any service that wants a Discord front-end without Augie
//! growing code that knows about it. The plugin advertises what it exposes;
//! Augie discovers, registers with Discord, and routes interactions back.
//!
//! ## The contract
//!
//! | Endpoint | Direction | Purpose |
//! |----------|-----------|---------|
//! | `GET /_augie/manifest` | Augie → plugin | Advertise commands ([`ServiceManifest`]) |
//! | `POST /_augie/command` | Augie → plugin | Invoke a command ([`CommandInvocation`] → [`CommandResponse`]) |
//! | `POST /_augie/component` | Augie → plugin | Button / select callback ([`ComponentInvocation`] → [`CommandResponse`]) |
//!
//! Only the first two are required. A plugin that never returns components
//! never receives component callbacks.
//!
//! ## Why this crate carries no Discord types
//!
//! It would be natural to put `twilight_model::http::interaction::InteractionResponseData`
//! on the wire and be done. That is not possible: **augminted-bots is on
//! twilight 0.17 and cnft.dev-workers is on twilight 0.16**. A shared crate
//! exposing twilight types could not be consumed by both without forcing one
//! repo through a twilight migration, and mixing majors produces the
//! duplicate-crate type mismatches this workspace has been bitten by before.
//!
//! So the protocol defines its own minimal, `serde`-only vocabulary
//! ([`PluginEmbed`], [`PluginComponent`], …) and each host converts at its own
//! edge. The crate depends on `serde` and nothing else, which also keeps it
//! trivially WASM-safe.
//!
//! ## Snowflakes are strings
//!
//! Every Discord ID on this protocol is a `String`, never `u64`. Snowflakes
//! exceed `Number.MAX_SAFE_INTEGER`, and both sides of this wire run in WASM
//! where a `u64` silently loses precision through JS. Parse at the edge if you
//! need an integer.

mod address;
mod invocation;
mod manifest;
mod response;

pub use address::*;
pub use invocation::*;
pub use manifest::*;
pub use response::*;

/// Path Augie fetches to discover a plugin's command surface.
pub const MANIFEST_PATH: &str = "/_augie/manifest";
/// Path Augie posts a [`CommandInvocation`] to.
pub const COMMAND_PATH: &str = "/_augie/command";
/// Path Augie posts a [`ComponentInvocation`] to.
pub const COMPONENT_PATH: &str = "/_augie/component";
