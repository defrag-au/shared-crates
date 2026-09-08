//! A Discord message, as data.
//!
//! [`MessageBody`] is everything Discord will render — text, embeds, buttons,
//! selects, Components V2 layout, attachments — with no transport and no
//! delivery semantics. Envelopes add those: `augie_plugin::CommandResponse`
//! adds ephemerality and activity launches, `discord_outbound::Target` adds
//! where to send it.
//!
//! # Why this is its own crate, and why it holds no twilight types
//!
//! It would be natural to put `twilight_model`'s message types on this wire and
//! be done. That is not possible: **augminted-bots is on twilight 0.17 and
//! cnft.dev-workers is on twilight 0.16.** A shared crate exposing twilight
//! types could not be consumed by both without forcing one repo through a
//! twilight migration, and mixing majors produces the duplicate-crate type
//! mismatches this workspace has been bitten by before.
//!
//! So the model is serde-only, and [`mod@wire`] renders it to Discord's message
//! JSON directly. No library send support is needed — Components V2 is JSON
//! plus the `IS_COMPONENTS_V2` flag — which is what lets **one** renderer serve
//! both hosts. Each host is still free to parse that JSON back into whatever
//! twilight version it likes at its own edge.
//!
//! These types began life inside `augie-plugin`, which is where the vocabulary
//! was first needed. They moved here when the outbound client needed them too:
//! leaving them put would have made the Discord client depend on the *plugin
//! protocol*, and would drag `render-protocol` and `tool-schema` into every
//! worker that only wants to post a message. See
//! `augminted-bots/docs/DISCORD_OUTBOUND_CONSOLIDATION_DESIGN.md`.
//!
//! ## The `Plugin*` names
//!
//! [`PluginEmbed`], [`PluginBlock`] and friends keep their original names
//! despite no longer being plugin-specific. Dropping the prefix would collide
//! with `twilight_model`'s `Embed`, `Component` and `Button` at exactly the
//! call sites that hold both — which is the confusion the move was meant to
//! reduce, not add to.
//!
//! # Snowflakes are strings
//!
//! Every Discord ID here is a `String`, never `u64`. Snowflakes exceed
//! `Number.MAX_SAFE_INTEGER`, and both sides of this wire run in WASM where a
//! `u64` silently loses precision through JS. Parse at the edge if you need an
//! integer.

mod components;
pub mod wire;

pub use components::*;

use serde::{Deserialize, Serialize};

/// Everything Discord will render. No transport, no delivery semantics.
///
/// The two vocabularies here are mutually exclusive: `layout` is Components V2
/// and Discord rejects a message that also carries `content` or `embeds`. See
/// [`PluginBlock`] — a sender drops the classic fields rather than sending
/// something that will 400.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embeds: Vec<PluginEmbed>,

    /// Action rows. Discord permits at most 5, each holding at most 5
    /// components; a sender validates before posting rather than letting
    /// Discord reject the whole message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<PluginActionRow>,

    /// Components V2 layout. See [`PluginBlock`] — mutually exclusive with
    /// `content` and `embeds`, which a sender drops when this is set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout: Vec<PluginBlock>,

    /// Files to upload with the message.
    ///
    /// Their presence is what decides multipart vs plain JSON — a caller never
    /// picks a transport, it just says whether there are bytes. Reference one
    /// from an embed or a gallery as `attachment://{filename}`.
    ///
    /// **Not carried on the plugin wire.** The bytes are `#[serde(skip)]`, so a
    /// body that crosses a service boundary arrives with this empty. That is
    /// deliberate: a plugin cannot attach bytes, and the field exists for the
    /// host and for anything posting directly. A plugin that wants a graphic
    /// describes it and lets the host render it — see
    /// `augie_plugin::CommandResponse::render`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
}

impl MessageBody {
    /// Plain text.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            ..Default::default()
        }
    }

    /// A Components V2 layout.
    pub fn layout(blocks: Vec<PluginBlock>) -> Self {
        Self {
            layout: blocks,
            ..Default::default()
        }
    }

    /// Nothing for Discord to render.
    ///
    /// Not the same as "nothing to do" — an envelope can be an empty body plus
    /// an activity launch, which is a real answer with no message.
    pub fn is_empty(&self) -> bool {
        self.content.is_none()
            && self.embeds.is_empty()
            && self.rows.is_empty()
            && self.layout.is_empty()
            && self.attachments.is_empty()
    }

    /// Is this a Components V2 message?
    ///
    /// Decided by `layout` being non-empty, and it decides in turn that
    /// `content` and `embeds` are dropped and the `IS_COMPONENTS_V2` flag is
    /// set. **The flag cannot be removed from a message once Discord has set
    /// it**, so a message posted as V2 must be edited as V2 forever.
    pub fn is_v2(&self) -> bool {
        !self.layout.is_empty()
    }

    /// Attach a file.
    pub fn with_attachment(mut self, attachment: Attachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Every `custom_id` in this body, in render order.
    ///
    /// A host uses this to register component routing before sending, so it can
    /// map a later click back to whoever drew the button.
    ///
    /// Classic `rows` only — a V2 `layout` nests rows inside containers, and
    /// walking that tree needs mutable access to rewrite the ids anyway, so the
    /// host does it in one pass rather than reading here and writing there.
    pub fn custom_ids(&self) -> Vec<&str> {
        self.rows
            .iter()
            .flat_map(|row| row.components.iter())
            .filter_map(PluginComponent::custom_id)
            .collect()
    }
}

