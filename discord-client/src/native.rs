use crate::{DiscordError, DiscordMessage, DiscordMessageResponse, DiscordRateLimitResponse, DiscordClient};
use reqwest::multipart;
use tracing::{debug, error, info, warn};
use core::future::Future;
use core::pin::Pin;

/// Native Discord bot client using reqwest (for augminted-bots)
pub struct NativeDiscordClient {
    client: reqwest::Client,
    bot_token: String,
}

impl NativeDiscordClient {
    pub fn new(bot_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            bot_token,
        }
    }
}

impl DiscordClient for NativeDiscordClient {
    type SendMessageFut<'a> = Pin<Box<dyn Future<Output = Result<DiscordMessageResponse, DiscordError>> + 'a>> where Self: 'a;

    fn send_message<'a>(
        &'a self,
        channel_id: &'a str,
        message: &'a DiscordMessage,
    ) -> Self::SendMessageFut<'a> {
        Box::pin(async move {
            info!("🔗 Sending Discord message with native client");

            let url = format!("https://discord.com/api/v10/channels/{}/messages", channel_id);

            // Check if we have attachments to send
            if let Some(attachments) = &message.attachments {
                if !attachments.is_empty() {
                    debug!("📎 Sending {} attachments via multipart", attachments.len());
                    return self.send_multipart_message(&url, message, attachments).await;
                }
            }

            // No attachments - send as JSON
            debug!("📄 Sending JSON-only message");
            let response = self
                .client
                .post(&url)
                .header("Authorization", format!("Bot {}", self.bot_token))
                .header("User-Agent", "defrag-discord-client/1.0")
                .json(message)
                .send()
                .await?;

            self.handle_message_response(response).await
        })
    }
}

impl NativeDiscordClient {
    async fn send_multipart_message(
        &self,
        url: &str,
        message: &DiscordMessage,
        attachments: &[crate::DiscordAttachment],
    ) -> Result<DiscordMessageResponse, DiscordError> {
        let mut form = multipart::Form::new();

        // Add files
        for (index, attachment) in attachments.iter().enumerate() {
            Self::validate_attachment(&attachment.file_data, &attachment.filename)?;
            
            form = form.part(
                format!("files[{index}]"),
                multipart::Part::bytes(attachment.file_data.clone())
                    .file_name(attachment.filename.clone())
                    .mime_str(Self::get_content_type(&attachment.filename))
                    .map_err(|e| DiscordError::Request(format!("Invalid mime type: {e}")))?,
            );
        }

        // Add JSON payload
        let payload = serde_json::to_string(message)?;
        form = form.text("payload_json", payload);

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bot {}", self.bot_token))
            .header("User-Agent", "defrag-discord-client/1.0")
            .multipart(form)
            .send()
            .await?;

        self.handle_message_response(response).await
    }

    async fn handle_message_response(&self, response: reqwest::Response) -> Result<DiscordMessageResponse, DiscordError> {
        let status = response.status();

        if response.status().is_success() {
            info!("✅ Discord message sent successfully");
            let message_response: DiscordMessageResponse = response.json().await?;
            Ok(message_response)
        } else if status == 429 {
            match response.json::<DiscordRateLimitResponse>().await {
                Ok(rate_limit) => {
                    warn!(
                        "⏱️ Discord rate limited: retry after {:.2}s (global: {})",
                        rate_limit.retry_after, rate_limit.global
                    );
                    Err(DiscordError::RateLimited {
                        retry_after: rate_limit.retry_after,
                        global: rate_limit.global,
                    })
                }
                Err(_) => {
                    warn!("⏱️ Discord rate limited but couldn't parse response");
                    Err(DiscordError::RateLimited {
                        retry_after: 1.0,
                        global: false,
                    })
                }
            }
        } else {
            let error_text = response.text().await.unwrap_or_default();
            error!("❌ Discord API error {}: {}", status, error_text);
            Err(DiscordError::Request(format!(
                "Discord API error {}: {}",
                status, error_text
            )))
        }
    }
}
