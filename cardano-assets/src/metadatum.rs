//! The direct `Metadatum` walk: a CIP-25 document read straight off the CBOR.
//!
//! The document path decodes the aux CBOR to a [`serde_json::Value`] per asset,
//! renders it, and deserializes it into [`crate::AssetEnvelope`] — right for a
//! consumer that holds one document, and wasteful for a walk that holds every
//! one. This module produces the same [`DecodedAsset`] off the `Metadatum` the
//! decode already has, so a folder over the chunk log pays one CBOR decode per
//! transaction rather than one per declared asset.
//!
//! Its specification is the document path, asset for asset: `fold-probe cip25
//! --extract compare` runs both over every declared asset of mainnet and counts
//! the disagreements (see `mitos/docs/design/ROW_ADDRESSING_AND_SEARCH.md` §5.5
//! — 200,132 of 11.1 M differ, and every one of them is accounted for). The
//! classification is not restated here: it is [`classify_field`], the registry
//! both paths share, so a field added to the registry moves both.
//!
//! Two things the walk has to keep straight that the document path gets for
//! free:
//!
//! - **A `files[]` entry must declare its `mediaType`.** [`crate::AssetFile`]
//!   requires it, so an entry without one declines the whole document in both
//!   paths. "Declared nothing" is a headline-only state, and the only spelling
//!   of "sniff this" a minter has.
//! - **The `unsigned_algorithms` set is bespoke.** Its trait values (per-pixel
//!   colours and distributions) are not recoverable by shape dispatch, so the
//!   `unsigs` subtree goes through the same [`unsig_traits`] the document path
//!   runs — via [`metadatum_to_json`] on that subtree alone. That is 0.3% of
//!   declared assets, and the only CBOR-to-JSON step here.
//!
//! Behind the `cip25` feature, like the rest of the label-721 decode.

use crate::cip25::{bytes_to_string, metadatum_key, metadatum_to_json};
use crate::extract::{
    FieldClass, SLOT_KEYS, SlotShape, classify_field, trait_eligible, unsig_traits,
};
use crate::{AssetId, DecodedAsset, Media, MediaRole, MediaType, Traits, UnsigData};
use pallas_codec::utils::KeyValuePairs;
use pallas_primitives::Metadatum;

/// A document as the walk read it: the asset, plus the declarations the walk
/// resolved on the way.
///
/// [`DecodedAsset`] keeps only the resolution — [`MediaType`] is the catalogue's
/// canonical form, and a medium that declared nothing is `Undefined`. What a
/// document *declared* is a different fact from what it meant, and reporting on
/// the corpus needs the other side of it: which spelling a medium carried, and
/// whether it carried one at all.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedDocument {
    /// The asset — id, name, media, traits.
    pub asset: DecodedAsset,
    /// Each medium's declaration before [`MediaType::of`], index-aligned with
    /// `asset.media`. `None` where the document carried none, which only a
    /// headline can be.
    pub declarations: Vec<Option<String>>,
    /// What `resolve_media_type` returned for the *document*: the top-level
    /// `mediaType`, else the media type of the file whose `src` is the image. A
    /// document with no image still declares one, which is what separates this
    /// from the headline's own declaration.
    pub declared_media_type: Option<String>,
}

/// Read one CIP-25 document straight off its `Metadatum`.
///
/// `policy` is the 28-byte policy id and `name` the raw on-chain asset name —
/// the pair [`crate::cip25_metadata_value`] looks the same document up by, and
/// together the asset's [`AssetId`].
///
/// `None` where the document path cannot produce an asset either: a known field
/// carrying a type it does not take, two of one field's aliases present, a
/// `files[]` entry missing `mediaType` or `src`, or an on-chain name
/// [`AssetId`] refuses.
#[must_use]
pub fn decode_document(
    policy: &[u8],
    name: &[u8],
    document: &Metadatum,
) -> Option<DecodedDocument> {
    let Envelope {
        name: declared_name,
        media,
        declared_media_type,
    } = envelope(document)?;
    let traits = metadatum_traits(document)?;
    let id = AssetId::new(hex::encode(policy), hex::encode(name)).ok()?;
    let declarations = media.iter().map(|m| m.declared.clone()).collect();
    let asset = DecodedAsset {
        id,
        name: declared_name,
        media: media.into_iter().map(|m| m.media).collect(),
        traits,
    };
    Some(DecodedDocument {
        asset,
        declarations,
        declared_media_type,
    })
}

