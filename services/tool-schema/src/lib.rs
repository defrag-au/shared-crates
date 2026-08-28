//! What a tool takes: **one type**, used as both the advertised schema and the
//! parse target.
//!
//! ## The mistake this replaces
//!
//! This crate used to hold a hand-written parameter list — name, description,
//! kind, required — which a service declared and then read back out of a
//! `serde_json::Value` by hand. Two descriptions of one thing, with nothing
//! tying them together, and every drift between them was invisible:
//!
//! | Symptom | Drift |
//! |---|---|
//! | a filter silently ignored | doc said `trait_bit`, code read `trait_bits` |
//! | 30 requested, 5 returned | prose said "1-5", code allowed 60 |
//! | `traits` dropped without a word | an undeclared key, ignored not rejected |
//! | `trait: ["ghoul"]` disappeared | declared a string, read with `as_str()` |
//!
//! None of those are catchable at runtime, because prose is not checkable. The
//! fix is structural: derive the schema from the type that parses the
//! arguments, so a bound lives on the field rather than in a sentence.
//!
//! ```ignore
//! #[derive(Deserialize, JsonSchema)]
//! #[serde(deny_unknown_fields)]
//! struct AssetsArgs {
//!     /// Collection name (e.g. "Perps") or 56-character hex policy id
//!     collection: String,
//!     /// How many to fetch. Shown 5 to a card, with paging.
//!     #[schemars(range(min = 1, max = 60))]
//!     count: Option<usize>,
//! }
//! ```
//!
//! ## serde is the validator
//!
//! A tool parses into its own argument type, and serde's errors are better
//! than anything a host could synthesise: *"unknown field `traits`, expected
//! one of `collection`, `trait`, `count`, `order`"* names the mistake and the
//! fix in one line. `deny_unknown_fields` is what turns a silently-dropped
//! argument into a message a model can correct from.
//!
//! So there is no separate validation step and no schema interpreter. The
//! host forwards arguments; the tool that declared them parses them.
//!
//! ## Still deliberately narrow
//!
//! The old vocabulary was flat scalars and bounded choices, so that a tool
//! could not invite a model to invent a query language. That constraint is
//! worth keeping and is now a *lint* over the generated schema
//! ([`assert_flat`]) rather than a vocabulary too weak to express the bounds
//! we actually need — which is precisely what cost us the `count` bug.

use schemars::{generate::SchemaSettings, SchemaGenerator};
use serde_json::Value;

// Re-exported so a tool author derives against the same version this crate
// generates with. Two schemars majors in one graph would produce two
// incompatible `JsonSchema` traits and a baffling error at the derive site.
pub use schemars;
pub use schemars::JsonSchema;

/// The JSON Schema a model is given for `T`'s arguments.
///
/// **The only place tool schema JSON is produced.** Both the service
/// advertising a tool and the host offering it to a model go through here, so
/// they cannot describe the same tool differently.
///
/// Two deliberate departures from schemars' defaults:
///
/// - **Subschemas are inlined.** A nested enum would otherwise become a
///   `$ref` into `$defs`, and provider support for following those in function
///   schemas is uneven. A self-contained schema works everywhere.
/// - **No `$schema` key.** It is meaningless to a function-calling API and
///   costs bytes in a prompt prefix that is cached by exact match.
pub fn schema_for<T: JsonSchema>() -> Value {
    let settings = SchemaSettings::draft2020_12().with(|settings| {
        settings.inline_subschemas = true;
        settings.meta_schema = None;
    });

    let mut schema = SchemaGenerator::new(settings)
        .into_root_schema_for::<T>()
        .to_value();

    // `title` is the Rust type name — `AssetsArgs` tells a model nothing and
    // leaks an implementation detail into the prompt.
    if let Some(object) = schema.as_object_mut() {
        object.remove("title");
    }

    schema
}

/// The schema for a tool that genuinely takes no arguments.
///
/// Explicit on purpose. A *missing* schema and a schema describing no
/// arguments must not be the same value: the first means "this manifest is too
/// old to say", the second means "there is nothing to pass". Conflated, a host
/// tells the model an unknown tool takes nothing, the model dutifully calls it
/// with `{}`, and the tool rejects it for a field it was never told about —
/// which reads as the model being stupid when it was being obedient.
pub fn no_arguments() -> Value {
    let mut schema = serde_json::Map::new();
    schema.insert("type".to_string(), Value::from("object"));
    schema.insert("properties".to_string(), Value::Object(serde_json::Map::new()));
    schema.insert("additionalProperties".to_string(), Value::from(false));
    Value::Object(schema)
}

