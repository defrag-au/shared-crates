//! What a plugin returns: [`CommandResponse`] and its rendering vocabulary.
//!
//! Deliberately a small subset of Discord's message model. It carries what a
//! service front-end actually needs — text, embeds, buttons, selects — and
//! nothing else. Each host converts to its own twilight version at the edge;
//! see the crate docs for why twilight types can't live on this wire.

use render_protocol::RenderRequest;
use serde::{Deserialize, Serialize};

/// A plugin's reply to an invocation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CommandResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embeds: Vec<PluginEmbed>,

    /// Action rows. Discord permits at most 5, each holding at most 5
    /// components; Augie validates before sending rather than letting Discord
    /// reject the whole message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<PluginActionRow>,

    /// Visible only to the invoking user. Default `false`, so a plugin has to
    /// opt in to ephemeral — the failure mode of an accidentally-public
    /// message is milder than an accidentally-hidden one.
    #[serde(default)]
    pub ephemeral: bool,

    /// A graphic to render and attach to this reply.
    ///
    /// **The host renders it, not the plugin.** This is not a stylistic choice:
    /// [`PluginEmbed`] can only reference an image by URL, and a plugin has no
    /// way to attach bytes — so a plugin wanting a graphic would otherwise have
    /// to host it at a public, guessable URL. Describing the graphic as markup
    /// and letting the host render and attach it keeps the image private to the
    /// message (ephemeral replies included) and keeps the rasteriser, fonts and
    /// image pipeline in one service instead of every plugin.
    ///
    /// The attachment is named `render.png`; reference it from an embed as
    /// `attachment://render.png` if you want it inside an embed rather than
    /// beneath the message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderRequest>,

    /// Components V2 layout. See [`PluginBlock`] — mutually exclusive with
    /// `content` and `embeds`, which the host drops when this is set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout: Vec<PluginBlock>,

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
            content: Some(content.into()),
            ..Default::default()
        }
    }

    /// Plain text reply, visible only to the caller. The right default for
    /// errors and for anything reporting the caller's own state.
    pub fn ephemeral_text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            ephemeral: true,
            ..Default::default()
        }
    }

    pub fn with_embed(mut self, embed: PluginEmbed) -> Self {
        self.embeds.push(embed);
        self
    }

    pub fn with_row(mut self, row: PluginActionRow) -> Self {
        self.rows.push(row);
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
            content: Some(content.into()),
            handoff: Some(handoff),
            ephemeral: true,
            ..Default::default()
        }
    }

    /// Every `custom_id` in this response, in row order.
    ///
    /// Augie uses this to register component routing before sending, so it can
    /// map a later click back to the originating plugin.
    pub fn custom_ids(&self) -> Vec<&str> {
        self.rows
            .iter()
            .flat_map(|row| row.components.iter())
            .filter_map(PluginComponent::custom_id)
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginEmbed {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// RGB, e.g. `0x4caf50`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<PluginEmbedField>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,

    /// ISO 8601. A string rather than a timestamp type to keep this crate
    /// dependency-free and to sidestep the u64-in-WASM problem entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEmbedField {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub inline: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginActionRow {
    pub components: Vec<PluginComponent>,
}

impl PluginActionRow {
    pub fn new(components: Vec<PluginComponent>) -> Self {
        Self { components }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginComponent {
    Button {
        /// The plugin's own id. Opaque to Augie, and handed back verbatim on
        /// the resulting [`crate::ComponentInvocation`].
        custom_id: String,
        label: String,
        #[serde(default)]
        style: ButtonStyle,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        #[serde(default)]
        disabled: bool,
    },
    /// A button that opens a URL. Has no `custom_id` because it produces no
    /// interaction — Discord handles it entirely client-side.
    LinkButton {
        url: String,
        label: String,
        #[serde(default)]
        disabled: bool,
    },
    Select {
        custom_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        options: Vec<SelectOption>,
        #[serde(default)]
        disabled: bool,
    },
}

impl PluginComponent {
    /// The component's `custom_id`, or `None` for a link button.
    pub fn custom_id(&self) -> Option<&str> {
        match self {
            PluginComponent::Button { custom_id, .. }
            | PluginComponent::Select { custom_id, .. } => Some(custom_id),
            PluginComponent::LinkButton { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonStyle {
    #[default]
    Primary,
    Secondary,
    Success,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub default: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(response.content.as_deref(), Some("connect a wallet"));
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

        assert_eq!(response.custom_ids(), vec!["comp:refresh:01J"]);
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

    #[test]
    fn component_tag_is_stable_on_the_wire() {
        let button = PluginComponent::Button {
            custom_id: "x".to_string(),
            label: "Go".to_string(),
            style: ButtonStyle::Danger,
            emoji: None,
            disabled: false,
        };
        let json = serde_json::to_string(&button).unwrap();
        assert!(json.contains(r#""type":"button""#), "{json}");
        assert!(json.contains(r#""style":"danger""#), "{json}");
    }
}

/// Components V2 layout.
///
/// # Mutually exclusive with `content` and `embeds`
///
/// Discord refuses a message that carries both, and the `IS_COMPONENTS_V2` flag
/// **cannot be removed once set** on a message. A plugin therefore picks one
/// vocabulary per reply: set `layout` for V2, or `content`/`embeds` for the
/// classic shape. The host ignores `content`/`embeds` when `layout` is present
/// rather than sending a message Discord will reject.
///
/// The trade is real: V2 gives grouped containers, accent colours and inline
/// galleries, but loses embeds and cannot be mixed. It is worth it when the
/// reply is a composed card; it is not worth it for a line of text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginBlock {
    /// A visually grouped card with an optional accent bar.
    Container {
        /// RGB accent bar. A tier colour or brand colour reads as deliberate
        /// where the default grey reads as unstyled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accent_color: Option<u32>,
        blocks: Vec<PluginBlock>,
    },

    /// Markdown text. The V2 replacement for `content` — headings (`##`), bold
    /// and links all work.
    Text { content: String },

    /// Images shown inline. `attachment://name` addresses a file on the same
    /// message, which is how a rendered graphic is placed inside a container.
    Gallery { items: Vec<GalleryItem> },

    /// Text with a small image beside it.
    ///
    /// The compact alternative to [`PluginBlock::Gallery`]: Discord sizes a
    /// gallery by how many items it holds, so a one-item gallery renders
    /// full-width no matter how small the source image is. A section's
    /// thumbnail is small by construction, which is what you want when the
    /// image is illustrating a line of text rather than being the point.
    Section {
        /// Lines of markdown shown beside the thumbnail.
        text: Vec<String>,
        thumbnail: GalleryItem,
    },

    /// A horizontal rule between sections.
    Separator {
        #[serde(default = "default_true")]
        divider: bool,
    },

    /// Buttons and selects, exactly as in the classic vocabulary.
    Row(PluginActionRow),
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GalleryItem {
    /// `attachment://name.png` for a file on this message, or an https URL.
    pub url: String,

    /// Alt text. Worth setting — a gallery with no description is unreadable
    /// to anyone using a screen reader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl GalleryItem {
    /// An image attached to this same message.
    pub fn attachment(name: &str, description: impl Into<String>) -> Self {
        Self {
            url: format!("attachment://{name}"),
            description: Some(description.into()),
        }
    }
}