/// The traits a document declares, or `None` where the walk cannot read it at
/// all.
///
/// Private: a consumer wants the asset, and [`decode_document`] is that. The
/// document path exposes the twin (`extract::extract_traits`) for callers that
/// hold a `Value` and nothing else.
fn metadatum_traits(value: &Metadatum) -> Option<Traits> {
    let Metadatum::Map(pairs) = value else {
        return None;
    };
    let mut traits = candidate_traits(pairs);
    traits.inner_mut().retain(|key, _| trait_eligible(key));

    // Surface facets (rarity/tier) — `extract_traits`' second pass.
    for (k, v) in pairs.iter() {
        let key = metadatum_key(k);
        if matches!(
            classify_field(&key),
            Some(FieldClass::Facet { surface: true })
        ) && !traits.contains_key(&key)
        {
            insert_if_present(&mut traits, key, v);
        }
    }
    Some(traits)
}

/// Structural trait extraction, BEFORE the registry filter — the document
/// path's `candidate_traits`, over a `Metadatum`: the `unsigned_algorithms`
/// bespoke set, else a structured slot's contents, else every flat top-level
/// field.
fn candidate_traits(pairs: &KeyValuePairs<Metadatum, Metadatum>) -> Traits {
    if let Some(traits) = unsig_candidate(pairs) {
        return traits;
    }
    match slot_value(pairs) {
        Some(v) => extract_slot(v),
        None => flat_all(pairs),
    }
}

/// The `unsigned_algorithms` set, where the document is that shape.
///
/// The one place this module renders CBOR to JSON, and deliberately: the
/// algorithm traits under `unsigs` are not recoverable by shape dispatch, so the
/// subtree goes to the same [`unsig_traits`] the document path uses, through the
/// same [`metadatum_to_json`] that path renders the whole document with.
///
/// `None` where the shape does not decode as [`UnsigData`] — which is exactly
/// where the document path's `from_value` fails and falls through to the generic
/// path, so a document whose `unsigs` key means something else is treated
/// identically by both.
fn unsig_candidate(pairs: &KeyValuePairs<Metadatum, Metadatum>) -> Option<Traits> {
    let (_, unsigs) = pairs
        .iter()
        .find(|(k, v)| metadatum_key(k) == "unsigs" && unsig_shaped(v))?;
    let unsigs: UnsigData = serde_json::from_value(metadatum_to_json(unsigs)).ok()?;
    let sibling = |want: &str| {
        pairs
            .iter()
            .find(|(k, _)| metadatum_key(k) == want)
            .map(|(_, v)| v)
    };
    let series = sibling("series").and_then(json_string_value);
    let source_key = sibling("source_key").map(json_strings).unwrap_or_default();
    let source_tx_id = sibling("source_tx_id").and_then(json_string_value);
    Some(unsig_traits(&unsigs, series, source_key, source_tx_id))
}

/// Whether `unsigs` is the structure [`UnsigData`] takes (its three required
/// keys). Declining on the key alone would mis-handle any collection that uses
/// the name for something else; a document this misses is counted by
/// `--extract compare` as a traits difference, not silently.
fn unsig_shaped(v: &Metadatum) -> bool {
    let Metadatum::Map(kv) = v else {
        return false;
    };
    let has = |want: &str| kv.iter().any(|(k, _)| metadatum_key(k) == want);
    has("index") && has("num_props") && has("properties")
}

/// One medium as the walk resolved it, with the declaration it was resolved
/// from.
struct DeclaredMedia {
    media: Media,
    /// The declaration before [`MediaType::of`]; `None` only where the document
    /// carried none, which only a headline can.
    declared: Option<String>,
}

