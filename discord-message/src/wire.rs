//! [`MessageBody`](crate::MessageBody)'s vocabulary as Discord's wire JSON.
//!
//! # Why this is serde-only
//!
//! The obvious way to turn a [`PluginBlock`] into Discord JSON is to build a
//! twilight `Component` and let twilight serialise it. That is what the hosts
//! did, once each, and it is exactly what cannot be shared: **augminted-bots is
//! on twilight 0.17 and cnft.dev-workers is on twilight 0.16**, so a renderer
//! naming a twilight type re-imports that split into the crate meant to end it.
//!
//! [`body`] renders a whole [`MessageBody`](crate::MessageBody) — the entry
//! point a sender wants. The per-piece functions below are public for callers
//! that hold one component rather than a message.
//!
//! No library support is needed. Components V2 is JSON plus the
//! `IS_COMPONENTS_V2` message flag, and Discord's message JSON is stable and
//! versioned by `/v10` in the URL — so the wire format itself is the shared
//! thing, and each host stays free to pick whatever library it likes for the
//! rest of its Discord surface.
//!
//! # Reading this module
//!
//! The `*Json` types are deliberately opaque: their fields are private and the
//! only way to build one is through the functions here. A component's `type`
//! discriminator and its payload have to agree, and there is no reason to let a
//! caller pair a container's `type` with a button's fields.
//!
//! Assert on the rendered JSON rather than on these types — the JSON is the
//! contract, the structs are just how it gets written.

use serde::Serialize;

use crate::components::{
    ButtonStyle, GalleryItem, PluginActionRow, PluginBlock, PluginComponent, PluginEmbed,
    SelectOption,
};

/// `MessageFlags::IS_COMPONENTS_V2`.
///
/// **Cannot be removed from a message once set.** A message posted as V2 must
/// be edited as V2 forever, which is why the flag is set from the very first
/// response rather than when the final content is known.
pub const IS_COMPONENTS_V2: u32 = 1 << 15;

/// `MessageFlags::EPHEMERAL`.
pub const EPHEMERAL: u32 = 1 << 6;

/// Discord's component `type` discriminators.
mod kind {
    pub const ACTION_ROW: u8 = 1;
    pub const BUTTON: u8 = 2;
    pub const TEXT_SELECT: u8 = 3;
    pub const SECTION: u8 = 9;
    pub const TEXT_DISPLAY: u8 = 10;
    pub const THUMBNAIL: u8 = 11;
    pub const MEDIA_GALLERY: u8 = 12;
    pub const SEPARATOR: u8 = 14;
    pub const CONTAINER: u8 = 17;
}

/// `SeparatorSpacingSize::Small`.
const SPACING_SMALL: u8 = 1;

/// Discord's `embed.type` for anything an application sends.
const EMBED_RICH: &str = "rich";

// ── Rendering ───────────────────────────────────────────────────────────────

/// A whole [`MessageBody`](crate::MessageBody) as Discord's message payload.
///
/// This is where the classic/V2 fork is decided, once: a body with a `layout`
/// becomes a V2 message, which means the `IS_COMPONENTS_V2` flag is set and
/// `content`/`embeds` are **dropped**. Discord rejects a message carrying both,
/// and the flag cannot be removed once set — so a body that set both gets the
/// layout it explicitly asked for rather than a 400.
///
/// Attachments are *declared* here (`attachments[N].id` matching the `files[N]`
/// part a multipart sender writes). The bytes are not this function's business.
pub fn body(body: &crate::MessageBody) -> MessagePayload {
    let v2 = body.is_v2();

    MessagePayload {
        content: if v2 { None } else { body.content.clone() },
        embeds: if v2 { Vec::new() } else { embeds(&body.embeds) },
        components: if v2 {
            components(&body.layout)
        } else {
            action_rows(&body.rows)
        },
        attachments: body
            .attachments
            .iter()
            .enumerate()
            .map(|(index, attachment)| AttachmentJson {
                id: index as u64,
                filename: attachment.filename.clone(),
                description: attachment.description.clone(),
            })
            .collect(),
        flags: if v2 { IS_COMPONENTS_V2 } else { 0 },
        message_reference: None,
    }
}

/// A Components V2 layout as Discord's `components` array.
pub fn components(blocks: &[PluginBlock]) -> Vec<ComponentJson> {
    blocks.iter().map(component).collect()
}

