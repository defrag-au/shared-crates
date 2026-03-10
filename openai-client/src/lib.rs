mod api;
pub mod err;

use err::OpenAiError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info};

pub struct OpenAI {
    api: api::Api,
    model: String,
    temperature: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiResponse {
    pub choices: Vec<ResponseChoice>,
}

impl OpenAiResponse {
    #[must_use]
    pub fn get_content(&self) -> Option<String> {
        self.choices
            .iter()
            .filter_map(|c| c.message.content.clone())
            .find(|s| !s.trim().is_empty())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseChoice {
    pub message: ResponseMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: Option<String>,

    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    pub id: String,

    #[serde(rename = "type")]
    pub type_: String,

    pub function: FunctionCall,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

impl OpenAI {
    #[must_use]
    pub fn for_api_key(api_key: &str) -> Self {
        Self {
            api: api::Api::for_api_key(api_key),
            model: "gpt-4o".to_string(),
            temperature: 0.7,
        }
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.api = self.api.with_base_url(base_url);
        self
    }

    #[must_use]
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Send a chat completion request with a list of messages.
    ///
    /// Each message is a (role, content) pair where role is "system", "user", or "assistant".
    pub async fn chat_completion(
        &self,
        messages: &[(&str, &str)],
    ) -> Result<String, OpenAiError> {
        let msgs: Vec<ChatMessage> = messages
            .iter()
            .map(|(role, content)| ChatMessage {
                role: role.to_string(),
                content: content.to_string(),
            })
            .collect();

        let request = json!({
            "model": self.model,
            "messages": msgs,
            "temperature": self.temperature,
        });

        let response = self.execute_request(request).await?;
        response
            .get_content()
            .ok_or(OpenAiError::Unknown)
    }

    async fn execute_request(
        &self,
        request: serde_json::Value,
    ) -> Result<OpenAiResponse, OpenAiError> {
        match self
            .api
            .post_with_details::<OpenAiResponse>("/chat/completions", &request)
            .await
        {
            Ok(response) => {
                if let Some(remaining) = response.rate_limit_remaining_requests() {
                    if remaining < 10 {
                        info!("Low OpenAI rate limit: {remaining} requests remaining");
                    }
                }
                Ok(response.data)
            }
            Err(err) => {
                if let Some(429) = err.status_code() {
                    let retry_after_seconds = err.retry_after_seconds();
                    let remaining_requests = err
                        .headers()
                        .and_then(|h| h.get("x-ratelimit-remaining-requests"))
                        .and_then(|v| v.parse::<u64>().ok());

                    if let Some(retry_after) = retry_after_seconds {
                        error!("OpenAI rate limit hit (429), retry after {retry_after}s");
                    } else {
                        error!("OpenAI rate limit hit (429), no retry-after header");
                    }

                    Err(OpenAiError::RateLimitExceeded {
                        retry_after_seconds,
                        remaining_requests,
                    })
                } else {
                    error!("OpenAI request failed: {err:?}");
                    Err(OpenAiError::ApiFailure(err))
                }
            }
        }
    }
}
