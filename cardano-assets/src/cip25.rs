//! CIP-25 mint-metadata decoding (transaction auxiliary-data, label 721).
//!
//! CIP-25 NFT metadata lives in the mint transaction's auxiliary data
//! under metadata label `721`, keyed `[721][policy][asset_name]`.
//! [`cip25_metadata_json`] navigates that structure out of the raw
//! aux-data CBOR (as a node's tx-metadata lookup returns it) and
//! renders the asset's metadata value to the same JSON shape this
//! crate already produces from CIP-25 JSON / CIP-68 datums — so the
//! existing [`AssetMetadata`] decoder and trait extraction handle it
//! unchanged.
//!
//! Sharing this decoder is the point: a mitos `collection-metadata`
//! cold-start (which resolves the mint TX from dolos `AssetState`,
//! then calls this) and the live `cip-25-mint` path both render
//! byte-identical `metadata_json`, so a downstream consumer's trait
//! bitmaps don't churn. See `mitos/docs/design/COLLECTION_MODULES.md`.
//!
//! Behind the `cip25` feature; pulls in `pallas-codec` / `pallas-primitives`.

use crate::AssetMetadata;
use pallas_primitives::alonzo::AuxiliaryData;
use pallas_primitives::{Metadata, Metadatum};
use serde_json::{Map, Value};

/// Decode the label-721 metadata for one asset out of raw transaction
/// auxiliary-data CBOR, returning it as a JSON string — the same
/// `metadata_json` wire shape the CIP-68 path produces. `None` when
/// the aux-data doesn't decode, carries no label 721, or has no entry
/// for `(policy, asset_name)`.
///
/// `policy` is the 28-byte policy id; `asset_name` is the raw on-chain
/// asset name. CIP-25 keys appear in the wild as both raw bytes and
/// hex/utf-8 text — both forms are matched.
pub fn cip25_metadata_json(aux_cbor: &[u8], policy: &[u8], asset_name: &[u8]) -> Option<String> {
    let value = cip25_metadata_value(aux_cbor, policy, asset_name)?;
    serde_json::to_string(&value).ok()
}

/// As [`cip25_metadata_json`] but returns the decoded [`Value`].
pub fn cip25_metadata_value(aux_cbor: &[u8], policy: &[u8], asset_name: &[u8]) -> Option<Value> {
    let aux: AuxiliaryData = pallas_codec::minicbor::decode(aux_cbor).ok()?;
    let metadata = aux_metadata(&aux)?;
    let label_721 = metadata.get(&721u64)?;

    // Policy key: CIP-25 spec says bytes, mainnet also uses the hex
    // string — try both.
    let policy_entry = find_in_map(label_721, &hex::encode(policy), policy)?;

    // Asset-name key: utf-8 string or raw bytes.
    let name_utf8 = std::str::from_utf8(asset_name).unwrap_or("");
    let asset_entry = find_in_map(policy_entry, name_utf8, asset_name)?;

    Some(metadatum_to_json(asset_entry))
}

/// Decode straight to a typed [`AssetMetadata`] — convenience for
/// callers that want name/traits directly rather than the JSON wire
/// form. Mirrors [`crate::cip68::decode_cip68_datum`].
pub fn decode_cip25_metadata(
    aux_cbor: &[u8],
    policy: &[u8],
    asset_name: &[u8],
) -> Option<AssetMetadata> {
    serde_json::from_value(cip25_metadata_value(aux_cbor, policy, asset_name)?).ok()
}

/// Extract the metadata map from any auxiliary-data era shape
/// (Shelley bare map, Shelley-MA array, or post-Alonzo tag-259 map).
fn aux_metadata(aux: &AuxiliaryData) -> Option<&Metadata> {
    match aux {
        AuxiliaryData::Shelley(m) => Some(m),
        AuxiliaryData::ShelleyMa(d) => Some(&d.transaction_metadata),
        AuxiliaryData::PostAlonzo(d) => d.metadata.as_ref(),
    }
}

/// Find a value in a `Metadatum::Map` by a key matching either the
/// UTF-8/hex string form or the raw byte form. `None` if `m` isn't a
/// map or no key matches.
fn find_in_map<'a>(m: &'a Metadatum, as_str: &str, as_bytes: &[u8]) -> Option<&'a Metadatum> {
    let Metadatum::Map(pairs) = m else {
        return None;
    };
    for (k, v) in pairs.iter() {
        let hit = match k {
            Metadatum::Text(s) => s == as_str,
            Metadatum::Bytes(b) => b.as_slice() == as_bytes,
            _ => false,
        };
        if hit {
            return Some(v);
        }
    }
    None
}

/// Render a `Metadatum` to `serde_json::Value`. Same conventions as
/// `cip68::plutus_to_json`: byte strings become UTF-8 (or `0x…` hex),
/// integers become numbers (decimal string outside the `i64` range),
/// maps become objects (keys string-coerced), arrays become arrays.
/// CIP-25 chunked strings (>64 bytes split into arrays) are left as
/// arrays — `AssetMetadata` rejoins them downstream.
fn metadatum_to_json(m: &Metadatum) -> Value {
    match m {
        Metadatum::Int(i) => {
            let raw = i128::from(*i);
            i64::try_from(raw).map_or_else(
                |_| Value::String(raw.to_string()),
                |v| Value::Number(v.into()),
            )
        }
        Metadatum::Bytes(b) => bytes_to_json(b.as_slice()),
        Metadatum::Text(s) => Value::String(s.clone()),
        Metadatum::Array(items) => Value::Array(items.iter().map(metadatum_to_json).collect()),
        Metadatum::Map(pairs) => {
            let mut obj = Map::new();
            for (k, v) in pairs.iter() {
                obj.insert(metadatum_key(k), metadatum_to_json(v));
            }
            Value::Object(obj)
        }
    }
}

