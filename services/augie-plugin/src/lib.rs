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
//! So the protocol defines its own minimal vocabulary ([`PluginEmbed`],
//! [`PluginComponent`], …) and each host converts at its own edge. The crate
//! depends on `serde` and `serde_json` and nothing else — the latter only
//! because a [`PluginTool`]'s schema and a [`ToolInvocation`]'s arguments are
//! genuinely free-form JSON — which keeps it trivially WASM-safe.
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
/// The plugin cannot render it alone: this crate is deliberately twilight-free
/// so both repos can pin it as a pure wire format, so nothing here knows how a
/// [`PluginBlock`] becomes a Discord component. Augie does. So the plugin sends
/// the layout it wants and the credentials proving it owns that message, and
/// Augie renders it with the same converter the interaction path uses.
///
/// # Addressed through the interaction webhook, not the channel
///
/// Two routes look plausible and only one works:
///
/// - `PATCH /channels/{channel}/messages/{message}` with the bot token —
///   **fails with `10008 Unknown Message`** whenever the message is
///   ephemeral. An ephemeral message is not a real channel message; it exists
///   only inside the interaction that produced it, so the channel route cannot
///   see it however much authority the bot has.
/// - `PATCH /webhooks/{application_id}/{interaction_token}/messages/{message_id}`
///   — works for both ephemeral and normal messages.
///
/// The message id is given explicitly rather than using `@original`, because a
/// component answered with `LAUNCH_ACTIVITY` neither creates nor updates a
/// message and so has no "original response" for `@original` to name.
///
/// **The 15-minute token lifetime is therefore unavoidable**, not a design
/// choice: an ephemeral message is only ever reachable through its
/// interaction, and Discord expires that. A refresh attempted later simply
/// fails and the message stays as it was.
///
/// # Authority
///
/// The interaction token *is* the authority — Discord issued it to whoever it
/// handed the interaction. Augie does not re-check ownership because it keeps
/// no record of which plugin owns which message; the endpoint is internal-key
/// gated, so the caller is a trusted service by construction.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RefreshMessage {
    pub target: MessageTarget,
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

/// How to address a message Augie should edit.
///
/// The two routes are not interchangeable, and picking the wrong one fails in
/// a way that reads as a missing message rather than a wrong endpoint — so the
/// choice is made explicit here rather than inferred.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageTarget {
    /// Through the interaction that produced the message.
    ///
    /// **Required for an ephemeral message**, which is not a real channel
    /// message and cannot be reached any other way. Also works for a
    /// non-ephemeral response or followup.
    ///
    /// Bounded by the interaction token's **15-minute** lifetime. For an
    /// ephemeral message that ceiling is unavoidable; for anything else,
    /// prefer [`Self::Channel`], which has none.
    ///
    /// `message_id` is explicit rather than `@original` because an interaction
    /// answered with `LAUNCH_ACTIVITY` creates no response for `@original` to
    /// name.
    Interaction {
        application_id: String,
        interaction_token: String,
        message_id: String,
    },
    /// Directly, with Augie's bot token.
    ///
    /// No expiry, so this is the right choice for anything edited long after
    /// the fact — a summary updated when a job finishes, a post revised by a
    /// cron. It is also the *only* route for a message with no interaction
    /// behind it.
    ///
    /// **Not valid for an ephemeral message**: Discord answers `10008 Unknown
    /// Message`, because from the channel's point of view it does not exist.
    Channel {
        channel_id: String,
        message_id: String,
    },
}

impl MessageTarget {
    /// The Discord endpoint that edits this message.
    pub fn edit_url(&self) -> String {
        match self {
            Self::Interaction {
                application_id,
                interaction_token,
                message_id,
            } => format!(
                "https://discord.com/api/v10/webhooks/{application_id}/{interaction_token}/messages/{message_id}"
            ),
            Self::Channel {
                channel_id,
                message_id,
            } => format!("https://discord.com/api/v10/channels/{channel_id}/messages/{message_id}"),
        }
    }

    /// Does this route need the bot token in an `Authorization` header?
    ///
    /// The interaction route does not — the token in the path *is* the auth,
    /// and sending a bot token alongside it is at best redundant.
    pub fn needs_bot_token(&self) -> bool {
        matches!(self, Self::Channel { .. })
    }

    /// Every field populated?
    pub fn is_complete(&self) -> bool {
        match self {
            Self::Interaction {
                application_id,
                interaction_token,
                message_id,
            } => {
                !application_id.is_empty()
                    && !interaction_token.is_empty()
                    && !message_id.is_empty()
            }
            Self::Channel {
                channel_id,
                message_id,
            } => !channel_id.is_empty() && !message_id.is_empty(),
        }
    }
}

#[cfg(test)]
mod message_target_tests {
    use super::*;

    fn interaction() -> MessageTarget {
        MessageTarget::Interaction {
            application_id: "app".into(),
            interaction_token: "tok".into(),
            message_id: "msg".into(),
        }
    }

    fn channel() -> MessageTarget {
        MessageTarget::Channel {
            channel_id: "chan".into(),
            message_id: "msg".into(),
        }
    }

    #[test]
    fn each_route_hits_its_own_endpoint() {
        // The interaction route reaches ephemeral messages; the channel route
        // is the only one that works without an interaction, and the only one
        // with no expiry. Swapping them yields `10008 Unknown Message`, which
        // reads as a missing message rather than a wrong URL — hence the test.
        assert_eq!(
            interaction().edit_url(),
            "https://discord.com/api/v10/webhooks/app/tok/messages/msg"
        );
        assert_eq!(
            channel().edit_url(),
            "https://discord.com/api/v10/channels/chan/messages/msg"
        );
    }

    #[test]
    fn only_the_channel_route_needs_the_bot_token() {
        // The interaction token in the path is itself the credential.
        assert!(!interaction().needs_bot_token());
        assert!(channel().needs_bot_token());
    }

    #[test]
    fn an_incomplete_target_is_rejected_before_it_reaches_discord() {
        // An empty segment silently produces a URL like `.../messages/` that
        // 404s indistinguishably from a genuinely missing message.
        assert!(interaction().is_complete());
        assert!(channel().is_complete());

        assert!(!MessageTarget::Interaction {
            application_id: "app".into(),
            interaction_token: String::new(),
            message_id: "msg".into(),
        }
        .is_complete());
        assert!(!MessageTarget::Channel {
            channel_id: "chan".into(),
            message_id: String::new(),
        }
        .is_complete());
    }

    #[test]
    fn the_variant_survives_a_round_trip() {
        // This crosses a service boundary *and* is persisted in run state, so
        // an untagged or renamed variant would deserialise as the wrong route.
        for target in [interaction(), channel()] {
            let json = serde_json::to_string(&target).expect("serialises");
            let back: MessageTarget = serde_json::from_str(&json).expect("round-trips");
            assert_eq!(back.edit_url(), target.edit_url());
            assert_eq!(back.needs_bot_token(), target.needs_bot_token());
        }
    }
}
