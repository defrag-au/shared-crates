//! Decode failures.
//!
//! Every one of these is a *named* outcome. The rule from the schema doc
//! (§2.2, §2.5) is that a malformed datum never panics and never yields a
//! partial value presented as whole — a reader that cannot understand a
//! definition stores the raw bytes and says so.

use crate::codec::UnknownFields;

/// Why a datum could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// A field with no declared default was absent.
    #[error("missing required field {id}")]
    MissingField { id: i64 },

    /// The same integer key appeared twice in one map. Rejected rather than
    /// last-write-wins: a canonical re-encode would silently drop one of
    /// them, and this format's whole promise is that what you decoded is
    /// what you can put back.
    #[error("duplicate field id {id}")]
    DuplicateField { id: i64 },

    /// An integer on the wire did not fit the declared Rust type. Never
    /// truncated (§2.4).
    #[error("integer {value} out of range for {target}")]
    OutOfRange { value: i128, target: &'static str },

    /// A bool is `0` or `1` and nothing else (§2.4).
    #[error("invalid bool encoding: {value}")]
    InvalidBool { value: i128 },

    /// The reader supports schema versions up to some N and was handed a
    /// higher one. The caller keeps the raw datum and shows "needs a newer
    /// worker" (§2.5).
    #[error("unsupported schema version {version}")]
    Unsupported { version: u64 },

    /// An enum map carried no key `0`.
    #[error("enum map has no tag at key 0")]
    MissingTag,

    /// A fixed-width byte field (a 28-byte hash, a 32-byte tx id) was the
    /// wrong length.
    #[error("expected {expected} bytes for {target}, found {actual}")]
    BadLength {
        target: &'static str,
        expected: usize,
        actual: usize,
    },

    /// A `String` field held bytes that are not UTF-8.
    #[error("invalid utf-8 in a text field")]
    InvalidUtf8,

    /// The PlutusData was the wrong shape — a `Map` where an `Array` was
    /// declared, and so on.
    #[error("expected {expected}, found {found}")]
    WrongShape {
        expected: &'static str,
        found: &'static str,
    },

    /// A `Constr` envelope carried an unexpected constructor index.
    #[error("expected constructor {expected}, found {found}")]
    WrongConstructor { expected: u64, found: u64 },

    /// A positional envelope was shorter than its frozen field count.
    #[error("envelope has {actual} fields, expected at least {expected}")]
    ShortEnvelope { expected: usize, actual: usize },
}

impl DecodeError {
    /// The kind, with no payload — for metrics, match arms over
    /// [`DecodeErrorKind::ALL`], and copy that must cover every case.
    pub const fn kind(&self) -> DecodeErrorKind {
        match self {
            Self::MissingField { .. } => DecodeErrorKind::MissingField,
            Self::DuplicateField { .. } => DecodeErrorKind::DuplicateField,
            Self::OutOfRange { .. } => DecodeErrorKind::OutOfRange,
            Self::InvalidBool { .. } => DecodeErrorKind::InvalidBool,
            Self::Unsupported { .. } => DecodeErrorKind::Unsupported,
            Self::MissingTag => DecodeErrorKind::MissingTag,
            Self::BadLength { .. } => DecodeErrorKind::BadLength,
            Self::InvalidUtf8 => DecodeErrorKind::InvalidUtf8,
            Self::WrongShape { .. } => DecodeErrorKind::WrongShape,
            Self::WrongConstructor { .. } => DecodeErrorKind::WrongConstructor,
            Self::ShortEnvelope { .. } => DecodeErrorKind::ShortEnvelope,
        }
    }
}

/// [`DecodeError`] with its payload stripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecodeErrorKind {
    MissingField,
    DuplicateField,
    OutOfRange,
    InvalidBool,
    Unsupported,
    MissingTag,
    BadLength,
    InvalidUtf8,
    WrongShape,
    WrongConstructor,
    ShortEnvelope,
}

impl DecodeErrorKind {
    pub const ALL: [DecodeErrorKind; 11] = [
        Self::MissingField,
        Self::DuplicateField,
        Self::OutOfRange,
        Self::InvalidBool,
        Self::Unsupported,
        Self::MissingTag,
        Self::BadLength,
        Self::InvalidUtf8,
        Self::WrongShape,
        Self::WrongConstructor,
        Self::ShortEnvelope,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingField => "missing_field",
            Self::DuplicateField => "duplicate_field",
            Self::OutOfRange => "out_of_range",
            Self::InvalidBool => "invalid_bool",
            Self::Unsupported => "unsupported",
            Self::MissingTag => "missing_tag",
            Self::BadLength => "bad_length",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::WrongShape => "wrong_shape",
            Self::WrongConstructor => "wrong_constructor",
            Self::ShortEnvelope => "short_envelope",
        }
    }
}

impl std::fmt::Display for DecodeErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The unknown ids and tags a decode carried through, reported alongside a
/// successful value.
///
/// An old reader on a new datum succeeds; this is how it *says* that it did
/// not understand everything, so an operator surface can show "2 fields this
/// build does not know" instead of implying the datum is fully understood.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnknownReport {
    /// Field ids seen at each path, outermost first.
    pub fields: Vec<(&'static str, Vec<i64>)>,
    /// Enum tags this build has no variant for.
    pub tags: Vec<(&'static str, i64)>,
}

impl UnknownReport {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.tags.is_empty()
    }

    pub fn note_fields(&mut self, at: &'static str, unknown: &UnknownFields) {
        if !unknown.is_empty() {
            self.fields.push((at, unknown.ids().collect()));
        }
    }

    pub fn note_tag(&mut self, at: &'static str, tag: i64) {
        self.tags.push((at, tag));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_distinct_string() {
        let mut seen = std::collections::HashSet::new();
        for kind in DecodeErrorKind::ALL {
            assert!(seen.insert(kind.as_str()), "duplicate string for {kind:?}");
        }
        assert_eq!(seen.len(), DecodeErrorKind::ALL.len());
    }

    #[test]
    fn kind_round_trips_through_every_error() {
        // One representative per variant — if a variant is added to
        // DecodeError without a `kind()` arm this fails to compile, and if
        // it is added to neither list the count assertion below fails.
        let errors = [
            DecodeError::MissingField { id: 1 },
            DecodeError::DuplicateField { id: 1 },
            DecodeError::OutOfRange {
                value: 1,
                target: "u8",
            },
            DecodeError::InvalidBool { value: 2 },
            DecodeError::Unsupported { version: 9 },
            DecodeError::MissingTag,
            DecodeError::BadLength {
                target: "PolicyId",
                expected: 28,
                actual: 27,
            },
            DecodeError::InvalidUtf8,
            DecodeError::WrongShape {
                expected: "map",
                found: "array",
            },
            DecodeError::WrongConstructor {
                expected: 0,
                found: 1,
            },
            DecodeError::ShortEnvelope {
                expected: 3,
                actual: 2,
            },
        ];
        let kinds: std::collections::HashSet<_> = errors.iter().map(|e| e.kind()).collect();
        assert_eq!(kinds.len(), DecodeErrorKind::ALL.len());
    }
}
