//! Asset name encoding utilities

/// Check if a string is valid hex-encoded data (even-length, all hex digits).
pub fn is_hex_encoded(s: &str) -> bool {
    s.len().is_multiple_of(2) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Normalize an asset name to hex format.
/// If already hex, returns as-is; otherwise encodes as hex.
pub fn normalize_asset_name_to_hex(name: &str) -> String {
    if is_hex_encoded(name) {
        name.to_string()
    } else {
        hex::encode(name.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_encoded() {
        assert!(is_hex_encoded("abcdef01"));
        assert!(is_hex_encoded("AABB"));
        assert!(is_hex_encoded("")); // empty is valid hex
    }

    #[test]
    fn test_not_hex_encoded() {
        assert!(!is_hex_encoded("abc")); // odd length
        assert!(!is_hex_encoded("hello!"));
        assert!(!is_hex_encoded("TestNFT")); // contains non-hex chars
    }

    #[test]
    fn test_normalize_hex_passthrough() {
        assert_eq!(normalize_asset_name_to_hex("abcdef01"), "abcdef01");
    }

    #[test]
    fn test_normalize_text_to_hex() {
        assert_eq!(
            normalize_asset_name_to_hex("TestNFT"),
            hex::encode("TestNFT")
        );
    }
}
