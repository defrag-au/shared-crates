//! Tools: what the model may call, and who runs it.

use serde::{Deserialize, Serialize};
pub use tool_schema::{ToolParameter, ToolParameterKind};

/// A tool the model may call.
///
/// Parameters are typed rather than raw JSON Schema, and converted to a schema
/// only where a provider needs one. See `tool_schema` for why: an untyped
/// schema on the wire can be the wrong shape in four different ways, none of
/// which anything notices until a model is offered a tool it cannot pass
/// arguments to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<ToolParameter>,
}

impl ToolDef {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Vec<ToolParameter>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }

    /// The JSON Schema a provider is sent for this tool.
    #[must_use]
    pub fn json_schema(&self) -> serde_json::Value {
        tool_schema::to_json_schema(&self.parameters)
    }
}

/// A call the model asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// The provider's id for this call. Must be echoed on the result, or the
    /// model has no way to tell which answer belongs to which question.
    pub id: String,

    pub name: String,

    /// Arguments as raw JSON text, exactly as the model produced them.
    ///
    /// Left unparsed on purpose: the executor knows the tool's schema and can
    /// deserialize straight into the right type, and a parse failure is then
    /// the executor's to report as a [`ToolOutcome::error`] the model can
    /// recover from — rather than something this crate turns into a dead run.
    pub arguments: String,
}

/// What running a tool produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub content: String,

    /// Marks content that describes a failure. The loop feeds it back either
    /// way; this is for the consumer's telemetry, and for an executor that
    /// wants to count failures without string-matching its own messages.
    pub is_error: bool,
}

impl ToolOutcome {
    #[must_use]
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    /// A failure the model should see and work around.
    ///
    /// Write these for the model, not the log: "no trait value matching
    /// 'ghoul' in this collection — did you mean 'Ghoul Skin'?" gives it
    /// somewhere to go, where "ERR_NOT_FOUND" does not.
    #[must_use]
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// Runs the tools the model asks for.
///
/// This is where the consumer's world shows up: HTTP calls to plugin services,
/// in-process queries, whatever. The loop only cares that a call goes in and
/// content comes out.
///
/// Note there is no `Result` — an executor reports failure as
/// [`ToolOutcome::error`], because the model recovering from a bad call is the
/// normal case, not an exception.
#[allow(async_fn_in_trait)] // Consumers are single-threaded (WASM workers); a Send bound would be a lie.
pub trait ToolExecutor {
    async fn execute(&self, call: &ToolCall) -> ToolOutcome;
}
