//! What a plugin returns: [`CommandResponse`] and its rendering vocabulary.
//!
//! Deliberately a small subset of Discord's message model. It carries what a
//! service front-end actually needs — text, embeds, buttons, selects — and
//! nothing else. Each host converts to its own twilight version at the edge;
//! see the crate docs for why twilight types can't live on this wire.

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