/// A file uploaded alongside a message.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    /// Filename as Discord will show it, and the name `attachment://` resolves.
    pub filename: String,

    /// The bytes.
    ///
    /// `#[serde(skip)]` because this type rides on JSON wires where megabytes
    /// of base64 would be absurd — the bytes belong in a multipart part, which
    /// is exactly where the sender puts them. A body that has crossed a service
    /// boundary therefore has empty attachment data, and re-serialising one
    /// silently loses the file rather than corrupting it.
    #[serde(skip)]
    pub data: Vec<u8>,

    /// Alt text. Worth setting for the same reason a gallery item's is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Attachment {
    pub fn new(filename: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            filename: filename.into(),
            data,
            description: None,
        }
    }

    pub fn described(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// How this file is addressed from an embed, a gallery or a thumbnail.
    pub fn uri(&self) -> String {
        format!("attachment://{}", self.filename)
    }

    /// Discord's `Content-Type` for the file, by extension.
    ///
    /// Deliberately not a validation: Discord accepts far more than images, and
    /// a wrong guess here costs a preview, where a rejected upload costs the
    /// message.
    pub fn content_type(&self) -> &'static str {
        let lower = self.filename.to_ascii_lowercase();
        if lower.ends_with(".png") {
            "image/png"
        } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            "image/jpeg"
        } else if lower.ends_with(".gif") {
            "image/gif"
        } else if lower.ends_with(".webp") {
            "image/webp"
        } else if lower.ends_with(".mp4") {
            "video/mp4"
        } else if lower.ends_with(".webm") {
            "video/webm"
        } else if lower.ends_with(".json") {
            "application/json"
        } else if lower.ends_with(".txt") {
            "text/plain"
        } else {
            "application/octet-stream"
        }
    }
}

/// How to address a message that already exists, for an edit.
///
/// The two routes are not interchangeable, and picking the wrong one fails in a
/// way that reads as a missing message rather than a wrong endpoint — so the
/// choice is made explicit here rather than inferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Directly, with the bot token.
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
                "{BASE_URL}/webhooks/{application_id}/{interaction_token}/messages/{message_id}"
            ),
            Self::Channel {
                channel_id,
                message_id,
            } => format!("{BASE_URL}/channels/{channel_id}/messages/{message_id}"),
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

/// Discord's versioned API root. Pinned here so the whole outbound path agrees
/// on one version rather than each call site spelling it out.
pub const BASE_URL: &str = "https://discord.com/api/v10";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_v2_body_is_decided_by_its_layout() {
        assert!(!MessageBody::text("hi").is_v2());
        assert!(MessageBody::layout(vec![PluginBlock::Text {
            content: "hi".into()
        }])
        .is_v2());
    }

    /// A body carrying only a file is not empty — that is a real message, and
    /// treating it as nothing would silently drop the upload.
    #[test]
    fn an_attachment_alone_is_not_an_empty_body() {
        let body = MessageBody::default().with_attachment(Attachment::new("render.png", vec![1]));
        assert!(!body.is_empty());
        assert!(MessageBody::default().is_empty());
    }

    /// The `attachment://` form is how a gallery, thumbnail or embed points at
    /// a file on the same message. Building it by hand is how a filename and
    /// its reference drift apart.
    #[test]
    fn an_attachment_names_its_own_uri() {
        let attachment = Attachment::new("render.png", vec![]);
        assert_eq!(attachment.uri(), "attachment://render.png");
        assert_eq!(
            GalleryItem::attachment("render.png", "x").url,
            attachment.uri()
        );
    }

    #[test]
    fn content_type_is_by_extension_and_case_insensitive() {
        assert_eq!(Attachment::new("a.PNG", vec![]).content_type(), "image/png");
        assert_eq!(Attachment::new("a.mp4", vec![]).content_type(), "video/mp4");
        assert_eq!(
            Attachment::new("a.bin", vec![]).content_type(),
            "application/octet-stream"
        );
    }

    /// Bytes must not ride a JSON wire. A body relayed between services arrives
    /// with the file gone rather than with megabytes of base64 in it.
    #[test]
    fn attachment_bytes_never_reach_the_wire() {
        let body = MessageBody::default()
            .with_attachment(Attachment::new("render.png", vec![0xDE, 0xAD, 0xBE, 0xEF]));
        let json = serde_json::to_string(&body).unwrap();

        assert!(json.contains("render.png"), "{json}");
        assert!(!json.contains("222"), "no byte array on the wire: {json}");

        let back: MessageBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back.attachments.len(), 1);
        assert!(
            back.attachments[0].data.is_empty(),
            "the bytes do not survive, and must not appear to"
        );
    }

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
