//! CIP-68 metadata in the shape Koios serves it.
//!
//! `POST /asset_info` returns a `cip68_metadata` field holding the reference
//! token's inline datum rendered as Plutus **detailed** JSON, keyed by the
//! requested asset's own CIP-67 label:
//!
//! ```json
//! { "100": { "constructor": 0, "fields": [ {"map": [...]}, {"int": 1}, ... ] } }
//! ```
//!
//! That is *not* the decoded metadata map — every key and string value is a
//! hex byte string, and the whole thing is wrapped in the `Constr 0
//! [metadata, version, extra]` envelope CIP-68 mandates. Handing it straight
//! to [`AssetMetadata`]'s deserializer never matches a known shape.
//!
//! [`decode_cip68_metadata`] unwraps the label, the constructor and the
//! detailed-JSON encoding, producing the same [`AssetMetadata68`] that
//! `cardano_assets::cip68::decode_cip68_datum` produces from raw datum CBOR.
//! The rendering rules are deliberately identical to that decoder's
//! `plutus_to_json`, so an asset resolves the same whether its datum arrived
//! as CBOR off a UTxO or as JSON from Koios.
//!
//! Unlike the CBOR path this needs no Plutus codec — Koios has already done
//! the CBOR decode — so it carries no `pallas` dependency and builds for wasm.

use cardano_assets::{AssetMetadata, AssetMetadata68, NftPurpose};
use serde_json::{Map, Value};

