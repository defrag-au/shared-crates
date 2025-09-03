use discord_client::{DiscordMessage, DiscordAttachment, DiscordEmbed, DiscordEmbedField, NativeDiscordClient, DiscordClient};
use std::env;

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

    // Example 2: Message with embed
    println!("Sending message with embed...");
    let embed = DiscordEmbed {
        title: Some("Native Client Example".to_string()),
        description: Some("This message was sent using the native discord-client".to_string()),
        color: Some(0x00ff00), // Green
        thumbnail: None,
        image: None,
        fields: vec![
            DiscordEmbedField {
                name: "Platform".to_string(),
                value: "Native Rust".to_string(),
                inline: true,
            },
            DiscordEmbedField {
                name: "HTTP Client".to_string(),
                value: "reqwest".to_string(),
                inline: true,
            },
        ],
        footer: None,
        timestamp: None,
    };

    let embed_message = DiscordMessage {
        content: Some("Check out this embed!".to_string()),
        embeds: Some(vec![embed]),
        attachments: None,
    };
    let response = client.send_message(&channel_id, &embed_message).await?;
    println!("Embed message sent! ID: {}", response.id);

    // Example 3: Message with attachment
    println!("Sending message with attachment...");
    
    // Create a small test PNG (1x1 transparent pixel)
    let test_png_data = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82
    ];

    let attachment = DiscordAttachment {
        id: "0".to_string(),
        filename: "example_test.png".to_string(),
        description: Some("Test image from native example".to_string()),
        file_data: test_png_data,
    };

    let attachment_message = DiscordMessage {
        content: Some("Here's an image attachment!".to_string()),
        embeds: None,
        attachments: Some(vec![attachment]),
    };
    let response = client.send_message(&channel_id, &attachment_message).await?;
    println!("Attachment message sent! ID: {}", response.id);

    println!("All examples completed successfully!");
    Ok(())
}