/// One V2 block as a Discord component.
pub fn component(block: &PluginBlock) -> ComponentJson {
    match block {
        PluginBlock::Container {
            accent_color,
            blocks,
        } => ComponentJson::Container(ContainerJson {
            kind: kind::CONTAINER,
            accent_color: *accent_color,
            components: components(blocks),
        }),

        PluginBlock::Text { content } => text_display(content),

        PluginBlock::Gallery { items } => ComponentJson::MediaGallery(MediaGalleryJson {
            kind: kind::MEDIA_GALLERY,
            items: items.iter().map(gallery_item).collect(),
        }),

        PluginBlock::Section { text, thumbnail } => ComponentJson::Section(SectionJson {
            kind: kind::SECTION,
            components: text.iter().map(|line| text_display(line)).collect(),
            accessory: Box::new(ComponentJson::Thumbnail(ThumbnailJson {
                kind: kind::THUMBNAIL,
                media: UnfurledMediaJson {
                    url: thumbnail.url.clone(),
                },
                description: thumbnail.description.clone(),
            })),
        }),

        PluginBlock::Separator { divider } => ComponentJson::Separator(SeparatorJson {
            kind: kind::SEPARATOR,
            divider: *divider,
            spacing: SPACING_SMALL,
        }),

        PluginBlock::Row(row) => action_row(row),
    }
}

/// A markdown text block. The V2 replacement for `content`.
pub fn text_display(content: &str) -> ComponentJson {
    ComponentJson::TextDisplay(TextDisplayJson {
        kind: kind::TEXT_DISPLAY,
        content: content.to_string(),
    })
}

/// Classic action rows as Discord's `components` array.
pub fn action_rows(rows: &[PluginActionRow]) -> Vec<ComponentJson> {
    rows.iter().map(action_row).collect()
}

/// One action row of buttons and selects.
pub fn action_row(row: &PluginActionRow) -> ComponentJson {
    ComponentJson::ActionRow(ActionRowJson {
        kind: kind::ACTION_ROW,
        components: row.components.iter().map(interactive).collect(),
    })
}

fn interactive(component: &PluginComponent) -> ComponentJson {
    match component {
        // `emoji` is dropped: the protocol carries it as a bare string, which
        // is ambiguous between a unicode emoji and a custom emoji's name, and
        // Discord wants a structured `{id, name, animated}`. Rendering the
        // wrong half fails the whole message, so nothing is rendered until the
        // protocol says which it means.
        PluginComponent::Button {
            custom_id,
            label,
            style,
            emoji: _,
            disabled,
        } => ComponentJson::Button(ButtonJson {
            kind: kind::BUTTON,
            style: button_style(*style),
            label: Some(label.clone()),
            custom_id: Some(custom_id.clone()),
            url: None,
            disabled: *disabled,
        }),

        PluginComponent::LinkButton {
            url,
            label,
            disabled,
        } => ComponentJson::Button(ButtonJson {
            kind: kind::BUTTON,
            style: BUTTON_LINK,
            label: Some(label.clone()),
            custom_id: None,
            url: Some(url.clone()),
            disabled: *disabled,
        }),

        PluginComponent::Select {
            custom_id,
            placeholder,
            options,
            disabled,
        } => ComponentJson::Select(SelectJson {
            kind: kind::TEXT_SELECT,
            custom_id: custom_id.clone(),
            placeholder: placeholder.clone(),
            options: options.iter().map(select_option).collect(),
            // Exactly one. The protocol has no multi-select, and Discord
            // defaults `min_values` to 1 but `max_values` to 1 as well — being
            // explicit costs two keys and removes the question.
            min_values: 1,
            max_values: 1,
            disabled: *disabled,
        }),
    }
}

fn select_option(option: &SelectOption) -> SelectOptionJson {
    SelectOptionJson {
        label: option.label.clone(),
        value: option.value.clone(),
        description: option.description.clone(),
        default: option.default,
    }
}

fn gallery_item(item: &GalleryItem) -> MediaGalleryItemJson {
    MediaGalleryItemJson {
        media: UnfurledMediaJson {
            url: item.url.clone(),
        },
        description: item.description.clone(),
    }
}

/// `ButtonStyle::Link`, which the protocol expresses as a distinct component
/// rather than a style — a link button has no `custom_id`, so making it a
/// variant is what stops one being asked for.
const BUTTON_LINK: u8 = 5;