/// Decode Koios's `cip68_metadata` field into typed [`AssetMetadata68`].
///
/// Returns `None` when the value is absent/null, carries no CIP-67 label, is
/// not a `Constr` with at least a metadata field, or when the decoded map
/// matches no known [`AssetMetadata`] shape.
///
/// [`AssetMetadata68::purpose`] comes from the CIP-67 label Koios keyed the
/// datum under — `100` is the reference token, `222` the user token — so a
/// caller that asked for the user token gets [`NftPurpose::UserNft`], matching
/// what the asset id itself says. The CBOR decoder cannot do this: it only
/// ever sees the reference token's datum.
#[must_use]
pub fn decode_cip68_metadata(cip68: &Value) -> Option<AssetMetadata68> {
    let (label, datum) = cip68.as_object()?.iter().next()?;

    let fields = datum.get("fields")?.as_array()?;
    let metadata_value = plutus_json_to_value(fields.first()?);
    let metadata: AssetMetadata = serde_json::from_value(metadata_value).ok()?;
    let version = fields
        .get(1)
        .and_then(|v| v.get("int"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(1);

    Some(AssetMetadata68 {
        purpose: purpose_for_label(label),
        version,
        metadata,
    })
}

/// The plain metadata map behind Koios's `cip68_metadata`, without typing it.
///
/// This is what the v2 trait extractor (`asset_from_metadata_value`) wants —
/// it reads the original document rather than the typed form, so a shape the
/// typed decoder cannot match still yields traits.
#[must_use]
pub fn cip68_metadata_value(cip68: &Value) -> Option<Value> {
    let (_, datum) = cip68.as_object()?.iter().next()?;
    let fields = datum.get("fields")?.as_array()?;
    Some(plutus_json_to_value(fields.first()?))
}

/// CIP-67 label → the purpose of the token that label names.
fn purpose_for_label(label: &str) -> NftPurpose {
    match label {
        "100" => NftPurpose::ReferenceNft,
        "222" => NftPurpose::UserNft,
        _ => NftPurpose::Unknown,
    }
}

/// Render one Plutus detailed-JSON node to plain JSON.
///
/// Mirrors `cardano_assets::cip68`'s `plutus_to_json` exactly: byte strings
/// become UTF-8 where valid and `0x…` hex otherwise, maps become objects,
/// constructors become `{ "__constructor": n, "fields": [...] }`, lists
/// become arrays.
#[must_use]
pub fn plutus_json_to_value(node: &Value) -> Value {
    if let Some(hex_bytes) = node.get("bytes").and_then(Value::as_str) {
        return bytes_to_json(hex_bytes);
    }
    if let Some(int) = node.get("int") {
        return int.clone();
    }
    if let Some(items) = node.get("list").and_then(Value::as_array) {
        return Value::Array(items.iter().map(plutus_json_to_value).collect());
    }
    if let Some(entries) = node.get("map").and_then(Value::as_array) {
        let mut obj = Map::new();
        for entry in entries {
            let Some(key) = entry.get("k").map(plutus_key) else {
                continue;
            };
            let value = entry.get("v").map_or(Value::Null, plutus_json_to_value);
            obj.insert(key, value);
        }
        return Value::Object(obj);
    }
    if let Some(fields) = node.get("fields").and_then(Value::as_array) {
        let mut obj = Map::new();
        obj.insert(
            "__constructor".to_owned(),
            node.get("constructor").cloned().unwrap_or(Value::from(0)),
        );
        obj.insert(
            "fields".to_owned(),
            Value::Array(fields.iter().map(plutus_json_to_value).collect()),
        );
        return Value::Object(obj);
    }
    node.clone()
}

/// A hex byte string as UTF-8 where it decodes, else `0x…` hex.
///
/// Anything that is not valid hex is passed through verbatim — Koios always
/// sends hex here, so a non-hex value is a wire change we would rather
/// surface than silently blank out.
fn bytes_to_json(hex_bytes: &str) -> Value {
    match hex::decode(hex_bytes) {
        Ok(raw) => match String::from_utf8(raw) {
            Ok(text) => Value::String(text),
            Err(_) => Value::String(format!("0x{hex_bytes}")),
        },
        Err(_) => Value::String(hex_bytes.to_owned()),
    }
}

/// Coerce a Plutus map key to a JSON object key. CIP-68 keys are byte
/// strings; integer keys are stringified.
fn plutus_key(node: &Value) -> String {
    if let Some(hex_bytes) = node.get("bytes").and_then(Value::as_str) {
        return match bytes_to_json(hex_bytes) {
            Value::String(s) => s,
            other => other.to_string(),
        };
    }
    if let Some(int) = node.get("int") {
        return int.to_string();
    }
    plutus_json_to_value(node).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::Asset;

    /// A live `POST /asset_info` `cip68_metadata` payload for the ADA Handle
    /// `$thiya` reference token — a `Constr 0 [metadata, version, extra]`
    /// with hex-encoded keys and values, exactly as Koios serves it.
    fn thiya_reference() -> Value {
        serde_json::json!({
            "100": {
                "constructor": 0,
                "fields": [
                    { "map": [
                        { "k": { "bytes": "6e616d65" }, "v": { "bytes": "247468697961" } },
                        { "k": { "bytes": "696d616765" }, "v": { "bytes": "697066733a2f2f7a62327268627066326f76344b51413953316457397247625a7a48526f57717563536d6755644c55503737664e56584375" } },
                        { "k": { "bytes": "6d6564696154797065" }, "v": { "bytes": "696d6167652f6a706567" } },
                        { "k": { "bytes": "726172697479" }, "v": { "bytes": "636f6d6d6f6e" } },
                        { "k": { "bytes": "6c656e677468" }, "v": { "int": 5 } }
                    ] },
                    { "int": 1 },
                    { "map": [
                        { "k": { "bytes": "64656661756c74" }, "v": { "int": 0 } }
                    ] }
                ]
            }
        })
    }

    #[test]
    fn decodes_reference_label_to_typed_metadata() {
        let decoded = decode_cip68_metadata(&thiya_reference()).expect("decodes");
        assert_eq!(decoded.purpose, NftPurpose::ReferenceNft);
        assert_eq!(decoded.version, 1);

        let asset = Asset::from(decoded.metadata);
        assert_eq!(asset.name, "$thiya");
        assert_eq!(
            asset.image,
            "ipfs://zb2rhbpf2ov4KQA9S1dW9rGbZzHRoWqucSmgUdLUP77fNVXCu"
        );
        assert_eq!(asset.media_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn purpose_follows_the_cip67_label() {
        let mut user = serde_json::Map::new();
        user.insert(
            "222".to_owned(),
            thiya_reference().get("100").expect("label").clone(),
        );
        let decoded = decode_cip68_metadata(&Value::Object(user)).expect("decodes");
        assert_eq!(decoded.purpose, NftPurpose::UserNft);
    }

    #[test]
    fn metadata_value_keeps_every_key_for_trait_extraction() {
        let raw = cip68_metadata_value(&thiya_reference()).expect("metadata map");
        // The typed `AssetMetadata` drops unknown keys; the raw map is what
        // the v2 trait extractor reads, so `rarity` has to survive here.
        assert_eq!(raw.get("rarity").and_then(Value::as_str), Some("common"));
        assert_eq!(raw.get("length").and_then(Value::as_i64), Some(5));
    }

    #[test]
    fn renders_chunked_byte_strings_as_a_list() {
        // CIP-68 splits any value over 64 bytes into a list of byte strings,
        // the same way CIP-25 does; the decoder must keep it an array so the
        // existing string-or-array metadata decoder can rejoin it.
        let node = serde_json::json!({ "list": [
            { "bytes": "697066733a2f2f" },
            { "bytes": "516d54657374" }
        ] });
        assert_eq!(
            plutus_json_to_value(&node),
            Value::Array(vec![
                Value::String("ipfs://".to_owned()),
                Value::String("QmTest".to_owned()),
            ])
        );
    }

    #[test]
    fn renders_non_utf8_bytes_as_hex() {
        let node = serde_json::json!({ "bytes": "deadbeef" });
        assert_eq!(
            plutus_json_to_value(&node),
            Value::String("0xdeadbeef".to_owned())
        );
    }

    #[test]
    fn absent_or_malformed_metadata_is_none() {
        assert!(decode_cip68_metadata(&Value::Null).is_none());
        assert!(decode_cip68_metadata(&serde_json::json!({})).is_none());
        assert!(decode_cip68_metadata(&serde_json::json!({ "100": { "int": 1 } })).is_none());
    }
}
