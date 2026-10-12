//! The digest vocabulary — content addressing (§3.4) and fixity (the core
//! table plus community extensions `0001`/`0009-digest-algorithms`).
//!
//! Two types, because the spec has two vocabularies:
//!
//! - [`ContentAlgorithm`] — what an object's `digestAlgorithm` may be. The spec
//!   locks this to `sha512` or `sha256`; the algorithm extensions are
//!   fixity-only, so this is complete at two.
//! - [`DigestAlgorithm`] — the fixity vocabulary: `md5`, `sha1`, `sha256`,
//!   `sha512`, `blake2b-160`, `blake2b-256`, `blake2b-384`, `blake2b-512`,
//!   `sha512/256` and `size` (§3.4 + extension 0009, obsoleting 0001).
//!   Unrecognised names parse to [`DigestAlgorithm::Other`] — a client
//!   "MUST ignore" unsupported fixity algorithms rather than fail (§3.4), so
//!   they are preserved on round-trip and simply cannot be verified.
//!
//! All digests are non-truncated lowercase hex except `size`, which is the
//! byte count in decimal. Comparisons go through [`digest_eq`] — "base16
//! digests are case insensitive" (§3.4).

use blake2::digest::{Update, VariableOutput};
use blake2::{Blake2b512, Blake2bVar};
use md5::Md5;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512, Sha512_256};

/// The algorithms OCFL permits for an object's `digestAlgorithm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentAlgorithm {
    /// The spec's `SHOULD` — the living-token standard's one digest policy.
    Sha512,
    Sha256,
}

impl ContentAlgorithm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha512 => "sha512",
            Self::Sha256 => "sha256",
        }
    }

    /// Lowercase hex digits in a full digest of this algorithm.
    pub const fn hex_len(self) -> usize {
        match self {
            Self::Sha512 => 128,
            Self::Sha256 => 64,
        }
    }

    /// The lowercase hex digest of `bytes`.
    pub fn digest(self, bytes: &[u8]) -> String {
        match self {
            Self::Sha512 => hex::encode(Sha512::digest(bytes)),
            Self::Sha256 => hex::encode(Sha256::digest(bytes)),
        }
    }

    /// Is `value` a well-formed, non-truncated digest for this algorithm?
    pub fn is_well_formed(self, value: &str) -> bool {
        value.len() == self.hex_len() && value.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

impl std::fmt::Display for ContentAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The full fixity vocabulary this client understands — core table plus
/// extension `0009-digest-algorithms` — with an open arm for names from
/// extensions it does not implement.
///
/// Ordered so it can key the inventory's `fixity` block.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DigestAlgorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
    Blake2b160,
    Blake2b256,
    Blake2b384,
    Blake2b512,
    Sha512_256,
    /// Not a hash: the size of the file in bytes as a decimal string (0009).
    Size,
    /// A name this client does not implement — preserved, ignored in
    /// verification, never misinterpreted.
    Other(String),
}

impl DigestAlgorithm {
    pub fn from_content(algorithm: ContentAlgorithm) -> Self {
        match algorithm {
            ContentAlgorithm::Sha512 => Self::Sha512,
            ContentAlgorithm::Sha256 => Self::Sha256,
        }
    }

    /// Parse a vocabulary name (case-insensitively for known names; unknown
    /// names are kept verbatim).
    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "md5" => Self::Md5,
            "sha1" => Self::Sha1,
            "sha256" => Self::Sha256,
            "sha512" => Self::Sha512,
            "blake2b-160" => Self::Blake2b160,
            "blake2b-256" => Self::Blake2b256,
            "blake2b-384" => Self::Blake2b384,
            "blake2b-512" => Self::Blake2b512,
            "sha512/256" => Self::Sha512_256,
            "size" => Self::Size,
            _ => Self::Other(name.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Md5 => "md5",
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
            Self::Blake2b160 => "blake2b-160",
            Self::Blake2b256 => "blake2b-256",
            Self::Blake2b384 => "blake2b-384",
            Self::Blake2b512 => "blake2b-512",
            Self::Sha512_256 => "sha512/256",
            Self::Size => "size",
            Self::Other(name) => name,
        }
    }

    /// True when this client can compute the algorithm (everything but
    /// [`Self::Other`]).
    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Other(_))
    }

    /// Digits in a well-formed value of this algorithm: hex length, or `None`
    /// for a variable-length algorithm or an unknown one.
    pub fn hex_len(&self) -> Option<usize> {
        match self {
            Self::Md5 => Some(32),
            Self::Sha1 => Some(40),
            Self::Sha256 | Self::Blake2b256 | Self::Sha512_256 => Some(64),
            Self::Blake2b384 => Some(96),
            Self::Sha512 | Self::Blake2b512 => Some(128),
            Self::Blake2b160 => Some(40),
            Self::Size | Self::Other(_) => None,
        }
    }

    /// The lowercase representation of `bytes` under this algorithm — `None`
    /// only for names this client does not implement.
    pub fn digest(&self, bytes: &[u8]) -> Option<String> {
        let value = match self {
            Self::Md5 => hex::encode(Md5::digest(bytes)),
            Self::Sha1 => hex::encode(Sha1::digest(bytes)),
            Self::Sha256 => hex::encode(Sha256::digest(bytes)),
            Self::Sha512 => hex::encode(Sha512::digest(bytes)),
            Self::Sha512_256 => hex::encode(Sha512_256::digest(bytes)),
            Self::Blake2b512 => hex::encode(Blake2b512::digest(bytes)),
            Self::Blake2b160 => blake2b_variable(bytes, 20)?,
            Self::Blake2b256 => blake2b_variable(bytes, 32)?,
            Self::Blake2b384 => blake2b_variable(bytes, 48)?,
            Self::Size => bytes.len().to_string(),
            Self::Other(_) => return None,
        };
        Some(value)
    }

    /// Is `value` well-formed for this algorithm? (False for unknown names —
    /// callers should consult [`Self::is_known`] first.)
    pub fn is_well_formed(&self, value: &str) -> bool {
        match self {
            Self::Size => !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
            Self::Other(_) => false,
            _ => match self.hex_len() {
                Some(length) => {
                    value.len() == length && value.bytes().all(|b| b.is_ascii_hexdigit())
                }
                None => false,
            },
        }
    }
}