fn button_style(style: ButtonStyle) -> u8 {
    match style {
        ButtonStyle::Primary => 1,
        ButtonStyle::Secondary => 2,
        ButtonStyle::Success => 3,
        ButtonStyle::Danger => 4,
    }
}

/// Classic embeds as Discord's `embeds` array.
pub fn embeds(embeds: &[PluginEmbed]) -> Vec<EmbedJson> {
    embeds.iter().map(embed).collect()
}

/// One embed.
pub fn embed(embed: &PluginEmbed) -> EmbedJson {
    EmbedJson {
        kind: EMBED_RICH,
        title: embed.title.clone(),
        description: embed.description.clone(),
        color: embed.color,
        url: embed.url.clone(),
        footer: embed
            .footer
            .as_ref()
            .map(|text| EmbedFooterJson { text: text.clone() }),
        fields: embed
            .fields
            .iter()
            .map(|field| EmbedFieldJson {
                name: field.name.clone(),
                value: field.value.clone(),
                inline: field.inline,
            })
            .collect(),
        thumbnail: embed
            .thumbnail_url
            .as_ref()
            .map(|url| EmbedMediaJson { url: url.clone() }),
        image: embed
            .image_url
            .as_ref()
            .map(|url| EmbedMediaJson { url: url.clone() }),
        timestamp: embed.timestamp.clone(),
    }
}

// ── The wire types ──────────────────────────────────────────────────────────

/// Discord's message payload — the JSON body of a send, and of an edit.
///
/// Built by [`body`]. The two setters cover the delivery bits a
/// [`MessageBody`](crate::MessageBody) deliberately does not carry, because
/// they are about *this* transmission rather than about what is rendered.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MessagePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<EmbedJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    components: Vec<ComponentJson>,
    /// Always present when there are files, and **always present on an edit
    /// that has none** — an edit omitting `attachments` keeps the message's
    /// existing files, where an empty array clears them. Callers that mean
    /// "leave them alone" must not go through this type.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<AttachmentJson>,
    #[serde(skip_serializing_if = "is_zero")]
    flags: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_reference: Option<MessageReferenceJson>,
}

fn is_zero(flags: &u32) -> bool {
    *flags == 0
}

