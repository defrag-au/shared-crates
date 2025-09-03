#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use discord_client::{DiscordMessage, DiscordAttachment, DiscordEmbed, WasmDiscordClient, DiscordClient};
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn test_wasm_json_message() {
        let search = web_sys::window().unwrap().location().search().unwrap();
        
        let bot_token = match search.contains("bot_token=") {
            true => {
                let url_encoded = search.split("bot_token=").nth(1).unwrap().split("&").next().unwrap();
                js_sys::decode_uri_component(url_encoded).unwrap().as_string().unwrap()
            }
            false => {
                web_sys::console::log_1(&"Skipping test: bot_token not provided in query params".into());
                return;
            }
        };

        let channel_id = match search.contains("channel_id=") {
            true => {
                let url_encoded = search.split("channel_id=").nth(1).unwrap().split("&").next().unwrap();
                js_sys::decode_uri_component(url_encoded).unwrap().as_string().unwrap()
            }
            false => {
                web_sys::console::log_1(&"Skipping test: channel_id not provided in query params".into());
                return;
            }
        };

        let client = WasmDiscordClient::new(bot_token);
        let message = DiscordMessage {
            content: Some("Test message from discord-client WASM".to_string()),
            embeds: None,
            attachments: None,
        };

        let result = client.send_message(&channel_id, &message).await;
        assert!(result.is_ok(), "Failed to send WASM message: {:?}", result);
    }

    #[wasm_bindgen_test]
    async fn test_wasm_message_with_embed() {
        let search = web_sys::window().unwrap().location().search().unwrap();
        
        let bot_token = match search.contains("bot_token=") {
            true => {
                let url_encoded = search.split("bot_token=").nth(1).unwrap().split("&").next().unwrap();
                js_sys::decode_uri_component(url_encoded).unwrap().as_string().unwrap()
            }
            false => {
                web_sys::console::log_1(&"Skipping test: bot_token not provided in query params".into());
                return;
            }
        };

        let channel_id = match search.contains("channel_id=") {
            true => {
                let url_encoded = search.split("channel_id=").nth(1).unwrap().split("&").next().unwrap();
                js_sys::decode_uri_component(url_encoded).unwrap().as_string().unwrap()
            }
            false => {
                web_sys::console::log_1(&"Skipping test: channel_id not provided in query params".into());
                return;
            }
        };

        let client = WasmDiscordClient::new(bot_token);
        let embed = DiscordEmbed {
            title: Some("WASM Test Embed".to_string()),
            description: Some("This is a test embed from discord-client WASM".to_string()),
            color: Some(0x0099ff),
            thumbnail: None,
            image: None,
            fields: vec![],
            footer: None,
            timestamp: None,
        };

        let message = DiscordMessage {
            content: Some("WASM message with embed".to_string()),
            embeds: Some(vec![embed]),
            attachments: None,
        };

        let result = client.send_message(&channel_id, &message).await;
        assert!(result.is_ok(), "Failed to send WASM message with embed: {:?}", result);
    }

    #[wasm_bindgen_test]
    async fn test_wasm_message_with_attachment() {
        let search = web_sys::window().unwrap().location().search().unwrap();
        
        let bot_token = match search.contains("bot_token=") {
            true => {
                let url_encoded = search.split("bot_token=").nth(1).unwrap().split("&").next().unwrap();
                js_sys::decode_uri_component(url_encoded).unwrap().as_string().unwrap()
            }
            false => {
                web_sys::console::log_1(&"Skipping test: bot_token not provided in query params".into());
                return;
            }
        };

        let channel_id = match search.contains("channel_id=") {
            true => {
                let url_encoded = search.split("channel_id=").nth(1).unwrap().split("&").next().unwrap();
                js_sys::decode_uri_component(url_encoded).unwrap().as_string().unwrap()
            }
            false => {
                web_sys::console::log_1(&"Skipping test: channel_id not provided in query params".into());
                return;
            }
        };

        let client = WasmDiscordClient::new(bot_token);
        
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
            filename: "wasm_test.png".to_string(),
            description: Some("WASM test attachment".to_string()),
            file_data: test_png_data,
        };

        let message = DiscordMessage {
            content: Some("WASM message with attachment".to_string()),
            embeds: None,
            attachments: Some(vec![attachment]),
        };

        let result = client.send_message(&channel_id, &message).await;
        assert!(result.is_ok(), "Failed to send WASM message with attachment: {:?}", result);
    }

    #[wasm_bindgen_test]
    fn test_wasm_attachment_validation() {
        // Test empty file
        assert!(WasmDiscordClient::validate_attachment(&[], "test.png").is_err());
        
        // Test unsupported file type
        assert!(WasmDiscordClient::validate_attachment(&[1, 2, 3], "test.txt").is_err());
        
        // Test file too large
        let large_file = vec![0u8; 9 * 1024 * 1024]; // 9MB
        assert!(WasmDiscordClient::validate_attachment(&large_file, "test.png").is_err());
        
        // Test valid file
        let valid_file = vec![0u8; 1024]; // 1KB
        assert!(WasmDiscordClient::validate_attachment(&valid_file, "test.png").is_ok());
    }

    #[wasm_bindgen_test]
    fn test_wasm_content_type_detection() {
        assert_eq!(WasmDiscordClient::get_content_type("test.png"), "image/png");
        assert_eq!(WasmDiscordClient::get_content_type("test.jpg"), "image/jpeg");
        assert_eq!(WasmDiscordClient::get_content_type("test.jpeg"), "image/jpeg");
        assert_eq!(WasmDiscordClient::get_content_type("test.gif"), "image/gif");
        assert_eq!(WasmDiscordClient::get_content_type("test.webp"), "image/webp");
        assert_eq!(WasmDiscordClient::get_content_type("test.unknown"), "application/octet-stream");
    }
}