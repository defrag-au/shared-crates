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

    /// The asset's **live art**, if its metadata carries any.
    ///
    /// A fully on-chain generative piece's artwork is an HTML document: a
    /// chunked `data:text/html;utf8,…` URI in `files[]` that a browser is
    /// meant to *run*, not decode into pixels. Nothing downstream of
    /// `image` can serve it — the IIIF/mirror path resolves one still per
    /// asset, and a document is not a still — so the read layer has to be
    /// told the piece is there. This is that answer.
    ///
    /// Matched on the declared `mediaType` **or** the `src` prefix, because
    /// both appear in the corpus: collections that declare `text/html`
    /// honestly, and collections whose file entry omits or misstates it while
    /// the payload is plainly a document. `files[].mediaType` is not validated
    /// by anything on chain, so trusting either alone misses real pieces.
    ///
    /// The top-level `image` is checked last — the metadata profile that
    /// aliases `image` to the on-chain HTML, so a marketplace that follows
    /// `image` still finds something. It is the profile with no cover, which
    /// [`LiveArt::cover`] then reports as `None`.
    #[must_use]
    pub fn live_art(&self) -> Option<LiveArt> {
        let src = self.live_art_src()?;
        // A still only counts as a cover if it is not the piece itself — the
        // aliased profile above points `image` at the same document, and
        // labelling that a cover would send a viewer off to render HTML as an
        // `<img>`.
        let cover = self
            .image
            .as_ref()
            .map(PrimitiveOrList::dechunked)
            .filter(|image| {
                let image = image.trim();
                !image.is_empty() && image != src && !is_html_uri(image)
            });
        Some(LiveArt { src, cover })
    }

    /// The live-art `src` alone. See [`Self::live_art`] for the matching
    /// rules.
    fn live_art_src(&self) -> Option<String> {
        if let Some(files) = &self.files {
            for file in files {
                let src = file.get_src();
                // An entry can declare `text/html` and carry nothing — a
                // truncated or hand-edited mint. Falling through to the next
                // file beats returning an empty document to an iframe.
                if src.trim().is_empty() {
                    continue;
                }
                if is_html_media_type(file.media_type()) || is_html_uri(&src) {
                    return Some(src);
                }
            }
        }
        self.image
            .as_ref()
            .map(PrimitiveOrList::dechunked)
            .filter(|image| is_html_uri(image))
    }
}

/// The `data:` prefix that means the payload is an HTML document.
const HTML_DATA_PREFIX: &str = "data:text/html";

/// A piece whose art is a **document to run** rather than a still to decode.
///
/// Returned by [`AssetEnvelope::live_art`]. The distinction is the whole
/// reason this type exists: a still is fetched, resized and cached once, and
/// a document is handed to a browser to execute, so the two cannot share a
/// read path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveArt {
    /// The piece. Chunking resolved: a `data:` URI to hand to an iframe
    /// verbatim, or — for the off-chain case — a URL to one.
    pub src: String,
    /// The still that stands in for the piece everywhere `<img>`-shaped
    /// (grid thumbnails, marketplace cards). `None` when the piece has no
    /// separate still — the strictly non-conformant, no-cover profile the
    /// import design leaves open.
    pub cover: Option<String>,
}

/// `mediaType` declares the file is a document. Compared case-insensitively
/// and by prefix, so the parameter forms (`text/html; charset=utf-8`) count.
fn is_html_media_type(media_type: &str) -> bool {
    media_type
        .trim()
        .to_ascii_lowercase()
        .starts_with("text/html")
}

