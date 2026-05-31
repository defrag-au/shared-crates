//! CIP-25 metadata building utilities
//!
//! Converts JSON metadata structures into CBOR auxiliary data suitable for
//! `StagingTransaction::add_auxiliary_data()`.
//!
//! Reference: <https://cips.cardano.org/cip/CIP-0025>

use pallas_codec::utils::{Int, KeyValuePairs};
use pallas_primitives::alonzo::AuxiliaryData;
use pallas_primitives::{Metadata, Metadatum};

use super::MetadataError;

/// Build CIP-25 compliant auxiliary data from a JSON metadata structure.
///
/// Expects input shaped like:
/// ```json
/// {
///   "721": {
///     "policy_id_hex": {
///       "asset_name": {
///         "name": "...",
///         "image": "...",
///         "attributes": {}
///       }
///     }
///   }
/// }
/// ```
///
/// Returns CBOR bytes (Alonzo PostAlonzo format with Tag 259, text keys) ready
/// for `StagingTransaction::add_auxiliary_data()`.
pub fn build_cip25_auxiliary_data(
    metadata_json: &serde_json::Value,
) -> Result<Vec<u8>, MetadataError> {
    use pallas_primitives::alonzo::PostAlonzoAuxiliaryData;
    use pallas_primitives::Fragment;

    let metadata_721 = metadata_json
        .get("721")
        .ok_or(MetadataError::MissingCip25Key)?;

    tracing::debug!("Building CIP-25 auxiliary data using pallas-primitives");

    let mut policy_map: Vec<(Metadatum, Metadatum)> = Vec::new();

    if let Some(policy_obj) = metadata_721.as_object() {
        for (policy_id_hex, assets_obj) in policy_obj {
            // Use text format for policy ID (matches working mainnet transactions)
            let policy_id_metadatum = Metadatum::Text(policy_id_hex.clone());

            let mut asset_map_entries: Vec<(Metadatum, Metadatum)> = Vec::new();

            if let Some(assets) = assets_obj.as_object() {
                for (asset_name, metadata) in assets {
                    // Use text format for asset name (matches working mainnet transactions)
                    let asset_name_metadatum = Metadatum::Text(asset_name.clone());
                    let metadata_metadatum = json_to_metadatum(metadata)?;
                    asset_map_entries.push((asset_name_metadatum, metadata_metadatum));
                }
            }

            let asset_map = Metadatum::Map(KeyValuePairs::from(asset_map_entries));
            policy_map.push((policy_id_metadatum, asset_map));
        }
    }

    // Build metadata map: { 721: <CIP-25 metadata> }
    let cip25_metadata = Metadatum::Map(KeyValuePairs::from(policy_map));
    let mut metadata = Metadata::new();
    metadata.insert(721, cip25_metadata);

    // CIP-674 ("msg" label) — additive: if the input JSON carries a
    // `"674"` key alongside the `"721"` CIP-25 blob, emit it verbatim
    // at metadata label 674. Used by the minting-engine to tag
    // inline refund outputs with `refund:<order_id>` for explorer
    // visibility + worker idempotency. See `MINT_REFUNDS.md` →
    // "Refund-output metadata tag". No impact on callers that don't
    // populate the `"674"` key.
    if let Some(cip674) = metadata_json.get("674") {
        let cip674_metadatum = json_to_metadatum(cip674)?;
        metadata.insert(674, cip674_metadatum);
    }

    // Use Alonzo PostAlonzo format with Tag 259 (matches working mainnet transactions)
    let auxiliary_data = AuxiliaryData::PostAlonzo(PostAlonzoAuxiliaryData {
        metadata: Some(metadata),
        native_scripts: None,
        plutus_scripts: None,
    });

    let auxiliary_bytes = auxiliary_data
        .encode_fragment()
        .map_err(|e| MetadataError::EncodeError(format!("Failed to encode auxiliary data: {e}")))?;

    tracing::debug!(
        "Encoded auxiliary data: {} bytes (Alonzo PostAlonzo with Tag 259, text keys)",
        auxiliary_bytes.len()
    );

    Ok(auxiliary_bytes)
}

