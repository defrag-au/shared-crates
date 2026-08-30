//! The model side: conversation shape, one turn of inference, token counts.

use crate::{ToolCall, ToolDef};

/// One message in the conversation the loop maintains.
///
/// A deliberately small vocabulary — four roles is all a tool-calling loop
/// needs, and keeping it provider-neutral is what lets the same loop run
/// against anything with a [`ChatModel`] impl.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Standing instructions. First, and stable — anything volatile in here
    /// (a timestamp, a guild id) invalidates the provider's prompt cache on
    /// every single request. Resolve that kind of thing inside a tool instead.
    System(String),

    User(String),

    /// The model's own turn, replayed back to it. Text and calls can both be
    /// present: a model often says what it's about to do and then does it.
    Assistant {
        content: Option<String>,
        calls: Vec<ToolCall>,
    },

    /// One tool's result. `call_id` matches the [`ToolCall::id`] it answers.
    ToolResult { call_id: String, content: String },
}

/// What the model produced in one turn.
///
/// Empty `calls` is how the loop knows the model is done — that, and not the
/// presence of text, is the terminating condition.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelTurn {
    pub content: Option<String>,
    pub calls: Vec<ToolCall>,
    pub usage: Usage,
}

impl ModelTurn {
    /// A final answer, with no calls.
    #[must_use]
    pub fn answer(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            calls: Vec::new(),
            usage: Usage::default(),
        }
    }

    /// A turn that asks for tools.
    #[must_use]
    pub fn calling(calls: Vec<ToolCall>) -> Self {
        Self {
            content: None,
            calls,
            usage: Usage::default(),
        }
    }

    #[must_use]
    pub fn with_usage(mut self, usage: Usage) -> Self {
        self.usage = usage;
        self
    }
}

/// Token counts, accumulated across every turn in a run.
///
/// Cached-prompt tokens are counted in `prompt_tokens` like any other — the
/// providers report them separately and inconsistently, and a consumer that
/// needs the split should read it from its own client rather than have this
/// crate pick a convention.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// Of `prompt_tokens`, how many were served from cache — a SUBSET, not an
    /// addition.
    ///
    /// Carried because it is most of the cost: cached input bills at a small
    /// fraction of uncached, so two requests of identical size can differ
    /// several-fold. Without it a token count cannot tell a cheap turn from an
    /// expensive one, and a change that quietly broke cache-prefix stability
    /// would show up nowhere.
    pub cached_prompt_tokens: u32,
    /// Of `completion_tokens`, how many were reasoning — again a SUBSET.
    ///
    /// Billed as output and routinely an order of magnitude larger than the
    /// visible reply, so a run reporting "69 out" can have spent nearer 870.
    pub reasoning_tokens: u32,
}

impl Usage {
    #[must_use]
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            cached_prompt_tokens: 0,
            reasoning_tokens: 0,
        }
    }

    /// The breakdowns, when the provider reported them.
    ///
    /// Separate from [`Self::new`] so a provider that reports neither keeps the
    /// two-argument constructor rather than passing zeros it does not know.
    #[must_use]
    pub fn with_details(mut self, cached_prompt_tokens: u32, reasoning_tokens: u32) -> Self {
        self.cached_prompt_tokens = cached_prompt_tokens;
        self.reasoning_tokens = reasoning_tokens;
        self
    }

    /// Saturating so a runaway loop reports a large number rather than
    /// wrapping to a small one — a wrong bill is better than a silent one.
    pub fn add(&mut self, other: Self) {
        self.prompt_tokens = self.prompt_tokens.saturating_add(other.prompt_tokens);
        self.completion_tokens = self.completion_tokens.saturating_add(other.completion_tokens);
        self.cached_prompt_tokens = self
            .cached_prompt_tokens
            .saturating_add(other.cached_prompt_tokens);
        self.reasoning_tokens = self.reasoning_tokens.saturating_add(other.reasoning_tokens);
    }

    /// Prompt tokens billed at the full rate — the total less what was cached.
    ///
    /// Saturating rather than asserting: a provider reporting more cached than
    /// prompt tokens is nonsense, but nonsense in a usage field should not
    /// panic a run that has already produced an answer.
    #[must_use]
    pub fn uncached_prompt_tokens(&self) -> u32 {
        self.prompt_tokens.saturating_sub(self.cached_prompt_tokens)
    }

    #[must_use]
    pub fn total(&self) -> u32 {
        self.prompt_tokens.saturating_add(self.completion_tokens)
    }
}

/// One turn of inference.
///
/// `tools` is empty on the final forced-answer turn — an implementation must
/// treat that as "offer nothing", not "offer everything".
#[allow(async_fn_in_trait)] // Consumers are single-threaded (WASM workers); a Send bound would be a lie.
pub trait ChatModel {
    async fn turn(&self, messages: &[Message], tools: &[ToolDef])
        -> Result<ModelTurn, AgentError>;
}

/// Why a run could not continue.
///
/// Only model-side failures land here. A tool that fails is not an error —
/// see [`crate::ToolOutcome::error`].
#[derive(Debug, Clone, PartialEq)]
pub enum AgentError {
    /// The provider call failed, or returned something unusable.
    Model(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(detail) => write!(f, "model call failed: {detail}"),
        }
    }
}

impl std::error::Error for AgentError {}
