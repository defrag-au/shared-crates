//! Portable client for the OpenAI chat-completions wire format.
//!
//! The wire format is spoken by more than OpenAI — x.ai and DeepSeek both
//! serve it — so [`OpenAI::with_base_url`] is how you point this at a
//! different provider rather than a reason to write another client.
//!
//! Supports tool calling: offer [`ToolSpec`]s on a [`ChatRequest`], read
//! [`ToolCall`]s back off the response, and feed results in as
//! [`RequestMessage::tool`]. Driving that exchange as a loop is the
//! `tool-agent` crate's job, not this one's — here there is only one
//! request and one response.

mod api;
pub mod err;

use err::OpenAiError;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

pub struct OpenAI {
    api: api::Api,
    model: String,
    temperature: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiResponse {
    pub choices: Vec<ResponseChoice>,

    /// Absent on some providers and on error shapes, so it defaults rather
    /// than failing the whole deserialize — a missing token count is worth
    /// less than the response it would have thrown away.
    #[serde(default)]
    pub usage: Usage,
}

/// Token counts for one request.
///
/// **The two breakdowns relate to their totals differently, and it is not
/// symmetric.** Observed against x.ai:
///
/// - `cached_tokens` is a SUBSET of `prompt_tokens`. Billed input is
///   `prompt_tokens - cached_tokens` at the full rate plus `cached_tokens` at
///   the cached rate.
/// - `reasoning_tokens` is ADDITIONAL to `completion_tokens`, not part of it —
///   a turn reporting 74 completion and 320 reasoning spent 394 output tokens.
///   x.ai bills them as separate line items.
///
/// Asserted from data rather than from the schema: OpenAI documents reasoning
/// as a subset, so a provider that reports more reasoning than completion is
/// the evidence that settles it. Anything computing a bill must not assume the
/// two behave alike.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,

    /// Nested on the wire; flattened here because callers want the number, not
    /// the shape. Absent from providers that don't report it, which reads as
    /// zero — indistinguishable from a genuine miss, and the right default
    /// either way since neither is worth failing a response over.
    #[serde(default)]
    pub prompt_tokens_details: TokenDetails,
    #[serde(default)]
    pub completion_tokens_details: TokenDetails,
}

/// The subset breakdowns providers nest under a total.
///
/// One type for both because the two fields never co-occur — a prompt has no
/// reasoning and a completion has no cache — and a second near-identical struct
/// would be two things to keep in step for no gain.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct TokenDetails {
    /// Of `prompt_tokens`, how many were served from cache. Billed at a small
    /// fraction of the uncached rate, so this is most of what separates a cheap
    /// request from an expensive one of the same size.
    #[serde(default)]
    pub cached_tokens: u32,
    /// Reasoning tokens, IN ADDITION to `completion_tokens` — see the type
    /// docs. Billed as output and routinely several times the visible reply, so
    /// a cost estimate that ignores it is not close.
    #[serde(default)]
    pub reasoning_tokens: u32,
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

/// One message on the request side of the wire.
///
/// `content` is optional because an assistant turn that only asks for tools
/// carries none, and `tool_calls` / `tool_call_id` are the two halves of a
/// call-and-result pair. Build these with the constructors rather than by
/// hand — the role strings are wire constants, not free text.
#[derive(Debug, Clone, Serialize)]
pub struct RequestMessage {
    pub role: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<RequestToolCall>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl RequestMessage {
    fn of_role(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::of_role("system", content)
    }

    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::of_role("user", content)
    }

    /// The model's own turn, replayed back. Carries whatever text it produced
    /// plus any calls it asked for — both may be present in the same turn.
    #[must_use]
    pub fn assistant(content: Option<String>, tool_calls: Vec<RequestToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            tool_calls,
            tool_call_id: None,
        }
    }

    /// One tool's result. `call_id` must match the `id` of the call it answers
    /// — providers reject a tool message that answers nothing.
    #[must_use]
    pub fn tool(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id.into()),
        }
    }
}

