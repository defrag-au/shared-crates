//! Typed wrapper for Cardano policy IDs.
//!
//! A policy ID is a 28-byte (56-character lowercase hex) hash that
//! identifies a minting script. Used as the first half of an
//! [`AssetId`](crate::AssetId) and as the canonical key for
//! collection-level operations.
//!
//! Wrapping it in a newtype lets callers reason about "this is a
//! policy id" at the type level — distinguishing it at compile
//! time from raw asset names, tx hashes, or arbitrary 56-char hex.
//! Validation runs once on construction; downstream code can rely
//! on the format.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 28 bytes = 56 lowercase hex characters.
pub(crate) const POLICY_ID_HEX_LEN: usize = 56;

/// Cardano policy ID — the script hash that identifies a minting
/// policy. Always 56 lowercase hex characters.
///
/// Construct with [`PolicyId::new`] (validated) or
/// [`PolicyId::new_unchecked`] (skip validation; use with care).
///
/// Serializes/deserializes as a plain JSON string for backward
/// compatibility with existing wire formats.
///
/// # Examples
///
/// ```
/// use cardano_assets::PolicyId;
///
/// let p = PolicyId::new("b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6").unwrap();
/// assert_eq!(p.as_str(), "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6");
///
/// // Wrong length — rejected.
/// assert!(PolicyId::new("abc").is_err());
///
/// // Non-hex characters — rejected.
/// assert!(PolicyId::new(&"z".repeat(56)).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PolicyId(String);

impl PolicyId {
    /// Construct a `PolicyId`, validating length (56) and that all
    /// characters are lowercase ASCII hex digits.
    pub fn new(s: impl Into<String>) -> Result<Self, PolicyIdError> {
        let s = s.into();
        Self::validate(&s)?;
        Ok(Self(s))
    }

    /// Construct without validation. Caller must ensure the string
    /// is 56 lowercase hex characters; otherwise downstream
    /// behaviour is undefined (most consumers re-validate, some
    /// don't).
    pub fn new_unchecked(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow as `&str`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Decode the hex form to its 28-byte binary representation.
    pub fn as_bytes(&self) -> Result<[u8; 28], PolicyIdError> {
        let mut out = [0u8; 28];
        let bytes = hex::decode(&self.0).map_err(|_| PolicyIdError::InvalidFormat)?;
        if bytes.len() != 28 {
            return Err(PolicyIdError::InvalidLength {
                expected: POLICY_ID_HEX_LEN,
                actual: self.0.len(),
            });
        }
        out.copy_from_slice(&bytes);
        Ok(out)
    }

    /// Move the inner `String` out.
    pub fn into_string(self) -> String {
        self.0
    }

    fn validate(s: &str) -> Result<(), PolicyIdError> {
        if s.len() != POLICY_ID_HEX_LEN {
            return Err(PolicyIdError::InvalidLength {
                expected: POLICY_ID_HEX_LEN,
                actual: s.len(),
            });
        }
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(PolicyIdError::InvalidFormat);
        }
        Ok(())
    }
}

impl fmt::Display for PolicyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PolicyId {
    type Err = PolicyIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl AsRef<str> for PolicyId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for PolicyId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PolicyId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        PolicyId::new(s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum PolicyIdError {
    InvalidLength { expected: usize, actual: usize },
    InvalidFormat,
}

impl fmt::Display for PolicyIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { expected, actual } => {
                write!(
                    f,
                    "Invalid policy ID length: expected {expected}, got {actual}"
                )
            }
            Self::InvalidFormat => {
                f.write_str("Invalid policy ID format: must be lowercase hexadecimal")
            }
        }
    }
}

impl std::error::Error for PolicyIdError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn known_good() -> &'static str {
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6"
    }

    #[test]
    fn accepts_valid_hex() {
        let p = PolicyId::new(known_good()).unwrap();
        assert_eq!(p.as_str(), known_good());
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(matches!(
            PolicyId::new("abc"),
            Err(PolicyIdError::InvalidLength { .. })
        ));
        assert!(matches!(
            PolicyId::new("a".repeat(55)),
            Err(PolicyIdError::InvalidLength { .. })
        ));
        assert!(matches!(
            PolicyId::new("a".repeat(57)),
            Err(PolicyIdError::InvalidLength { .. })
        ));
    }

    #[test]
    fn rejects_non_hex() {
        assert!(matches!(
            PolicyId::new("z".repeat(56)),
            Err(PolicyIdError::InvalidFormat)
        ));
    }

    #[test]
    fn as_bytes_round_trips() {
        let p = PolicyId::new(known_good()).unwrap();
        let bytes = p.as_bytes().unwrap();
        assert_eq!(hex::encode(bytes), known_good());
    }

    #[test]
    fn serde_round_trip_via_json() {
        let p = PolicyId::new(known_good()).unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, format!("\"{}\"", known_good()));
        let back: PolicyId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn fromstr_works() {
        let p: PolicyId = known_good().parse().unwrap();
        assert_eq!(p.as_str(), known_good());
    }

    #[test]
    fn display_matches_inner() {
        let p = PolicyId::new(known_good()).unwrap();
        assert_eq!(format!("{p}"), known_good());
    }
}
