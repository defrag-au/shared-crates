use crate::DiscordError;

// The pre-`MessageBody` surface. Behind the `twilight` feature because it is
// the only thing here that names a twilight type — see the crate docs.
#[cfg(feature = "twilight")]
use crate::{
    AttachmentInput, DiscordClient, DiscordMessage, DiscordRateLimitResponse, BASE_URL,
};
#[cfg(feature = "twilight")]
use core::future::Future;
#[cfg(feature = "twilight")]
use core::pin::Pin;
#[cfg(feature = "twilight")]
use reqwest::multipart;
#[cfg(feature = "twilight")]
use tracing::{debug, error, info, warn};
#[cfg(feature = "twilight")]
use twilight_model::channel::Message;

/// Discord bot client over `reqwest`.
///
/// Despite the name, `reqwest` also compiles for `wasm32-unknown-unknown` —
/// this is the stack augminted-bots' workers already use. Pick by which HTTP
/// client the rest of your crate links, not by target.
pub struct NativeDiscordClient {
    client: reqwest::Client,
    bot_token: String,
    rate_limits: crate::ratelimit::RateLimitTracker,
}

impl NativeDiscordClient {
    pub fn new(bot_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            bot_token,
            rate_limits: crate::ratelimit::RateLimitTracker::new(),
        }
    }
}

impl crate::DiscordOutbound for NativeDiscordClient {
    async fn execute(
        &self,
        request: crate::HttpRequest,
    ) -> Result<crate::HttpResponse, DiscordError> {
        let method = match request.method {
            crate::Method::Post => reqwest::Method::POST,
            crate::Method::Patch => reqwest::Method::PATCH,
        };

        let mut builder = self.client.request(method, &request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }

        let response = builder.body(request.body).send().await?;
        let status = response.status().as_u16();
        // Kept verbatim rather than parsed: a Cloudflare 1015 body is HTML, and
        // an error that fails to parse must still be reportable.
        let body = response.text().await.unwrap_or_default();

        Ok(crate::HttpResponse { status, body })
    }

    fn now_ms(&self) -> u64 {
        #[cfg(target_arch = "wasm32")]
        {
            // `SystemTime::now()` panics on wasm32-unknown-unknown.
            js_sys::Date::now() as u64
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        }
    }

    fn bot_token(&self) -> Option<&str> {
        Some(&self.bot_token)
    }

    fn rate_limits(&self) -> &crate::ratelimit::RateLimitTracker {
        &self.rate_limits
    }

    async fn sleep_ms(&self, millis: u64) {
        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
        // On wasm there is no portable sleep at this layer — a worker has
        // `Delay`, a browser has `setTimeout`, and this crate has neither
        // without picking one. Skipping the wait means the request goes out and
        // may come back 429, which the caller sees. That is the honest
        // degradation: a visible rate limit beats a silent hang.
        #[cfg(target_arch = "wasm32")]
        let _ = millis;
    }
}

#[cfg(feature = "twilight")]
impl DiscordClient for NativeDiscordClient {
    type SendMessageFut<'a>
        = Pin<Box<dyn Future<Output = Result<Message, DiscordError>> + 'a>>
    where
        Self: 'a;
    type EditMessageFut<'a>
        = Pin<Box<dyn Future<Output = Result<Message, DiscordError>> + 'a>>
    where
        Self: 'a;
    type EditMessageWithAttachmentsFut<'a>
        = Pin<Box<dyn Future<Output = Result<Message, DiscordError>> + 'a>>
    where
        Self: 'a;

    fn send_message<'a>(
        &'a self,
        channel_id: &'a str,
        message: &'a DiscordMessage,
    ) -> Self::SendMessageFut<'a> {
        Box::pin(async move {
            info!("🔗 Sending Discord message with native client");

            let url = format!("{BASE_URL}/channels/{}/messages", channel_id);

            // Check if we have attachments to send
            if let Some(attachments) = &message.attachments {
                if !attachments.is_empty() {
                    debug!("📎 Sending {} attachments via multipart", attachments.len());
                    return self
                        .send_multipart_message(&url, message, attachments)
                        .await;
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

    fn edit_message<'a>(
        &'a self,
        channel_id: &'a str,
        message_id: &'a str,
        edit: &'a crate::DiscordMessageEdit,
    ) -> Self::EditMessageFut<'a> {
        Box::pin(async move {
            info!("✏️ Editing Discord message (native)");
            let url = format!(
                "https://discord.com/api/v10/channels/{}/messages/{}",
                channel_id, message_id
            );

            let response = self
                .client
                .patch(&url)
                .header("Authorization", format!("Bot {}", self.bot_token))
                .header("User-Agent", "defrag-discord-client/1.0")
                .json(edit)
                .send()
                .await?;

            self.handle_message_response(response).await
        })
    }

    fn edit_message_with_attachments<'a>(
        &'a self,
        channel_id: &'a str,
        message_id: &'a str,
        edit: &'a crate::DiscordMessageEdit,
        attachments: &'a [AttachmentInput],
    ) -> Self::EditMessageWithAttachmentsFut<'a> {
        Box::pin(async move {
            info!("✏️ Editing Discord message with new attachments (native)");
            let url = format!(
                "https://discord.com/api/v10/channels/{}/messages/{}",
                channel_id, message_id
            );

            let mut form = multipart::Form::new();

            // Add new files
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

            // Add JSON payload for edit
            let payload = serde_json::to_string(edit)?;
            form = form.text("payload_json", payload);

            let response = self
                .client
                .patch(&url)
                .header("Authorization", format!("Bot {}", self.bot_token))
                .header("User-Agent", "defrag-discord-client/1.0")
                .multipart(form)
                .send()
                .await?;

            self.handle_message_response(response).await
        })
    }
}

#[cfg(feature = "twilight")]
impl NativeDiscordClient {
    async fn send_multipart_message(
        &self,
        url: &str,
        message: &DiscordMessage,
        attachments: &[AttachmentInput],
    ) -> Result<Message, DiscordError> {
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

    async fn handle_message_response(
        &self,
        response: reqwest::Response,
    ) -> Result<Message, DiscordError> {
        let status = response.status();

        if response.status().is_success() {
            info!("✅ Discord message sent successfully");
            let message_response: Message = response.json().await?;
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
