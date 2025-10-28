use crate::{
    AttachmentInput, DiscordClient, DiscordError, DiscordMessage, DiscordRateLimitResponse,
    BASE_URL,
};
use core::future::Future;
use core::pin::Pin;
use gloo_net::http::Request;
use tracing::{error, info, warn};
use twilight_model::channel::Message;
use worker_stack::js_sys;
use worker_stack::wasm_bindgen::JsValue;
use worker_stack::web_sys::{Blob, BlobPropertyBag, FormData};

/// WASM Discord bot client using gloo-net (for cnft.dev-workers)
pub struct WasmDiscordClient {
    bot_token: String,
}

impl WasmDiscordClient {
    pub fn new(bot_token: String) -> Self {
        Self { bot_token }
    }
}

impl DiscordClient for WasmDiscordClient {
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
            info!("🔗 Sending Discord message with WASM client");
            let url = format!("{BASE_URL}/channels/{channel_id}/messages",);

            // Check if we have attachments to send
            if let Some(attachments) = &message.attachments {
                if !attachments.is_empty() {
                    info!(
                        "📎 Detected {} attachments, switching to multipart mode",
                        attachments.len()
                    );
                    return self
                        .send_multipart_message(&url, message, attachments)
                        .await;
                }
            }
            let request = Request::post(&url)
                .header("Authorization", &format!("Bot {}", self.bot_token))
                .header("User-Agent", "defrag-discord-client/1.0")
                .header("Content-Type", "application/json")
                .json(message)
                .map_err(|e| DiscordError::Gloo(format!("Request creation failed: {e:?}")))?;

            let response = request
                .send()
                .await
                .map_err(|e| DiscordError::Gloo(format!("Request failed: {e:?}")))?;

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
            info!("✏️ Editing Discord message (WASM)");
            let url = format!(
                "https://discord.com/api/v10/channels/{}/messages/{}",
                channel_id, message_id
            );

            let request = Request::patch(&url)
                .header("Authorization", &format!("Bot {}", self.bot_token))
                .header("User-Agent", "defrag-discord-client/1.0")
                .header("Content-Type", "application/json")
                .json(edit)
                .map_err(|e| DiscordError::Gloo(format!("Edit request creation failed: {e:?}")))?;

            let response = request
                .send()
                .await
                .map_err(|e| DiscordError::Gloo(format!("Edit request failed: {e:?}")))?;

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
            info!("✏️ Editing Discord message with new attachments (WASM)");
            let url = format!(
                "https://discord.com/api/v10/channels/{}/messages/{}",
                channel_id, message_id
            );

            // Build FormData
            let form_data = FormData::new()
                .map_err(|_| DiscordError::Gloo("Failed to create FormData".to_string()))?;

            // Add files
            for (index, attachment) in attachments.iter().enumerate() {
                Self::validate_attachment(&attachment.file_data, &attachment.filename)?;

                let uint8_array =
                    js_sys::Uint8Array::new_with_length(attachment.file_data.len() as u32);
                uint8_array.copy_from(&attachment.file_data);

                let blob_options = BlobPropertyBag::new();
                blob_options.set_type(Self::get_content_type(&attachment.filename));

                let blob = Blob::new_with_u8_array_sequence_and_options(
                    &js_sys::Array::of1(&uint8_array),
                    &blob_options,
                )
                .map_err(|_| DiscordError::Gloo("Failed to create Blob".to_string()))?;

                form_data
                    .append_with_blob_and_filename(
                        &format!("files[{index}]"),
                        &blob,
                        &attachment.filename,
                    )
                    .map_err(|_| DiscordError::Gloo("Failed to append file".to_string()))?;
            }

            // Add JSON payload
            let payload = serde_json::to_string(edit)?;
            form_data
                .append_with_str("payload_json", &payload)
                .map_err(|_| DiscordError::Gloo("Failed to append payload_json".to_string()))?;

