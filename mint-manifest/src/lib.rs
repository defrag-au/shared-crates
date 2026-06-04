//! Mint manifest — the mint-ready interchange format between collection
//! *generators* (the hodlcroft compositor + its IPFS pinner, future tools) and
//! the minting *import* pipeline.
//!
//! See `cnft.dev-workers/docs/design/MINT_MANIFEST.md` for the full contract.
//! In short: a manifest is JSONL (one [`MintManifestEntry`] per line) describing
//! **mint-ready, off-chain-media** pieces — `image` is a pre-pinned `ipfs://` /
//! `ar://` URI (the pipeline never uploads or pins). Each entry carries the
//! piece's *fields*; the importer composes the CIP-25 inner from them via
//! [`MintManifestEntry::to_cip25_inner`].
//!
//! This crate is pure data + composition — no I/O, no chunking (the CIP-25
//! builder auto-chunks long strings at encode time).

use cardano_assets::{MintSupply, Traits};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One mint-ready piece in a manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintManifestEntry {
    /// On-chain asset name — the generator decides it (the pipeline never
    /// invents one); hex-encoded to `asset_name_hex` downstream. Must be
    /// ≤ 32 bytes (enforced by the import validate pass).
    pub asset_name: String,
    /// CIP-25 display name.
    pub name: String,
    /// Optional CIP-25 description. May be a plain string of any length — the
    /// CIP-25 builder chunks it at encode time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Primary media — a pre-pinned `ipfs://` or `ar://` URI.
    pub image: String,
    /// Additional media (animations, alt formats). Optional, multiple.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<ManifestFile>,
    /// Traits — any accepted `cardano_assets::Traits` shape (single value →
    /// string, multi-value → array on-chain). Arbitrary fields like `artist`
    /// ride here as trait keys.
    #[serde(default)]
    pub traits: Traits,
    /// Print run. Absent ⇒ `Quota(1)` (unique 1-of-1), a number ⇒ `Quota(n)`,
    /// explicit `null` ⇒ `Uncapped` (→ a `NULL` supply column).
    #[serde(default)]
    pub max_supply: MintSupply,
}

/// A CIP-25 `files[]` entry. `src` is an `ipfs://` / `ar://` URI; `media_type`
/// is optional (emitted as the CIP-25 `mediaType` key when present).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub src: String,
}

impl ManifestFile {
    /// Compose this file as a CIP-25 `files[]` object (`mediaType` camelCase).
    fn to_cip25(&self) -> Value {
        let mut m = Map::new();
        if let Some(name) = &self.name {
            m.insert("name".into(), Value::String(name.clone()));
        }
        if let Some(media_type) = &self.media_type {
            m.insert("mediaType".into(), Value::String(media_type.clone()));
        }
        m.insert("src".into(), Value::String(self.src.clone()));
        Value::Object(m)
    }
}

impl MintManifestEntry {
    /// Compose the per-asset CIP-25 **inner** object (the map stored verbatim in
    /// `collection_assets.metadata_blob` and wrapped as
    /// `{"721":{policy:{asset_name:<inner>}}}` at mint time):
    /// `name` + optional `description` + `image` + optional `files` + each trait
    /// as a top-level key. No chunking — the CIP-25 builder auto-chunks long
    /// strings at encode time.
    pub fn to_cip25_inner(&self) -> Value {
        let mut inner = Map::new();
        inner.insert("name".into(), Value::String(self.name.clone()));
        if let Some(desc) = &self.description {
            inner.insert("description".into(), Value::String(desc.clone()));
        }
        inner.insert("image".into(), Value::String(self.image.clone()));
        if !self.files.is_empty() {
            inner.insert(
                "files".into(),
                Value::Array(self.files.iter().map(ManifestFile::to_cip25).collect()),
            );
        }
        // Traits → top-level keys: single value as a string, multi-value as an
        // array (CIP-25 convention; mirrors `compose_cip25_inner`).
        for (key, values) in self.traits.iter() {
            let val = match values.as_slice() {
                [single] => Value::String(single.clone()),
                many => Value::Array(many.iter().map(|s| Value::String(s.clone())).collect()),
            };
            inner.insert(key.clone(), val);
        }
        Value::Object(inner)
    }

