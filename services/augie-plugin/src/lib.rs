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
//! | `GET /_augie/manifest` | Augie → plugin | Advertise commands and tools ([`ServiceManifest`]) |
//! | `POST /_augie/command` | Augie → plugin | Invoke a command ([`CommandInvocation`] → [`CommandResponse`]) |
//! | `POST /_augie/component` | Augie → plugin | Button / select callback ([`ComponentInvocation`] → [`CommandResponse`]) |
//! | `POST /_augie/tool` | Augie → plugin | Run a tool for an agent ([`ToolInvocation`] → [`ToolResponse`]) |
//!
//! Only the first two are required. A plugin that never returns components
//! never receives component callbacks, and one that advertises no
//! [`PluginTool`]s is never routed to by an agent.
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
//! So the vocabulary is serde-only, and it lives in **`discord-message`** —
//! [`discord_message::MessageBody`], `PluginEmbed`, `PluginBlock` and the
//! `wire` renderer that turns them into Discord's JSON. It is a separate crate
//! because the outbound client needs the same types, and a Discord client
//! depending on the plugin protocol would be backwards.
//!
//! Import those directly (`use discord_message::PluginBlock;`) — this crate
//! does not re-export them, so there is one place each type lives.
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
mod tool;

pub use address::*;
pub use invocation::*;
pub use manifest::*;
pub use response::*;
pub use tool::*;

/// Path Augie fetches to discover a plugin's command surface.
pub const MANIFEST_PATH: &str = "/_augie/manifest";
/// Path Augie posts a [`CommandInvocation`] to.
pub const COMMAND_PATH: &str = "/_augie/command";
/// Path Augie posts a [`ComponentInvocation`] to.
pub const COMPONENT_PATH: &str = "/_augie/component";
/// Path Augie posts a [`ToolInvocation`] to, on an agent's behalf.
pub const TOOL_PATH: &str = "/_augie/tool";

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
/// The plugin cannot send it alone: editing a message means holding either the
/// bot token or the interaction token, and a plugin has neither. So the plugin
/// sends the layout it wants and the credentials proving it owns that message,
/// and Augie renders it with the same converter the interaction path uses.
///
/// # Addressed through the interaction webhook, not the channel
///
/// Which route to use is [`discord_message::MessageTarget`]'s business, and the
/// two are not interchangeable — see its docs. The short version: an ephemeral
/// message is only reachable through its interaction, which is what bounds a
/// refresh to the token's 15 minutes.
///
/// # Authority
///
/// The interaction token *is* the authority — Discord issued it to whoever it
/// handed the interaction. Augie does not re-check ownership because it keeps
/// no record of which plugin owns which message; the endpoint is internal-key
/// gated, so the caller is a trusted service by construction.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RefreshMessage {
    pub target: discord_message::MessageTarget,
    /// Guild the message lives in, and the plugin's name in that guild's
    /// config.
    ///
    /// Needed because a redrawn layout's buttons must be **re-registered**:
    /// Augie rewrites each `custom_id` to a generated wire id and stores the
    /// plugin's address against it, so a button on a refreshed message is
    /// routable at all. Augie resolves that address from the guild's own
    /// config rather than taking it from the request — a plugin naming its
    /// own address would be a plugin choosing where interactions get sent.
    pub guild_id: String,
    pub service: String,
    /// The layout to render. `ephemeral` is ignored — a message's ephemerality
    /// is fixed when it is created and cannot be edited.
    pub response: CommandResponse,
}

/// Path a service posts a [`NotifyUser`] to. Augie's side, like
/// [`REFRESH_PATH`].
pub const NOTIFY_USER_PATH: &str = "/notify-user";

/// Ask Augie to send one Discord user a direct message.
///
/// # Why Augie sends it
///
/// A DM needs the bot token, and the bot token stays in Augie — the same reason
/// [`RefreshMessage`] exists. The caller says *who* and *what*; Augie owns the
/// credential and the rendering.
///
/// # No components
///
/// Augie routes a button click through the guild's plugin config, and a click
/// in a DM has no guild. So a DM is content only: a body carrying action rows,
/// at the top level or nested in a layout, is refused rather than sent with
/// buttons that could only ever report "this interaction failed".
///
/// # Authority
///
/// Internal-key gated, like the refresh. Anyone holding the key can DM any
/// user the bot can reach, so this is for trusted services telling a user
/// about *their own* account — never a broadcast channel.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotifyUser {
    /// The recipient's snowflake.
    pub user_id: String,
    /// What to send. `content` and `embeds`, or a V2 `layout` without rows.
    pub body: discord_message::MessageBody,
}

impl NotifyUser {
    /// Does the body carry anything clickable? Augie refuses those; see the
    /// type's docs for why.
    pub fn has_components(&self) -> bool {
        fn any_row(blocks: &[discord_message::PluginBlock]) -> bool {
            blocks.iter().any(|block| match block {
                discord_message::PluginBlock::Row(_) => true,
                discord_message::PluginBlock::Container { blocks, .. } => any_row(blocks),
                _ => false,
            })
        }
        !self.body.rows.is_empty() || any_row(&self.body.layout)
    }
}

/// How a [`NotifyUser`] went, when it went as expected.
///
/// Both arms are a `200`. A user who does not accept DMs is an outcome the
/// caller should expect and log quietly, not an error for it to retry —
/// Discord will refuse again for exactly the same reason. Real failures (bad
/// key, malformed body, Discord down) are non-2xx statuses instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyOutcome {
    Delivered,
    /// DMs from server members are off, or the user shares no server with
    /// the bot.
    Unreachable,
}

#[cfg(test)]
mod notify_tests {
    use super::*;
    use discord_message::{MessageBody, PluginActionRow, PluginBlock};

    fn notify(body: MessageBody) -> NotifyUser {
        NotifyUser {
            user_id: "42".to_string(),
            body,
        }
    }

    #[test]
    fn plain_content_has_no_components() {
        assert!(!notify(MessageBody::text("linked")).has_components());
    }

    /// A row anywhere — top level, or nested in a container — is a component.
    #[test]
    fn a_row_anywhere_is_a_component() {
        let top = MessageBody {
            rows: vec![PluginActionRow::new(vec![])],
            ..Default::default()
        };
        assert!(notify(top).has_components());

        let nested = MessageBody {
            layout: vec![PluginBlock::Container {
                accent_color: None,
                blocks: vec![PluginBlock::Row(PluginActionRow::new(vec![]))],
            }],
            ..Default::default()
        };
        assert!(notify(nested).has_components());
    }

    /// The outcome is a stable lower-case word on the wire, since both repos
    /// read it.
    #[test]
    fn outcomes_serialise_as_words() {
        assert_eq!(
            serde_json::to_string(&NotifyOutcome::Unreachable).unwrap(),
            r#""unreachable""#
        );
    }
}

// (`MessageTarget` and its tests moved to `discord-message`. It is how *any*
// sender addresses an existing message, not something about plugins — and the
// outbound client needs the same two routes for its `edit` verb.)
