//! What can be wrong with bytes claiming to be part of the index.
//!
//! Every variant is a refusal — nothing here is recoverable by guessing. The
//! format's rule is *never truncate, never pad over*: a structural surprise
//! must read as corruption, because a plausible-looking shorter span or name
//! is the one output this format must not produce.

use std::fmt;

/// A read-side failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The input ended before the bytes being read were available.
    Truncated {
        /// Offset the read started at.
        at: usize,
        /// Bytes the read wanted.
        want: usize,
    },
    /// The first four bytes were not the object's magic.
    BadMagic {
        /// What the format says the object is.
        expected: [u8; 4],
        /// What was found.
        found: [u8; 4],
    },
    /// A format version this build does not implement.
    UnsupportedVersion(u8),
    /// A `family` byte outside `0..=2`.
    BadFamily(u8),
    /// An `event` byte outside `1..=3`.
    BadEvent(u8),
    /// A reserved field that must be zero was not.
    ReservedNotZero {
        /// Offset into the object.
        at: usize,
    },
    /// A text manifest field that is missing.
    MissingField(&'static str),
    /// A text manifest line that is not `key value`, or a known field whose
    /// value is not understood.
    BadField(&'static str),
    /// A length or offset that runs past its container.
    BadLength,
    /// A record consumed a different byte count than its own length field
    /// declared — the bytes and the format disagree.
    RecordLengthMismatch {
        /// `record_len` as stored.
        declared: usize,
        /// What decoding actually consumed.
        consumed: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { at, want } => {
                write!(f, "truncated: wanted {want} bytes at offset {at}")
            }
            Self::BadMagic { expected, found } => {
                write!(f, "bad magic: expected {expected:?}, found {found:?}")
            }
            Self::UnsupportedVersion(v) => write!(f, "unsupported format version {v}"),
            Self::BadFamily(b) => write!(f, "unknown family byte {b}"),
            Self::BadEvent(b) => write!(f, "unknown event byte {b}"),
            Self::MissingField(key) => write!(f, "manifest field '{key}' is missing"),
            Self::BadField(key) => write!(f, "manifest field '{key}' is malformed"),
            Self::ReservedNotZero { at } => {
                write!(f, "reserved field at offset {at} is not zero")
            }
            Self::BadLength => write!(f, "a length or offset runs past its container"),
            Self::RecordLengthMismatch { declared, consumed } => write!(
                f,
                "record declares {declared} bytes but consumed {consumed}"
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

/// A write-side failure: the format cannot state what the caller handed it.
///
/// There is no truncating constructor. A field that does not fit its width is
/// a builder bug to fix at the source — a silently shortened value would
/// parse back as a shorter, still-plausible answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// A field was longer than the format can state.
    FieldTooLong {
        /// Which field.
        field: &'static str,
        /// Its actual byte length.
        len: usize,
    },
    /// A version this build cannot encode.
    UnsupportedVersion(u8),
    /// A candidate carried a name filter but the shard would be encoded as
    /// v1, whose entry layout cannot state one.
    FilterInV1,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FieldTooLong { field, len } => {
                write!(f, "{field} is {len} bytes; the format cannot state it")
            }
            Self::UnsupportedVersion(version) => {
                write!(f, "cannot encode directory version {version}")
            }
            Self::FilterInV1 => write!(
                f,
                "a v1 directory cannot carry a candidate name filter — encode it as v2"
            ),
        }
    }
}

impl std::error::Error for EncodeError {}
