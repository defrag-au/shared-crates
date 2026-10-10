//! asset-metadata-index — the wire format of the R2-hosted resolved asset index.
//!
//! `(policy, asset)` → **resolved metadata and media links**, decoded once by a
//! builder on the box and read from R2/KV at the edge without CBOR work or a
//! CIP-25 / CIP-68 branch at the call site.
//!
//! # Pure, on purpose
//!
//! Bytes in, data out — no I/O, no network, no dependencies — so the box-side
//! builder (mitos) and the edge-side reader (a Cloudflare Worker) compile the
//! same code and cannot drift from the committed format. Every layout here has
//! a byte-stability test; the contract is
//! `cnft.dev-workers/docs/design/ASSET_METADATA_INDEX.md`.
//!
//! # The three objects
//!
//! - [`Pointer`] — a small text manifest, the one mutable object; names the
//!   built coverage.
//! - [`DirectoryView`] — per-policy-id-first-byte shards mapping a policy to
//!   the chunks that hold its records.
//! - [`SegmentView`] — one object per sealed chunk, immutable forever, holding
//!   the resolved [`Record`]s and a per-policy index into them.
//!
//! # Standing
//!
//! `latest()` answers the only question that matters for serving: which of an
//! asset's records is current. Latest wins by `(chunk, body_offset)` — chain
//! order — and a burn as the latest event clears the asset. The same rule for
//! both families.
//!
//! # The read walk
//!
//! [`resolve_asset`] is the reader: four rounds of batched reads through a
//! [`RangeSource`] — pointer and shard, segment headers, index regions, record
//! slices — ending in the same `latest` rule. The source owns the transport
//! (files natively, KV and R2 at the edge); the crate owns the order of the
//! reads and the bytes they must return.

#![warn(missing_docs)]

mod bytes;
mod directory;
mod error;
mod pointer;
mod record;
mod segment;
mod walk;

pub use directory::{
    DIRECTORY_HEADER_LEN, DirectoryEntry, DirectoryHeader, DirectoryShard, DirectoryView,
};
pub use error::{DecodeError, EncodeError};
pub use pointer::{Pointer, VERSION_KEY};
pub use record::{
    Event, Family, MediaLink, Provenance, RECORD_HEAD_LEN, Record, Records, ResolvedAsset,
    Standing, latest, recency, records, resolve,
};
pub use segment::{
    INDEX_ENTRY_LEN, IndexEntry, SEGMENT_HEADER_LEN, SegmentBuilder, SegmentHeader, SegmentView,
};
pub use walk::{
    DEFAULT_PREFIX, RangeSource, Read, WalkError, directory_key, pointer_key, resolve_asset,
    segment_key,
};

/// The format version every object in v1 carries.
pub const FORMAT_VERSION: u8 = 1;

/// Policy ids are 28 bytes on Cardano, everywhere in this format.
pub const POLICY_ID_LEN: usize = 28;

/// Network byte for the mainnet build.
pub const NETWORK_MAINNET: u8 = 0;

/// Network byte for a preprod build (not produced today; reserved with the value).
pub const NETWORK_PREPROD: u8 = 1;

pub(crate) const SEGMENT_MAGIC: [u8; 4] = *b"AMXS";
pub(crate) const DIRECTORY_MAGIC: [u8; 4] = *b"AMXD";
