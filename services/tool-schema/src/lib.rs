//! What a tool takes, as data rather than as JSON.
//!
//! ## Why this crate exists
//!
//! A tool's arguments were previously a `serde_json::Value` holding a
//! hand-written JSON Schema, authored in one service and interpreted in
//! another. Nothing checked the two agreed, and nothing could: a `Value` is
//! equally happy being an object, a string, `null`, or an object missing the
//! key the reader wanted. The failure mode is a model silently offered a tool
//! with no arguments, which looks exactly like a model choosing not to pass
//! one.
//!
//! So the parameters are typed all the way across the wire, and **JSON Schema
//! is generated in exactly one function** ([`to_json_schema`]). What a service
//! advertises and what a model is sent are then the same code path rather than
//! two descriptions that have to be kept in step.
//!
//! ## Deliberately less expressive than JSON Schema
//!
//! Flat, scalar-or-string-array, with bounded `choices`. No nested objects, no
//! `oneOf`. That is the point: a tool taking a free-text filter invites a model
//! to invent a query language, and invites a question typed in a public channel
//! to steer one. A tool taking `window: last_24h | last_7d` cannot be asked for
//! a window that doesn't exist.
//!
//! ## Where this sits
//!
//! Depended on by the plugin protocol (`augie-plugin`, which advertises tools)
//! and by the agent loop (`tool-agent`, which offers them to a model). Not the
//! reverse — either of those depending on the other would drag a Discord
//! protocol into a provider-neutral loop, or vice versa.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One argument a tool accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolParameter {
    /// Argument name, as the model will send it.
    pub name: String,

    /// What it means, written for the model.
    ///
    /// Worth as much care as the tool's own description: this is where a model
    /// learns that `collection` takes a name and not a policy id.
    pub description: String,

    pub kind: ToolParameterKind,

    /// Whether the model must supply it. Optional arguments are how a tool
    /// offers a default it resolves itself — see `bargains`, which falls back
    /// to the guild's own collection.
    #[serde(default)]
    pub required: bool,

    /// Bounded set of acceptable values. Empty means free input.
    ///
    /// Prefer choices wherever the set is knowable. They become an `enum` in
    /// the generated schema, which makes an invalid value unrepresentable
    /// rather than merely discouraged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
}

impl ToolParameter {
    /// A required string.
    pub fn required(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            kind: ToolParameterKind::String,
            required: true,
            choices: Vec::new(),
        }
    }

    /// An optional string — the tool supplies its own default.
    pub fn optional(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            kind: ToolParameterKind::String,
            required: false,
            choices: Vec::new(),
        }
    }

    #[must_use]
    pub fn of_kind(mut self, kind: ToolParameterKind) -> Self {
        self.kind = kind;
        self
    }

    /// Restrict to a known set of values.
    #[must_use]
    pub fn choosing(mut self, choices: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.choices = choices.into_iter().map(Into::into).collect();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolParameterKind {
    String,
    Integer,
    Number,
    Boolean,
    /// A list of strings. The only compound shape offered, because trait
    /// filters and id lists are the one place a scalar genuinely won't do.
    StringArray,
}

impl ToolParameterKind {
    /// The JSON Schema type name for this kind.
    fn schema_type(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::StringArray => "array",
        }
    }
}

/// Generate the JSON Schema a model is given for these parameters.
///
/// **The only place schema JSON is produced.** Both the service advertising a
/// tool and the host offering it to a model go through here, so they cannot
/// describe the same tool differently.
pub fn to_json_schema(parameters: &[ToolParameter]) -> serde_json::Value {
    let properties: BTreeMap<String, PropertySchema> = parameters
        .iter()
        .map(|param| (param.name.clone(), PropertySchema::from(param)))
        .collect();

    let required = parameters
        .iter()
        .filter(|param| param.required)
        .map(|param| param.name.clone())
        .collect();

    let schema = ObjectSchema {
        kind: "object",
        properties,
        required,
        // Refuse arguments the tool never declared. Without this a model can
        // invent a plausible-looking extra and have it silently ignored, which
        // reads to it as "I passed that and it was honoured".
        additional_properties: false,
    };

    serde_json::to_value(schema).unwrap_or_else(|_| serde_json::Value::Null)
}

