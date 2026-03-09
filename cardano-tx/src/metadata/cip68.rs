//! CIP-68 datum building for reference tokens
//!
//! CIP-68 defines a standard for storing NFT metadata in inline datums on reference tokens.
//! The datum structure is: `#6.121([metadata, version, extra])`
//!
//! Reference: <https://cips.cardano.org/cip/CIP-0068>

use cardano_assets::AssetMetadata;
use pallas_primitives::alonzo::{BigInt, Constr, PlutusData};
use pallas_primitives::{Fragment, MaybeIndefArray};

use crate::helpers::is_hex_encoded;

use super::MetadataError;

/// CIP-68 datum version (1 = initial version)
const CIP68_VERSION: i64 = 1;

/// Build a CIP-68 datum from [`AssetMetadata`].
///
/// The datum structure follows CIP-68:
/// ```text
/// datum = #6.121([metadata_map, version, extra])
/// ```
///
/// Returns CBOR bytes suitable for use as an inline datum on reference tokens.
pub fn build_cip68_datum(metadata: &AssetMetadata) -> Result<Vec<u8>, MetadataError> {
    let metadata_map = metadata_to_plutus_map(metadata)?;

    // Build the CIP-68 constructor: Constr 0 with [metadata, version, extra]
    // #6.121 is represented as Constr with alternative = 0
    let datum = PlutusData::Constr(Constr {
        tag: 121, // Constr 0 uses tag 121
        any_constructor: None,
        fields: MaybeIndefArray::Def(vec![
            metadata_map,
            PlutusData::BigInt(BigInt::Int(CIP68_VERSION.into())),
            PlutusData::Array(MaybeIndefArray::Def(vec![])), // extra = empty array
        ]),
    });

    datum
        .encode_fragment()
        .map_err(|e| MetadataError::EncodeError(format!("Failed to encode CIP-68 datum: {e}")))
}

/// Convert [`AssetMetadata`] to a PlutusData Map.
fn metadata_to_plutus_map(metadata: &AssetMetadata) -> Result<PlutusData, MetadataError> {
    let json_value = serde_json::to_value(metadata)
        .map_err(|e| MetadataError::EncodeError(format!("Failed to serialize metadata: {e}")))?;

    json_to_plutus_data(&json_value)
}

/// Convert a JSON value to [`PlutusData`].
///
/// Type mapping:
/// - `null` → Constr 0 (empty)
/// - `bool` → Constr 0 (false) / Constr 1 (true)
/// - `number` → BigInt (floats stored as string bytes)
/// - `string` → BoundedBytes (split into 64-byte chunks if >64 bytes)
/// - `array` → Array
/// - `object` → Map (null values skipped)
pub fn json_to_plutus_data(value: &serde_json::Value) -> Result<PlutusData, MetadataError> {
    match value {
        serde_json::Value::Null => Ok(PlutusData::Constr(Constr {
            tag: 121, // Constr 0
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![]),
        })),
        serde_json::Value::Bool(b) => {
            let tag = if *b { 122 } else { 121 };
            Ok(PlutusData::Constr(Constr {
                tag,
                any_constructor: None,
                fields: MaybeIndefArray::Def(vec![]),
            }))
        }
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PlutusData::BigInt(BigInt::Int(i.into())))
            } else if let Some(u) = n.as_u64() {
                Ok(PlutusData::BigInt(BigInt::Int((u as i64).into())))
            } else {
                // Floating point: store as string bytes
                Ok(PlutusData::BoundedBytes(n.to_string().into_bytes().into()))
            }
        }
        serde_json::Value::String(s) => {
            let bytes = s.as_bytes();
            if bytes.len() <= 64 {
                Ok(PlutusData::BoundedBytes(bytes.to_vec().into()))
            } else {
                // CIP-68: split into 64-byte chunks
                let chunks: Vec<PlutusData> = bytes
                    .chunks(64)
                    .map(|chunk| PlutusData::BoundedBytes(chunk.to_vec().into()))
                    .collect();
                Ok(PlutusData::Array(MaybeIndefArray::Def(chunks)))
            }
        }
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<PlutusData>, MetadataError> =
                arr.iter().map(json_to_plutus_data).collect();
            Ok(PlutusData::Array(MaybeIndefArray::Def(items?)))
        }
        serde_json::Value::Object(obj) => {
            let mut map_items: Vec<(PlutusData, PlutusData)> = Vec::new();

            for (key, val) in obj {
                // Skip null values in objects
                if val.is_null() {
                    continue;
                }

                let key_data = PlutusData::BoundedBytes(key.as_bytes().to_vec().into());
                let val_data = json_to_plutus_data(val)?;
                map_items.push((key_data, val_data));
            }

            Ok(PlutusData::Map(map_items.into()))
        }
    }
}

