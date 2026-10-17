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
    /// A listing `form` byte outside `0..=3`.
    BadForm(u8),
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
            Self::BadForm(b) => write!(f, "unknown listing form byte {b}"),
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
    /// A listing carried two entries with the same name. One entry per name
    /// is the listing's contract — a duplicate is a builder bug, not
    /// something to silently collapse.
    DuplicateName,
    /// A v3 listing entry states no form. Every entry of the layout states
    /// one — a listing parsed from a v2 object carries none and cannot be
    /// re-encoded; rebuild it from the segments (the `--fold-only` reseed).
    MissingForm,
    /// A record's quantity contradicts its event: a mint must state a positive
    /// quantity, a burn a negative one, and an update neither. The sign is
    /// summed into an asset's supply, so a wrong one would move a standing
    /// asset out of its listing — a builder bug that must not reach bytes.
    BadQuantity {
        /// Which event the record states.
        event: crate::Event,
        /// The quantity the caller stated.
        qty: i64,
    },
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
            Self::DuplicateName => write!(
                f,
                "a listing carries one entry per name; a duplicate was handed to the builder"
            ),
            Self::MissingForm => write!(
                f,
                "a v3 listing entry states no form — a listing parsed from a v2 object \
                 cannot be re-encoded; reseed it from the segments"
            ),
            Self::BadQuantity { event, qty } => write!(
                f,
                "a {event:?} record states quantity {qty}; the sign must match the event"
            ),
        }
    }
}

impl std::error::Error for EncodeError {}
