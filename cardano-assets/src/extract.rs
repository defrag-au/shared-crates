//! v2 trait extraction: envelope + composable trait patterns.
//!
//! The original [`crate::AssetMetadata`] models metadata as ~8
//! whole-document `#[serde(untagged)]` variants, each re-declaring the
//! common envelope (name/image/mediaType/files/…) alongside one trait
//! encoding. Matching is therefore by *whole-document deserialization
//! success* and by *variant order* — which is fragile: a document that
//! doesn't fit its intended variant silently falls through to whichever
//! other variant happens to deserialize, often producing wrong traits
//! (e.g. an OpenSea `{trait_type,value}` array collapsing into bogus
//! `trait_type`/`value` keys, or a single numeric attribute knocking a
//! whole nested map into a fallback that drops every real trait).
//!
//! This module separates the two concerns:
//!   1. A single [`AssetEnvelope`] captures the common fields once.
//!   2. [`extract_traits`] dispatches on the JSON *shape* of the trait
//!      data, so the same small set of orthogonal patterns compose
//!      regardless of which collection produced them.
//!
//! It is deliberately built alongside the existing enum (not wired into
//! the live path yet) so its output can be diffed against v1 over the
//! whole fixture corpus before any cutover. See
//! `tests/extract_corpus.rs`.

use crate::{Asset, AssetFile, PrimitiveOrList, Traits, UnsigData};
use serde::Deserialize;
use std::collections::HashMap;

/// Metadata keys that form the common envelope of an NFT and must never
/// be surfaced as traits in flat-extraction mode. Compared lowercased,
/// so this is the single source of truth shared by the typed envelope
/// fields and the flat-trait filter (v1 had two lists that could drift).
pub const ENVELOPE_KEYS: &[&str] = &[
    "name",
    "image",
    "mediatype",
    "files",
    "description",
    "project",
    "collection",
    "collection name",
    "twitter",
    "website",
    "discord",
    "github",
    "medium",
    "minter",
    "publisher",
    "sha256",
    "url",
];

/// Keys whose value, when structured, holds the asset's traits.
const SLOT_KEYS: &[&str] = &["traits", "attributes", "properties"];

/// Universal trait keys that are kept even when they sit as a *sibling*
/// of a structured slot (where siblings are otherwise ignored as
/// metadata noise). These are cross-collection trait concepts, not
/// per-collection fields — the curated inverse of [`ENVELOPE_KEYS`].
/// Compared lowercased. Example: Funplastic nests its visual traits in
/// an `attributes` map but puts `Rarity` at the top level.
const PROMOTE_KEYS: &[&str] = &["rarity", "tier"];

/// The common metadata envelope. Typed fields are parsed once; every
/// other field lands in `rest` for trait extraction.
#[derive(Deserialize, Debug, Clone)]
pub struct AssetEnvelope {
    #[serde(default, alias = "Name", alias = "title")]
    pub name: Option<String>,
    #[serde(default)]
    pub image: Option<PrimitiveOrList<String>>,
    #[serde(default, alias = "mediaType", alias = "mediatype")]
    pub media_type: Option<String>,
    #[serde(default)]
    pub files: Option<Vec<AssetFile>>,
    #[serde(flatten)]
    pub rest: HashMap<String, serde_json::Value>,
}

impl AssetEnvelope {
    /// Project the envelope into an [`Asset`], extracting traits from
    /// `rest` by shape. Mirrors the v1 `From<AssetMetadata>` output
    /// (name/image/media_type/traits) but via the composable extractor.
    pub fn into_asset(self) -> Asset {
        let image = self
            .image
            .as_ref()
            .map(PrimitiveOrList::dechunked)
            .unwrap_or_default();
        let media_type = self.resolve_media_type(&image);
        let traits = extract_traits(&self.rest);
        Asset {
            name: self.name.unwrap_or_default(),
            image,
            media_type,
            traits,
            rarity_rank: None,
            tags: vec![],
        }
    }

    /// Top-level `mediaType` if present, else the media type of the file
    /// whose `src` matches the (dechunked) image URL — matching v1's
    /// `extract_media_type` fallback.
    fn resolve_media_type(&self, image: &str) -> Option<String> {
        if self.media_type.is_some() {
            return self.media_type.clone();
        }
        if let Some(files) = &self.files {
            for file in files {
                if file.get_src() == image {
                    return Some(file.media_type().to_string());
                }
            }
        }
        None
    }
}

