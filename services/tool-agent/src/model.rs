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
/// This used to carry the two totals only, on the reasoning that providers
/// report the breakdowns inconsistently and a consumer wanting the split should
/// get it from its own client. That was the wrong call: the split IS the cost —
/// a 98%-cached turn and a cold one of identical size differ several-fold — so
/// every consumer had to rebuild it, or more likely went without and read the
/// totals as if they meant something comparable.
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
    /// Reasoning tokens, IN ADDITION to `completion_tokens` — NOT a subset,
    /// unlike `cached_prompt_tokens`. A turn reporting 74 completion and 320
    /// reasoning spent 394 output tokens, and x.ai bills them separately.
    ///
    /// The asymmetry is the trap: the two breakdowns look alike and compose
    /// with their totals in opposite directions.
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

    /// Every token the request is billed for.
    ///
    /// Reasoning is added because it is not part of `completion_tokens`;
    /// cached is not, because it already is part of `prompt_tokens`. Getting
    /// that backwards under-counts a reasoning model by several times over.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.prompt_tokens
            .saturating_add(self.completion_tokens)
            .saturating_add(self.reasoning_tokens)
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

#[cfg(test)]
mod usage_tests {
    use super::Usage;

    /// The asymmetry, pinned. Cached is INSIDE the prompt total; reasoning is
    /// OUTSIDE the completion total. They look alike and compose in opposite
    /// directions, and getting it backwards silently under-counts a reasoning
    /// model several-fold.
    ///
    /// The numbers are a real turn: `13433 in (13184 cached) / 74 out (320
    /// reasoning)`.
    #[test]
    fn cached_is_inside_the_total_and_reasoning_is_outside_it() {
        let usage = Usage::new(13_433, 74).with_details(13_184, 320);

        // Billed at the full input rate: the prompt less what was cached.
        assert_eq!(usage.uncached_prompt_tokens(), 249);
        // Everything billed, so reasoning counts and cached does not double.
        assert_eq!(usage.total(), 13_433 + 74 + 320);
    }

    /// Summed across rounds, each part independently — a run's cache hit rate
    /// is only meaningful if the two halves accumulate together.
    #[test]
    fn rounds_accumulate_every_part() {
        let mut usage = Usage::new(100, 10).with_details(80, 40);
        usage.add(Usage::new(200, 20).with_details(150, 60));

        assert_eq!(usage.prompt_tokens, 300);
        assert_eq!(usage.cached_prompt_tokens, 230);
        assert_eq!(usage.completion_tokens, 30);
        assert_eq!(usage.reasoning_tokens, 100);
        assert_eq!(usage.uncached_prompt_tokens(), 70);
    }

    /// A provider reporting neither breakdown still totals correctly — the
    /// pre-existing behaviour, unchanged.
    #[test]
    fn a_provider_without_breakdowns_is_unaffected() {
        let usage = Usage::new(100, 10);

        assert_eq!(usage.uncached_prompt_tokens(), 100);
        assert_eq!(usage.total(), 110);
    }
}
