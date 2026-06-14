//! Golden corpus for the v2 envelope+pattern extractor
//! (`cardano_assets::extract`).
//!
//! For every per-asset metadata fixture this pins the v2 output and
//! states its relationship to the legacy v1 (`AssetMetadata` -> `Asset`)
//! output. Most fixtures must be byte-for-byte identical to v1
//! (`Expect::SameAsV1`); the rest are *intentional corrections* of
//! known v1 bugs, each spelled out so any future drift is caught and
//! every behavioural difference is reviewed rather than silent.
//!
//! **Exhaustiveness is enforced.** `corpus_is_exhaustive` walks
//! `resources/test/` and fails if any fixture is neither in the corpus
//! below nor in `NON_METADATA_FIXTURES` (with a reason). So a new
//! fixture can't silently go unverified under v2 — you must classify it.

use cardano_assets::{asset_from_metadata_json, Asset, AssetMetadata, Traits};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test")
}

fn fixture(name: &str) -> String {
    fs::read_to_string(fixtures_dir().join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

fn sorted(t: &Traits) -> BTreeMap<String, Vec<String>> {
    t.inner().clone().into_iter().collect()
}

fn v1_traits(raw: &str) -> BTreeMap<String, Vec<String>> {
    match serde_json::from_str::<AssetMetadata>(raw) {
        Ok(meta) => sorted(&Asset::from(meta).traits),
        Err(_) => BTreeMap::new(),
    }
}

fn v2_traits(raw: &str) -> BTreeMap<String, Vec<String>> {
    sorted(&asset_from_metadata_json(raw).expect("v2 decode").traits)
}

fn map(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    pairs
        .iter()
        .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
        .collect()
}

enum Expect {
    /// v2 reproduces v1 exactly (behaviour-preserving).
    SameAsV1,
    /// v2 == v1 with these keys removed — v1 leaked an envelope field
    /// into traits, or folded a sibling of a structured trait slot.
    V1Minus(&'static [&'static str]),
    /// v2 produces this exact trait map, where v1 was wrong enough that
    /// "v1 minus keys" doesn't describe it. `note` says why.
    Exact {
        traits: &'static [(&'static str, &'static [&'static str])],
        note: &'static str,
    },
}

