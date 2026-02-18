use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Expected length of a Cardano transaction hash in hex characters (32 bytes = 64 hex chars)
const TX_HASH_HEX_LENGTH: usize = 64;

/// A Cardano transaction hash (32-byte Blake2b-256 digest, represented as 64 hex characters).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TxHash(String);

impl TxHash {
    /// Create a new TxHash with validation.
    pub fn new(hash: String) -> Result<Self, TxHashError> {
        if hash.len() != TX_HASH_HEX_LENGTH {
            return Err(TxHashError::InvalidLength {
                expected: TX_HASH_HEX_LENGTH,
                actual: hash.len(),
            });
        }
        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(TxHashError::InvalidFormat);
        }
        Ok(Self(hash))
    }

    /// Create a TxHash without validation (use when the hash comes from a trusted source).
    pub fn new_unchecked(hash: String) -> Self {
        Self(hash)
    }

    /// Get the hash as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for TxHash {
    type Err = TxHashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.to_string())
    }
}

impl AsRef<str> for TxHash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Errors from TxHash construction.
#[derive(Debug, Clone, PartialEq)]
pub enum TxHashError {
    InvalidLength { expected: usize, actual: usize },
    InvalidFormat,
}

impl fmt::Display for TxHashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxHashError::InvalidLength { expected, actual } => {
                write!(
                    f,
                    "invalid tx hash length: expected {expected}, got {actual}"
                )
            }
            TxHashError::InvalidFormat => {
                write!(f, "invalid tx hash format: must be hexadecimal")
            }
        }
    }
}

impl std::error::Error for TxHashError {}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_HASH: &str = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";

    #[test]
    fn test_new_valid() {
        let tx = TxHash::new(VALID_HASH.to_string()).expect("should be valid");
        assert_eq!(tx.as_str(), VALID_HASH);
    }

    #[test]
    fn test_display() {
        let tx = TxHash::new_unchecked(VALID_HASH.to_string());
        assert_eq!(tx.to_string(), VALID_HASH);
    }

    #[test]
    fn test_from_str() {
        let tx: TxHash = VALID_HASH.parse().expect("should parse");
        assert_eq!(tx.as_str(), VALID_HASH);
    }

    #[test]
    fn test_invalid_length() {
        let result = TxHash::new("abcd".to_string());
        assert!(matches!(
            result,
            Err(TxHashError::InvalidLength {
                expected: 64,
                actual: 4
            })
        ));
    }

    #[test]
    fn test_invalid_format() {
        let bad = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        assert_eq!(bad.len(), 64);
        let result = TxHash::new(bad.to_string());
        assert!(matches!(result, Err(TxHashError::InvalidFormat)));
    }

    #[test]
    fn test_serde_roundtrip() {
        let tx = TxHash::new_unchecked(VALID_HASH.to_string());
        let json = serde_json::to_string(&tx).unwrap();
        let parsed: TxHash = serde_json::from_str(&json).unwrap();
        assert_eq!(tx, parsed);
    }
}