/// The envelope as [`DecodedAsset`] wants it: a name that can say "no name", and
/// the media list — headline first, one entry per medium.
struct Envelope {
    name: Option<String>,
    media: Vec<DeclaredMedia>,
    /// `resolve_media_type` for the document.
    declared_media_type: Option<String>,
}

/// One `files[]` entry, as [`crate::AssetFile`] reads it.
struct FileEntry {
    media_type: String,
    name: Option<String>,
    src: String,
}

/// The keys [`crate::AssetEnvelope`] consumes before trait extraction — its
/// field names and aliases, matched case-sensitively as serde matches them.
///
/// Serde removes them from `rest`, so the walk must drop them from the flat
/// candidates too: the registry alone would not, because its envelope arm has
/// `mediatype` but not the snake_case `media_type`. `every_consumed_key_leaves_rest`
/// pins this list to that type.
const ENVELOPE_CONSUMED: [&str; 8] = [
    "name",
    "Name",
    "title",
    "image",
    "media_type",
    "mediaType",
    "mediatype",
    "files",
];

/// The document's envelope, over a `Metadatum`.
///
/// `None` where serde would fail on the same document — a known field carrying a
/// type it does not take, or two of one field's aliases present — because
/// [`crate::AssetEnvelope`] errors there too, and a walk that accepted it would
/// be inventing an asset the document path cannot produce.
fn envelope(document: &Metadatum) -> Option<Envelope> {
    let Metadatum::Map(pairs) = document else {
        return None;
    };
    let mut name = None;
    let mut image = None;
    let mut media = None;
    let mut files: Vec<FileEntry> = Vec::new();
    for (k, v) in pairs.iter() {
        match metadatum_key(k).as_str() {
            "name" | "Name" | "title" => {
                if name.is_some() {
                    return None;
                }
                name = Some(json_string_value(v)?);
            }
            "image" => {
                if image.is_some() {
                    return None;
                }
                image = Some(dechunked(v)?);
            }
            "media_type" | "mediaType" | "mediatype" => {
                if media.is_some() {
                    return None;
                }
                media = Some(json_string_value(v)?);
            }
            "files" => files = file_entries(v)?,
            _ => {}
        }
    }
    // `resolve_media_type`: the top-level `mediaType`, else the media type of the
    // file whose (dechunked) `src` is the image.
    let resolved = media.or_else(|| {
        let image = image.as_deref().unwrap_or_default();
        files
            .iter()
            .find(|f| f.src == image)
            .map(|f| f.media_type.clone())
    });
    // The file the headline's own source points at is the headline, not a second
    // medium: folded in, never listed twice.
    let consumed = image
        .as_ref()
        .and_then(|src| files.iter().position(|f| &f.src == src));
    let mut out = Vec::new();
    if let Some(src) = image {
        out.push(DeclaredMedia {
            media: Media {
                src,
                media_type: MediaType::of(resolved.as_deref().unwrap_or_default()),
                name: consumed
                    .and_then(|i| files.get(i))
                    .and_then(|f| f.name.clone()),
                role: MediaRole::Headline,
            },
            declared: resolved.clone(),
        });
    }
    for (i, f) in files.iter().enumerate() {
        if Some(i) == consumed {
            continue;
        }
        out.push(DeclaredMedia {
            media: Media {
                src: f.src.clone(),
                media_type: MediaType::of(&f.media_type),
                name: f.name.clone(),
                role: MediaRole::File,
            },
            declared: Some(f.media_type.clone()),
        });
    }
    Some(Envelope {
        name,
        media: out,
        declared_media_type: resolved,
    })
}

/// `PrimitiveOrList<String>` + `dechunked()`: a string as itself, or CIP-25's
/// >64-byte chunking as an array concatenated in order.
fn dechunked(v: &Metadatum) -> Option<String> {
    match v {
        Metadatum::Text(_) | Metadatum::Bytes(_) => json_string_value(v),
        Metadatum::Array(items) => items
            .iter()
            .map(json_string_value)
            .collect::<Option<Vec<String>>>()
            .map(|chunks| chunks.concat()),
        _ => None,
    }
}