    /// Manifest-specific gate: `image` and every `files[].src` must be an
    /// off-chain `ipfs://` / `ar://` link (the manifest never carries inline
    /// `data:` art — that's the on-chain `ConstructDirSource` path). The other
    /// gates (asset_name, tx-size, …) are the shared `minting_core` validate
    /// pass; this is the bit only the manifest knows.
    pub fn check_media_links(&self) -> Result<(), String> {
        if !is_offchain_uri(&self.image) {
            return Err(format!(
                "image must be an ipfs:// or ar:// URI, got: {}",
                self.image
            ));
        }
        for (i, file) in self.files.iter().enumerate() {
            if !is_offchain_uri(&file.src) {
                return Err(format!(
                    "files[{i}].src must be an ipfs:// or ar:// URI, got: {}",
                    file.src
                ));
            }
        }
        Ok(())
    }
}

fn is_offchain_uri(s: &str) -> bool {
    s.starts_with("ipfs://") || s.starts_with("ar://")
}

/// Parse a JSONL manifest (one [`MintManifestEntry`] per line). Blank /
/// whitespace-only lines are skipped. Fails fast on the first malformed line,
/// reporting its 1-based line number.
pub fn parse_jsonl(content: &str) -> Result<Vec<MintManifestEntry>, String> {
    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry = serde_json::from_str::<MintManifestEntry>(line)
            .map_err(|e| format!("line {}: {e}", idx + 1))?;
        out.push(entry);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"{"asset_name":"FirstOnes0002","name":"Lumen","description":"a long line","image":"ipfs://Qmimg","files":[{"name":"anim","media_type":"video/mp4","src":"ipfs://Qmanim"}],"traits":{"Order":"Stable","Palette":["pA870","pA14"]},"max_supply":50}"#;

    #[test]
    fn deserializes_full_entry() {
        let e: MintManifestEntry = serde_json::from_str(LINE).unwrap();
        assert_eq!(e.asset_name, "FirstOnes0002");
        assert_eq!(e.name, "Lumen");
        assert_eq!(e.description.as_deref(), Some("a long line"));
        assert_eq!(e.image, "ipfs://Qmimg");
        assert_eq!(e.files.len(), 1);
        assert_eq!(e.files[0].media_type.as_deref(), Some("video/mp4"));
        assert_eq!(e.max_supply, MintSupply::Quota(50));
    }

    #[test]
    fn max_supply_three_states() {
        // absent ⇒ Quota(1)
        let e: MintManifestEntry =
            serde_json::from_str(r#"{"asset_name":"a","name":"n","image":"ipfs://x"}"#).unwrap();
        assert_eq!(e.max_supply, MintSupply::Quota(1));
        // explicit null ⇒ Uncapped (→ NULL column)
        let e: MintManifestEntry = serde_json::from_str(
            r#"{"asset_name":"a","name":"n","image":"ipfs://x","max_supply":null}"#,
        )
        .unwrap();
        assert_eq!(e.max_supply, MintSupply::Uncapped);
    }

    #[test]
    fn composes_cip25_inner() {
        let e: MintManifestEntry = serde_json::from_str(LINE).unwrap();
        let inner = e.to_cip25_inner();
        assert_eq!(inner["name"], Value::String("Lumen".into()));
        assert_eq!(inner["description"], Value::String("a long line".into()));
        assert_eq!(inner["image"], Value::String("ipfs://Qmimg".into()));
        // single-value trait → string; multi-value → array
        assert_eq!(inner["Order"], Value::String("Stable".into()));
        assert_eq!(
            inner["Palette"],
            Value::Array(vec!["pA870".into(), "pA14".into()])
        );
        // files[] uses the CIP-25 `mediaType` key
        let files = inner["files"].as_array().unwrap();
        assert_eq!(files[0]["mediaType"], Value::String("video/mp4".into()));
        assert_eq!(files[0]["src"], Value::String("ipfs://Qmanim".into()));
    }

    #[test]
    fn check_media_links_accepts_ipfs_and_ar_rejects_others() {
        let mut e: MintManifestEntry = serde_json::from_str(LINE).unwrap();
        assert!(e.check_media_links().is_ok());
        e.image = "ar://abc".into();
        assert!(e.check_media_links().is_ok());
        e.image = "https://evil.example/x.png".into();
        assert!(e.check_media_links().is_err());
    }

    #[test]
    fn parse_jsonl_skips_blanks() {
        let content = format!("{LINE}\n\n  \n{LINE}\n");
        let entries = parse_jsonl(&content).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn parse_jsonl_reports_bad_line() {
        let content = format!("{LINE}\nnot json\n");
        let err = parse_jsonl(&content).unwrap_err();
        assert!(err.starts_with("line 2:"), "got {err}");
    }
}
