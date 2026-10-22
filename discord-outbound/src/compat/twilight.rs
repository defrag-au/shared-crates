//! Bridges for consumers still building twilight embeds.
//!
//! [`plugin_embed`] is the one conversion in the tree: it lets a renderer keep
//! using `twilight_util`'s `EmbedBuilder` while the message it is part of is a
//! [`MessageBody`](discord_message::MessageBody). That is what lets a single
//! message carry a Components V2 layout *and* classic embeds without either
//! side owning a twilight type.

use discord_message::{PluginEmbed, PluginEmbedField};
use twilight_model::channel::message::embed::Embed;

// Expose Twilight builders for ergonomics.
pub use twilight_util::builder::embed::EmbedBuilder as TwEmbedBuilder;

/// A twilight embed as the shared message model's embed.
///
/// # What is dropped, and why
///
/// `author`, `provider` and `video` have no counterpart in
/// [`PluginEmbed`](discord_message::PluginEmbed). Nothing in either repo sets
/// them, and carrying a field no producer writes is how a deliberately small
/// vocabulary stops being one. A caller that needs one should add it to the
/// model, not smuggle it through here.
///
/// The timestamp is the one thing that is not a straight move: twilight holds a
/// parsed [`Timestamp`](twilight_model::util::Timestamp), and the model holds
/// the ISO 8601 string Discord expects on the wire. `iso_8601` is twilight's own
/// formatter, so the two cannot disagree about the format — and the microseconds
/// are dropped because every caller here works in whole seconds.
pub fn plugin_embed(embed: Embed) -> PluginEmbed {
    PluginEmbed {
        title: embed.title,
        description: embed.description,
        color: embed.color,
        fields: embed
            .fields
            .into_iter()
            .map(|field| PluginEmbedField {
                name: field.name,
                value: field.value,
                inline: field.inline,
            })
            .collect(),
        footer: embed.footer.map(|footer| footer.text),
        url: embed.url,
        thumbnail_url: embed.thumbnail.map(|thumbnail| thumbnail.url),
        image_url: embed.image.map(|image| image.url),
        timestamp: embed
            .timestamp
            .map(|timestamp| timestamp.iso_8601().with_microseconds(false).to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use twilight_model::util::Timestamp;
    use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, ImageSource};

    /// Every field the notifier writes survives the trip. A field that silently
    /// vanished here would disappear from a live notification, which is exactly
    /// the class of bug a conversion shared by five renderers must not have.
    #[test]
    fn a_renderer_embed_arrives_whole() {
        let embed = EmbedBuilder::new()
            .title("Aliens minted")
            .description("a description")
            .color(0x2ecc71)
            .field(EmbedFieldBuilder::new("Traits", "**Body:** Grey").build())
            .field(EmbedFieldBuilder::new("Inline", "yes").inline().build())
            .image(ImageSource::attachment("attachment.jpg").unwrap())
            .timestamp(Timestamp::from_micros(1_700_000_000_000_000).unwrap())
            .build();

        let converted = plugin_embed(embed);

        assert_eq!(converted.title.as_deref(), Some("Aliens minted"));
        assert_eq!(converted.description.as_deref(), Some("a description"));
        assert_eq!(converted.color, Some(0x2ecc71));
        assert_eq!(converted.fields.len(), 2);
        assert!(!converted.fields[0].inline);
        assert!(converted.fields[1].inline);
        assert_eq!(
            converted.image_url.as_deref(),
            Some("attachment://attachment.jpg")
        );
        assert_eq!(
            converted.timestamp.as_deref(),
            Some("2023-11-14T22:13:20+00:00"),
            "the model holds the wire form, formatted by twilight itself"
        );
    }

    /// An embed with nothing set stays empty rather than gaining defaults.
    #[test]
    fn an_empty_embed_stays_empty() {
        let converted = plugin_embed(EmbedBuilder::new().build());

        assert_eq!(converted, PluginEmbed::default());
    }
}
