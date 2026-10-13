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
//! # The four objects
//!
//! - [`Pointer`] — a small text manifest, the one mutable object; names the
//!   built coverage.
//! - [`DirectoryView`] — per-policy-id-first-byte shards mapping a policy to
//!   the chunks that hold its records; v2 shards carry a name filter per
//!   candidate, which is what keeps a resolve's read count off the policy's
//!   size ([`candidates_for`](DirectoryView::candidates_for)).
//! - [`SegmentView`] — one object per sealed chunk, immutable forever, holding
//!   the resolved [`Record`]s and a per-policy index into them.
//! - [`ListingView`] — one object per policy: its standing assets,
//!   name-sorted, walked a window at a time by [`entry_at`] when a page is
//!   wanted. The enumeration read ("everything in this collection") where
//!   the walk answers one asset at a time. Each entry states its [`Form`] —
//!   which standard carries the metadata and, for CIP-68, whether the name is
//!   the reference token that carries the datum or a form a holder holds —
//!   so a collection view reads without parsing name prefixes.
//!
//! # Standing
//!
//! [`standing`] answers the only question that matters for serving: does the
//! asset exist, and what does it look like. Quantities are summed across its
//! records in chain order — a mint adds, a burn removes, an update moves none
//! — floored at zero so a burn the build never saw minted cannot erase an
//! asset. An asset stands while the sum is above zero, and the metadata in
//! force is the newest non-burn record. The same rule for both families.
//! Summing rather than "latest wins" is what a CIP-25 dynamic update (mint the
//! new copy, then burn the old) and a native token's supply both require.
//!
//! # The read walk
//!
//! [`resolve_asset`] is the reader: the pointer and shard, then one batched
//! round over the candidate segments — whole objects, with a ranges fallback
//! for the ones past [`WHOLE_SEGMENT_CAP`] — ending in the same `standing`
//! rule. The source owns the transport (files natively, KV and CDN at the
//! edge); the crate owns the order of the reads and the bytes they must
//! return.
//!
//! # The daily sweep
//!
//! [`patch_shard`] is the delta derivation the refresh chain runs between
//! full rebuilds: it folds the chunks that sealed since a cutoff into a
//! published v2 directory, rebuilding only the entries those chunks touched.

#![warn(missing_docs)]

mod bytes;
mod delta;
mod directory;
mod error;
mod filter;
mod listing;
mod pointer;
mod record;
mod segment;
mod walk;

pub use delta::{DeltaError, PatchedShard, patch_shard};
pub use directory::{
    DIRECTORY_HEADER_LEN, DirectoryCandidate, DirectoryEntry, DirectoryHeader, DirectoryShard,
    DirectoryView,
};
pub use error::{DecodeError, EncodeError};
pub use filter::{FilterBuilder, might_contain};
pub use listing::{
    CIP68_REFERENCE_LABEL, Form, LISTING_HEADER_LEN, LISTING_MAX_ENTRY_LEN, ListingBuilder,
    ListingEntry, ListingHeader, ListingView, entry_at,
};
pub use pointer::{Pointer, VERSION_KEY};
pub use record::{
    Event, Family, MediaLink, Provenance, RECORD_HEAD_LEN, Record, Records, ResolvedAsset,
    Standing, recency, records, resolve, standing,
};
pub use segment::{
    INDEX_ENTRY_LEN, IndexEntry, SEGMENT_HEADER_LEN, SegmentBuilder, SegmentHeader, SegmentView,
};
pub use walk::{
    DEFAULT_PREFIX, RangeSource, Read, WHOLE_SEGMENT_CAP, WalkError, directory_key, listing_key,
    pointer_key, resolve_asset, segment_key,
};

/// The segment (and the record inside it) layout this build writes and reads.
///
/// v2 added the record's signed `qty` — the quantity its transaction moved —
/// which is what standing sums. There is no v1 reader: the v1 tree was
/// superseded wholesale (`index/asset-meta/v2`), not read alongside.
pub const SEGMENT_VERSION: u8 = 2;

/// The collection-listing layout this build writes and reads.
///
/// v2 added the per-entry `supply`; v3 added the per-entry `form` byte and
/// the header's `display_count`. A v3 reader also reads v2 — the one
/// transition a running tree has: the reseed rewrites every listing in place,
/// and until it has, the previously deployed reader must keep serving. A v2
/// entry states no form, so the display view refuses a v2 listing rather than
/// guessing.
pub const LISTING_VERSION: u8 = 3;

/// The superseded listing layout — `supply`, no `form` byte, no
/// `display_count`. Read (so a pre-reseed object keeps serving), never
/// written.
pub const LISTING_V2: u8 = 2;

/// The pointer manifest's version line (`ami_version`).
///
/// v2 marks a tree whose segments carry record quantities.
pub const POINTER_VERSION: u8 = 2;

/// Directory entry-layout version 1 — chunk lists only. The live full
/// build published it once; no reader here still accepts it.
pub const DIRECTORY_V1: u8 = 1;

/// Directory entry-layout version 2 — each candidate also carries a Bloom
/// filter over the names of that `(policy, chunk)`'s records, so a resolve
/// reads only the candidates that can hold the name it was asked for.
pub const DIRECTORY_V2: u8 = 2;

/// Policy ids are 28 bytes on Cardano, everywhere in this format.
pub const POLICY_ID_LEN: usize = 28;

/// Network byte for the mainnet build.
pub const NETWORK_MAINNET: u8 = 0;

/// Network byte for a preprod build (not produced today; reserved with the value).
pub const NETWORK_PREPROD: u8 = 1;

pub(crate) const SEGMENT_MAGIC: [u8; 4] = *b"AMXS";
pub(crate) const DIRECTORY_MAGIC: [u8; 4] = *b"AMXD";
pub(crate) const LISTING_MAGIC: [u8; 4] = *b"AMXL";
