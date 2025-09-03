use discord_client::{DiscordMessage, AttachmentInput, NativeDiscordClient, DiscordClient};
use discord_client::compat::twilight::TwEmbedBuilder;
use twilight_util::builder::embed::EmbedFieldBuilder;
use std::env;
use std::fs;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let bot_token = env::var("DISCORD_BOT_TOKEN")
        .expect("DISCORD_BOT_TOKEN environment variable must be set");

    let channel_id = env::var("DISCORD_CHANNEL_ID")
        .expect("DISCORD_CHANNEL_ID environment variable must be set");

    let client = NativeDiscordClient::new(bot_token);

    // Example 1: Simple text message
    println!("Sending simple text message...");
    let simple_message = DiscordMessage {
        content: Some("Hello from discord-client native example!".to_string()),
        embeds: None,
        attachments: None,
    };
    let response = client.send_message(&channel_id, &simple_message).await?;
    println!("Message sent! ID: {}", response.id);

    // Edit the first message content to demonstrate editing
    println!("Editing the first message...");
    let edit = discord_client::DiscordMessageEdit {
        content: Some("Hello from discord-client (edited)!".to_string()),
        embeds: None,
    };
    let response_id = response.id.to_string();
    let edited = client.edit_message(&channel_id, &response_id, &edit).await?;
    println!("Message edited! New content: {}", edited.content);

    // Example 2: Message with embed (Twilight builder)
    println!("Sending message with embed...");
    let embed = TwEmbedBuilder::new()
        .title("Native Client Example")
        .description("This message was sent using the native discord-client")
        .color(0x00ff00)
        .field(EmbedFieldBuilder::new("Platform", "Native Rust").inline())
        .field(EmbedFieldBuilder::new("HTTP Client", "reqwest").inline())
        .build();

    let embed_message = DiscordMessage {
        content: Some("Check out this embed!".to_string()),
        embeds: Some(vec![embed]),
        attachments: None,
    };
    let response = client.send_message(&channel_id, &embed_message).await?;
    println!("Embed message sent! ID: {}", response.id);

    // Example 3: Message with attachment
    println!("Sending message with attachment...");

    // If DISCORD_IMAGE_PATH is set, upload that file; otherwise send a tiny test PNG.
    let (file_data, filename, description) = match env::var("DISCORD_IMAGE_PATH") {
        Ok(path_str) => {
            let path = Path::new(&path_str);
            let bytes = fs::read(path)?;
            let fname = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("upload.bin")
                .to_string();
            (bytes, fname, Some("Uploaded from DISCORD_IMAGE_PATH".to_string()))
        }
        Err(_) => {
            // Create a small test PNG (1x1 transparent pixel)
            let test_png_data = vec![
                0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
                0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
                0x89, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
                0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
                0x42, 0x60, 0x82
            ];
            (
                test_png_data,
                "example_test.png".to_string(),
                Some("Test image from native example".to_string()),
            )
        }
    };

    let attachment = AttachmentInput {
        id: "0".to_string(),
        filename,
        description,
        file_data,
    };

    let attachment_message = DiscordMessage {
        content: Some("Here's an image attachment!".to_string()),
        embeds: None,
        attachments: Some(vec![attachment]),
    };
    let attachment_response = client.send_message(&channel_id, &attachment_message).await?;
    println!("Attachment message sent! ID: {} (attachments: {})", attachment_response.id, attachment_response.attachments.len());

    // Example 4: Edit text of the attachment message; attachment should be preserved
    println!("Editing attachment message content (attachment should remain)...");
    let edit_keep_attachment = discord_client::DiscordMessageEdit {
        content: Some("Updated text; attachment should still be visible.".to_string()),
        embeds: None,
    };
    let attachment_response_id = attachment_response.id.to_string();
    let edited_keep = client
        .edit_message(&channel_id, &attachment_response_id, &edit_keep_attachment)
        .await?;
    println!(
        "Edited message. Attachments still present: {}",
        edited_keep.attachments.len()
    );

    // Example 5: Add another attachment (and an embed) to the existing message
    println!("Adding another attachment and an embed to the existing message...");
    let (file_data2, filename2) = match env::var("DISCORD_IMAGE_PATH2") {
        Ok(path_str) => {
            let path = Path::new(&path_str);
            let bytes = fs::read(path)?;
            let fname = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("upload2.bin")
                .to_string();
            (bytes, fname)
        }
        Err(_) => {
            // fallback to the same small PNG
            let data = vec![
                0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
                0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
                0x89, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
                0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
                0x42, 0x60, 0x82
            ];
            (data, "extra.png".to_string())
        }
    };

    let new_attachment = AttachmentInput {
        id: "0".to_string(),
        filename: filename2,
        description: Some("New attachment added via edit".to_string()),
        file_data: file_data2,
    };

    let embed2 = TwEmbedBuilder::new()
        .title("Edited with extra attachment")
        .description("We added another file and updated content")
        .color(0x3366ff)
        .field(EmbedFieldBuilder::new("Action", "edit + attach").inline())
        .build();

    let edit_with_new = discord_client::DiscordMessageEdit {
        content: Some("Content updated during attachment add".to_string()),
        embeds: Some(vec![embed2]),
    };

    let edited_with_new = client
        .edit_message_with_attachments(
            &channel_id,
            &attachment_response_id,
            &edit_with_new,
            &[new_attachment],
        )
        .await?;

    println!(
        "Edited with new attachment. Now {} attachment(s).",
        edited_with_new.attachments.len()
    );

    println!("All examples completed successfully!");
    Ok(())
}