            let request = Request::patch(&url)
                .header("Authorization", &format!("Bot {}", self.bot_token))
                .header("User-Agent", "defrag-discord-client/1.0")
                .body(JsValue::from(form_data))
                .map_err(|e| {
                    DiscordError::Gloo(format!("Multipart edit request creation failed: {e:?}"))
                })?;

            let response = request
                .send()
                .await
                .map_err(|e| DiscordError::Gloo(format!("Multipart edit request failed: {e:?}")))?;

            self.handle_message_response(response).await
        })
    }
}

impl WasmDiscordClient {
    async fn send_multipart_message(
        &self,
        url: &str,
        message: &DiscordMessage,
        attachments: &[AttachmentInput],
    ) -> Result<Message, DiscordError> {
        info!(
            "🔧 Sending multipart message with {} attachments",
            attachments.len()
        );

        // Create FormData for multipart request
        let form_data = FormData::new().map_err(|e| {
            error!("❌ Failed to create FormData: {e:?}");
            DiscordError::Gloo("Failed to create FormData".to_string())
        })?;

        // Add files
        for (index, attachment) in attachments.iter().enumerate() {
            Self::validate_attachment(&attachment.file_data, &attachment.filename)?;

            // Create Uint8Array from binary data
            let uint8_array =
                js_sys::Uint8Array::new_with_length(attachment.file_data.len() as u32);
            uint8_array.copy_from(&attachment.file_data);

            // Create blob options
            let blob_options = BlobPropertyBag::new();
            blob_options.set_type(Self::get_content_type(&attachment.filename));

            // Create Blob from image data
            let blob = Blob::new_with_u8_array_sequence_and_options(
                &js_sys::Array::of1(&uint8_array),
                &blob_options,
            )
            .map_err(|e| {
                error!("❌ Failed to create Blob for attachment {index}: {e:?}");
                DiscordError::Gloo(format!("Failed to create Blob for attachment {index}"))
            })?;

            // Add file to form data
            form_data
                .append_with_blob_and_filename(
                    &format!("files[{index}]"),
                    &blob,
                    &attachment.filename,
                )
                .map_err(|e| {
                    error!("❌ Failed to append file {index} to FormData: {e:?}");
                    DiscordError::Gloo(format!("Failed to append file {index}"))
                })?;
        }

        // Add JSON payload
        let payload = serde_json::to_string(message).map_err(|e| {
            error!("❌ Failed to serialize message: {e:?}");
            DiscordError::Serialization(e)
        })?;
        form_data
            .append_with_str("payload_json", &payload)
            .map_err(|e| {
                error!("❌ Failed to append payload_json: {e:?}");
                DiscordError::Gloo("Failed to append payload_json".to_string())
            })?;

        // Send multipart request
        let request = Request::post(url)
            .header("Authorization", &format!("Bot {}", self.bot_token))
            .header("User-Agent", "defrag-discord-client/1.0")
            .body(JsValue::from(form_data))
            .map_err(|e| {
                error!("❌ Failed to create multipart request: {e:?}");
                DiscordError::Gloo(format!("Multipart request creation failed: {e:?}"))
            })?;

        let response = request.send().await.map_err(|e| {
            error!("❌ Multipart request send failed: {e:?}");
            DiscordError::Gloo(format!("Multipart request failed: {e:?}"))
        })?;

        self.handle_message_response(response).await
    }

    async fn handle_message_response(
        &self,
        response: gloo_net::http::Response,
    ) -> Result<Message, DiscordError> {
        let status = response.status();

        if response.ok() {
            info!("✅ Discord message sent successfully");
            let response_text = response
                .text()
                .await
                .map_err(|e| DiscordError::Gloo(format!("Failed to get response text: {e:?}")))?;

            let message_response: Message = serde_json::from_str(&response_text)
                .map_err(|e| DiscordError::Gloo(format!("Failed to parse response JSON: {e:?}")))?;
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
