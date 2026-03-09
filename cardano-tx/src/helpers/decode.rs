//! Hex decoding helpers for Cardano transaction primitives

use crate::error::TxBuildError;

/// Decode a transaction hash from hex string to 32-byte array.
pub fn decode_tx_hash(hex_str: &str) -> Result<[u8; 32], TxBuildError> {
    hex::decode(hex_str)
        .map_err(|e| TxBuildError::InvalidHex(format!("Invalid tx_hash hex: {e}")))?
        .try_into()
        .map_err(|_| TxBuildError::InvalidHex("tx_hash must be 32 bytes".to_string()))
}

/// Decode a policy ID from hex string to 28-byte array.
pub fn decode_policy_id(hex_str: &str) -> Result<[u8; 28], TxBuildError> {
    hex::decode(hex_str)
        .map_err(|e| TxBuildError::InvalidHex(format!("Invalid policy ID hex: {e}")))?
        .try_into()
        .map_err(|_| TxBuildError::InvalidHex("Policy ID must be 28 bytes".to_string()))
}

/// Decode an asset name from hex string to bytes.
///
/// Falls back to treating the string as UTF-8 bytes if it doesn't look like hex.
pub fn decode_asset_name(hex_or_text: &str) -> Vec<u8> {
    use crate::helpers::is_hex_encoded;

    if is_hex_encoded(hex_or_text) {
        hex::decode(hex_or_text).unwrap_or_else(|_| hex_or_text.as_bytes().to_vec())
    } else {
        hex_or_text.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_tx_hash_valid() {
        let hash_hex = "a".repeat(64);
        let result = decode_tx_hash(&hash_hex).unwrap();
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_decode_tx_hash_invalid_length() {
        assert!(decode_tx_hash("abcd").is_err());
    }

    #[test]
    fn test_decode_tx_hash_invalid_hex() {
        assert!(decode_tx_hash("xyz").is_err());
    }

    #[test]
    fn test_decode_policy_id_valid() {
        let policy_hex = "a".repeat(56);
        let result = decode_policy_id(&policy_hex).unwrap();
        assert_eq!(result.len(), 28);
    }

    #[test]
    fn test_decode_policy_id_invalid_length() {
        assert!(decode_policy_id("abcd").is_err());
    }

    #[test]
    fn test_decode_asset_name_hex() {
        let result = decode_asset_name("48656c6c6f"); // "Hello" in hex
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn test_decode_asset_name_text_fallback() {
        let result = decode_asset_name("TestNFT");
        assert_eq!(result, b"TestNFT");
    }
}