/// Parse raw asset metadata JSON into an [`Asset`] via the v2 path.
pub fn asset_from_metadata_json(json: &str) -> Result<Asset, serde_json::Error> {
    let envelope: AssetEnvelope = serde_json::from_str(json)?;
    Ok(envelope.into_asset())
}

/// As [`asset_from_metadata_json`], but from an already-parsed
/// `serde_json::Value`. For callers that hold the raw metadata as a
/// `Value` (e.g. an indexer response) — avoids a re-serialize round-trip
/// and, critically, lets them feed the *original* document to v2 rather
/// than laundering it through the v1 `AssetMetadata` enum first.
pub fn asset_from_metadata_value(value: serde_json::Value) -> Result<Asset, serde_json::Error> {
    let envelope: AssetEnvelope = serde_json::from_value(value)?;
    Ok(envelope.into_asset())
}

/// The structured shapes a trait slot can take. A value-only string
/// array (`["A","B"]`) is deliberately NOT a structured slot — it is
/// treated as a flat multi-value field, matching how v1 surfaced
/// SpaceBudz-style `traits` arrays (the array under its own key, with
/// sibling scalars still becoming traits).
enum SlotShape {
    /// `{ "Background": "Crimson", ... }`
    Map,
    /// `[ { "trait_type"|"name": K, "value": V }, ... ]` (OpenSea / gophers)
    Codified,
    /// `[ { "K": "V" }, ... ]` (one or more single-key objects per element)
    ObjectArray,
    /// `[ "State: Delusional", ... ]`
    ColonDelimited,
}