/// `files` as [`crate::AssetFile`] reads it: `mediaType` (or `mediatype`) and
/// `src` required, `name` optional but a string when present, everything else
/// ignored. `None` when an entry is not that shape — the document serde would
/// fail on.
fn file_entries(v: &Metadatum) -> Option<Vec<FileEntry>> {
    let Metadatum::Array(items) = v else {
        return None;
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items.iter() {
        let Metadatum::Map(kv) = item else {
            return None;
        };
        let mut media_type = None;
        let mut src = None;
        let mut name = None;
        for (k, v) in kv.iter() {
            match metadatum_key(k).as_str() {
                "mediaType" | "mediatype" => {
                    if media_type.is_some() {
                        return None;
                    }
                    media_type = Some(json_string_value(v)?);
                }
                "src" => {
                    if src.is_some() {
                        return None;
                    }
                    src = Some(dechunked(v)?);
                }
                "name" => {
                    if name.is_some() {
                        return None;
                    }
                    name = Some(json_string_value(v)?);
                }
                _ => {}
            }
        }
        out.push(FileEntry {
            media_type: media_type?,
            name,
            src: src?,
        });
    }
    Some(out)
}

/// The slot shapes [`classify_slot`] recognises, on a `Metadatum` this time —
/// the document path's [`SlotShape`], reused so the two cannot drift apart.
fn classify_slot(value: &Metadatum) -> Option<SlotShape> {
    match value {
        Metadatum::Map(_) => Some(SlotShape::Map),
        Metadatum::Array(items) if !items.is_empty() => {
            if items.iter().all(|i| matches!(i, Metadatum::Map(_))) {
                let codified = items.iter().all(|i| {
                    let Metadatum::Map(kv) = i else {
                        return false;
                    };
                    let has = |want: &str| kv.iter().any(|(k, _)| metadatum_key(k) == want);
                    has("value") && (has("trait_type") || has("name"))
                });
                Some(if codified {
                    SlotShape::Codified
                } else {
                    SlotShape::ObjectArray
                })
            } else if items.iter().all(is_json_string) {
                let any_colon = items
                    .iter()
                    .any(|i| json_string_value(i).is_some_and(|s| s.contains(':')));
                any_colon.then_some(SlotShape::ColonDelimited)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The document path renders text and bytes as JSON strings; the shape tests
/// care which.
fn is_json_string(m: &Metadatum) -> bool {
    matches!(m, Metadatum::Text(_) | Metadatum::Bytes(_))
}

/// A JSON string value as the document path sees it — text as itself, bytes as
/// UTF-8 or `0x…` hex.
fn json_string_value(m: &Metadatum) -> Option<String> {
    match m {
        Metadatum::Text(s) => Some(s.clone()),
        Metadatum::Bytes(b) => Some(bytes_to_string(b)),
        _ => None,
    }
}

/// `json_to_strings`, over a `Metadatum`: scalars to one trimmed string, arrays
/// to their scalar elements, objects and empties to nothing.
fn json_strings(v: &Metadatum) -> Vec<String> {
    match v {
        Metadatum::Text(s) => trimmed(s),
        Metadatum::Bytes(b) => trimmed(&bytes_to_string(b)),
        Metadatum::Int(i) => vec![i128::from(*i).to_string()],
        Metadatum::Array(items) => items
            .iter()
            .flat_map(|item| match item {
                Metadatum::Text(s) => trimmed(s),
                Metadatum::Bytes(b) => trimmed(&bytes_to_string(b)),
                Metadatum::Int(i) => vec![i128::from(*i).to_string()],
                _ => Vec::new(),
            })
            .collect(),
        Metadatum::Map(_) => Vec::new(),
    }
}

fn trimmed(s: &str) -> Vec<String> {
    let t = s.trim();
    if t.is_empty() {
        Vec::new()
    } else {
        vec![t.to_string()]
    }
}

/// `insert_if_present`: a trait goes in only when the value coerces to something.
fn insert_if_present(traits: &mut Traits, key: String, value: &Metadatum) {
    let values = json_strings(value);
    if !values.is_empty() {
        traits.insert_vec(key, values);
    }
}

/// The first structured slot's value, or `None` for the flat path.
fn slot_value(pairs: &KeyValuePairs<Metadatum, Metadatum>) -> Option<&Metadatum> {
    for slot in SLOT_KEYS {
        if let Some(v) = pairs
            .iter()
            .find(|(k, _)| metadatum_key(k).to_lowercase() == *slot)
            .map(|(_, v)| v)
            && classify_slot(v).is_some()
        {
            return Some(v);
        }
    }
    None
}

/// `flat_all`: every top-level field the envelope did not consume.
fn flat_all(pairs: &KeyValuePairs<Metadatum, Metadatum>) -> Traits {
    let mut traits = Traits::new();
    for (k, v) in pairs.iter() {
        let key = metadatum_key(k);
        if !ENVELOPE_CONSUMED.contains(&key.as_str()) {
            insert_if_present(&mut traits, key, v);
        }
    }
    traits
}

/// `extract_slot`, over a `Metadatum`.
fn extract_slot(value: &Metadatum) -> Traits {
    let mut traits = Traits::new();
    match classify_slot(value) {
        Some(SlotShape::Map) => {
            if let Metadatum::Map(map) = value {
                for (k, v) in map.iter() {
                    insert_if_present(&mut traits, metadatum_key(k), v);
                }
            }
        }
        Some(SlotShape::Codified) => {
            if let Metadatum::Array(items) = value {
                for item in items.iter() {
                    let Metadatum::Map(kv) = item else { continue };
                    let find = |want: &str| kv.iter().find(|(k, _)| metadatum_key(k) == want);
                    // `trait_type` wins if present, and the key must be a JSON
                    // string — exactly what `.as_str()` demands upstream.
                    let key = find("trait_type")
                        .or_else(|| find("name"))
                        .and_then(|(_, v)| json_string_value(v));
                    let (Some(key), Some((_, val))) = (key, find("value")) else {
                        continue;
                    };
                    insert_if_present(&mut traits, key, val);
                }
            }
        }
        Some(SlotShape::ObjectArray) => {
            if let Metadatum::Array(items) = value {
                for item in items.iter() {
                    let Metadatum::Map(kv) = item else { continue };
                    for (k, v) in kv.iter() {
                        insert_if_present(&mut traits, metadatum_key(k), v);
                    }
                }
            }
        }
        Some(SlotShape::ColonDelimited) => {
            if let Metadatum::Array(items) = value {
                for item in items.iter() {
                    let Some(s) = json_string_value(item) else {
                        continue;
                    };
                    let Some((key, val)) = s.split_once(':') else {
                        continue;
                    };
                    let (key, val) = (key.trim(), val.trim());
                    if !key.is_empty() && !val.is_empty() {
                        traits.insert_single(key.to_string(), val.to_string());
                    }
                }
            }
        }
        None => {}
    }
    traits
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: &[u8] = &[0xab; 28];

    fn text(s: &str) -> Metadatum {
        Metadatum::Text(s.to_string())
    }

    fn map(pairs: Vec<(&str, Metadatum)>) -> Metadatum {
        Metadatum::Map(KeyValuePairs::Def(
            pairs.into_iter().map(|(k, v)| (text(k), v)).collect(),
        ))
    }

    /// A CBOR small integer (`0x00`..`0x17` are 0..23), so a test can carry an
    /// integer value without reaching for pallas's `Int` constructor.
    fn int(n: u8) -> Metadatum {
        assert!(n < 24, "small ints only");
        pallas_codec::minicbor::decode::<Metadatum>(&[n]).expect("small int decodes")
    }

    /// What the document path makes of the same bytes, for the parity tests —
    /// the same two steps `fold-probe --extract compare` takes.
    fn platform_traits(document: &Metadatum) -> Traits {
        crate::asset_from_metadata_value(metadatum_to_json(document))
            .expect("the document path reads it")
            .traits
    }

    /// The walk classifies through the crate's own registry, so the fields that
    /// are never traits stay out of the set.
    #[test]
    fn the_walk_applies_the_registry() {
        let doc = map(vec![
            ("name", text("Luffy")),
            ("image", text("ipfs://x")),
            ("description", text("a pirate")),
            ("id", text("42")),
            ("Background", text("Frost Realm")),
            ("Class", text("Archer")),
            ("Rarity", text("Epic")),
        ]);
        let traits = metadatum_traits(&doc).expect("flat map");
        assert_eq!(
            traits.get_single("Background").as_deref(),
            Some("Frost Realm")
        );
        assert_eq!(traits.get_single("Class").as_deref(), Some("Archer"));
        assert_eq!(traits.get_single("Rarity").as_deref(), Some("Epic"));
        assert!(!traits.contains_key("name"));
        assert!(!traits.contains_key("image"));
        assert!(!traits.contains_key("description"));
        assert!(!traits.contains_key("id"));
    }

    /// A recognised slot wins over the flat fields, and the codified shape reads
    /// `trait_type` (or `name`) with `value`.
    #[test]
    fn the_walk_reads_a_codified_slot_and_ignores_flat_fields() {
        let doc = map(vec![
            ("name", text("N")),
            (
                "attributes",
                Metadatum::Array(vec![
                    map(vec![
                        ("trait_type", text("Class")),
                        ("value", text("Archer")),
                    ]),
                    map(vec![("name", text("Eyes")), ("value", text("Red"))]),
                ]),
            ),
            ("Stray", text("not a trait, a slot won")),
        ]);
        let traits = metadatum_traits(&doc).expect("slot");
        assert_eq!(traits.get_single("Class").as_deref(), Some("Archer"));
        assert_eq!(traits.get_single("Eyes").as_deref(), Some("Red"));
        assert!(!traits.contains_key("Stray"));
    }

    /// A `properties`/`traits` slot of colon-delimited strings, trimmed on both
    /// sides, with the non-conforming entry simply not a trait.
    #[test]
    fn the_walk_reads_a_colon_delimited_slot() {
        let doc = map(vec![(
            "properties",
            Metadatum::Array(vec![
                text("State: Delusional"),
                text("Mood:  Calm "),
                text("no colon"),
            ]),
        )]);
        let traits = metadatum_traits(&doc).expect("slot");
        assert_eq!(traits.get_single("State").as_deref(), Some("Delusional"));
        assert_eq!(traits.get_single("Mood").as_deref(), Some("Calm"));
        assert_eq!(traits.keys().count(), 2);
    }

    /// Values coerce the way the document path coerces them: an integer is a
    /// decimal string, an array is multi-valued and trims, and an empty string or
    /// a nested object is not a trait at all.
    #[test]
    fn the_walk_coerces_values_like_the_document() {
        let doc = map(vec![
            ("Level", int(15)),
            ("Empty", text("")),
            ("Nested", map(vec![("Inner", text("dropped"))])),
            (
                "Layers",
                Metadatum::Array(vec![text(" A "), text(""), text("B")]),
            ),
        ]);
        let traits = metadatum_traits(&doc).expect("flat map");
        assert_eq!(traits.get_single("Level").as_deref(), Some("15"));
        assert_eq!(
            traits.get("Layers"),
            Some(&vec!["A".to_string(), "B".to_string()])
        );
        assert!(!traits.contains_key("Empty"));
        assert!(!traits.contains_key("Nested"));
    }

    /// The one branch that renders CBOR to JSON, and the reason it does: the
    /// algorithm traits under `unsigs` cannot be recovered by shape dispatch, so
    /// both paths hand the subtree to the same reader. A document in this shape
    /// therefore produces the same `Traits` here as the document path produces
    /// from the same bytes.
    #[test]
    fn the_unsig_shape_gets_the_bespoke_set_from_either_path() {
        let doc = map(vec![
            ("title", text("Unsig 1")),
            ("image", text("ipfs://x")),
            ("series", text("Series 1")),
            ("source_key", text("abc")),
            (
                "unsigs",
                map(vec![
                    ("index", int(1)),
                    ("num_props", int(3)),
                    ("properties", map(vec![("Colour", text("Red"))])),
                ]),
            ),
        ]);
        let walked = metadatum_traits(&doc).expect("the bespoke set");
        assert_eq!(walked, platform_traits(&doc));
        assert_eq!(walked.get_single("num_props").as_deref(), Some("3"));
        assert_eq!(walked.get_single("Colour").as_deref(), Some("Red"));
        // The registry drops the provenance members on both paths.
        assert!(!walked.contains_key("index"));
        assert!(!walked.contains_key("source_key"));
    }

    /// An `unsigs` key that is not the bespoke shape is not the bespoke set:
    /// both paths fall through to the generic extraction rather than declining.
    #[test]
    fn an_unsig_key_of_another_shape_is_not_the_bespoke_set() {
        let doc = map(vec![
            ("unsigs", text("just a string")),
            ("Class", text("Archer")),
        ]);
        let walked = metadatum_traits(&doc).expect("the generic set");
        assert_eq!(walked.get_single("Class").as_deref(), Some("Archer"));
        assert_eq!(walked, platform_traits(&doc));
    }

    /// Every key the walk treats as consumed really is consumed by
    /// [`crate::AssetEnvelope`] — the list is a mirror of that type's fields and
    /// aliases, and serde's `flatten` is the only thing that keeps the two in
    /// step. A field added there without a line here would become a trait.
    #[test]
    fn every_consumed_key_leaves_rest() {
        for key in ENVELOPE_CONSUMED {
            // One key at a time: two aliases of one field in a document is itself
            // a document `AssetEnvelope` rejects.
            let mut object = serde_json::Map::new();
            object.insert(
                key.to_string(),
                if key == "files" {
                    serde_json::Value::Array(Vec::new())
                } else {
                    serde_json::Value::String("n".into())
                },
            );
            object.insert("Class".into(), serde_json::Value::String("Archer".into()));
            let envelope: crate::AssetEnvelope =
                serde_json::from_value(serde_json::Value::Object(object))
                    .expect("a document with one envelope field");
            assert!(
                !envelope.rest.contains_key(key),
                "{key} is in `rest`, so the document path would make it a trait"
            );
            assert!(envelope.rest.contains_key("Class"));
        }
    }

    /// The walk produces the whole `DecodedAsset`: the id the document path has
    /// no way to carry, the media list the old flatten dropped, and the image
    /// chunk-joined the way CIP-25 demands.
    #[test]
    fn decode_document_builds_an_id_and_a_media_list() {
        let chunks = || Metadatum::Array(vec![text("ipfs://Qm"), text("abcdef")]);
        let doc = map(vec![
            ("name", text("Luffy")),
            ("image", chunks()),
            (
                "files",
                Metadatum::Array(vec![
                    map(vec![
                        ("mediaType", text("image/png")),
                        ("name", text("the still")),
                        ("src", chunks()),
                    ]),
                    map(vec![
                        ("mediaType", text("video/mp4")),
                        ("src", text("ipfs://Qmvideo")),
                    ]),
                ]),
            ),
            ("Class", text("Archer")),
        ]);
        let walked = decode_document(POLICY, b"Luffy", &doc).expect("an asset");
        let asset = &walked.asset;
        assert_eq!(asset.id.policy_id, hex::encode(POLICY));
        assert_eq!(asset.id.asset_name_hex, hex::encode(b"Luffy"));
        assert_eq!(asset.name.as_deref(), Some("Luffy"));
        assert_eq!(asset.image(), Some("ipfs://Qmabcdef"));
        // The media type is not at the top level, so it comes from the file whose
        // (dechunked) `src` is the image — which is consumed by the headline...
        assert_eq!(asset.media_type().as_str(), Some("image/png"));
        // ...and the declared spelling survives beside it, which is what the
        // alias table is chosen from.
        assert_eq!(walked.declared_media_type.as_deref(), Some("image/png"));
        assert_eq!(
            walked.declarations,
            vec![Some("image/png".to_string()), Some("video/mp4".to_string())]
        );
        assert_eq!(
            asset.headline().and_then(|m| m.name.as_deref()),
            Some("the still")
        );
        // ...and is not repeated as a second medium. The video is a file.
        let files: Vec<&Media> = asset.files().collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].src, "ipfs://Qmvideo");
        assert_eq!(files[0].media_type.as_str(), Some("video/mp4"));
        assert_eq!(asset.traits.get_single("Class").as_deref(), Some("Archer"));
    }

    /// A known field carrying the wrong type, a duplicated alias, a file without
    /// its required fields, or an empty on-chain name: each is something the
    /// document path cannot produce, so the walk declines instead of inventing an
    /// asset.
    #[test]
    fn decode_document_declines_what_the_document_path_cannot_produce() {
        let bad_name = map(vec![("name", int(7)), ("Class", text("Archer"))]);
        assert!(decode_document(POLICY, b"x", &bad_name).is_none());
        let duplicate = map(vec![("name", text("a")), ("Name", text("b"))]);
        assert!(decode_document(POLICY, b"x", &duplicate).is_none());
        let bad_file = map(vec![(
            "files",
            Metadatum::Array(vec![map(vec![("src", text("ipfs://x"))])]),
        )]);
        assert!(decode_document(POLICY, b"x", &bad_file).is_none());
        let plain = map(vec![("Class", text("Archer"))]);
        assert!(decode_document(POLICY, b"", &plain).is_none());
    }

    /// A declared type with no medium to attach it to: the headline is absent, so
    /// the media list is empty — and the declaration survives only in
    /// `declared_media_type`, where the corpus reporting reads it.
    #[test]
    fn a_declared_type_with_no_medium_is_still_recorded() {
        let doc = map(vec![("mediaType", text("image/png"))]);
        let walked = decode_document(POLICY, b"x", &doc).expect("an asset");
        assert!(walked.asset.media.is_empty());
        assert!(walked.asset.headline().is_none());
        assert_eq!(walked.declared_media_type.as_deref(), Some("image/png"));
    }

    /// The one `name` difference in 11.1 M assets: the same asset declared twice
    /// under its policy. `cip25_metadata_value` finds metadata with `find_in_map`,
    /// which stops at the first declaration; this walk enumerates the document as
    /// written, so it reads both. Both cannot be right about one asset — and the
    /// corpus says the case is one asset in eleven million.
    #[test]
    fn a_duplicated_declaration_is_read_twice_here_and_once_upstream() {
        use std::collections::BTreeMap;

        let declaration =
            |name: &str| Metadatum::Map(KeyValuePairs::Def(vec![(text("name"), text(name))]));
        let doc = Metadatum::Map(KeyValuePairs::Def(vec![
            (
                text("LoveForCrypto1of25"),
                declaration("LoveForCrypto 1 of 25"),
            ),
            (
                text("LoveForCrypto1of25"),
                declaration("LoveForCrypto 25 of 25"),
            ),
        ]));

        // Upstream sees the first declaration, whichever one you ask about.
        let mut metadata: pallas_primitives::Metadata = BTreeMap::new();
        metadata.insert(
            721u64,
            Metadatum::Map(KeyValuePairs::Def(vec![(
                Metadatum::Bytes(POLICY.to_vec().into()),
                doc.clone(),
            )])),
        );
        let cbor = pallas_codec::minicbor::to_vec(
            pallas_primitives::alonzo::AuxiliaryData::Shelley(metadata),
        )
        .expect("encodes");
        let found =
            crate::cip25_metadata_value(&cbor, POLICY, b"LoveForCrypto1of25").expect("found");
        assert_eq!(
            found.get("name").and_then(|v| v.as_str()),
            Some("LoveForCrypto 1 of 25")
        );

        // This walk sees both, which is where the difference comes from.
        let Metadatum::Map(entries) = &doc else {
            panic!("a map")
        };
        let names: Vec<Option<String>> = entries
            .iter()
            .map(|(_, v)| {
                decode_document(POLICY, b"LoveForCrypto1of25", v)
                    .expect("an asset")
                    .asset
                    .name
            })
            .collect();
        assert_eq!(
            names,
            vec![
                Some("LoveForCrypto 1 of 25".to_string()),
                Some("LoveForCrypto 25 of 25".to_string())
            ]
        );
    }
}
