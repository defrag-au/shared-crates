//! Tools: what the model may call, and who runs it.

use serde::{Deserialize, Serialize};

/// A tool the model may call.
///
/// The schema is carried as JSON because it is **generated** — from the type
/// the tool parses its arguments into ([`tool_schema::schema_for`]) — rather
/// than hand-written. The hazard that once argued for a typed intermediate was
/// an unchecked hand-written `Value`; deriving it removes that, and a typed
/// intermediate would now only be re-serialised into exactly this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments. `None` for a tool that takes none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

impl ToolDef {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        schema: Option<serde_json::Value>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            schema,
        }
    }

    /// The JSON Schema a provider is sent for this tool.
    ///
    /// Falls back to "takes nothing" only because a provider requires the
    /// field. **A tool with no schema should not be offered at all** — see
    /// [`Self::arguments_are_known`]. Telling a model that an unknown tool
    /// accepts nothing is not a safe default: it will call it with `{}`,
    /// correctly, and be rejected for a field it was never shown.
    #[must_use]
    pub fn json_schema(&self) -> serde_json::Value {
        self.schema
            .clone()
            .unwrap_or_else(tool_schema::no_arguments)
    }

    /// Do we know what this tool takes?
    ///
    /// False means the manifest predates `input_schema`, not that the tool is
    /// argument-less — a tool that genuinely takes none says so explicitly
    /// with [`tool_schema::no_arguments`]. The distinction exists so a host
    /// can decline to offer what it cannot describe.
    #[must_use]
    pub fn arguments_are_known(&self) -> bool {
        self.schema.is_some()
    }

    /// The argument names, for logs and traces.
    ///
    /// `calls=1` alone cannot distinguish "the model never passed the
    /// argument" from "it passed one and ignored the answer", and those have
    /// completely different fixes.
    #[must_use]
    pub fn parameter_names(&self) -> Vec<&str> {
        self.schema
            .as_ref()
            .and_then(|schema| schema.get("properties"))
            .and_then(serde_json::Value::as_object)
            .map(|properties| properties.keys().map(String::as_str).collect())
            .unwrap_or_default()
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

/// How a tool call ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToolStatus {
    #[default]
    Ok,
    /// Failed in a way the model can work around.
    Failed,
    /// The tool asked the user something and cannot proceed until answered.
    /// The loop stops offering tools after this — see [`crate::run`].
    AwaitingInput,
}

/// What running a tool produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub content: String,
    pub status: ToolStatus,
}

impl ToolOutcome {
    #[must_use]
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            status: ToolStatus::Ok,
        }
    }

    /// The tool put a question to the user.
    ///
    /// Terminal for the run: there is nothing to retry, and the answer arrives
    /// later through a different channel entirely.
    #[must_use]
    pub fn awaiting_input(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            status: ToolStatus::AwaitingInput,
        }
    }

    pub fn is_error(&self) -> bool {
        self.status == ToolStatus::Failed
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
            status: ToolStatus::Failed,
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