/// Every per-asset metadata fixture and its v2 expectation. The single
/// source of truth for what the v2 extractor must produce.
fn corpus_cases() -> Vec<(&'static str, Expect)> {
    use Expect::*;
    vec![
        // ---- behaviour-preserving: identical to v1 -------------------
        ("traits-chadano.json", SameAsV1), // codified {trait_type,value}
        ("traits-mallards.json", SameAsV1), // array of single-key objects
        ("traits-anscestors.json", SameAsV1), // flat top-level keys ("Type" is a visual trait)
        ("traits-toolhead.json", SameAsV1), // nested attributes map (single+multi)
        ("traits-wiseowl.json", SameAsV1), // flat, chunked image ("type" visual)
        // ---- registry-driven corrections (field classification) ------
        // v1 leaked the collection name (`collection`/`Collection`) as a
        // per-asset trait; the registry classifies it as a facet (dropped
        // from the trait surface).
        ("bankcard2500.json", V1Minus(&["collection"])),
        // oldmoney leaked a pile of facet/provenance fields as traits:
        // `Collection`/`Series` (facets), `Number`/`Note Number`/`Plate
        // Number`/`Serial Number` (provenance identifiers). Registry drops
        // them from the trait surface; the visual traits (Base, Blood, …)
        // stay.
        (
            "traits-oldmoney.json",
            V1Minus(&[
                "Collection",
                "Number",
                "Note Number",
                "Plate Number",
                "Serial Number",
                "Series",
            ]),
        ),
        // gopher's codified traits redundantly include `Name` (the display
        // name); the registry classifies `name` as envelope → dropped from
        // the trait surface even inside a slot.
        ("traits-gopher.json", V1Minus(&["Name"])),
        // `series` is a sibling of the `attributes` slot — metadata, not
        // a visual trait. v1 folded it in via merge_extra_fields.
        ("derpbird07490.json", V1Minus(&["series"])),
        // Asset-shaped fixtures (carry an `id`): a flat `traits` map with
        // `id` as a sibling. v1 flattened `id` in as a trait; v2 reads
        // the map slot and drops the `id` sibling. Same content in both
        // (https_image is the Pirate #84 record with an https image).
        ("5069726174653834.json", V1Minus(&["id"])),
        ("https_image.json", V1Minus(&["id"])),
        // The big one: a numeric `Rank: 1` inside `attributes` made the
        // v1 `Attributed` variant fail to deserialize, so it fell through
        // to `FlattenedMixed`, which DROPPED the whole attributes object
        // and emitted the metadata siblings instead. v2 reads the map
        // (coercing the number) and ignores the siblings.
        (
            "pred09193.json",
            Exact {
                traits: &[
                    ("Background", &["Infiltration"]),
                    ("Body", &["Farmer's Body"]),
                    ("Face", &["Trooper's Face"]),
                    ("Head", &["Marksman's ???"]),
                    ("Rank", &["1"]),
                    ("Skin Color", &["Gray"]),
                ],
                note: "numeric attribute no longer discards the trait set",
            },
        ),
        // BlockGen authority tokens have no `name` (v1 -> `Untitled` ->
        // empty traits) but DO carry `properties: {type: master}`, a
        // structured slot. v2 surfaces it and ignores the envelope
        // siblings (artist/medium/vendor/projectPolicyId).
        (
            "blockgen-auth-master.json",
            Exact {
                traits: &[("type", &["master"])],
                note: "properties slot on a nameless authority token",
            },
        ),
        (
            "blockgen-auth-playground.json",
            Exact {
                traits: &[("type", &["master"])],
                note: "properties slot on a nameless authority token",
            },
        ),
        (
            "blockgen-artist-hookman.json",
            Exact {
                traits: &[("type", &["master"])],
                note: "properties slot on a nameless authority token",
            },
        ),
        (
            "blockgen-artist-autrecoeur.json",
            Exact {
                traits: &[("type", &["master"])],
                note: "properties slot on a nameless authority token",
            },
        ),
        (
            "blockgen-artist-charlesmachin.json",
            Exact {
                traits: &[("type", &["master"])],
                note: "properties slot on a nameless authority token",
            },
        ),
        // Funplastic: visual traits nested in an `attributes` map, with
        // `Rarity` as a top-level sibling. v1 surfaced only the flat
        // siblings (Artist/royalties/Rarity/…) and dropped the whole
        // attributes block. v2 reads the slot AND promotes `Rarity` (a
        // PROMOTE_KEY) while ignoring the metadata siblings (Artist,
        // Artist Website, Id, royalties). Name-Size/Used Slots are kept
        // here (the worker's all-numeric cardinality filter drops them
        // downstream, not the extractor).
        (
            "funplastic.json",
            Exact {
                traits: &[
                    ("Body Color", &["B"]),
                    ("Ears", &["Bush"]),
                    ("Ears Color", &["C"]),
                    ("Faces", &["Ummm"]),
                    ("Faces Color", &["A"]),
                    ("Horns", &["Dead Cat"]),
                    ("Horns Color", &["C"]),
                    ("Items", &["None"]),
                    ("Material", &["BubbleGum"]),
                    ("Medallion", &["Cuadrado"]),
                    ("Medallion Color", &["C"]),
                    ("Name-Size", &["7"]),
                    ("On Top", &["Dino"]),
                    ("On Top Color", &["C"]),
                    ("Scene", &["Plantibolas"]),
                    ("Scene Color", &["Cold Winter"]),
                    ("Tails", &["Mono"]),
                    ("Tails Color", &["A"]),
                    ("Used Slots", &["6"]),
                    ("Rarity", &["Eggcellent"]),
                ],
                note: "attributes slot + promoted Rarity sibling, metadata siblings dropped",
            },
        ),
    ]
}

