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

    /// Marks `content` as describing a failure. The model sees it either way —
    /// a tool that fails usefully is how an agent recovers rather than stalls.
    #[serde(default)]
    pub is_error: bool,

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
            is_error: false,
            presentation: None,
        }
    }

    /// A failure the model should see and work around.
    #[must_use]
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
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
        // `is_error` rides along so the failure state is explicit on every
        // result rather than inferred from absence — same call as on
        // `CommandResponse::ephemeral`.
        assert!(json.contains("is_error"), "{json}");
    }

    #[test]
    fn an_error_is_still_content_the_model_reads() {
        let response = ToolResponse::error("no trait value matching 'ghoul'");
        assert!(response.is_error);
        assert!(response.content.contains("ghoul"));
    }
}