/// A `BTreeMap`, not a `HashMap`: the schema sits in the cached prompt prefix,
/// and prompt caching is a byte-prefix match — key order that varied between
/// requests would quietly cost a cache miss every time.
#[derive(Serialize)]
struct ObjectSchema {
    #[serde(rename = "type")]
    kind: &'static str,
    properties: BTreeMap<String, PropertySchema>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    required: Vec<String>,
    #[serde(rename = "additionalProperties")]
    additional_properties: bool,
}

#[derive(Serialize)]
struct PropertySchema {
    #[serde(rename = "type")]
    kind: &'static str,
    description: String,
    /// Element type, for arrays only.
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<ItemSchema>,
    #[serde(rename = "enum", skip_serializing_if = "Vec::is_empty")]
    choices: Vec<String>,
}

#[derive(Serialize)]
struct ItemSchema {
    #[serde(rename = "type")]
    kind: &'static str,
}

impl From<&ToolParameter> for PropertySchema {
    fn from(param: &ToolParameter) -> Self {
        Self {
            kind: param.kind.schema_type(),
            description: param.description.clone(),
            items: matches!(param.kind, ToolParameterKind::StringArray)
                .then_some(ItemSchema { kind: "string" }),
            choices: param.choices.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_required_string_becomes_a_property_and_a_requirement() {
        let schema = to_json_schema(&[ToolParameter::required(
            "collection",
            "Collection name, e.g. \"SpaceBudz\"",
        )]);

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["collection"]["type"], "string");
        assert!(schema["properties"]["collection"]["description"]
            .as_str()
            .is_some_and(|d| d.contains("SpaceBudz")));
        assert_eq!(schema["required"][0], "collection");
        assert_eq!(schema["additionalProperties"], false);
    }

    /// The live bug this crate exists to prevent: an optional parameter still
    /// has to appear in `properties`, or the model has no way to pass it.
    #[test]
    fn an_optional_parameter_is_still_offered() {
        let schema = to_json_schema(&[ToolParameter::optional(
            "collection",
            "Omit to use this server's collection",
        )]);

        assert!(
            schema["properties"]["collection"].is_object(),
            "an optional parameter must still be offered: {schema}"
        );
        // Not required, so the key is omitted rather than an empty array.
        assert!(schema.get("required").is_none(), "{schema}");
    }

    #[test]
    fn no_parameters_is_an_object_with_no_properties() {
        let schema = to_json_schema(&[]);
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"], serde_json::json!({}));
    }

    #[test]
    fn choices_become_an_enum() {
        let schema = to_json_schema(&[
            ToolParameter::required("window", "How far back to look")
                .choosing(["last_24h", "last_7d", "last_30d"]),
        ]);
        assert_eq!(schema["properties"]["window"]["enum"][1], "last_7d");
    }

    #[test]
    fn a_string_array_declares_its_item_type() {
        let schema = to_json_schema(&[
            ToolParameter::required("trait_bits", "Trait bit indices")
                .of_kind(ToolParameterKind::StringArray),
        ]);
        assert_eq!(schema["properties"]["trait_bits"]["type"], "array");
        assert_eq!(schema["properties"]["trait_bits"]["items"]["type"], "string");
    }

    /// The schema rides in the cached prompt prefix, so identical input must
    /// produce byte-identical output.
    #[test]
    fn generation_is_deterministic() {
        let params = [
            ToolParameter::required("zebra", "z"),
            ToolParameter::optional("alpha", "a"),
            ToolParameter::required("middle", "m"),
        ];
        let first = to_json_schema(&params).to_string();
        let second = to_json_schema(&params).to_string();
        assert_eq!(first, second);
        // And ordered, not merely stable within one process.
        assert!(
            first.find("alpha") < first.find("middle")
                && first.find("middle") < first.find("zebra"),
            "{first}"
        );
    }

    #[test]
    fn parameters_round_trip_on_the_wire() {
        let params = vec![ToolParameter::optional("collection", "name").choosing(["a", "b"])];
        let json = serde_json::to_string(&params).unwrap();
        let back: Vec<ToolParameter> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, params);
    }
}