/// Classify a candidate slot value. Returns `None` when the value is not
/// a *structured* trait container (e.g. a value-only string array, a
/// scalar, or an empty array), in which case flat extraction applies.
fn classify_slot(value: &serde_json::Value) -> Option<SlotShape> {
    match value {
        serde_json::Value::Object(_) => Some(SlotShape::Map),
        serde_json::Value::Array(items) if !items.is_empty() => {
            if items.iter().all(serde_json::Value::is_object) {
                let codified = items.iter().all(|item| {
                    let obj = item.as_object().expect("checked is_object");
                    obj.contains_key("value")
                        && (obj.contains_key("trait_type") || obj.contains_key("name"))
                });
                Some(if codified {
                    SlotShape::Codified
                } else {
                    SlotShape::ObjectArray
                })
            } else if items.iter().all(serde_json::Value::is_string) {
                let any_colon = items
                    .iter()
                    .any(|v| v.as_str().is_some_and(|s| s.contains(':')));
                any_colon.then_some(SlotShape::ColonDelimited)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract traits from `rest`. If a structured trait slot
/// (`traits`/`attributes`/`properties`) is present, traits come solely
/// from it (sibling scalars are envelope/metadata noise). Otherwise all
/// non-envelope scalar/array fields are treated as flat traits.
pub fn extract_traits(rest: &HashMap<String, serde_json::Value>) -> Traits {
    // Bespoke: unsigned_algorithms. Its traits are *algorithmic*
    // (index, num_props, and the per-pixel colors/distributions under
    // `unsigs.properties`) rather than plain key/values, so the generic
    // shape-dispatch can't recover them. Detect the `unsigs` structure
    // and delegate to the shared builder — identical to the v1 path, so
    // both the maestro fallback and the mitos/v2 path yield the rich set.
    if let Some(unsigs) = rest
        .get("unsigs")
        .and_then(|v| serde_json::from_value::<UnsigData>(v.clone()).ok())
    {
        let series = rest
            .get("series")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let source_key = rest
            .get("source_key")
            .map(json_to_strings)
            .unwrap_or_default();
        let source_tx_id = rest
            .get("source_tx_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        return unsig_traits(&unsigs, series, source_key, source_tx_id);
    }

    for slot in SLOT_KEYS {
        if let Some((_, value)) = rest.iter().find(|(k, _)| k.to_lowercase() == *slot) {
            if let Some(shape) = classify_slot(value) {
                let mut traits = extract_slot(shape, value);
                promote_siblings(&mut traits, rest);
                return traits;
            }
        }
    }
    flat_extract(rest)
}

/// Build the trait set for an unsigned_algorithms asset from its
/// `unsigs` structure + the sibling provenance fields. Shared by the v1
/// `From<AssetMetadata>` path and the v2 extractor so the two can't
/// drift. Mirrors the original v1 logic exactly.
pub(crate) fn unsig_traits(
    unsigs: &UnsigData,
    series: Option<String>,
    source_key: Vec<String>,
    source_tx_id: Option<String>,
) -> Traits {
    let mut traits = Traits::new();
    if let Some(s) = series {
        traits.insert_single("series".to_string(), s);
    }
    traits.insert_multi("source_key".to_string(), source_key);
    if let Some(tx_id) = source_tx_id {
        traits.insert_single("source_tx_id".to_string(), tx_id);
    }
    traits.insert_single("index".to_string(), unsigs.index.to_string());
    traits.insert_single("num_props".to_string(), unsigs.num_props.to_string());
    for (key, values) in unsigs.properties.inner() {
        traits.insert_multi(key.clone(), values.clone());
    }
    traits
}

/// Fold any [`PROMOTE_KEYS`] siblings of a structured slot into the
/// trait set — the few universal trait concepts (e.g. `rarity`) that are
/// worth keeping even though the slot rule otherwise drops siblings.
fn promote_siblings(traits: &mut Traits, rest: &HashMap<String, serde_json::Value>) {
    for (key, value) in rest {
        if PROMOTE_KEYS.contains(&key.to_lowercase().as_str()) {
            insert_if_present(traits, key.clone(), value);
        }
    }
}

fn extract_slot(shape: SlotShape, value: &serde_json::Value) -> Traits {
    let mut traits = Traits::new();
    match shape {
        SlotShape::Map => {
            if let Some(map) = value.as_object() {
                for (key, val) in map {
                    insert_if_present(&mut traits, key.clone(), val);
                }
            }
        }
        SlotShape::Codified => {
            for item in value.as_array().into_iter().flatten() {
                let Some(obj) = item.as_object() else {
                    continue;
                };
                let key = obj
                    .get("trait_type")
                    .or_else(|| obj.get("name"))
                    .and_then(|v| v.as_str());
                if let (Some(key), Some(val)) = (key, obj.get("value")) {
                    insert_if_present(&mut traits, key.to_string(), val);
                }
            }
        }
        SlotShape::ObjectArray => {
            for item in value.as_array().into_iter().flatten() {
                if let Some(obj) = item.as_object() {
                    for (key, val) in obj {
                        insert_if_present(&mut traits, key.clone(), val);
                    }
                }
            }
        }
        SlotShape::ColonDelimited => {
            for item in value.as_array().into_iter().flatten() {
                if let Some((key, val)) = item.as_str().and_then(|s| s.split_once(':')) {
                    let key = key.trim();
                    let val = val.trim();
                    if !key.is_empty() && !val.is_empty() {
                        traits.insert_single(key.to_string(), val.to_string());
                    }
                }
            }
        }
    }
    traits
}

fn flat_extract(rest: &HashMap<String, serde_json::Value>) -> Traits {
    let mut traits = Traits::new();
    for (key, val) in rest {
        // Trim before the envelope check so data-quality warts like a
        // leading-space key (" Project") still match an envelope key and
        // get excluded rather than leaking in as a trait.
        if ENVELOPE_KEYS.contains(&key.trim().to_lowercase().as_str()) {
            continue;
        }
        insert_if_present(&mut traits, key.clone(), val);
    }
    traits
}

/// Insert a trait under `key`, coercing the JSON value to one or more
/// strings. Numbers/bools stringify; string arrays stay multi-valued;
/// objects, nulls and empty results are skipped.
fn insert_if_present(traits: &mut Traits, key: String, value: &serde_json::Value) {
    let values = json_to_strings(value);
    if !values.is_empty() {
        traits.insert_vec(key, values);
    }
}

/// Coerce a JSON value to trait string(s): scalars to a single trimmed
/// string, arrays to their scalar elements (one level, trimmed,
/// non-empty), objects/nulls to nothing.
fn json_to_strings(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                vec![]
            } else {
                vec![s.to_string()]
            }
        }
        serde_json::Value::Number(n) => vec![n.to_string()],
        serde_json::Value::Bool(b) => vec![b.to_string()],
        serde_json::Value::Array(arr) => arr
            .iter()
            .flat_map(|v| match v {
                serde_json::Value::String(s) => {
                    let s = s.trim();
                    (!s.is_empty()).then(|| s.to_string())
                }
                serde_json::Value::Number(n) => Some(n.to_string()),
                serde_json::Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}
