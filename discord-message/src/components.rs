//! The rendering vocabulary: embeds, buttons, selects, Components V2 blocks.
//!
//! Deliberately a small subset of Discord's message model. It carries what a
//! service front-end actually needs and nothing else. [`mod@crate::wire`] turns
//! it into Discord's JSON; see the crate docs for why no twilight types live
//! here.

use serde::{Deserialize, Serialize};

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
        /// The drawer's own id. Opaque to the host, and handed back verbatim on
        /// the resulting interaction.
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

/// Discord's cap on the total number of components in one message.
///
/// **Every nested component counts**, which is easy to underestimate: a
/// [`PluginBlock::Section`] is itself one, plus one per line of text, plus its
/// thumbnail — so a ten-row list with three lines each is over sixty and
/// Discord rejects the whole message with
/// `COMPONENT_MAX_TOTAL_COMPONENTS_EXCEEDED`.
pub const MAX_V2_COMPONENTS: usize = 40;

/// How many components a layout will cost, counted the way Discord counts.
pub fn count_components(blocks: &[PluginBlock]) -> usize {
    blocks.iter().map(count_block).sum()
}

fn count_block(block: &PluginBlock) -> usize {
    match block {
        PluginBlock::Container { blocks, .. } => 1 + count_components(blocks),
        // The section, each line of text, and the thumbnail.
        PluginBlock::Section { text, .. } => 1 + text.len() + 1,
        PluginBlock::Row(row) => 1 + row.components.len(),
        PluginBlock::Text { .. } | PluginBlock::Gallery { .. } | PluginBlock::Separator { .. } => 1,
    }
}

/// Drop trailing content until the layout fits, returning how many blocks went.
///
/// Trailing rather than proportional: these lists are ordered (cheapest first,
/// best discount first), so the tail is the least interesting part. A caller
/// that truncates should say so — a shortened list that doesn't admit it reads
/// as a complete one.
pub fn truncate_to_fit(blocks: &mut Vec<PluginBlock>, budget: usize) -> usize {
    let mut dropped = 0;
    while count_components(blocks) > budget {
        // Prefer trimming inside the last container, since that is where a
        // list lives; fall back to dropping top-level blocks. The count is
        // taken before the borrow so the two don't overlap.
        let top_level = blocks.len();
        let trimmed_inner = match blocks.last_mut() {
            Some(PluginBlock::Container { blocks: inner, .. }) if inner.len() > 1 => {
                inner.pop();
                true
            }
            _ => false,
        };

        if !trimmed_inner {
            if top_level > 1 {
                blocks.pop();
            } else {
                break;
            }
        }
        dropped += 1;
    }
    dropped
}

/// Components V2 layout.
///
/// # Mutually exclusive with `content` and `embeds`
///
/// Discord refuses a message that carries both, and the `IS_COMPONENTS_V2` flag
/// **cannot be removed once set** on a message. A sender therefore picks one
/// vocabulary per message: set `layout` for V2, or `content`/`embeds` for the
/// classic shape. A body carrying both is sent as V2, with `content`/`embeds`
/// dropped, rather than as a message Discord will reject.
///
/// The trade is real: V2 gives grouped containers, accent colours and inline
/// galleries, but loses embeds and cannot be mixed. It is worth it when the
/// message is a composed card; it is not worth it for a line of text.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The live failure: ten three-line sections is over sixty components and
    /// Discord rejects the entire message, so the reply is lost rather than
    /// shortened.
    #[test]
    fn a_section_costs_more_than_it_looks() {
        let section = PluginBlock::Section {
            text: vec!["a".into(), "b".into(), "c".into()],
            thumbnail: GalleryItem::attachment("x.png", "x"),
        };
        // itself + three lines + the thumbnail
        assert_eq!(count_components(std::slice::from_ref(&section)), 5);

        let ten = PluginBlock::Container {
            accent_color: None,
            blocks: (0..10).map(|_| section.clone()).collect(),
        };
        assert!(
            count_components(std::slice::from_ref(&ten)) > MAX_V2_COMPONENTS,
            "ten rows must exceed the cap — that is the bug this guards"
        );
    }

    #[test]
    fn truncation_trims_the_tail_until_it_fits() {
        let section = PluginBlock::Section {
            text: vec!["a".into(), "b".into()],
            thumbnail: GalleryItem::attachment("x.png", "x"),
        };
        let mut layout = vec![PluginBlock::Container {
            accent_color: None,
            blocks: (0..12).map(|_| section.clone()).collect(),
        }];

        let dropped = truncate_to_fit(&mut layout, MAX_V2_COMPONENTS);
        assert!(dropped > 0, "an over-budget layout must lose something");
        assert!(count_components(&layout) <= MAX_V2_COMPONENTS);

        // And it kept the head, not an arbitrary slice.
        let PluginBlock::Container { blocks, .. } = &layout[0] else {
            panic!("container survives");
        };
        assert!(!blocks.is_empty());
    }

    #[test]
    fn a_layout_within_budget_is_left_alone() {
        let mut layout = vec![PluginBlock::Container {
            accent_color: None,
            blocks: vec![PluginBlock::Text {
                content: "hi".into(),
            }],
        }];
        assert_eq!(truncate_to_fit(&mut layout, MAX_V2_COMPONENTS), 0);
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

    #[test]
    fn custom_ids_skips_link_buttons() {
        let row = PluginActionRow::new(vec![
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
        ]);

        let ids: Vec<&str> = row
            .components
            .iter()
            .filter_map(PluginComponent::custom_id)
            .collect();
        assert_eq!(ids, vec!["comp:refresh:01J"]);
    }
}