/// Fixtures in `resources/test/` that are deliberately NOT per-asset
/// metadata, so they're out of scope for the v2 extractor. Listed here
/// (with a reason) so `corpus_is_exhaustive` can prove the corpus covers
/// everything else — nothing is silently skipped.
const NON_METADATA_FIXTURES: &[(&str, &str)] = &[
    (
        "blackflag-traits.json",
        "collection trait-summary (parsed as TraitSummarySorted), not per-asset metadata",
    ),
    (
        "swatch_tagged_union.json",
        "empty/invalid fixture (decode error expected)",
    ),
    (
        "ug_mint.json",
        "CIP-25 mint-tx CBOR aux-data (decode_cip25 test input), not asset-metadata JSON",
    ),
];

#[test]
fn extractor_corpus() {
    use Expect::*;
    for (name, expect) in corpus_cases() {
        let raw = fixture(name);
        let v2 = v2_traits(&raw);
        match expect {
            SameAsV1 => {
                assert_eq!(v2, v1_traits(&raw), "{name}: v2 must match v1");
            }
            V1Minus(removed) => {
                let mut want = v1_traits(&raw);
                for k in removed {
                    assert!(want.remove(*k).is_some(), "{name}: v1 lacked key {k:?}");
                }
                assert_eq!(v2, want, "{name}: v2 must equal v1 minus {removed:?}");
            }
            Exact { traits, note } => {
                assert_eq!(v2, map(traits), "{name}: {note}");
            }
        }
    }
}

/// Guard: every `*.json` fixture on disk must be accounted for — either
/// pinned in `corpus_cases()` or listed in `NON_METADATA_FIXTURES`. A
/// fixture added without classifying it fails here, so the v2 corpus
/// can never silently fall behind the fixture set.
#[test]
fn corpus_is_exhaustive() {
    let covered: std::collections::HashSet<&str> = corpus_cases().iter().map(|(n, _)| *n).collect();
    let skipped: std::collections::HashSet<&str> =
        NON_METADATA_FIXTURES.iter().map(|(n, _)| *n).collect();

    let mut unclassified = Vec::new();
    for entry in fs::read_dir(fixtures_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map(|x| x == "json").unwrap_or(false) {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !covered.contains(name.as_str()) && !skipped.contains(name.as_str()) {
                unclassified.push(name);
            }
        }
    }
    unclassified.sort();
    assert!(
        unclassified.is_empty(),
        "fixtures not covered by the v2 corpus (add to corpus_cases() or \
         NON_METADATA_FIXTURES): {unclassified:?}"
    );
}

/// SpaceBudz-style CIP-68: `traits` is a value-only string array (kept
/// as a multi-value flat field, NOT a structured slot) and `type` is a
/// flat sibling that should still surface; `image`/`sha256` must not.
/// (No fixture file — locked here to match the existing v1 inline test.)
#[test]
fn value_only_traits_array_is_flat() {
    let json = r#"{
        "name": "SpaceBud #0",
        "traits": ["Star Suit", "Chestplate", "Belt", "Covered Helmet"],
        "type": "Frog",
        "image": "ipfs://bafkreicbn7uu2wyfpzjgterlumqk2pxisww7hszdfupwy2lydtsq3prufq",
        "sha256": "416fe94d5b057e5269922ba320ad3ee895adf3cb232d1f6c69781ce50dbe342c"
    }"#;
    let asset = asset_from_metadata_json(json).expect("decode");
    assert_eq!(asset.name, "SpaceBud #0");
    assert_eq!(asset.traits.get("type"), Some(&vec!["Frog".to_string()]));
    let mut gadgets = asset.traits.get("traits").cloned().unwrap_or_default();
    gadgets.sort();
    assert_eq!(
        gadgets,
        vec!["Belt", "Chestplate", "Covered Helmet", "Star Suit"]
    );
    assert!(!asset.traits.contains_key("sha256"));
    assert!(!asset.traits.contains_key("image"));
}

/// Colon-delimited attribute arrays (BlockOwls-style) parse into pairs.
#[test]
fn colon_delimited_attributes() {
    let json = r#"{
        "name": "BlockOwl #1",
        "image": "ipfs://abc",
        "attributes": ["State: Delusional", "Mood: Gloomy"]
    }"#;
    let asset = asset_from_metadata_json(json).expect("decode");
    assert_eq!(
        asset.traits.get("State"),
        Some(&vec!["Delusional".to_string()])
    );
    assert_eq!(asset.traits.get("Mood"), Some(&vec!["Gloomy".to_string()]));
}
