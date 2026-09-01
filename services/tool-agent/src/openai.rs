//! Adapter letting an `openai_client::OpenAI` drive the loop.
//!
//! Behind the `openai` feature. The mapping is mechanical — this crate's
//! provider-neutral vocabulary onto the chat-completions wire — and lives here
//! rather than in `openai-client` so that crate stays a client and doesn't
//! grow an opinion about loops.

use crate::{AgentError, ChatModel, Message, ModelTurn, ToolCall, ToolDef, Usage};
use openai_client::{OpenAI, RequestMessage, RequestToolCall, ToolSpec};

impl ChatModel for OpenAI {
    async fn turn(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<ModelTurn, AgentError> {
        let request = self.request(
            messages.iter().map(to_wire).collect(),
            tools
                .iter()
                .map(|tool| {
                    ToolSpec::function(
                        tool.name.as_str(),
                        tool.description.as_str(),
                        tool.json_schema(),
                    )
                })
                .collect(),
        );

        let response = self
            .chat(&request)
            .await
            .map_err(|err| AgentError::Model(format!("{err:?}")))?;

        let usage = Usage::new(
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
        )
        .with_details(
            response.usage.prompt_tokens_details.cached_tokens,
            response.usage.completion_tokens_details.reasoning_tokens,
        );

        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AgentError::Model("provider returned no choices".to_string()))?;

        Ok(ModelTurn {
            // Whitespace-only content is treated as absent: a turn that only
            // asks for tools sometimes carries an empty string rather than
            // null, and passing that through would show the user a blank reply.
            content: choice
                .message
                .content
                .filter(|text| !text.trim().is_empty()),
            calls: choice
                .message
                .tool_calls
                .into_iter()
                .map(|call| ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments: call.function.arguments,
                })
                .collect(),
            usage,
        })
    }
}

fn to_wire(message: &Message) -> RequestMessage {
    match message {
        Message::System(text) => RequestMessage::system(text.as_str()),
        Message::User(text) => RequestMessage::user(text.as_str()),
        Message::Assistant { content, calls } => RequestMessage::assistant(
            content.clone(),
            calls
                .iter()
                .map(|call| {
                    RequestToolCall::function(
                        call.id.as_str(),
                        call.name.as_str(),
                        call.arguments.as_str(),
                    )
                })
                .collect(),
        ),
        Message::ToolResult { call_id, content } => {
            RequestMessage::tool(call_id.as_str(), content.as_str())
        }
    }
}