/// Build a CIP-67 prefixed asset name for user or reference tokens.
///
/// The `prefix_hex` is the 4-byte label (e.g. [`cip67::NFT_USER`](super::cip67::NFT_USER),
/// [`cip67::REFERENCE`](super::cip67::REFERENCE)). The `base_name` is either a raw UTF-8
/// name or an already hex-encoded name.
///
/// # Example
/// ```
/// use cardano_tx::metadata::{cip67, cip68};
///
/// let user_name = cip68::get_prefixed_asset_name("TestNFT", cip67::NFT_USER);
/// assert!(hex::encode(&user_name).starts_with("000de140"));
///
/// let ref_name = cip68::get_prefixed_asset_name("TestNFT", cip67::REFERENCE);
/// assert!(hex::encode(&ref_name).starts_with("000643b0"));
/// ```
pub fn get_prefixed_asset_name(base_name: &str, prefix_hex: &str) -> Vec<u8> {
    let prefix = hex::decode(prefix_hex).expect("Valid hex prefix constant");
    let name_bytes = if is_hex_encoded(base_name) {
        hex::decode(base_name).unwrap_or_else(|_| base_name.as_bytes().to_vec())
    } else {
        base_name.as_bytes().to_vec()
    };

    let mut result = prefix;
    result.extend(name_bytes);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::cip67;
    use cardano_assets::{PrimitiveOrList, Traits};

    #[test]
    fn test_build_simple_cip68_datum() {
        let metadata = AssetMetadata::Flattened {
            name: "Test NFT #1".to_string(),
            image: PrimitiveOrList::Primitive("ipfs://QmTest123".to_string()),
            media_type: Some("image/png".to_string()),
            project: None,
            description: Some(PrimitiveOrList::Primitive("A test NFT".to_string())),
            files: None,
            publisher: None,
            discord: None,
            twitter: None,
            website: None,
            github: None,
            medium: None,
            sha256: None,
            url: None,
            traits: Traits::new(),
        };

        let datum_bytes = build_cip68_datum(&metadata).expect("Should build datum");

        assert!(!datum_bytes.is_empty());
        // The datum should start with tag 121 (Constr 0) which is 0xd8 0x79
        assert_eq!(datum_bytes[0], 0xd8);
        assert_eq!(datum_bytes[1], 0x79);
    }

    #[test]
    fn test_user_asset_name_nft() {
        let name = get_prefixed_asset_name("TestNFT", cip67::NFT_USER);
        let hex_name = hex::encode(&name);
        assert!(hex_name.starts_with("000de140"));
        assert!(hex_name.ends_with(&hex::encode("TestNFT")));
    }

    #[test]
    fn test_reference_asset_name() {
        let name = get_prefixed_asset_name("TestNFT", cip67::REFERENCE);
        let hex_name = hex::encode(&name);
        assert!(hex_name.starts_with("000643b0"));
        assert!(hex_name.ends_with(&hex::encode("TestNFT")));
    }

    #[test]
    fn test_long_string_chunking() {
        let long_string = "a".repeat(100);
        let json_val = serde_json::Value::String(long_string);

        let plutus = json_to_plutus_data(&json_val).expect("Should convert");

        match plutus {
            PlutusData::Array(MaybeIndefArray::Def(chunks)) => {
                assert_eq!(chunks.len(), 2); // 100 bytes = 64 + 36
            }
            _ => panic!("Expected array for long string"),
        }
    }

    #[test]
    fn test_json_null_to_constr0() {
        let plutus = json_to_plutus_data(&serde_json::Value::Null).unwrap();
        match plutus {
            PlutusData::Constr(c) => assert_eq!(c.tag, 121),
            _ => panic!("Expected Constr for null"),
        }
    }

    #[test]
    fn test_json_bool_to_constr() {
        let t = json_to_plutus_data(&serde_json::json!(true)).unwrap();
        let f = json_to_plutus_data(&serde_json::json!(false)).unwrap();
        match (t, f) {
            (PlutusData::Constr(ct), PlutusData::Constr(cf)) => {
                assert_eq!(ct.tag, 122); // true = Constr 1
                assert_eq!(cf.tag, 121); // false = Constr 0
            }
            _ => panic!("Expected Constr for booleans"),
        }
    }
}
