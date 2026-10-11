//! Digest algorithms OCFL 1.1 permits for content addressing — and nothing else.
//!
//! §3.4: "For content-addressing, OCFL Objects MUST use either `sha512` or
//! `sha256`, and SHOULD use `sha512`." Digests are non-truncated, base16
//! encoded, and compare **case-insensitively**.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};

/// The two algorithms OCFL 1.1 accepts for `digestAlgorithm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DigestAlgorithm {
    /// The spec's `SHOULD` — the living-token standard's one digest policy.
    Sha512,
    Sha256,
}

impl DigestAlgorithm {
    /// The algorithm's name as it appears in an inventory (`"sha512"`).
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

impl std::fmt::Display for DigestAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// OCFL digests compare case-insensitively — "different tools will generate
/// digests in uppercase or lowercase" (§3.4). Comparisons of inventory digests
/// go through this, never `==`.
pub fn digest_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bitstream_vectors_match_the_spec_examples() {
        assert_eq!(
            DigestAlgorithm::Sha512.digest(b""),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
             47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
        assert!(
            DigestAlgorithm::Sha256
                .digest(b"")
                .starts_with("e3b0c44298fc1c149afbf4c8996fb92427ae41e4")
        );
    }

    #[test]
    fn well_formed_is_length_and_charset_only() {
        let alg = DigestAlgorithm::Sha512;
        assert!(alg.is_well_formed(&"a".repeat(128)));
        assert!(alg.is_well_formed(&"A0".repeat(64))); // uppercase hex is legal
        assert!(!alg.is_well_formed(&"a".repeat(127)));
        assert!(!alg.is_well_formed(&"z".repeat(128)));
    }

    #[test]
    fn digest_comparisons_ignore_case() {
        assert!(digest_eq("ABC123", "abc123"));
        assert!(!digest_eq("abc", "abd"));
    }
}
