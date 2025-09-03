use crate::{DiscordError, DiscordMessage, DiscordRateLimitResponse, DiscordWebhookClient};
use reqwest::multipart;
use tracing::{debug, error, info, warn};

/// Native Discord client using reqwest (for augminted-bots)
pub struct NativeDiscordClient {
    client: reqwest::Client,
}

impl NativeDiscordClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for NativeDiscordClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscordWebhookClient for NativeDiscordClient {
    async fn send_webhook(
        &self,
        webhook_url: &str,
        message: &DiscordMessage,
    ) -> Result<(), DiscordError> {
        info!("🔗 Sending Discord webhook with native client");

        // Check if we have attachments to send
        if let Some(attachments) = &message.attachments {
            if !attachments.is_empty() {
                debug!("📎 Sending {} attachments via multipart", attachments.len());
                return self.send_multipart_webhook(webhook_url, message, attachments).await;
            }
        }

        // No attachments - send as JSON
        debug!("📄 Sending JSON-only webhook");
        let response = self
            .client
            .post(webhook_url)
            .header("User-Agent", "defrag-discord-client/1.0")
            .json(message)
            .send()
            .await?;

        self.handle_response(response).await
    }
}

impl NativeDiscordClient {
    async fn send_multipart_webhook(
        &self,
        webhook_url: &str,
        message: &DiscordMessage,
        attachments: &[crate::DiscordAttachment],
    ) -> Result<(), DiscordError> {
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
            .post(webhook_url)
            .header("User-Agent", "defrag-discord-client/1.0")
            .multipart(form)
            .send()
            .await?;

        self.handle_response(response).await
    }

    async fn handle_response(&self, response: reqwest::Response) -> Result<(), DiscordError> {
        let status = response.status();

        if response.status().is_success() {
            info!("✅ Discord webhook sent successfully");
            Ok(())
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
            error!("❌ Discord webhook error {}: {}", status, error_text);
            Err(DiscordError::Request(format!(
                "Discord webhook error {}: {}",
                status, error_text
            )))
        }
    }
}