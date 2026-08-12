//! What Augie sends a plugin: [`CommandInvocation`] and [`ComponentInvocation`].
//!
//! Both are *normalised* — a plugin never sees a raw Discord interaction, and
//! therefore needs no Discord library to participate.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::PermissionClass;

/// A slash-command invocation, already resolved and authorised by Augie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandInvocation {
    /// Top-level command name, e.g. `"comp"`.
    pub command: String,

    /// Subcommand, if the command declared any, e.g. `"create"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,

    /// Supplied options, keyed by parameter name. Absent optional parameters
    /// are simply missing rather than present-and-null.
    #[serde(default)]
    pub options: HashMap<String, OptionValue>,

    pub user: InvokingUser,

    /// Guild the command was run in. Always present — Augie rejects plugin
    /// commands in DMs before dispatch, since permission classes are
    /// guild-scoped and meaningless outside one.
    pub guild_id: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,

    /// The caller's resolved class. Augie has already gated on this; it is
    /// passed so a plugin can apply its own defence in depth without needing
    /// to interpret role IDs.
    pub permission_class: PermissionClass,

    /// Discord interaction token, for a plugin that wants to send its own
    /// follow-ups on a long operation. Valid for 15 minutes.
    pub interaction_token: String,

    /// Discord application ID, needed to address the follow-up webhook.
    pub application_id: String,
}

/// A button click or select-menu submission.
///
/// `custom_id` is **the plugin's own id**, exactly as it set it on the
/// component it returned. Augie stores its routing separately and restores the
/// original before dispatch, so a plugin never sees Augie's wire ids and never
/// has to encode routing into its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentInvocation {
    pub custom_id: String,

    /// Selected values for a select menu; empty for a button.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,

    pub user: InvokingUser,
    pub guild_id: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,

    pub permission_class: PermissionClass,
    pub interaction_token: String,
    pub application_id: String,
}

/// The Discord user who triggered an invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvokingUser {
    /// Snowflake as a string — see the crate docs.
    pub id: String,

    /// Discord username (no discriminator).
    pub username: String,

    /// Guild nickname if set, else global name, else username. This is what a
    /// plugin should render; `username` is for logs and audit records.
    pub display_name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_hash: Option<String>,
}

/// A resolved command option.
///
/// Externally tagged so the wire form is self-describing and a plugin can't
/// silently coerce an integer option into a string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionValue {
    String(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
    /// A user snowflake, as a string.
    User(String),
}

impl OptionValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OptionValue::String(s) | OptionValue::User(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            OptionValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            OptionValue::Number(n) => Some(*n),
            OptionValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            OptionValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

impl CommandInvocation {
    /// Fetch an option by name.
    pub fn option(&self, name: &str) -> Option<&OptionValue> {
        self.options.get(name)
    }

    /// `"comp create"` — convenient for a single `match` over the whole
    /// command surface rather than nested matching on command + subcommand.
    pub fn route(&self) -> String {
        match &self.subcommand {
            Some(sub) => format!("{} {sub}", self.command),
            None => self.command.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invocation(sub: Option<&str>) -> CommandInvocation {
        CommandInvocation {
            command: "comp".to_string(),
            subcommand: sub.map(str::to_string),
            options: HashMap::from([
                ("min_ada".to_string(), OptionValue::Integer(51)),
                ("name".to_string(), OptionValue::String("Area 51".to_string())),
            ]),
            user: InvokingUser {
                id: "179744071361757184".to_string(),
                username: "spanners".to_string(),
                display_name: "Spanners".to_string(),
                avatar_hash: None,
            },
            guild_id: "1283465958945456149".to_string(),
            channel_id: None,
            permission_class: PermissionClass::Admin,
            interaction_token: "tok".to_string(),
            application_id: "1372830411196993578".to_string(),
        }
    }

    #[test]
    fn route_joins_subcommand() {
        assert_eq!(invocation(Some("create")).route(), "comp create");
        assert_eq!(invocation(None).route(), "comp");
    }

    #[test]
    fn option_accessors_are_type_checked() {
        let inv = invocation(Some("create"));
        assert_eq!(inv.option("min_ada").and_then(OptionValue::as_i64), Some(51));
        // An integer option must not masquerade as a string.
        assert_eq!(inv.option("min_ada").and_then(OptionValue::as_str), None);
        assert_eq!(
            inv.option("name").and_then(OptionValue::as_str),
            Some("Area 51")
        );
        assert!(inv.option("nope").is_none());
    }

    #[test]
    fn snowflakes_survive_beyond_max_safe_integer() {
        // 1283465958945456149 > 2^53; the whole reason these are strings.
        let inv = invocation(None);
        let round_tripped: CommandInvocation =
            serde_json::from_str(&serde_json::to_string(&inv).unwrap()).unwrap();
        assert_eq!(round_tripped.guild_id, "1283465958945456149");
    }
}
