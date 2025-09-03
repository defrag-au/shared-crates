use serde::{Deserialize, Serialize};
use core::future::Future;
use core::pin::Pin;
use twilight_model::channel::Message;
use twilight_model::channel::message::embed::Embed as TwEmbed;

/// Outbound message payload with optional attachments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordMessage {
    pub content: Option<String>,
    pub embeds: Option<Vec<TwEmbed>>, // Twilight embed types
    pub attachments: Option<Vec<AttachmentInput>>, // For multipart file uploads
}

/// Attachment input for file uploads (binary data is not serialized to JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInput {
    pub id: String,
    pub filename: String,
    pub description: Option<String>,
    #[serde(skip)]
    pub file_data: Vec<u8>,
}

/// Discord rate limit response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordRateLimitResponse {
    pub message: String,
    pub retry_after: f64,
    pub global: bool,
}

// Response type: leverage Twilight message model

/// Common interface for Discord bot API operations
pub trait DiscordClient {
    /// Future type for `send_message` (avoids async fn in traits)
    type SendMessageFut<'a>: Future<Output = Result<Message, crate::DiscordError>> + 'a
    where
        Self: 'a;

    /// Future type for `edit_message`
    type EditMessageFut<'a>: Future<Output = Result<Message, crate::DiscordError>> + 'a
    where
        Self: 'a;

    /// Future type for `edit_message_with_attachments`
    type EditMessageWithAttachmentsFut<'a>: Future<Output = Result<Message, crate::DiscordError>> + 'a
    where
        Self: 'a;

    /// Send a message to a Discord channel with optional attachments
    fn send_message<'a>(
        &'a self,
        channel_id: &'a str,
        message: &'a DiscordMessage,
    ) -> Self::SendMessageFut<'a>;

    /// Edit an existing message (content/embeds). Omit fields to leave unchanged.
    fn edit_message<'a>(
        &'a self,
        channel_id: &'a str,
        message_id: &'a str,
        edit: &'a DiscordMessageEdit,
    ) -> Self::EditMessageFut<'a>;

    /// Edit a message and add new file attachments (existing attachments are preserved by default).
    fn edit_message_with_attachments<'a>(
        &'a self,
        channel_id: &'a str,
        message_id: &'a str,
        edit: &'a DiscordMessageEdit,
        attachments: &'a [AttachmentInput],
    ) -> Self::EditMessageWithAttachmentsFut<'a>;

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

/// Payload for editing a message. Attachments are intentionally omitted to avoid accidental removal.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordMessageEdit {
    pub content: Option<String>,
    pub embeds: Option<Vec<TwEmbed>>,
}
