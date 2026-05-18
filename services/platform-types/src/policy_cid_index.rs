//! Wire types for the collection-ownership Policy CID Index —
//! `GET /api/cids/{policy_id}`.
//!
//! The endpoint returns every IPFS CID a policy's assets reference,
//! resolved from on-chain CIP-25 / CIP-68 metadata. The producer is
//! `cnft.dev-workers/workers/collection-ownership`; the primary
//! consumer is the Hodlcroft archivist, which polls it to learn what
//! to pin.

use serde::{Deserialize, Serialize};

/// Resolution state of a policy's CID index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CidIndexStatus {
    /// No resolution has run yet — the index is empty.
    Pending,
    /// A backfill is in progress; `cids` is incomplete.
    Resolving,
    /// A backfill page failed and will be retried — partial data.
    Partial,
    /// Fully resolved.
    Complete,
}

/// Response body of `GET /api/cids/{policy_id}`.
///
/// `cids` is one page of the policy's CID set, sorted by CID byte
/// order; page through with `next_cursor` until it is `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CidIndexResponse {
    /// Response-format version.
    pub schema_version: u32,
    /// Resolution state of the index.
    pub status: CidIndexStatus,
    /// Increments whenever the resolved CID set changes — drives the
    /// endpoint's `ETag`.
    pub cid_generation: u64,
    /// Unix seconds of the last CID-set change, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<i64>,
    /// Number of CIDs on this page.
    pub count: u32,
    /// CIDv1-normalised CIDs for this page.
    pub cids: Vec<String>,
    /// Opaque cursor for the next page; `None` on the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
