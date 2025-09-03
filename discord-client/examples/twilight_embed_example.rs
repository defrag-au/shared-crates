#![cfg(feature = "twilight")]

use discord_client::{
    compat::twilight::TwEmbedBuilder,
    DiscordAttachment, DiscordClient, DiscordMessage, NativeDiscordClient,
};
use std::{env, fs, path::Path};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let bot_token = env::var("DISCORD_BOT_TOKEN")?;
    let channel_id = env::var("DISCORD_CHANNEL_ID")?;

    let client = NativeDiscordClient::new(bot_token);

    // Build an embed using twilight-util's builder and convert to our type
    let t_embed = TwEmbedBuilder::new()
        .title("Status")
        .description("Built with twilight-util and sent via discord-client")
        .color(0x33cc99)
        .build();
    let embed = t_embed.into();

    // Optional attachment for initial send
    let attachment_opt = match env::var("DISCORD_IMAGE_PATH") {
        Ok(path_str) => {
            let path = Path::new(&path_str);
            let bytes = fs::read(path)?;
            let fname = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("upload.bin")
                .to_string();
            Some(DiscordAttachment {
                id: "0".to_string(),
                filename: fname,
                description: Some("Initial attachment (Twilight example)".to_string()),
                file_data: bytes,
            })
        }
        Err(_) => None,
    };

    let message = DiscordMessage {
        content: Some("Message with Twilight-built embed".to_string()),
        embeds: Some(vec![embed]),
        attachments: attachment_opt.as_ref().map(|a| vec![a.clone()]),
    };

    let sent = client.send_message(&channel_id, &message).await?;

    // Now edit the embed using the builder again
    let t_embed2 = TwEmbedBuilder::new()
        .title("Status (edited)")
        .description("Updated via edit_message")
        .color(0x3366ff)
        .build();
    let embed2 = t_embed2.into();

    let edit = discord_client::DiscordMessageEdit {
        content: Some("Updated content with Twilight-built embed".to_string()),
        embeds: Some(vec![embed2]),
    };

    let _updated = client.edit_message(&channel_id, &sent.id, &edit).await?;

    // Optionally add a new attachment on edit
    if let Ok(path2) = env::var("DISCORD_IMAGE_PATH2") {
        let path = Path::new(&path2);
        let bytes = fs::read(path)?;
        let fname = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("upload2.bin")
            .to_string();

        let new_attachment = DiscordAttachment {
            id: "0".to_string(),
            filename: fname,
            description: Some("Attachment added via edit (Twilight example)".to_string()),
            file_data: bytes,
        };

        let _edited_with_new = client
            .edit_message_with_attachments(&channel_id, &sent.id, &discord_client::DiscordMessageEdit { content: None, embeds: None }, &[new_attachment])
            .await?;
    }
    Ok(())
}