/// A call being replayed back to the model in an assistant turn.
#[derive(Debug, Clone, Serialize)]
pub struct RequestToolCall {
    pub id: String,

    #[serde(rename = "type")]
    pub kind: &'static str,

    pub function: RequestFunctionCall,
}

impl RequestToolCall {
    #[must_use]
    pub fn function(id: impl Into<String>, name: impl Into<String>, arguments: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: "function",
            function: RequestFunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestFunctionCall {
    pub name: String,
    /// A JSON object, as a string. That is the wire shape, not an oversight.
    pub arguments: String,
}

/// A tool offered to the model.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    pub kind: &'static str,

    pub function: FunctionSpec,
}

impl ToolSpec {
    #[must_use]
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            kind: "function",
            function: FunctionSpec {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments object.
    pub parameters: serde_json::Value,
}

/// A complete chat-completions request.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<RequestMessage>,

    /// Omitted entirely when empty. An empty `tools` array is not the same as
    /// no `tools` key to every provider, and sending one is how you tell the
    /// model it may no longer call anything.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
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
    pub async fn chat_completion(&self, messages: &[(&str, &str)]) -> Result<String, OpenAiError> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages: messages
                .iter()
                .map(|(role, content)| RequestMessage::of_role(role, *content))
                .collect(),
            tools: Vec::new(),
            temperature: Some(self.temperature),
        };

        let response = self.execute_request(&request).await?;
        response.get_content().ok_or(OpenAiError::Unknown)
    }

    /// Send a request built by the caller and hand back the whole response.
    ///
    /// The tool-calling entry point: unlike [`OpenAI::chat_completion`] this
    /// does not reduce the response to its text, because a turn that asks for
    /// tools has no text to reduce to.
    pub async fn chat(&self, request: &ChatRequest) -> Result<OpenAiResponse, OpenAiError> {
        self.execute_request(request).await
    }

    /// Build a request against this client's configured model, so callers
    /// don't restate it (and can't drift from it).
    #[must_use]
    pub fn request(&self, messages: Vec<RequestMessage>, tools: Vec<ToolSpec>) -> ChatRequest {
        ChatRequest {
            model: self.model.clone(),
            messages,
            tools,
            temperature: Some(self.temperature),
        }
    }

    async fn execute_request(
        &self,
        request: &ChatRequest,
    ) -> Result<OpenAiResponse, OpenAiError> {
        match self
            .api
            .post_with_details::<_, OpenAiResponse>("/chat/completions", request)
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

#[cfg(test)]
mod usage_tests {
    use super::*;

    /// The breakdowns are NESTED on the wire. Reading them off the top level
    /// would silently yield zero — indistinguishable from a provider that
    /// doesn't report them, and the whole point is telling those apart.
    #[test]
    fn the_nested_breakdowns_are_read() {
        let usage: Usage = serde_json::from_str(
            r#"{
                "prompt_tokens": 13431,
                "completion_tokens": 869,
                "prompt_tokens_details": {"cached_tokens": 7070},
                "completion_tokens_details": {"reasoning_tokens": 800}
            }"#,
        )
        .expect("parses");

        assert_eq!(usage.prompt_tokens, 13431);
        assert_eq!(usage.prompt_tokens_details.cached_tokens, 7070);
        assert_eq!(usage.completion_tokens_details.reasoning_tokens, 800);
    }

    /// A provider reporting neither must still parse. Both default to zero,
    /// which reads as "nothing cached, nothing reasoned" — wrong only in the
    /// direction that over-states cost, and never worth failing a response
    /// that already carries an answer.
    #[test]
    fn a_response_without_breakdowns_still_parses() {
        let usage: Usage =
            serde_json::from_str(r#"{"prompt_tokens": 100, "completion_tokens": 20}"#)
                .expect("parses");

        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.prompt_tokens_details.cached_tokens, 0);
        assert_eq!(usage.completion_tokens_details.reasoning_tokens, 0);
    }
}
