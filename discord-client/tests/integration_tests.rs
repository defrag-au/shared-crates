use discord_client::compat::twilight::TwEmbedBuilder;
use discord_client::{AttachmentInput, DiscordClient, DiscordMessage, NativeDiscordClient};
use std::env;
use tokio;

#[tokio::test]
async fn test_native_json_message() {
    dotenv::dotenv().ok();

    let bot_token = match env::var("DISCORD_BOT_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            println!("Skipping test: DISCORD_BOT_TOKEN not set");
            return;
        }
    };

    let channel_id = match env::var("DISCORD_CHANNEL_ID") {
        Ok(id) => id,
        Err(_) => {
            println!("Skipping test: DISCORD_CHANNEL_ID not set");
            return;
        }
    };

    let client = NativeDiscordClient::new(bot_token);
    let message = DiscordMessage {
        content: Some("Test message from discord-client native".to_string()),
        embeds: None,
        attachments: None,
    };

    let result = client.send_message(&channel_id, &message).await;
    assert!(result.is_ok(), "Failed to send message: {:?}", result);
}

#[tokio::test]
async fn test_native_message_with_embed() {
    dotenv::dotenv().ok();

    let bot_token = match env::var("DISCORD_BOT_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            println!("Skipping test: DISCORD_BOT_TOKEN not set");
            return;
        }
    };

    let channel_id = match env::var("DISCORD_CHANNEL_ID") {
        Ok(id) => id,
        Err(_) => {
            println!("Skipping test: DISCORD_CHANNEL_ID not set");
            return;
        }
    };

    let client = NativeDiscordClient::new(bot_token);
    let embed = TwEmbedBuilder::new()
        .title("Test Embed")
        .description("This is a test embed from discord-client")
        .color(0x00ff00)
        .build();

    let message = DiscordMessage {
        content: Some("Message with embed".to_string()),
        embeds: Some(vec![embed]),
        attachments: None,
    };

    let result = client.send_message(&channel_id, &message).await;
    assert!(
        result.is_ok(),
        "Failed to send message with embed: {:?}",
        result
    );
}

#[tokio::test]
async fn test_native_message_with_attachment() {
    dotenv::dotenv().ok();

    let bot_token = match env::var("DISCORD_BOT_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            println!("Skipping test: DISCORD_BOT_TOKEN not set");
            return;
        }
    };

    let channel_id = match env::var("DISCORD_CHANNEL_ID") {
        Ok(id) => id,
        Err(_) => {
            println!("Skipping test: DISCORD_CHANNEL_ID not set");
            return;
        }
    };

    let client = NativeDiscordClient::new(bot_token);

    // Create a small test PNG (1x1 transparent pixel)
    let test_png_data = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let attachment = AttachmentInput {
        id: "0".to_string(),
        filename: "test.png".to_string(),
        description: Some("Test attachment".to_string()),
        file_data: test_png_data,
    };

    let message = DiscordMessage {
        content: Some("Message with attachment".to_string()),
        embeds: None,
        attachments: Some(vec![attachment]),
    };

    let result = client.send_message(&channel_id, &message).await;
    assert!(
        result.is_ok(),
        "Failed to send message with attachment: {:?}",
        result
    );
}

#[test]
fn test_attachment_validation() {
    // Test empty file
    assert!(NativeDiscordClient::validate_attachment(&[], "test.png").is_err());

    // Test unsupported file type
    assert!(NativeDiscordClient::validate_attachment(&[1, 2, 3], "test.txt").is_err());

    // Test file too large
    let large_file = vec![0u8; 9 * 1024 * 1024]; // 9MB
    assert!(NativeDiscordClient::validate_attachment(&large_file, "test.png").is_err());

    // Test valid file
    let valid_file = vec![0u8; 1024]; // 1KB
    assert!(NativeDiscordClient::validate_attachment(&valid_file, "test.png").is_ok());
}

#[test]
fn test_content_type_detection() {
    assert_eq!(
        NativeDiscordClient::get_content_type("test.png"),
        "image/png"
    );
    assert_eq!(
        NativeDiscordClient::get_content_type("test.jpg"),
        "image/jpeg"
    );
    assert_eq!(
        NativeDiscordClient::get_content_type("test.jpeg"),
        "image/jpeg"
    );
    assert_eq!(
        NativeDiscordClient::get_content_type("test.gif"),
        "image/gif"
    );
    assert_eq!(
        NativeDiscordClient::get_content_type("test.webp"),
        "image/webp"
    );
    assert_eq!(
        NativeDiscordClient::get_content_type("test.unknown"),
        "application/octet-stream"
    );
}
