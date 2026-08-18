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

/// Path a *plugin* posts a [`RefreshMessage`] to — the one direction that runs
/// plugin → Augie rather than the other way.
///
/// On Augie's side, not a plugin's: it lives here because both ends must agree
/// on the shape, exactly like the paths above.
pub const REFRESH_PATH: &str = "/refresh";

/// Ask Augie to re-render a message a plugin already owns.
///
/// # Why this exists
///
/// A plugin renders by *returning* a [`CommandResponse`] from an interaction.
/// That covers every case where Discord is the one asking. It does not cover a
/// plugin learning something out-of-band — a Discord Activity committing over
/// plain HTTP, a queue consumer finishing a job — and needing the original
/// message to stop showing stale state.
///
/// The plugin cannot render it alone: this crate is deliberately twilight-free
/// so both repos can pin it as a pure wire format, so nothing here knows how a
/// [`PluginBlock`] becomes a Discord component. Augie does. So the plugin sends
/// the layout it wants and the credentials proving it owns that message, and
/// Augie renders it with the same converter the interaction path uses.
///
/// # Why channel + message, not an interaction token
///
/// The obvious route — `PATCH /webhooks/{app}/{token}/messages/@original` —
/// does not work here. A component answered with `LAUNCH_ACTIVITY` neither
/// creates nor updates a message, so that interaction has no "original
/// response" for `@original` to name. The token also dies after 15 minutes,
/// and a player can browse a roster for longer than that.
///
/// Augie is the bot that posted the message, so it edits the message directly
/// with its own token. No expiry, and no dependence on what `@original` means
/// for a response type that produces nothing.
///
/// # Authority
///
/// Augie does not re-check ownership — it keeps no record of which plugin owns
/// which message. The endpoint is internal-key gated, so the caller is a
/// trusted service by construction. Note this is strictly *more* power than an
/// interaction token conferred: it can edit any message the bot authored.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RefreshMessage {
    pub channel_id: String,
    /// The message to redraw — from [`ComponentInvocation::message_id`].
    pub message_id: String,
    /// The layout to render. `ephemeral` is ignored — a message's ephemerality
    /// is fixed when it is created and cannot be edited.
    pub response: CommandResponse,
}
