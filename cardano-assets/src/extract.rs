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

// =========================================================================
// Field registry — the single classification of known metadata field
// names, applied UNIFORMLY regardless of JSON layout (slot contents,
// slot siblings, or flat top-level). Replaces the old envelope/promote
// lists + the accidental flat-vs-slot behaviour. See
// `cnft.dev-workers/docs/design/STRUCTURED_METADATA_CAPTURE.md`.
//
// Four categories:
//   - Envelope    : asset core / display — never a trait, never a facet.
//   - Facet       : low-cardinality semantic; `surface` = also a
//                   filterable collection-ownership trait (rarity/tier);
//                   else capture-only (artist/series/…).
//   - Provenance  : per-asset identifier / economics — captured, never a
//                   trait.
//   - (unknown)   : not in the registry → a visual trait, decided
//                   structurally (slot contents / flat top-level).
//
// Only the trait output is wired today (a field is a collection-ownership
// trait iff it's unknown OR a surfaced facet); the envelope/facet/
// provenance split feeds the future structured-metadata capture.
// All comparisons are trimmed + lowercased.

/// Envelope (asset core / display). Kept `pub` — was the crate's public
/// "not a trait" list; now the registry's envelope arm.
pub const ENVELOPE_KEYS: &[&str] = &[
    "name",
    "title",
    "image",
    "mediatype",
    "files",
    "description",
    "url",
    "sha256",
    "twitter",
    "website",
    "discord",
    "github",
];

/// Facets that ALSO surface as filterable collection-ownership traits.
const FACET_SURFACE: &[&str] = &["rarity", "tier"];

/// Facets captured for data collection but NOT surfaced as traits.
const FACET_CAPTURE: &[&str] = &[
    "artist",
    "series",
    "medium",
    "vendor",
    "publisher",
    "project",
    "collection",
    "collection name",
];

/// Per-asset identifiers / economics — captured, never a trait.
const PROVENANCE_FIELDS: &[&str] = &[
    "id",
    "tokenid",
    "edition",
    "number",
    "index",
    "serialnumber",
    "note number",
    "plate number",
    "serial number",
    "seed",
    "piece",
    "authnft",
    "royalties",
    "copyright",
    "minter",
    "assetkind",
    "phasesupply",
    "totalsupply",
    "source_key",
    "source_tx_id",
];

/// Keys whose value, when structured, holds the asset's traits.
const SLOT_KEYS: &[&str] = &["traits", "attributes", "properties"];

/// Registry category for a known field name. See module-level table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldClass {
    Envelope,
    Facet { surface: bool },
    Provenance,
}

/// Classify a known field name (trimmed, lowercased). `None` = unknown,
/// so the structural rule decides (slot contents / flat top-level → a
/// visual trait; slot sibling → not a trait).
pub fn classify_field(name: &str) -> Option<FieldClass> {
    let key = name.trim().to_lowercase();
    let key = key.as_str();
    if ENVELOPE_KEYS.contains(&key) {
        Some(FieldClass::Envelope)
    } else if FACET_SURFACE.contains(&key) {
        Some(FieldClass::Facet { surface: true })
    } else if FACET_CAPTURE.contains(&key) {
        Some(FieldClass::Facet { surface: false })
    } else if PROVENANCE_FIELDS.contains(&key) {
        Some(FieldClass::Provenance)
    } else {
        None
    }
}

/// Whether a field should appear in the collection-ownership trait set:
/// unknown fields (visual traits) and surfaced facets (rarity/tier) only.
fn trait_eligible(name: &str) -> bool {
    match classify_field(name) {
        None => true,
        Some(FieldClass::Facet { surface }) => surface,
        Some(FieldClass::Envelope | FieldClass::Provenance) => false,
    }
}

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

/// Extract the collection-ownership trait set from `rest`.
///
/// Structural extraction produces candidate traits (a slot's contents,
/// or all flat top-level fields, or the unsig bespoke set); the field
/// **registry** then filters them UNIFORMLY — a candidate survives only
/// if it's unknown (a visual trait) or a surfaced facet (rarity/tier).
/// Envelope / capture-only facets / provenance are dropped wherever they
/// appear (slot contents, flat fields, or the unsig set), so the same
/// field is treated identically regardless of JSON layout.
pub fn extract_traits(rest: &HashMap<String, serde_json::Value>) -> Traits {
    let mut traits = candidate_traits(rest);
    traits.inner_mut().retain(|key, _| trait_eligible(key));

    // Surface facets (rarity/tier) carried as slot siblings — not already
    // captured by the structural pass above.
    for (key, value) in rest {
        if matches!(
            classify_field(key),
            Some(FieldClass::Facet { surface: true })
        ) && !traits.contains_key(key)
        {
            insert_if_present(&mut traits, key.clone(), value);
        }
    }
    traits
}

/// Structural trait extraction, BEFORE the registry filter: the
/// unsigned_algorithms bespoke set, else a structured slot's contents,
/// else all flat top-level fields.
fn candidate_traits(rest: &HashMap<String, serde_json::Value>) -> Traits {
    // Bespoke: unsigned_algorithms — algorithmic traits (index,
    // num_props, per-pixel colors/distributions under `unsigs.properties`)
    // the generic shape-dispatch can't recover. Shared with the v1 path
    // via `unsig_traits`; the caller's registry filter then drops the
    // provenance/facet members (index, series, source_*), keeping
    // num_props + colors + distributions.
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
                return extract_slot(shape, value);
            }
        }
    }
    flat_all(rest)
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

/// Every flat top-level field as a candidate trait. The registry filter
/// in `extract_traits` then removes the envelope / facet / provenance
/// ones (including data-quality warts like a leading-space key — the
/// registry trims before matching).
fn flat_all(rest: &HashMap<String, serde_json::Value>) -> Traits {
    let mut traits = Traits::new();
    for (key, val) in rest {
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
