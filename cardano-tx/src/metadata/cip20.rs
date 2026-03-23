//! CIP-20 transaction message metadata
//!
//! Builds label-674 auxiliary data containing a `msg` array of strings.
//! Used for branding/identification on DEX swap transactions.
//!
//! Reference: <https://cips.cardano.org/cip/CIP-0020>

use pallas_codec::utils::KeyValuePairs;
use pallas_primitives::alonzo::AuxiliaryData;
use pallas_primitives::{Metadata, Metadatum};

use super::MetadataError;

/// Build CIP-20 auxiliary data from a list of message strings.
///
/// Produces: `{ 674: { "msg": ["line1", "line2", ...] } }`
///
/// Each string must be ≤64 bytes (CIP-20 requirement). Longer strings
/// are truncated with a warning.
pub fn build_cip20_auxiliary_data(messages: &[String]) -> Result<Vec<u8>, MetadataError> {
    use pallas_primitives::alonzo::PostAlonzoAuxiliaryData;
    use pallas_primitives::Fragment;

    if messages.is_empty() {
        return Err(MetadataError::UnsupportedValue(
            "CIP-20 msg array must not be empty".to_string(),
        ));
    }

    // Build msg array — truncate strings >64 bytes per CIP-20 spec
    let msg_items: Vec<Metadatum> = messages
        .iter()
        .map(|s| {
            if s.len() > 64 {
                tracing::warn!("CIP-20 message truncated to 64 bytes: {s}");
                Metadatum::Text(s[..64].to_string())
            } else {
                Metadatum::Text(s.clone())
            }
        })
        .collect();

    // { "msg": [messages...] }
    let inner_map = Metadatum::Map(KeyValuePairs::from(vec![(
        Metadatum::Text("msg".to_string()),
        Metadatum::Array(msg_items),
    )]));

    // { 674: { "msg": [...] } }
    let mut metadata = Metadata::new();
    metadata.insert(674, inner_map);

    let auxiliary_data = AuxiliaryData::PostAlonzo(PostAlonzoAuxiliaryData {
        metadata: Some(metadata),
        native_scripts: None,
        plutus_scripts: None,
    });

    let bytes = auxiliary_data
        .encode_fragment()
        .map_err(|e| MetadataError::EncodeError(format!("CIP-20 encode: {e}")))?;

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_simple_message() {
        let msgs = vec![
            "AlienSwap".to_string(),
            "Split: Splash 71% / CSWAP 29%".to_string(),
        ];
        let bytes = build_cip20_auxiliary_data(&msgs).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_empty_messages_rejected() {
        let result = build_cip20_auxiliary_data(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_long_string_truncated() {
        let long = "a".repeat(100);
        let bytes = build_cip20_auxiliary_data(&[long]).unwrap();
        assert!(!bytes.is_empty());
    }
}
