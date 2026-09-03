//! What a plugin returns: [`CommandResponse`], the *interaction* envelope.
//!
//! The presentation it carries — text, embeds, buttons, selects, Components V2
//! layout — is [`discord_message::MessageBody`], which lives in its own crate
//! because the outbound client needs the same vocabulary. What is left here is
//! only what an interaction adds: ephemerality, activity launches, host-side
//! rendering, handoffs. See
//! `augminted-bots/docs/DISCORD_OUTBOUND_CONSOLIDATION_DESIGN.md`.

use discord_message::MessageBody;
use render_protocol::RenderRequest;
use serde::{Deserialize, Serialize};

/// A plugin's reply to an invocation.
///
/// The presentation lives in [`MessageBody`], flattened onto this struct's JSON
/// so the wire shape is unchanged by the split. Everything else here is
/// *delivery* — meaningful only because an interaction is what is being
/// answered.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CommandResponse {
    /// What Discord renders. Flattened: `content`, `embeds`, `rows` and
    /// `layout` remain top-level keys on the wire.
    #[serde(flatten)]
    pub body: MessageBody,

    /// Visible only to the invoking user. Default `false`, so a plugin has to
    /// opt in to ephemeral — the failure mode of an accidentally-public
    /// message is milder than an accidentally-hidden one.
    #[serde(default)]
    pub ephemeral: bool,

    /// A graphic to render and attach to this reply.
    ///
    /// **The host renders it, not the plugin.** This is not a stylistic choice:
    /// an embed can only reference an image by URL, and `MessageBody`'s own
    /// `attachments` do not cross this wire (the bytes are `serde(skip)`) — so
    /// a plugin wanting a graphic would otherwise have to host it at a public,
    /// guessable URL. Describing the graphic as markup and letting the host
    /// render and attach it keeps the image private to the message (ephemeral
    /// replies included) and keeps the rasteriser, fonts and image pipeline in
    /// one service instead of every plugin.
    ///
    /// The attachment is named `render.png`; reference it from an embed as
    /// `attachment://render.png` if you want it inside an embed rather than
    /// beneath the message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderRequest>,

    /// Replace the message the component was attached to, rather than posting
    /// a new one. Only meaningful on a [`crate::ComponentInvocation`] reply —
    /// ignored on a command reply, where there is no prior message.
    ///
    /// Pagination wants this: without it every page click leaves another copy
    /// of the embed in the channel.
    #[serde(default)]
    pub update_message: bool,

    /// Launch the app's Activity instead of posting a message.
    ///
    /// The host answers with interaction callback type 12 (`LAUNCH_ACTIVITY`).
    /// Discord then opens the Activity for the invoking user, and — this is
    /// the part worth knowing — the callback carries **no payload**: nothing
    /// in this response reaches the Activity. The Activity learns what it was
    /// opened for by asking the server after it authenticates, keyed on the
    /// user and guild Discord hands it. So a plugin setting this should have
    /// already recorded whatever the Activity needs to find.
    ///
    /// Every other field is ignored when this is set; a launch has no message.
    /// Only meaningful for apps with Activities enabled, and only from an
    /// interaction — never from a followup.
    #[serde(default)]
    pub launch_activity: bool,

    /// Hand the caller off to a surface only the host can open.
    ///
    /// See [`WellKnownCommand`]. Set this when the plugin's answer is "you
    /// can't do this yet, and the next step isn't mine to render".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<WellKnownCommand>,
}

/// A surface every deployment has, that the host opens on a plugin's behalf.
///
/// # Why a closed set and not a URL
///
/// A handoff carries a *minted identity token* — the host signs a claim about
/// who the caller is and puts it in a link. If a plugin could name an
/// arbitrary target, any plugin could point a user, carrying that token, at
/// any surface it liked. The set is closed so the host stays the authority on
/// what identity gets minted for.
///
/// # Why these are "well known"
///
/// Some surfaces are platform-wide: every guild that plays anything needs
/// wallet linking. Making each one declare `[command.link-wallet]` is config
/// burden with no decision in it — the answer is the same everywhere until
/// someone deliberately differs.
///
/// So the host resolves in two steps:
///
/// 1. The guild's own `[command.{name}]`, if it declares one. Existing ad-hoc
///    config keeps working untouched, and a guild pointing players at a
///    bespoke funnel still gets that.
/// 2. Otherwise the built-in default below, with the destination read from a
///    host environment variable — so dev links against dev.
///
/// A plugin never sees which applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WellKnownCommand {
    /// The caller has no linked wallet and needs one.
    LinkWallet,
}