impl MessagePayload {
    /// OR in more message flags — [`EPHEMERAL`], typically.
    ///
    /// Additive rather than assigning, because [`body`] has already set
    /// [`IS_COMPONENTS_V2`] if the body needed it, and overwriting that would
    /// turn a V2 message into one Discord rejects.
    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags |= flags;
        self
    }

    /// Post this as a reply to an existing message.
    pub fn replying_to(mut self, message_id: impl Into<String>) -> Self {
        self.message_reference = Some(MessageReferenceJson {
            message_id: message_id.into(),
            fail_if_not_exists: false,
        });
        self
    }

    /// Is this a Components V2 payload?
    pub fn is_v2(&self) -> bool {
        self.flags & IS_COMPONENTS_V2 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AttachmentJson {
    /// Index into the multipart `files[N]` parts. Discord matches them up by
    /// this number, so it is a position rather than an identifier.
    id: u64,
    filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MessageReferenceJson {
    message_id: String,
    /// `false` so a reply to a deleted message posts as an ordinary message
    /// instead of failing. Discord's own default is `true`, and inheriting it
    /// means a racing delete costs the whole reply.
    fail_if_not_exists: bool,
}

/// A Discord message component.
///
/// Untagged: each variant already carries its own `type`, so the enum is a Rust
/// convenience that leaves no trace on the wire.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ComponentJson {
    ActionRow(ActionRowJson),
    Button(ButtonJson),
    Select(SelectJson),
    Section(SectionJson),
    TextDisplay(TextDisplayJson),
    Thumbnail(ThumbnailJson),
    MediaGallery(MediaGalleryJson),
    Separator(SeparatorJson),
    Container(ContainerJson),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContainerJson {
    #[serde(rename = "type")]
    kind: u8,
    /// Omitted, not null: Discord reads an absent accent as "no bar" and a null
    /// as an explicit clear, and only the first is what "the plugin set none"
    /// means.
    #[serde(skip_serializing_if = "Option::is_none")]
    accent_color: Option<u32>,
    components: Vec<ComponentJson>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextDisplayJson {
    #[serde(rename = "type")]
    kind: u8,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SectionJson {
    #[serde(rename = "type")]
    kind: u8,
    components: Vec<ComponentJson>,
    accessory: Box<ComponentJson>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThumbnailJson {
    #[serde(rename = "type")]
    kind: u8,
    media: UnfurledMediaJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MediaGalleryJson {
    #[serde(rename = "type")]
    kind: u8,
    items: Vec<MediaGalleryItemJson>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MediaGalleryItemJson {
    media: UnfurledMediaJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnfurledMediaJson {
    /// `attachment://name.ext` for a file on this message, or an https URL.
    url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeparatorJson {
    #[serde(rename = "type")]
    kind: u8,
    divider: bool,
    spacing: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionRowJson {
    #[serde(rename = "type")]
    kind: u8,
    components: Vec<ComponentJson>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ButtonJson {
    #[serde(rename = "type")]
    kind: u8,
    style: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectJson {
    #[serde(rename = "type")]
    kind: u8,
    custom_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    placeholder: Option<String>,
    options: Vec<SelectOptionJson>,
    min_values: u8,
    max_values: u8,
    disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectOptionJson {
    label: String,
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmbedJson {
    /// Always `"rich"`. Discord requires the field on anything an application
    /// sends, and every other value describes an embed Discord itself
    /// generated.
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    footer: Option<EmbedFooterJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fields: Vec<EmbedFieldJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumbnail: Option<EmbedMediaJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<EmbedMediaJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmbedFooterJson {
    text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmbedFieldJson {
    name: String,
    value: String,
    inline: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmbedMediaJson {
    url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{PluginEmbedField, SelectOption};
    use serde_json::{json, Value};

    fn rendered(block: PluginBlock) -> Value {
        serde_json::to_value(component(&block)).expect("serialises")
    }

    fn button(id: &str) -> PluginComponent {
        PluginComponent::Button {
            custom_id: id.to_string(),
            label: "Go".to_string(),
            style: ButtonStyle::Danger,
            emoji: None,
            disabled: false,
        }
    }

    /// The wire format is the contract this module exists to own, so the tests
    /// assert the exact JSON rather than the Rust shape that produced it.
    #[test]
    fn a_container_nests_its_children_under_type_17() {
        let value = rendered(PluginBlock::Container {
            accent_color: Some(0x4caf50),
            blocks: vec![
                PluginBlock::Text {
                    content: "hello".into(),
                },
                PluginBlock::Separator { divider: true },
            ],
        });

        assert_eq!(
            value,
            json!({
                "type": 17,
                "accent_color": 0x4caf50,
                "components": [
                    { "type": 10, "content": "hello" },
                    { "type": 14, "divider": true, "spacing": 1 },
                ],
            })
        );
    }

    /// An absent accent must be *absent*, not `null`: Discord reads a null as
    /// an explicit clear, which is a different instruction from "unset".
    #[test]
    fn an_unset_accent_is_omitted_rather_than_nulled() {
        let value = rendered(PluginBlock::Container {
            accent_color: None,
            blocks: vec![],
        });
        assert_eq!(value, json!({ "type": 17, "components": [] }));
    }

    #[test]
    fn a_section_carries_its_thumbnail_as_an_accessory() {
        let value = rendered(PluginBlock::Section {
            text: vec!["**Budz #1**".into(), "-# 40 ADA".into()],
            thumbnail: GalleryItem::attachment("render.png", "the card"),
        });

        assert_eq!(
            value,
            json!({
                "type": 9,
                "components": [
                    { "type": 10, "content": "**Budz #1**" },
                    { "type": 10, "content": "-# 40 ADA" },
                ],
                "accessory": {
                    "type": 11,
                    "media": { "url": "attachment://render.png" },
                    "description": "the card",
                },
            })
        );
    }

    #[test]
    fn a_gallery_wraps_each_url_in_an_unfurled_media_item() {
        let value = rendered(PluginBlock::Gallery {
            items: vec![
                GalleryItem::attachment("a.png", "first"),
                GalleryItem {
                    url: "https://example.com/b.png".into(),
                    description: None,
                },
            ],
        });

        assert_eq!(
            value,
            json!({
                "type": 12,
                "items": [
                    { "media": { "url": "attachment://a.png" }, "description": "first" },
                    { "media": { "url": "https://example.com/b.png" } },
                ],
            })
        );
    }

    /// Style numbers, not names. Getting one wrong renders a differently
    /// coloured button rather than an error, so they are pinned individually.
    #[test]
    fn button_styles_map_to_discords_numbers() {
        let styles = [
            (ButtonStyle::Primary, 1),
            (ButtonStyle::Secondary, 2),
            (ButtonStyle::Success, 3),
            (ButtonStyle::Danger, 4),
        ];
        for (style, expected) in styles {
            assert_eq!(button_style(style), expected, "{style:?}");
        }
    }

    /// A link button is style 5 with **no** `custom_id`. Sending one with both
    /// a url and a custom_id is rejected outright.
    #[test]
    fn a_link_button_carries_a_url_and_no_custom_id() {
        let value = serde_json::to_value(action_row(&PluginActionRow::new(vec![
            button("comp:refresh"),
            PluginComponent::LinkButton {
                url: "https://example.com".into(),
                label: "Open".into(),
                disabled: true,
            },
        ])))
        .unwrap();

        assert_eq!(
            value,
            json!({
                "type": 1,
                "components": [
                    {
                        "type": 2,
                        "style": 4,
                        "label": "Go",
                        "custom_id": "comp:refresh",
                        "disabled": false,
                    },
                    {
                        "type": 2,
                        "style": 5,
                        "label": "Open",
                        "url": "https://example.com",
                        "disabled": true,
                    },
                ],
            })
        );
    }

    #[test]
    fn a_select_is_a_text_select_bounded_to_one_value() {
        let value = serde_json::to_value(action_row(&PluginActionRow::new(vec![
            PluginComponent::Select {
                custom_id: "trait:face".into(),
                placeholder: Some("Pick a trait".into()),
                options: vec![
                    SelectOption {
                        label: "Ghoulish".into(),
                        value: "ghoulish".into(),
                        description: Some("12 assets".into()),
                        default: false,
                    },
                    SelectOption {
                        label: "Necro".into(),
                        value: "necro".into(),
                        description: None,
                        default: true,
                    },
                ],
                disabled: false,
            },
        ])))
        .unwrap();

        assert_eq!(
            value,
            json!({
                "type": 1,
                "components": [{
                    "type": 3,
                    "custom_id": "trait:face",
                    "placeholder": "Pick a trait",
                    "options": [
                        {
                            "label": "Ghoulish",
                            "value": "ghoulish",
                            "description": "12 assets",
                            "default": false,
                        },
                        { "label": "Necro", "value": "necro", "default": true },
                    ],
                    "min_values": 1,
                    "max_values": 1,
                    "disabled": false,
                }],
            })
        );
    }

    /// Rows nest inside V2 containers, so the same renderer has to reach them
    /// there — a container's buttons are not a separate `components` array.
    #[test]
    fn a_row_renders_the_same_inside_a_container() {
        let row = PluginActionRow::new(vec![button("a")]);
        let standalone = serde_json::to_value(action_row(&row)).unwrap();
        let nested = rendered(PluginBlock::Container {
            accent_color: None,
            blocks: vec![PluginBlock::Row(row)],
        });
        assert_eq!(nested["components"][0], standalone);
    }

    #[test]
    fn an_embed_declares_itself_rich() {
        let value = serde_json::to_value(embed(&PluginEmbed {
            title: Some("Epoch 641".into()),
            description: Some("Rewards are in.".into()),
            color: Some(0x4caf50),
            fields: vec![PluginEmbedField {
                name: "Rum".into(),
                value: "12".into(),
                inline: true,
            }],
            footer: Some("Black Flag".into()),
            url: Some("https://example.com".into()),
            thumbnail_url: Some("https://example.com/t.png".into()),
            image_url: None,
            timestamp: None,
        }))
        .unwrap();

        assert_eq!(
            value,
            json!({
                "type": "rich",
                "title": "Epoch 641",
                "description": "Rewards are in.",
                "color": 0x4caf50,
                "url": "https://example.com",
                "footer": { "text": "Black Flag" },
                "fields": [{ "name": "Rum", "value": "12", "inline": true }],
                "thumbnail": { "url": "https://example.com/t.png" },
            })
        );
    }

    /// An empty embed is still `{"type":"rich"}` and never `{}` — Discord
    /// requires the discriminator on anything an application sends.
    #[test]
    fn an_empty_embed_still_carries_its_type() {
        let value = serde_json::to_value(embed(&PluginEmbed::default())).unwrap();
        assert_eq!(value, json!({ "type": "rich" }));
    }

    /// `PluginEmbed::timestamp` is already on the protocol and the twilight
    /// path silently dropped it. Nothing sets it yet, so rendering it is a gap
    /// closed rather than a behaviour change — but it is the one place this
    /// renderer deliberately differs from what it replaces.
    #[test]
    fn a_timestamp_reaches_the_wire() {
        let value = serde_json::to_value(embed(&PluginEmbed {
            timestamp: Some("2026-09-02T00:00:00Z".into()),
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(value["timestamp"], "2026-09-02T00:00:00Z");
    }

    /// The flag is the whole difference between a V2 message and a rejected
    /// one, and it cannot be removed once Discord has set it.
    #[test]
    fn the_v2_flag_is_bit_fifteen() {
        assert_eq!(IS_COMPONENTS_V2, 32_768);
        assert_eq!(EPHEMERAL, 64);
    }

    // ── The whole-body payload ──────────────────────────────────────────

    fn payload(body: &crate::MessageBody) -> Value {
        serde_json::to_value(super::body(body)).unwrap()
    }

    #[test]
    fn a_classic_body_carries_content_embeds_and_rows() {
        let value = payload(&crate::MessageBody {
            content: Some("hi".into()),
            embeds: vec![PluginEmbed {
                title: Some("t".into()),
                ..Default::default()
            }],
            rows: vec![PluginActionRow::new(vec![button("a")])],
            ..Default::default()
        });

        assert_eq!(value["content"], "hi");
        assert_eq!(value["embeds"][0]["title"], "t");
        assert_eq!(value["components"][0]["type"], 1);
        assert!(value.get("flags").is_none(), "no flags to set: {value}");
    }

    /// The rule the whole V2 path turns on. Discord rejects a message carrying
    /// both vocabularies, and the flag cannot be removed once set — so a body
    /// that set both gets the layout, and `content`/`embeds` are dropped rather
    /// than 400ing the message.
    #[test]
    fn a_v2_body_drops_content_and_embeds_and_sets_the_flag() {
        let value = payload(&crate::MessageBody {
            content: Some("dropped".into()),
            embeds: vec![PluginEmbed::default()],
            rows: vec![PluginActionRow::new(vec![button("ignored")])],
            layout: vec![PluginBlock::Text {
                content: "kept".into(),
            }],
            ..Default::default()
        });

        assert!(value.get("content").is_none(), "{value}");
        assert!(value.get("embeds").is_none(), "{value}");
        assert_eq!(value["flags"], IS_COMPONENTS_V2);
        // The layout wins the `components` slot; the classic rows do not also
        // appear, which would be two vocabularies in one message again.
        assert_eq!(value["components"].as_array().unwrap().len(), 1);
        assert_eq!(value["components"][0]["content"], "kept");
    }

    /// `attachments[N].id` is a *position*: it has to match the `files[N]` part
    /// a multipart sender writes, or Discord pairs the wrong file with the
    /// wrong declaration.
    #[test]
    fn attachments_are_declared_by_index() {
        let value = payload(
            &crate::MessageBody::default()
                .with_attachment(crate::Attachment::new("a.png", vec![1]))
                .with_attachment(crate::Attachment::new("b.png", vec![2]).described("the second")),
        );

        assert_eq!(
            value["attachments"],
            json!([
                { "id": 0, "filename": "a.png" },
                { "id": 1, "filename": "b.png", "description": "the second" },
            ])
        );
    }

    #[test]
    fn ephemeral_ors_in_beside_the_v2_flag() {
        let v2 = crate::MessageBody::layout(vec![PluginBlock::Text {
            content: "x".into(),
        }]);
        let value = serde_json::to_value(super::body(&v2).with_flags(EPHEMERAL)).unwrap();

        assert_eq!(value["flags"], IS_COMPONENTS_V2 | EPHEMERAL);
        assert!(
            super::body(&v2).with_flags(EPHEMERAL).is_v2(),
            "adding a flag must not clear the V2 bit"
        );
    }

    /// A reply to a message someone deleted mid-flight should still post.
    /// Discord's own default for this field is `true`, which would lose it.
    #[test]
    fn a_reply_survives_its_target_being_deleted() {
        let value =
            serde_json::to_value(super::body(&crate::MessageBody::text("re")).replying_to("123"))
                .unwrap();
        assert_eq!(
            value["message_reference"],
            json!({ "message_id": "123", "fail_if_not_exists": false })
        );
    }
}
