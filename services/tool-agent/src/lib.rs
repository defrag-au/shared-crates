//! The tool-calling loop: offer a model some tools, run whatever it asks for,
//! feed the results back, repeat until it answers.
//!
//! ## Why this is its own crate
//!
//! Two consumers want this loop over the same tool definitions but reach them
//! completely differently. Augie aggregates tools from every plugin service
//! and executes them over HTTP with a Discord identity attached; a service's
//! own front-end drives only its own tools, in-process, with a session
//! identity. Neither is a special case of the other, and neither wants the
//! other's transport compiled in.
//!
//! So the loop takes two traits and owns no I/O:
//!
//! - [`ChatModel`] — one turn of inference. Implemented for
//!   `openai_client::OpenAI` behind the `openai` feature.
//! - [`ToolExecutor`] — actually running a call. Always the consumer's.
//!
//! Which also means the loop is testable with no network at all: script a
//! [`ChatModel`], stub a [`ToolExecutor`], and assert on what got called.
//!
//! ## Three decisions worth knowing
//!
//! **The round cap is a cost gate, not a safety net.** Every round is a model
//! call plus a tool call, and a model that keeps asking for one more lookup
//! will happily spend your budget doing it. [`Limits::max_tool_rounds`] bounds
//! that, and it is deliberately small.
//!
//! **Running out of rounds still produces an answer.** When the cap is hit the
//! loop makes one final turn with the tool list *empty*, so the model has to
//! answer from what it already gathered. An agent that goes silent because it
//! ran out of budget reads as broken; one that says "here's what I found so
//! far" reads as honest.
//!
//! **A failing tool is not a failing run.** [`ToolOutcome::error`] rides back
//! to the model as ordinary result content, because "that collection has no
//! trait called `ghoul`" is something the model can recover from — by asking,
//! or by trying a different value. Only [`ChatModel`] failures abort.
//!
//! ## What this crate deliberately does not do
//!
//! No streaming, no conversation persistence, no retries. Persistence in
//! particular belongs to the consumer: what a Discord agent stores between
//! turns (the resolved query, keyed by message id) has nothing in common with
//! what a web front-end stores, and guessing wrong here would be worse than
//! not guessing.

mod agent;
mod model;
mod tool;

#[cfg(feature = "openai")]
mod openai;

pub use agent::*;
pub use model::*;
pub use tool::*;
