#![allow(dead_code)]

use crate::{DiscordEmbed, DiscordEmbedField, DiscordEmbedFooter, DiscordEmbedImage};

pub use twilight_util::builder::embed::EmbedBuilder as TwEmbedBuilder;

use twilight_model::channel::message::embed as tw;

impl From<tw::Embed> for DiscordEmbed {
    fn from(e: tw::Embed) -> Self {
        let thumbnail = e.thumbnail.map(|t| DiscordEmbedImage { url: t.url });
        let image = e.image.map(|i| DiscordEmbedImage { url: i.url });
        let footer = e.footer.map(|f| DiscordEmbedFooter {
            text: f.text,
            icon_url: f.icon_url,
        });
        let fields = e
            .fields
            .into_iter()
            .map(|f| DiscordEmbedField {
                name: f.name,
                value: f.value,
                inline: f.inline,
            })
            .collect();

        let timestamp = None; // Timestamp formatting intentionally omitted

        DiscordEmbed {
            title: e.title,
            description: e.description,
            color: e.color,
            thumbnail,
            image,
            fields,
            footer,
            timestamp,
        }
    }
}

impl From<DiscordEmbed> for tw::Embed {
    fn from(e: DiscordEmbed) -> Self {
        let thumbnail = e.thumbnail.map(|t| tw::EmbedThumbnail {
            height: None,
            width: None,
            proxy_url: None,
            url: t.url,
        });

        let image = e.image.map(|i| tw::EmbedImage {
            height: None,
            width: None,
            proxy_url: None,
            url: i.url,
        });

        let footer = e.footer.map(|f| tw::EmbedFooter { proxy_icon_url: None, icon_url: f.icon_url, text: f.text });

        let fields = e
            .fields
            .into_iter()
            .map(|f| tw::EmbedField {
                inline: f.inline,
                name: f.name,
                value: f.value,
            })
            .collect();

        let color = e.color;

        // Timestamp omitted for simplicity (can parse to twilight_model::util::Timestamp if needed)
        let timestamp = None;

        tw::Embed {
            author: None,
            color,
            description: e.description,
            fields,
            footer,
            image,
            kind: "rich".to_string(),
            provider: None,
            thumbnail,
            timestamp,
            title: e.title,
            url: None,
            video: None,
        }
    }
}