/// Render bytes as a UTF-8 string, or a `0x…` hex string if invalid.
fn bytes_to_json(bytes: &[u8]) -> Value {
    match std::str::from_utf8(bytes) {
        Ok(s) => Value::String(s.to_owned()),
        Err(_) => Value::String(format!("0x{}", hex::encode(bytes))),
    }
}

/// Coerce a `Metadatum` map key to a JSON object key.
fn metadatum_key(m: &Metadatum) -> String {
    match m {
        Metadatum::Text(s) => s.clone(),
        Metadatum::Bytes(b) => match std::str::from_utf8(b.as_slice()) {
            Ok(s) => s.to_owned(),
            Err(_) => format!("0x{}", hex::encode(b.as_slice())),
        },
        Metadatum::Int(i) => i128::from(*i).to_string(),
        Metadatum::Array(_) | Metadatum::Map(_) => "<complex-key>".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Asset;
    use pallas_codec::utils::KeyValuePairs;
    use std::collections::BTreeMap;

    const POLICY: &[u8] = &[0xab; 28];

    fn text(s: &str) -> Metadatum {
        Metadatum::Text(s.to_owned())
    }

    /// Build raw aux-data CBOR for `{721: {policy: {asset: {...}}}}`
    /// using the bare Shelley metadata-map shape. `policy_key` /
    /// `asset_key` let a test choose text-vs-bytes key encodings.
    fn build_aux(policy_key: Metadatum, asset_key: Metadatum) -> Vec<u8> {
        let asset_value = Metadatum::Map(KeyValuePairs::Def(vec![
            (text("name"), text("AL0001")),
            (
                text("image"),
                text("ipfs://QmExampleCidForAl0001Token0000000000000"),
            ),
            (text("Class"), text("Archer")),
        ]));
        let policy_map = Metadatum::Map(KeyValuePairs::Def(vec![(asset_key, asset_value)]));
        let label_map = Metadatum::Map(KeyValuePairs::Def(vec![(policy_key, policy_map)]));
        let mut metadata: Metadata = BTreeMap::new();
        metadata.insert(721u64, label_map);
        pallas_codec::minicbor::to_vec(AuxiliaryData::Shelley(metadata)).expect("encode aux")
    }

    #[test]
    fn decodes_cip25_with_text_keys() {
        // Policy as hex-text key, asset name as utf-8 text key (the
        // CIP-25-spec-ish shape).
        let cbor = build_aux(text(&hex::encode(POLICY)), text("AL0001"));
        let json = cip25_metadata_json(&cbor, POLICY, b"AL0001").expect("decodes");
        assert!(json.contains("\"Class\":\"Archer\""), "json was {json}");

        let am = decode_cip25_metadata(&cbor, POLICY, b"AL0001").expect("typed decode");
        let asset = Asset::from(am);
        assert_eq!(asset.name, "AL0001");
    }

    #[test]
    fn decodes_cip25_with_byte_keys() {
        // Policy + asset name as raw byte keys (also seen on mainnet).
        let cbor = build_aux(
            Metadatum::Bytes(POLICY.to_vec().into()),
            Metadatum::Bytes(b"AL0001".to_vec().into()),
        );
        let am = decode_cip25_metadata(&cbor, POLICY, b"AL0001").expect("typed decode");
        let asset = Asset::from(am);
        assert_eq!(asset.name, "AL0001");
    }

    #[test]
    fn missing_asset_returns_none() {
        let cbor = build_aux(text(&hex::encode(POLICY)), text("AL0001"));
        assert!(cip25_metadata_json(&cbor, POLICY, b"AL9999").is_none());
    }

    #[test]
    fn garbage_cbor_returns_none() {
        assert!(cip25_metadata_json(&[0xff, 0xfe, 0xfd], POLICY, b"AL0001").is_none());
    }

    /// Consumer leg: the facade emits `metadata_json`, and a
    /// downstream worker re-parses it via `AssetMetadata` to extract
    /// traits. This pins that the real IslaNOVA "Apex Legends" CIP-25
    /// shape (a flat trait map — the deploy target) matches a known
    /// `AssetMetadata` variant and yields traits, so the worker's
    /// trait pipeline lights up for CIP-25 collections.
    #[test]
    fn islanova_cip25_metadata_extracts_traits() {
        let json = r#"{"Aura":"None","Background":"Frost Realm","Base":"Sandstone Skin","Class":"Archer","Clothes":"Shadow-Cloak Wrap","Eyes":"Red Laser Eyes","Headwear":"Ranger Utility Headwrap","Mouth":"Grit Teeth Expression","Top Layer":"None","Weapon":"Crescent Longbow","image":"ipfs://QmVv2ZhM6oRfn4FXxbLWNzEizyVfeX9eUKKVokCyhuL5pM","name":"islaNOVA: Apex Legends #0001"}"#;
        let am: AssetMetadata = serde_json::from_str(json)
            .expect("IslaNOVA CIP-25 metadata matches an AssetMetadata variant");
        let asset = Asset::from(am);
        assert_eq!(asset.name, "islaNOVA: Apex Legends #0001");
        assert_eq!(asset.traits.get_single("Class").as_deref(), Some("Archer"));
        assert_eq!(
            asset.traits.get_single("Weapon").as_deref(),
            Some("Crescent Longbow")
        );
    }
}
