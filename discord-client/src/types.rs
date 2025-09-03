use serde::{Deserialize, Serialize};

/// Discord message with optional attachments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordMessage {
    pub content: Option<String>,
    pub embeds: Option<Vec<DiscordEmbed>>,
    pub attachments: Option<Vec<DiscordAttachment>>,
}

/// Discord attachment for file uploads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordAttachment {
    pub id: String,
    pub filename: String,
    pub description: Option<String>,
    #[serde(skip)]
    pub file_data: Vec<u8>, // Binary data - not serialized in JSON
}

/// Discord embed structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<u32>,
    pub thumbnail: Option<DiscordEmbedImage>,
    pub image: Option<DiscordEmbedImage>,
    pub fields: Vec<DiscordEmbedField>,
    pub footer: Option<DiscordEmbedFooter>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbedField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbedImage {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbedFooter {
    pub text: String,
    pub icon_url: Option<String>,
}

/// Discord rate limit response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordRateLimitResponse {
    pub message: String,
    pub retry_after: f64,
    pub global: bool,
}

/// Common interface for Discord webhook operations
pub trait DiscordWebhookClient {
    /// Send a webhook message with optional attachments
    async fn send_webhook(
        &self,
        webhook_url: &str,
        message: &DiscordMessage,
    ) -> Result<(), crate::DiscordError>;

    /// Validate attachment data before sending
    fn validate_attachment(data: &[u8], filename: &str) -> Result<(), crate::DiscordError> {
        const MAX_FILE_SIZE: usize = 8 * 1024 * 1024; // 8MB limit

        if data.is_empty() {
            return Err(crate::DiscordError::InvalidAttachment(
                "File data is empty".to_string(),
            ));
        }

        if data.len() > MAX_FILE_SIZE {
            return Err(crate::DiscordError::InvalidAttachment(format!(
                "File too large: {}KB > 8MB limit",
                data.len() / 1024
            )));
        }

        let valid_extensions = ["png", "jpg", "jpeg", "gif", "webp"];
        if !valid_extensions.iter().any(|ext| filename.ends_with(ext)) {
            return Err(crate::DiscordError::InvalidAttachment(format!(
                "Unsupported file type: {filename}"
            )));
        }

        Ok(())
    }

    /// Get content type from filename
    fn get_content_type(filename: &str) -> &'static str {
        if filename.ends_with(".png") {
            "image/png"
        } else if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
            "image/jpeg"
        } else if filename.ends_with(".gif") {
            "image/gif"
        } else if filename.ends_with(".webp") {
            "image/webp"
        } else {
            "application/octet-stream"
        }
    }
}