impl std::fmt::Display for DigestAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for DigestAlgorithm {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DigestAlgorithm {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(Self::from_name(&name))
    }
}

fn blake2b_variable(bytes: &[u8], size: usize) -> Option<String> {
    let mut hasher = Blake2bVar::new(size).ok()?;
    hasher.update(bytes);
    let mut output = vec![0u8; size];
    hasher.finalize_variable(&mut output).ok()?;
    Some(hex::encode(output))
}

/// OCFL digests compare case-insensitively (§3.4). Comparisons of inventory
/// digests go through this, never `==`.
pub fn digest_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_algorithms_match_the_spec_examples() {
        assert_eq!(
            ContentAlgorithm::Sha512.digest(b""),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
             47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
        assert!(
            ContentAlgorithm::Sha256
                .digest(b"")
                .starts_with("e3b0c44298fc1c149afbf4c8996fb92427ae41e4")
        );
    }

    #[test]
    fn every_fixity_algorithm_matches_its_spec_vector() {
        // §3.4's table and extension 0009's, with the empty-bitstream
        // vectors the specs print.
        assert_eq!(
            DigestAlgorithm::Md5.digest(b"").unwrap(),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
        assert_eq!(
            DigestAlgorithm::Sha1.digest(b"").unwrap(),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(
            DigestAlgorithm::Blake2b160.digest(b"").unwrap(),
            "3345524abf6bbe1809449224b5972c41790b6cf2"
        );
        assert!(
            DigestAlgorithm::Blake2b256
                .digest(b"")
                .unwrap()
                .starts_with("0e5751c026e543b2e8ab2eb06099daa1d1e5df47")
        );
        assert!(
            DigestAlgorithm::Blake2b384
                .digest(b"")
                .unwrap()
                .starts_with("b32811423377f52d7862286ee1a72ee540524380")
        );
        assert!(
            DigestAlgorithm::Sha512_256
                .digest(b"")
                .unwrap()
                .starts_with("c672b8d1ef56ed28ab87c3622c5114069bdd3ad7")
        );
        // `blake2b-512` is core vocabulary (§3.4), not extension 0009's.
        assert!(
            DigestAlgorithm::Blake2b512
                .digest(b"")
                .unwrap()
                .starts_with("786a02f742015903c6c6fd852552d272912f4740")
        );
        assert_eq!(DigestAlgorithm::Size.digest(b"").unwrap(), "0");
        assert_eq!(DigestAlgorithm::Size.digest(b"12345").unwrap(), "5");
        // A vector the layout extension specs share: md5("object-01").
        assert_eq!(
            DigestAlgorithm::Md5.digest(b"object-01").unwrap(),
            "ff75534492485eabb39f86356728884e"
        );
    }

    #[test]
    fn unknown_names_round_trip_as_other() {
        let parsed = DigestAlgorithm::from_name("xxh3-128");
        assert_eq!(parsed, DigestAlgorithm::Other("xxh3-128".to_string()));
        assert!(!parsed.is_known());
        assert!(parsed.digest(b"x").is_none());
        let json = serde_json::to_string(&parsed).unwrap();
        assert_eq!(json, "\"xxh3-128\"");
        assert_eq!(
            serde_json::from_str::<DigestAlgorithm>(&json).unwrap(),
            parsed
        );
    }

    #[test]
    fn well_formed_is_shape_only() {
        assert!(DigestAlgorithm::Md5.is_well_formed(&"a".repeat(32)));
        assert!(!DigestAlgorithm::Md5.is_well_formed(&"a".repeat(40)));
        assert!(DigestAlgorithm::Size.is_well_formed("12345"));
        assert!(!DigestAlgorithm::Size.is_well_formed("12a"));
        assert!(!DigestAlgorithm::Size.is_well_formed(""));
    }
}