/// `src` is an HTML document by its own prefix — the shape a piece ships as
/// (`data:text/html;utf8,<!doctype%20html>…`).
///
/// `str::get` rather than a byte slice so a multi-byte first character yields
/// `None` instead of panicking, and the trailing separator check keeps a
/// hypothetical `data:text/html5` from being mistaken for a document.
fn is_html_uri(src: &str) -> bool {
    let src = src.trim_start();
    match src.get(..HTML_DATA_PREFIX.len()) {
        Some(prefix) if prefix.eq_ignore_ascii_case(HTML_DATA_PREFIX) => {
            matches!(
                src.as_bytes().get(HTML_DATA_PREFIX.len()),
                Some(b';' | b',')
            )
        }
        _ => false,
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
        if let Some((_, value)) = rest.iter().find(|(k, _)| k.to_lowercase() == *slot)
            && let Some(shape) = classify_slot(value)
        {
            return extract_slot(shape, value);
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

#[cfg(test)]
mod live_art_tests {
    use super::*;

    /// A real on-chain-HTML piece: `files[0]` declares `text/html` and its
    /// `src` is a chunked `data:text/html;utf8,…` (157 chunks, 10,040 bytes
    /// joined), with an IPFS still as the top-level `image`. This is the
    /// shape the whole live-art path exists for.
    const BLOCKGEN_ARTIST: &str =
        include_str!("../resources/test/blockgen-artist-charlesmachin.json");

    fn envelope(json: &str) -> AssetEnvelope {
        serde_json::from_str(json).expect("fixture must deserialize")
    }

    #[test]
    fn a_chunked_html_file_is_the_live_art_and_the_image_is_its_cover() {
        let art = envelope(BLOCKGEN_ARTIST)
            .live_art()
            .expect("the fixture ships on-chain art");

        // Every chunk joined in order, not just the first — a viewer handed a
        // truncated document renders a fragment and looks like a broken piece.
        assert_eq!(art.src.len(), 10_040);
        assert!(art.src.starts_with("data:text/html;utf8,<html>"));
        assert!(art.src.ends_with("</script></canvas></body></html>"));

        assert_eq!(
            art.cover.as_deref(),
            Some("ipfs://QmTDNG1jw2dNDF5s6oVESUX22ydCfobZt7sTxtAtJroDkk")
        );
    }

    #[test]
    fn a_still_only_collection_has_no_live_art() {
        let json = r#"{"name":"Pirate #1","image":"ipfs://QmStill","mediaType":"image/png"}"#;
        assert_eq!(envelope(json).live_art(), None);
    }

    #[test]
    fn the_payload_decides_when_mediatype_lies() {
        // `mediaType` says PNG, the payload is a document. Nothing validates
        // the field, so the prefix has to be able to win on its own.
        let json = r#"{"image":"ipfs://QmStill","files":[{"mediaType":"image/png","src":"data:text/html;utf8,<html></html>"}]}"#;
        let art = envelope(json).live_art().expect("prefix must match");
        assert_eq!(art.src, "data:text/html;utf8,<html></html>");
        assert_eq!(art.cover.as_deref(), Some("ipfs://QmStill"));
    }

    #[test]
    fn the_declared_mediatype_decides_when_the_src_is_served() {
        // Off-chain art: a URL to an HTML document. No `data:` prefix to
        // match, so the declared type is the only signal there is.
        let json = r#"{"image":"ipfs://QmStill","files":[{"mediaType":"text/html; charset=utf-8","src":"https://example.test/piece.html"}]}"#;
        let art = envelope(json).live_art().expect("declared html must match");
        assert_eq!(art.src, "https://example.test/piece.html");
    }

    #[test]
    fn an_image_aliased_to_the_html_is_the_piece_and_there_is_no_cover() {
        // The no-cover profile: `image` points at the on-chain document so a
        // marketplace that follows `image` finds something. Reporting that as
        // a cover would send a viewer to render HTML into an `<img>`.
        let json = r#"{"image":["data:text/html;utf8,<html>","</html>"],"files":[{"mediaType":"text/html","src":"data:text/html;utf8,<html></html>"}]}"#;
        let art = envelope(json)
            .live_art()
            .expect("the aliased image is the art");
        assert_eq!(art.src, "data:text/html;utf8,<html></html>");
        assert_eq!(art.cover, None);
    }

    #[test]
    fn the_aliased_image_is_found_when_files_carry_no_document() {
        // Same profile, but the document lives only in `image` — a piece whose
        // `files[]` entry was dropped or never minted.
        let json = r#"{"image":"data:text/html;utf8,<html></html>","files":[{"mediaType":"image/png","src":"ipfs://QmStill"}]}"#;
        let art = envelope(json)
            .live_art()
            .expect("image must be the fallback");
        assert_eq!(art.src, "data:text/html;utf8,<html></html>");
        assert_eq!(art.cover, None);
    }

    #[test]
    fn an_empty_document_does_not_win_over_a_real_file() {
        // A `text/html` entry carrying nothing — a truncated or hand-edited
        // mint. Handing an iframe an empty document is worse than the next
        // file, which is where the real art is.
        let json = r#"{"files":[{"mediaType":"text/html","src":""},{"mediaType":"text/html","src":"data:text/html;utf8,<html>real</html>"}]}"#;
        let art = envelope(json)
            .live_art()
            .expect("the second file is the art");
        assert_eq!(art.src, "data:text/html;utf8,<html>real</html>");
    }

    #[test]
    fn a_plain_image_collection_reports_no_live_art() {
        // The overwhelmingly common case, and the one that must not pay for
        // this: a piece whose only media is a still.
        let json = r#"{"name":"Toolhead","image":"ipfs://QmStill","files":[{"mediaType":"image/webp","src":"ipfs://QmHiRes"}]}"#;
        assert_eq!(envelope(json).live_art(), None);
    }

    #[test]
    fn html_uri_matching_is_exact_at_the_separator() {
        assert!(is_html_uri("data:text/html;utf8,<html>"));
        assert!(is_html_uri("data:text/html;base64,PGh0bWw+"));
        assert!(is_html_uri("data:text/html,<html>"));
        assert!(is_html_uri("DATA:TEXT/HTML;utf8,<html>"));
        assert!(is_html_uri("  data:text/html;utf8,<html>"));

        assert!(!is_html_uri("data:text/html5;utf8,<html>"));
        assert!(!is_html_uri("data:image/png;base64,iVBOR"));
        assert!(!is_html_uri("ipfs://QmStill"));
        assert!(!is_html_uri(""));
        // Multi-byte first character: must not panic on a byte slice.
        assert!(!is_html_uri("日本語 data:text/html"));
    }
}