/// Complain about a tool schema that is more expressive than a tool schema
/// should be.
///
/// Not a correctness check — an over-rich schema still *works*. It is a design
/// guard: a tool taking a free-text filter or a nested object invites a model
/// to invent a query language, and invites a question typed in a public
/// channel to steer one. A tool taking `order: first | random | rarest` cannot
/// be asked for an order that does not exist.
///
/// Call it from a test over every advertised tool.
pub fn assert_flat(schema: &Value) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();

    let Some(object) = schema.as_object() else {
        return Err(vec!["schema is not an object".to_string()]);
    };

    if object.get("type").and_then(Value::as_str) != Some("object") {
        problems.push("top level must be an object".to_string());
    }

    // An inlined schema has no business carrying definitions.
    for key in ["$defs", "definitions"] {
        if object.contains_key(key) {
            problems.push(format!("`{key}` present — subschemas should be inlined"));
        }
    }

    let Some(properties) = object.get("properties").and_then(Value::as_object) else {
        // A tool taking no arguments is fine.
        return finish(problems);
    };

    for (name, property) in properties {
        for problem in flatness_problems(property) {
            problems.push(format!("`{name}`: {problem}"));
        }
    }

    finish(problems)
}

fn finish(problems: Vec<String>) -> Result<(), Vec<String>> {
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// What is wrong with one property, if anything.
fn flatness_problems(property: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    let Some(object) = property.as_object() else {
        return problems;
    };

    if object.contains_key("$ref") {
        problems.push("is a `$ref` — subschemas should be inlined".to_string());
    }

    // `Option<T>` renders as a type union with null, which is expected and
    // fine; anything else nested is not.
    for (key, value) in object {
        match key.as_str() {
            "properties" => problems.push("is a nested object".to_string()),
            "type" if is_object_type(value) => problems.push("is a nested object".to_string()),
            "items" if item_type(value) != Some("string") => {
                problems.push("is an array of something other than strings".to_string());
            }
            _ => {}
        }
    }

    problems
}

fn is_object_type(value: &Value) -> bool {
    match value {
        Value::String(name) => name == "object",
        Value::Array(names) => names.iter().any(|n| n.as_str() == Some("object")),
        _ => false,
    }
}

fn item_type(items: &Value) -> Option<&str> {
    items.get("type").and_then(|t| match t {
        Value::String(name) => Some(name.as_str()),
        Value::Array(names) => names.iter().find_map(|n| n.as_str()).filter(|n| *n != "null"),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(rename_all = "lowercase")]
    enum Order {
        First,
        Random,
        Rarest,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct AssetsArgs {
        /// Collection name (e.g. "Perps") or 56-character hex policy id
        collection: String,
        /// A trait to narrow to
        #[serde(default)]
        r#trait: Option<String>,
        /// How many to fetch
        #[schemars(range(min = 1, max = 60))]
        #[serde(default)]
        count: Option<u32>,
        // Never read, and that is the whole point: these fixtures exist to be
        // DERIVED from, and every assertion below is about the generated
        // schema rather than about a value. `expect` rather than `allow` so
        // the day one of them gains a real reader, this says so.
        #[expect(dead_code)]
        #[serde(default)]
        order: Option<Order>,
    }

    /// The whole point: a bound stated to the model comes from the field, so
    /// it cannot say "1-5" while the code allows 60.
    #[test]
    fn a_range_on_the_field_reaches_the_model() {
        let schema = schema_for::<AssetsArgs>();
        let count = &schema["properties"]["count"];

        // Reachable whether or not the Option wraps it in a union.
        let json = serde_json::to_string(count).unwrap();
        assert!(json.contains("\"minimum\":1"), "{json}");
        assert!(json.contains("\"maximum\":60"), "{json}");
    }

    /// Doc comments are the description a model reads, so the prose and the
    /// field it describes cannot drift apart — they are the same declaration.
    #[test]
    fn doc_comments_become_descriptions() {
        let schema = schema_for::<AssetsArgs>();
        let description = schema["properties"]["collection"]["description"]
            .as_str()
            .expect("collection is described");
        assert!(description.contains("56-character hex"), "{description}");
    }

    #[test]
    fn required_and_optional_are_distinguished() {
        let schema = schema_for::<AssetsArgs>();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .expect("required list")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(required, vec!["collection"]);
    }

    /// A nested enum would otherwise be a `$ref` into `$defs`, and provider
    /// support for following those in function schemas is uneven.
    #[test]
    fn subschemas_are_inlined_rather_than_referenced() {
        let schema = schema_for::<AssetsArgs>();
        let json = serde_json::to_string(&schema).unwrap();

        assert!(!json.contains("$ref"), "{json}");
        assert!(!json.contains("$defs"), "{json}");
        // The enum's values survive the inlining.
        assert!(json.contains("rarest"), "{json}");
    }

    /// `title` is the Rust type name, and `$schema` is meaningless to a
    /// function-calling API — both are prompt bytes for nothing.
    #[test]
    fn implementation_details_are_stripped() {
        let schema = schema_for::<AssetsArgs>();
        assert!(schema.get("title").is_none(), "{schema}");
        assert!(schema.get("$schema").is_none(), "{schema}");
    }

    #[test]
    fn a_flat_tool_schema_passes_the_lint() {
        assert!(assert_flat(&schema_for::<AssetsArgs>()).is_ok());
    }

    /// The design guard: a tool that takes a nested object has grown a query
    /// language, which is what the narrow vocabulary existed to prevent.
    #[test]
    fn a_nested_argument_is_reported() {
        // Shapes to derive a schema from, not values to read — see `AssetsArgs`.
        #[derive(JsonSchema)]
        struct Filter {
            #[expect(dead_code)]
            field: String,
        }
        #[derive(JsonSchema)]
        struct Nested {
            #[expect(dead_code)]
            filter: Filter,
        }

        let problems = assert_flat(&schema_for::<Nested>()).unwrap_err();
        assert!(
            problems.iter().any(|p| p.contains("nested object")),
            "{problems:?}"
        );
    }

    /// String arrays are the one compound shape allowed — id lists and trait
    /// filters are the place a scalar genuinely will not do.
    #[test]
    fn a_string_array_is_allowed_but_other_arrays_are_not() {
        #[derive(JsonSchema)]
        struct Strings {
            #[expect(dead_code)]
            names: Vec<String>,
        }
        assert!(assert_flat(&schema_for::<Strings>()).is_ok());

        #[derive(JsonSchema)]
        struct Numbers {
            #[expect(dead_code)]
            counts: Vec<u32>,
        }
        let problems = assert_flat(&schema_for::<Numbers>()).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("array")), "{problems:?}");
    }

    /// serde is the validator, and its message is the model's correction.
    /// This is what turns a silently-dropped argument into a fixable one.
    #[test]
    fn an_undeclared_argument_is_rejected_by_name() {
        let error = serde_json::from_str::<AssetsArgs>(
            r#"{"collection":"Black Flag","traits":"ghoul"}"#,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("unknown field `traits`"), "{error}");
        assert!(error.contains("expected one of"), "{error}");
        assert!(error.contains("`trait`"), "{error}");
    }

    /// The filter that vanished: declared a string, sent as an array, read
    /// with `as_str()` and therefore absent. Parsing reports it instead.
    #[test]
    fn a_wrongly_typed_argument_is_rejected_rather_than_read_as_absent() {
        let error = serde_json::from_str::<AssetsArgs>(
            r#"{"collection":"Black Flag","trait":["ghoul"]}"#,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("invalid type: sequence"), "{error}");
        assert!(error.contains("expected a string"), "{error}");
    }

    #[test]
    fn a_value_outside_its_choices_is_rejected() {
        let error =
            serde_json::from_str::<AssetsArgs>(r#"{"collection":"x","order":"cheapest"}"#)
                .unwrap_err()
                .to_string();
        assert!(error.contains("unknown variant `cheapest`"), "{error}");
        assert!(error.contains("rarest"), "{error}");
    }

    #[test]
    fn a_well_formed_call_parses() {
        let args: AssetsArgs = serde_json::from_str(
            r#"{"collection":"Black Flag","trait":"ghoul","count":30,"order":"rarest"}"#,
        )
        .expect("parses");
        assert_eq!(args.collection, "Black Flag");
        assert_eq!(args.r#trait.as_deref(), Some("ghoul"));
        assert_eq!(args.count, Some(30));
    }
}