impl WellKnownCommand {
    /// The guild command that overrides this default, if declared.
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::LinkWallet => "link-wallet",
        }
    }

    /// The widget action the target expects.
    pub fn default_action(&self) -> &'static str {
        match self {
            Self::LinkWallet => "link_wallet",
        }
    }

    /// Host environment variable naming the destination.
    ///
    /// Not a compiled-in URL: a default that is right in production and wrong
    /// in dev would send test users at the live linker, which is worse than
    /// having no default at all.
    pub fn target_var(&self) -> &'static str {
        match self {
            Self::LinkWallet => "WALLET_LINK_URL",
        }
    }

    /// Copy used when neither the guild nor the plugin supplied any.
    pub fn default_message(&self) -> &'static str {
        match self {
            Self::LinkWallet => {
                "🔗 **Link your wallet**\n\nConnect a Cardano wallet to prove ownership. \
                 You'll be asked to sign a message — this costs nothing and never moves funds."
            }
        }
    }
}

impl CommandResponse {
    /// Plain text reply, visible to the channel.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            body: MessageBody::text(content),
            ..Default::default()
        }
    }

    /// Plain text reply, visible only to the caller. The right default for
    /// errors and for anything reporting the caller's own state.
    pub fn ephemeral_text(content: impl Into<String>) -> Self {
        Self {
            body: MessageBody::text(content),
            ephemeral: true,
            ..Default::default()
        }
    }

    /// A Components V2 reply. See [`discord_message::PluginBlock`].
    pub fn layout(blocks: Vec<discord_message::PluginBlock>) -> Self {
        Self {
            body: MessageBody::layout(blocks),
            ..Default::default()
        }
    }

    pub fn with_embed(mut self, embed: discord_message::PluginEmbed) -> Self {
        self.body.embeds.push(embed);
        self
    }

    pub fn with_row(mut self, row: discord_message::PluginActionRow) -> Self {
        self.body.rows.push(row);
        self
    }

    pub fn ephemeral(mut self) -> Self {
        self.ephemeral = true;
        self
    }

    /// Replace the message the clicked component belongs to. See
    /// [`CommandResponse::update_message`].
    pub fn updating(mut self) -> Self {
        self.update_message = true;
        self
    }

    /// Launch the app's Activity. See [`CommandResponse::launch_activity`] —
    /// nothing else in the response is sent, so this is a constructor rather
    /// than a modifier.
    pub fn launch_activity() -> Self {
        Self {
            launch_activity: true,
            ..Default::default()
        }
    }

    /// Hand off to a host-owned surface, with copy explaining why.
    ///
    /// The content becomes the button's message — a plugin knows *why* the
    /// user is here ("you need a wallet to deploy your own crew"), which is
    /// more useful than the generic prompt someone gets for asking to link
    /// one. It is also the whole reply if the host cannot open the surface at
    /// all, so it should stand on its own in words.
    pub fn handoff(handoff: WellKnownCommand, content: impl Into<String>) -> Self {
        Self {
            body: MessageBody::text(content),
            handoff: Some(handoff),
            ephemeral: true,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use discord_message::{
        ButtonStyle, PluginActionRow, PluginBlock, PluginComponent, PluginEmbed,
    };

    /// The command name is the join between a well-known default and a guild's
    /// ad-hoc override — augie looks up `[command.{name}]` by exactly this
    /// string. If it drifted from the TOML, every guild's override would be
    /// silently ignored in favour of the default, which is the failure mode
    /// this whole two-step exists to avoid.
    #[test]
    fn a_well_known_command_names_the_config_key_that_overrides_it() {
        assert_eq!(WellKnownCommand::LinkWallet.command_name(), "link-wallet");
        assert_eq!(WellKnownCommand::LinkWallet.default_action(), "link_wallet");
    }

    /// The destination comes from the host environment, never compiled in — a
    /// default that is right in production and wrong in dev would point test
    /// users at the live linker.
    #[test]
    fn the_destination_is_an_env_var_not_a_url() {
        let var = WellKnownCommand::LinkWallet.target_var();
        assert_eq!(var, "WALLET_LINK_URL");
        assert!(!var.contains("://"), "must name a variable, not a URL");
    }

    /// The plugin's copy is what the user reads, so a handoff without one is a
    /// blank message if the host cannot open the surface.
    #[test]
    fn a_handoff_carries_standalone_copy_and_is_private() {
        let response = CommandResponse::handoff(WellKnownCommand::LinkWallet, "connect a wallet");
        assert_eq!(response.handoff, Some(WellKnownCommand::LinkWallet));
        assert_eq!(response.body.content.as_deref(), Some("connect a wallet"));
        // A linked wallet ties an on-chain address to a Discord identity; the
        // channel is not entitled to watch someone being asked for one.
        assert!(response.ephemeral);
    }

    #[test]
    fn custom_ids_skips_link_buttons() {
        let response = CommandResponse::text("standings").with_row(PluginActionRow::new(vec![
            PluginComponent::Button {
                custom_id: "comp:refresh:01J".to_string(),
                label: "Refresh".to_string(),
                style: ButtonStyle::Secondary,
                emoji: None,
                disabled: false,
            },
            PluginComponent::LinkButton {
                url: "https://aliens.epochify.space".to_string(),
                label: "Globe".to_string(),
                disabled: false,
            },
        ]));

        assert_eq!(response.body.custom_ids(), vec!["comp:refresh:01J"]);
    }

    #[test]
    fn empty_collections_are_omitted_from_the_wire() {
        let json = serde_json::to_string(&CommandResponse::text("hi")).unwrap();
        assert!(!json.contains("embeds"), "{json}");
        assert!(!json.contains("rows"), "{json}");
        // `ephemeral` is a plain bool with a false default, so it does ride
        // along — that's deliberate, it makes the visibility explicit on
        // every response rather than inferred from absence.
        assert!(json.contains("ephemeral"), "{json}");
    }

    /// The body is `#[serde(flatten)]`ed, so extracting it must not have moved
    /// anything on the wire. Every plugin in two repos serialises this shape,
    /// and they update on independent rev bumps — a nested `body` object would
    /// deserialise as an all-default response on the host, which renders as a
    /// blank message rather than an error.
    #[test]
    fn the_body_stays_flat_on_the_wire() {
        let response = CommandResponse {
            body: MessageBody {
                content: Some("hi".into()),
                embeds: vec![PluginEmbed {
                    title: Some("t".into()),
                    ..Default::default()
                }],
                rows: vec![PluginActionRow::new(vec![PluginComponent::LinkButton {
                    url: "https://example.com".into(),
                    label: "Go".into(),
                    disabled: false,
                }])],
                layout: vec![PluginBlock::Text {
                    content: "block".into(),
                }],
                // A plugin cannot attach bytes, so this is always empty here —
                // named rather than defaulted so adding a body field is a
                // compile error in this test rather than a silent gap in it.
                attachments: Vec::new(),
            },
            ephemeral: true,
            ..Default::default()
        };

        let value: serde_json::Value = serde_json::to_value(&response).unwrap();
        assert!(value.get("body").is_none(), "no nested envelope: {value}");
        for key in ["content", "embeds", "rows", "layout", "ephemeral"] {
            assert!(
                value.get(key).is_some(),
                "`{key}` must stay top-level: {value}"
            );
        }

        // And the pre-split JSON still parses, which is the direction that
        // actually breaks: an old plugin talking to a new host.
        let legacy = r#"{"content":"hi","embeds":[],"rows":[],"layout":[],"ephemeral":true}"#;
        let parsed: CommandResponse = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.body.content.as_deref(), Some("hi"));
        assert!(parsed.ephemeral);
    }
}