/// Build auxiliary data from an arbitrary transaction-metadata JSON
/// object whose top-level keys are numeric metadata labels (e.g.
/// `{ "674": { "msg": ["refund:…"] } }`).
///
/// Unlike [`build_cip25_auxiliary_data`] this does **not** require a
/// `721` key — it emits every present label verbatim via
/// [`json_to_metadatum`], so it serves metadata-only transactions that
/// carry no CIP-25 mint blob. Used by the standalone refund (Mode B)
/// and settlement txs, which tag themselves with CIP-674 (`msg`) but
/// mint nothing. (Long `msg` strings chunk per the same CIP-25 rules as
/// the mint path.)
pub fn build_metadata_auxiliary_data(
    metadata_json: &serde_json::Value,
) -> Result<Vec<u8>, MetadataError> {
    use pallas_primitives::alonzo::PostAlonzoAuxiliaryData;
    use pallas_primitives::Fragment;

    let obj = metadata_json.as_object().ok_or_else(|| {
        MetadataError::UnsupportedValue("metadata root must be a JSON object of labels".to_string())
    })?;

    let mut metadata = Metadata::new();
    for (label_str, value) in obj {
        let label: u64 = label_str.parse().map_err(|_| {
            MetadataError::UnsupportedValue(format!(
                "metadata label '{label_str}' is not a numeric key"
            ))
        })?;
        metadata.insert(label, json_to_metadatum(value)?);
    }

    let auxiliary_data = AuxiliaryData::PostAlonzo(PostAlonzoAuxiliaryData {
        metadata: Some(metadata),
        native_scripts: None,
        plutus_scripts: None,
    });
    auxiliary_data
        .encode_fragment()
        .map_err(|e| MetadataError::EncodeError(format!("Failed to encode auxiliary data: {e}")))
}

