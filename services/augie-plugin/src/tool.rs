//! Agent-callable tools: [`ToolInvocation`] and [`ToolResponse`].
//!
//! ## Why this isn't [`crate::CommandInvocation`]
//!
//! A tool call looks like a command call and is not one, in two ways that
//! matter:
//!
//! **There is no interaction.** A command invocation carries an
//! `interaction_token` and `application_id` because a slash command *is* a
//! Discord interaction. An agent answering an `@mention` over the gateway has
//! neither — the trigger was an ordinary message. Reusing the command type
//! would mean two required fields filled with empty strings, and a plugin
//! reasonably trusting them.
//!
//! **Arguments are richer than options.** [`crate::OptionValue`] mirrors
//! Discord's option types, which are flat scalars. A tool's arguments are
//! whatever its JSON Schema declares — `{"trait_bits": [47]}` has no
//! `OptionValue` representation at all.
//!
//! ## Why tools are a separate list from commands
//!
//! They overlap but neither contains the other. `bargains` is worth both
//! advertising as `/bargains` and offering to an agent. `resolve_traits` —
//! "given the word 'ghoul', which trait value is that?" — is only ever a step
//! an agent takes on the way to something else; nobody types it. And a
//! destructive command may be worth registering for a human to run
//! deliberately while being exactly what you don't want a model reaching for.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{CommandResponse, InvokingUser, PermissionClass};

/// An agent asking a plugin to run one of its advertised tools.
///
/// Sent to `POST /_augie/tool`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInvocation {
    /// The tool's advertised name.
    pub tool: String,

    /// Arguments, already parsed and matching the tool's declared schema.
    ///
    /// Augie parses the model's raw output before dispatching, so a plugin
    /// receives JSON rather than a string that might not be JSON at all. A
    /// parse failure is reported to the model as a [`ToolResponse::error`]
    /// without the plugin ever being called.
    #[serde(default)]
    pub arguments: serde_json::Value,

    pub user: InvokingUser,

    pub guild_id: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,

    /// The caller's resolved class. Augie has already filtered the tool list
    /// to what this class may use; passed for the same defence-in-depth reason
    /// as on a command invocation.
    pub permission_class: PermissionClass,

    /// Per-guild configuration, verbatim from the guild's opt-in block —
    /// identical to [`crate::CommandInvocation::config`].
    ///
    /// This is load-bearing for agents specifically. It is how a guild's
    /// `policy_id` reaches the tool **without passing through the model**: the
    /// user says "show me the ghouls", the model never names a collection, and
    /// the tool resolves one from config. A model-supplied policy id would be
    /// both a hallucination risk and an injection lever, since the question
    /// came from a public channel.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config: HashMap<String, String>,
}

/// How a tool call ended.
///
/// An enum rather than a pair of booleans: "failed" and "needs the user to
/// choose" are different states with different consequences — one invites the
/// model to work around it, the other must stop it dead — and a second flag
/// would let both be true at once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    /// The tool answered.
    #[default]
    Ok,
    /// The tool failed in a way the model can work around — a bad argument, an
    /// unknown name. The content explains it.
    Failed,
    /// The tool has asked the user a question and cannot proceed until it is
    /// answered.
    ///
    /// The question lives in `presentation`, as components the user interacts
    /// with; the answer arrives later as a component callback, not as another
    /// tool call. **The model must not retry or guess** — there is nothing to
    /// retry, and guessing is the behaviour this exists to replace.
    AwaitingInput,
}

/// What a tool produced.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolResponse {
    /// What the model reads.
    ///
    /// Write it for a model, not a log or a user: dense, factual, and specific
    /// enough to act on. `"no trait value matching 'ghoul' — closest are
    /// 'Ghoul Skin', 'Ghoulish Grin'"` gives it somewhere to go;
    /// `"ERR_NOT_FOUND"` does not.
    pub content: String,

    #[serde(default)]
    pub status: ToolStatus,

    /// How to *show* this result, if plain prose won't do.
    ///
    /// The model writes the words; a plugin that can do better than words —
    /// a gallery of thumbnails, a rendered chart, paging buttons — supplies
    /// that here. Rendering stays with the service that owns the data, which
    /// is the same reason [`CommandResponse::render`] exists.
    ///
    /// **The last presentation in a run wins.** A run that calls
    /// `resolve_traits` then `find_assets` should show the assets, and later
    /// calls are nearer the answer than earlier ones. A tool whose result is
    /// only an intermediate step should leave this `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<CommandResponse>,
}

impl ToolResponse {
    #[must_use]
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            status: ToolStatus::Ok,
            presentation: None,
        }
    }

    /// The tool needs the user to choose before it can answer.
    ///
    /// `content` is what the model reads — it should say a question was asked,
    /// so the model relays that rather than inventing a result. `showing` then
    /// carries the question itself.
    #[must_use]
    pub fn asking(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            status: ToolStatus::AwaitingInput,
            presentation: None,
        }
    }

    pub fn is_error(&self) -> bool {
        self.status == ToolStatus::Failed
    }

    pub fn awaiting_input(&self) -> bool {
        self.status == ToolStatus::AwaitingInput
    }

    /// A failure the model should see and work around.
    #[must_use]
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            status: ToolStatus::Failed,
            presentation: None,
        }
    }

    /// Attach the reply Discord should show for this result.
    #[must_use]
    pub fn showing(mut self, presentation: CommandResponse) -> Self {
        self.presentation = Some(presentation);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_carry_shapes_an_option_value_cannot() {
        // The reason this type exists rather than reusing CommandInvocation.
        let json = r#"{
            "tool": "find_assets",
            "arguments": {"trait_bits": [47, 12], "page": 0},
            "user": {"id": "1", "username": "u", "display_name": "U"},
            "guild_id": "2",
            "permission_class": "everyone"
        }"#;

        let parsed: ToolInvocation = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.tool, "find_assets");
        assert_eq!(
            parsed.arguments["trait_bits"].as_array().unwrap().len(),
            2
        );
    }

    #[test]
    fn a_result_with_no_presentation_omits_it_from_the_wire() {
        let json = serde_json::to_string(&ToolResponse::ok("112 matches")).unwrap();
        assert!(!json.contains("presentation"), "{json}");
        // `status` rides along so the outcome is explicit on every result
        // rather than inferred from absence.
        assert!(json.contains("status"), "{json}");
    }

    /// Awaiting input is not a failure. A model told "this failed" retries or
    /// works around it; a model told "I asked the user" must do neither.
    #[test]
    fn awaiting_input_is_distinct_from_failure() {
        let asking = ToolResponse::asking("asked which trait they meant");
        assert!(asking.awaiting_input());
        assert!(!asking.is_error(), "a question is not a failure");

        let failed = ToolResponse::error("nope");
        assert!(failed.is_error());
        assert!(!failed.awaiting_input());
    }

    #[test]
    fn an_error_is_still_content_the_model_reads() {
        let response = ToolResponse::error("no trait value matching 'ghoul'");
        assert!(response.is_error());
        assert!(response.content.contains("ghoul"));
    }
}
