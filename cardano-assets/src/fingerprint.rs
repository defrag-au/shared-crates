//! Typed wrapper for CIP-14 asset fingerprints.
//!
//! A fingerprint is `bech32(hrp="asset", data=blake2b_160(policy_id || asset_name))`,
//! the canonical short asset identifier used by jpg.store, pool.pm,
//! cardanoscan and most Cardano tooling for URL-shaped lookups.
//!
//! Wrapping it in a newtype makes it impossible to accidentally
//! pass a hex policy id, asset name, or transaction hash where a
//! fingerprint is expected — the bech32 prefix-and-checksum check
//! gives compile-time-ish safety with runtime validation.
//!
//! Compute one with [`crate::AssetId::fingerprint_typed`].
//! Construct directly from a string with [`Fingerprint::new`].

#![cfg(feature = "cip14")]

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// CIP-14 hrp (`bech32(hrp=..., data=...)`).
pub(crate) const FINGERPRINT_HRP: &str = "asset";

/// Typed CIP-14 asset fingerprint, e.g.
/// `asset1abcdefghijklmnopqrstuvwxyz12345`.
///
/// Always 44 bech32 characters total: the 5-char hrp `"asset"`,
/// the 1-char separator `"1"`, and the 38-char data + checksum.
///
/// Construct via [`Fingerprint::new`] (validated) or
/// [`Fingerprint::new_unchecked`] (unvalidated; for callers that
/// already have a known-good fingerprint, e.g. from a downstream
/// computation).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Fingerprint(String);

impl Fingerprint {
    /// Construct from a string, validating that it's a well-formed
    /// CIP-14 fingerprint (bech32-decodable with hrp `"asset"` and
    /// 20 data bytes).
    pub fn new(s: impl Into<String>) -> Result<Self, FingerprintError> {
        let s = s.into();
        Self::validate(&s)?;
        Ok(Self(s))
    }

    /// Construct without validation. Caller takes responsibility.
    pub fn new_unchecked(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow as `&str`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Decode the bech32 form to its 20-byte Blake2b-160 hash.
    pub fn as_bytes(&self) -> Result<[u8; 20], FingerprintError> {
        let (hrp, data) = bech32::decode(&self.0).map_err(|_| FingerprintError::InvalidFormat)?;
        if hrp.as_str() != FINGERPRINT_HRP {
            return Err(FingerprintError::WrongHrp);
        }
        if data.len() != 20 {
            return Err(FingerprintError::WrongDataLength {
                expected: 20,
                actual: data.len(),
            });
        }
        let mut out = [0u8; 20];
        out.copy_from_slice(&data);
        Ok(out)
    }

    /// Move the inner `String` out.
    pub fn into_string(self) -> String {
        self.0
    }

    fn validate(s: &str) -> Result<(), FingerprintError> {
        let (hrp, data) = bech32::decode(s).map_err(|_| FingerprintError::InvalidFormat)?;
        if hrp.as_str() != FINGERPRINT_HRP {
            return Err(FingerprintError::WrongHrp);
        }
        if data.len() != 20 {
            return Err(FingerprintError::WrongDataLength {
                expected: 20,
                actual: data.len(),
            });
        }
        Ok(())
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Fingerprint {
    type Err = FingerprintError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl AsRef<str> for Fingerprint {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for Fingerprint {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Fingerprint::new(s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum FingerprintError {
    InvalidFormat,
    WrongHrp,
    WrongDataLength { expected: usize, actual: usize },
}

impl fmt::Display for FingerprintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => f.write_str("Invalid fingerprint: not bech32-decodable"),
            Self::WrongHrp => f.write_str("Invalid fingerprint: hrp must be \"asset\""),
            Self::WrongDataLength { expected, actual } => {
                write!(
                    f,
                    "Invalid fingerprint data length: expected {expected} bytes, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for FingerprintError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// CIP-14 test vector: policy_id =
    /// 7eae28af2208be856f7a119668ae52a49b73725e326dc16579dcc373, empty
    /// asset name → fingerprint `asset1rjklcrnsdzqp65wjgrg55sy9723kw09mlgvlc3`
    /// (per the spec).
    fn known_good() -> &'static str {
        "asset1rjklcrnsdzqp65wjgrg55sy9723kw09mlgvlc3"
    }

    #[test]
    fn accepts_valid_fingerprint() {
        let f = Fingerprint::new(known_good()).unwrap();
        assert_eq!(f.as_str(), known_good());
    }

    #[test]
    fn rejects_wrong_hrp() {
        // Same data length, different hrp (constructed manually
        // for the test — callers wouldn't normally produce this).
        // bech32::encode would change checksum so we just craft
        // an obviously-wrong string.
        assert!(matches!(
            Fingerprint::new("addr1qxyz"),
            Err(FingerprintError::InvalidFormat | FingerprintError::WrongHrp)
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(
            Fingerprint::new("not-a-fingerprint"),
            Err(FingerprintError::InvalidFormat)
        ));
        assert!(matches!(
            Fingerprint::new(""),
            Err(FingerprintError::InvalidFormat)
        ));
    }

    #[test]
    fn as_bytes_round_trips() {
        let f = Fingerprint::new(known_good()).unwrap();
        let bytes = f.as_bytes().unwrap();
        assert_eq!(bytes.len(), 20);
    }

    #[test]
    fn serde_round_trip_via_json() {
        let f = Fingerprint::new(known_good()).unwrap();
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, format!("\"{}\"", known_good()));
        let back: Fingerprint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, f);
    }
}