/// Convert a [`serde_json::Value`] to a pallas [`Metadatum`].
///
/// CIP-25 rules:
/// - Strings longer than 64 chars are split into 64-char chunks
/// - Booleans and nulls are not supported
pub fn json_to_metadatum(value: &serde_json::Value) -> Result<Metadatum, MetadataError> {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Metadatum::Int(Int::from(i)))
            } else if let Some(u) = n.as_u64() {
                let i = i64::try_from(u).map_err(|_| {
                    MetadataError::UnsupportedValue(format!(
                        "Number {u} too large for metadata (max i64)"
                    ))
                })?;
                Ok(Metadatum::Int(Int::from(i)))
            } else {
                Err(MetadataError::UnsupportedValue(
                    "Floating point numbers not supported in metadata".to_string(),
                ))
            }
        }
        serde_json::Value::String(s) => {
            // CIP-25: strings longer than 64 chars should be split into array
            if s.len() > 64 {
                let chunks: Vec<Metadatum> = s
                    .as_bytes()
                    .chunks(64)
                    .map(|chunk| {
                        Metadatum::Text(
                            String::from_utf8(chunk.to_vec())
                                .unwrap_or_else(|_| String::from_utf8_lossy(chunk).to_string()),
                        )
                    })
                    .collect();
                Ok(Metadatum::Array(chunks))
            } else {
                Ok(Metadatum::Text(s.clone()))
            }
        }
        serde_json::Value::Array(arr) => {
            let mut metadatum_arr = Vec::new();
            for item in arr {
                metadatum_arr.push(json_to_metadatum(item)?);
            }
            Ok(Metadatum::Array(metadatum_arr))
        }
        serde_json::Value::Object(obj) => {
            let mut metadatum_map = Vec::new();
            for (key, val) in obj {
                let key_metadatum = Metadatum::Text(key.clone());
                let val_metadatum = json_to_metadatum(val)?;
                metadatum_map.push((key_metadatum, val_metadatum));
            }
            Ok(Metadatum::Map(KeyValuePairs::from(metadatum_map)))
        }
        serde_json::Value::Bool(_) => Err(MetadataError::UnsupportedValue(
            "Boolean values not directly supported in metadata".to_string(),
        )),
        serde_json::Value::Null => Err(MetadataError::UnsupportedValue(
            "Null values not supported in metadata".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cip25_simple() {
        let json = serde_json::json!({
            "721": {
                "abc123": {
                    "MyNFT": {
                        "name": "My NFT",
                        "image": "ipfs://QmTest"
                    }
                }
            }
        });

        let bytes = build_cip25_auxiliary_data(&json).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_missing_721_key() {
        let json = serde_json::json!({ "wrong": {} });
        let err = build_cip25_auxiliary_data(&json).unwrap_err();
        assert!(matches!(err, MetadataError::MissingCip25Key));
    }

    /// Golden: a `"674"` key alongside `"721"` emits the CIP-674 `msg`
    /// label into the same auxiliary data — the inline-refund tag from
    /// `MINT_REFUNDS.md`. Decode-free check: short text metadata encodes
    /// the literal UTF-8 bytes, so the `refund:<id>` tags appear verbatim
    /// in the CBOR when present and are absent otherwise.
    #[test]
    fn test_cip674_refund_msg_label_emitted_with_721() {
        let with_674 = serde_json::json!({
            "721": { "abc123": { "MyNFT": { "name": "My NFT" } } },
            "674": { "msg": ["refund:ord1", "refund:ord2"] }
        });
        let without_674 = serde_json::json!({
            "721": { "abc123": { "MyNFT": { "name": "My NFT" } } }
        });

        let bytes_with = build_cip25_auxiliary_data(&with_674).unwrap();
        let bytes_without = build_cip25_auxiliary_data(&without_674).unwrap();

        // The 674 label adds content beyond the 721 blob...
        assert!(
            bytes_with.len() > bytes_without.len(),
            "674 label should enlarge the auxiliary data"
        );
        // ...and both refund tags are present verbatim in the CBOR.
        let has = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
        assert!(has(&bytes_with, b"refund:ord1"), "first refund tag missing");
        assert!(
            has(&bytes_with, b"refund:ord2"),
            "second refund tag missing"
        );
        // Absent when no 674 key is supplied.
        assert!(
            !has(&bytes_without, b"refund:ord1"),
            "no 674 key → no refund tag in metadata"
        );
    }

    /// `build_metadata_auxiliary_data` emits a CIP-674-only blob (no 721
    /// required) — the standalone refund/settlement tx tag.
    #[test]
    fn test_build_metadata_auxiliary_data_674_only() {
        let json = serde_json::json!({ "674": { "msg": ["settle:pol:1"] } });
        let bytes = build_metadata_auxiliary_data(&json).unwrap();
        assert!(bytes
            .windows(b"settle:pol:1".len())
            .any(|w| w == b"settle:pol:1"));
    }

    #[test]
    fn test_build_metadata_auxiliary_data_rejects_non_numeric_label() {
        let json = serde_json::json!({ "msg": "oops" }); // not a numeric label
        assert!(matches!(
            build_metadata_auxiliary_data(&json),
            Err(MetadataError::UnsupportedValue(_))
        ));
    }

    #[test]
    fn test_build_metadata_auxiliary_data_rejects_non_object() {
        let json = serde_json::json!(["not", "an", "object"]);
        assert!(matches!(
            build_metadata_auxiliary_data(&json),
            Err(MetadataError::UnsupportedValue(_))
        ));
    }

    #[test]
    fn test_json_to_metadatum_string() {
        let val = serde_json::Value::String("hello".to_string());
        let m = json_to_metadatum(&val).unwrap();
        assert!(matches!(m, Metadatum::Text(s) if s == "hello"));
    }

    #[test]
    fn test_json_to_metadatum_long_string_chunks() {
        let long = "a".repeat(100);
        let val = serde_json::Value::String(long);
        let m = json_to_metadatum(&val).unwrap();
        match m {
            Metadatum::Array(chunks) => {
                assert_eq!(chunks.len(), 2); // 64 + 36
            }
            _ => panic!("Expected array for long string"),
        }
    }

    #[test]
    fn test_json_to_metadatum_number() {
        let val = serde_json::json!(42);
        let m = json_to_metadatum(&val).unwrap();
        assert!(matches!(m, Metadatum::Int(_)));
    }

    #[test]
    fn test_json_to_metadatum_bool_rejected() {
        let val = serde_json::json!(true);
        assert!(json_to_metadatum(&val).is_err());
    }

    #[test]
    fn test_json_to_metadatum_null_rejected() {
        let val = serde_json::Value::Null;
        assert!(json_to_metadatum(&val).is_err());
    }
}
