//! Where a message goes, and what comes back.

use serde::{Deserialize, Serialize};

use discord_message::BASE_URL;

/// Where to post a **new** message.
///
/// Distinct from [`discord_message::MessageTarget`], which addresses a message
/// that already exists: a send has no message id and an edit always does, so
/// collapsing them would mean an `Option<String>` that is required on one path
/// and meaningless on the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// A channel, with the bot token.
    Channel {
        channel_id: String,
        /// Post as a reply to this message. A reply to a message that has since
        /// been deleted still posts — see `wire::MessagePayload::replying_to`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to: Option<String>,
    },
    /// A followup on an interaction, through its webhook.
    ///
    /// Needs no bot token — the interaction token in the path *is* the
    /// credential. Bounded by its 15-minute lifetime.
    Followup {
        application_id: String,
        interaction_token: String,
    },
}

impl Target {
    /// A plain channel post.
    pub fn channel(channel_id: impl Into<String>) -> Self {
        Self::Channel {
            channel_id: channel_id.into(),
            reply_to: None,
        }
    }

    /// A reply to a specific message in a channel.
    pub fn reply(channel_id: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self::Channel {
            channel_id: channel_id.into(),
            reply_to: Some(message_id.into()),
        }
    }

    /// A followup on an interaction.
    pub fn followup(
        application_id: impl Into<String>,
        interaction_token: impl Into<String>,
    ) -> Self {
        Self::Followup {
            application_id: application_id.into(),
            interaction_token: interaction_token.into(),
        }
    }

    /// The Discord endpoint that creates a message here.
    pub fn send_url(&self) -> String {
        match self {
            Self::Channel { channel_id, .. } => {
                format!("{BASE_URL}/channels/{channel_id}/messages")
            }
            Self::Followup {
                application_id,
                interaction_token,
            } => format!("{BASE_URL}/webhooks/{application_id}/{interaction_token}"),
        }
    }

    /// Does this route need the bot token in an `Authorization` header?
    ///
    /// The followup route does not — the token in the path is the auth, and
    /// sending a bot token alongside it is at best redundant.
    pub fn needs_bot_token(&self) -> bool {
        matches!(self, Self::Channel { .. })
    }

    /// The message this post replies to, if any. Only a channel post can.
    pub fn reply_to(&self) -> Option<&str> {
        match self {
            Self::Channel { reply_to, .. } => reply_to.as_deref(),
            Self::Followup { .. } => None,
        }
    }
}

/// What Discord says it created.
///
/// Deliberately not `twilight_model::channel::Message`. Two reasons, and the
/// second is the load-bearing one:
///
/// 1. **Nothing needs the rest.** Across both repos, `.id` is the only field
///    ever read off a sent or edited message.
/// 2. **A twilight type here would pin a twilight major into this crate**, and
///    augminted-bots is on 0.17 while cnft.dev-workers is on 0.16 — so the one
///    client meant to serve both could not.
///
/// If a caller ever genuinely needs the full message, it should fetch it, not
/// have every send pay to parse one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentMessage {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_route_hits_its_own_endpoint() {
        assert_eq!(
            Target::channel("123").send_url(),
            "https://discord.com/api/v10/channels/123/messages"
        );
        // A followup POSTs to the webhook root — no `/messages` suffix, which
        // is the edit route's shape and 404s here.
        assert_eq!(
            Target::followup("app", "tok").send_url(),
            "https://discord.com/api/v10/webhooks/app/tok"
        );
    }

    #[test]
    fn only_the_channel_route_needs_the_bot_token() {
        assert!(Target::channel("123").needs_bot_token());
        assert!(!Target::followup("app", "tok").needs_bot_token());
    }

    /// Discord has no "reply" on the followup route — a followup already
    /// belongs to its interaction. Letting one be set would silently drop it.
    #[test]
    fn only_a_channel_post_can_be_a_reply() {
        assert_eq!(Target::reply("123", "456").reply_to(), Some("456"));
        assert_eq!(Target::channel("123").reply_to(), None);
        assert_eq!(Target::followup("app", "tok").reply_to(), None);
    }

    /// Discord ids exceed `Number.MAX_SAFE_INTEGER`, and this crate runs in
    /// WASM on both sides. A `u64` here would silently lose precision through
    /// JS, which reads as a message that cannot be found rather than as a
    /// parse error.
    #[test]
    fn a_returned_id_survives_beyond_max_safe_integer() {
        let json = r#"{"id":"1234567890123456789","channel_id":"987654321098765432"}"#;
        let sent: SentMessage = serde_json::from_str(json).unwrap();
        assert_eq!(sent.id, "1234567890123456789");
    }

    /// Discord's message object carries dozens of fields this type ignores.
    /// Parsing must not fail because of them.
    #[test]
    fn the_rest_of_discords_message_object_is_ignored() {
        let json = r#"{"id":"1","channel_id":"2","content":"hi","tts":false,
                       "author":{"id":"3"},"embeds":[],"attachments":[]}"#;
        let sent: SentMessage = serde_json::from_str(json).unwrap();
        assert_eq!(sent.id, "1");
        assert_eq!(sent.channel_id.as_deref(), Some("2"));
    }
}
