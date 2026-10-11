//! The directory: which chunks hold a policy's records — and, in v2, which of
//! those could hold a given asset name.
//!
//! One object per policy-id first byte (256 shards), so a shard only changes
//! when a policy inside it mints or updates — a stable collection's shard is
//! untouched for as long as the collection is. Entries are additive: a reader
//! that sees a stale shard scans the frontier segments named by the pointer,
//! so staleness can cost work but never produce a wrong answer.
//!
//! Shards are read whole (hundreds of entries) and scanned; no intra-shard
//! index. Two entry-layout versions are read here: v1 carries chunk lists,
//! v2 adds a per-candidate Bloom filter ([`crate::FilterBuilder`]) over the
//! names of that `(policy, chunk)`'s records, so a resolve reads only the
//! candidates that can hold the asset it was handed. In both versions
//! entries are sorted by `policy_id` — the scan's early exit is what keeps a
//! whole-shard read cheap.

use crate::bytes::{array_at, take_u16_prefixed, u8_at, u16_at, u32_at};
use crate::error::{DecodeError, EncodeError};
use crate::filter::might_contain;
use crate::{DIRECTORY_MAGIC, DIRECTORY_V1, DIRECTORY_V2, POLICY_ID_LEN};

/// Fixed header length.
pub const DIRECTORY_HEADER_LEN: usize = 32;

/// A shard's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryHeader {
    /// Entry-layout version — [`DIRECTORY_V1`] or [`DIRECTORY_V2`].
    pub version: u8,
    /// Network byte.
    pub network: u8,
    /// Shard ordinal — equal to the policy id's first byte.
    pub shard: u8,
    /// Number of entries that follow.
    pub entry_count: u32,
    /// The chunk the directory is complete through; segments newer than this
    /// are the frontier a reader also consults.
    pub directory_through_chunk: u16,
}

impl DirectoryHeader {
    /// Parse from the front of a shard object.
    pub fn parse(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut at = 0usize;
        let magic = array_at::<4>(bytes, &mut at)?;
        if magic != DIRECTORY_MAGIC {
            return Err(DecodeError::BadMagic {
                expected: DIRECTORY_MAGIC,
                found: magic,
            });
        }
        let version = u8_at(bytes, &mut at)?;
        if version != DIRECTORY_V1 && version != DIRECTORY_V2 {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let network = u8_at(bytes, &mut at)?;
        let shard = u8_at(bytes, &mut at)?;
        if u8_at(bytes, &mut at)? != 0 {
            return Err(DecodeError::ReservedNotZero { at: at - 1 });
        }
        let entry_count = u32_at(bytes, &mut at)?;
        let directory_through_chunk = u16_at(bytes, &mut at)?;
        for _ in 0..18 {
            if u8_at(bytes, &mut at)? != 0 {
                return Err(DecodeError::ReservedNotZero { at: at - 1 });
            }
        }
        Ok(Self {
            version,
            network,
            shard,
            entry_count,
            directory_through_chunk,
        })
    }

    /// Encode the 32-byte header.
    #[must_use]
    pub fn encode(&self) -> [u8; DIRECTORY_HEADER_LEN] {
        let mut out = [0u8; DIRECTORY_HEADER_LEN];
        out[0..4].copy_from_slice(&DIRECTORY_MAGIC);
        out[4] = self.version;
        out[5] = self.network;
        out[6] = self.shard;
        out[8..12].copy_from_slice(&self.entry_count.to_le_bytes());
        out[12..14].copy_from_slice(&self.directory_through_chunk.to_le_bytes());
        out
    }
}

/// One candidate chunk for a policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryCandidate {
    /// The chunk.
    pub chunk: u16,
    /// The name filter for this `(policy, chunk)` — always empty in v1, and
    /// *unfiltered: always read* when empty in v2.
    pub filter: Vec<u8>,
}

/// One policy's directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// The policy.
    pub policy_id: [u8; POLICY_ID_LEN],
    /// Candidate chunks, strictly ascending by chunk.
    pub candidates: Vec<DirectoryCandidate>,
}

/// A whole shard, borrowed.
pub struct DirectoryView<'a> {
    /// The parsed header.
    pub header: DirectoryHeader,
    bytes: &'a [u8],
}

impl<'a> DirectoryView<'a> {
    /// Parse a whole shard object.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            header: DirectoryHeader::parse(bytes)?,
            bytes,
        })
    }

    /// The chunks recorded for `policy_id`, ascending, or `None` when the
    /// policy has no entry — the inspect/tooling view; the read path asks
    /// [`Self::candidates_for`].
    pub fn chunks_for(
        &self,
        policy_id: &[u8; POLICY_ID_LEN],
    ) -> Result<Option<Vec<u16>>, DecodeError> {
        Ok(self.candidates(policy_id)?.map(|candidates| {
            candidates
                .into_iter()
                .map(|candidate| candidate.chunk)
                .collect()
        }))
    }

    /// The candidate chunks whose filter admits `name`. Every candidate in a
    /// v1 shard, and every unfiltered candidate anywhere: an empty filter
    /// means *always a candidate*, and a Bloom has no false negatives, so
    /// pruning cannot hide a name that was inserted — a false positive only
    /// costs a read, and the walk still confirms every candidate it reads by
    /// parsing it.
    pub fn candidates_for(
        &self,
        policy_id: &[u8; POLICY_ID_LEN],
        name: &[u8],
    ) -> Result<Option<Vec<u16>>, DecodeError> {
        Ok(self.candidates(policy_id)?.map(|candidates| {
            candidates
                .into_iter()
                .filter(|candidate| {
                    candidate.filter.is_empty() || might_contain(&candidate.filter, name)
                })
                .map(|candidate| candidate.chunk)
                .collect()
        }))
    }

    /// The candidates stored for `policy_id`, or `None` when the policy has
    /// no entry. Entries are sorted, so the scan can stop early.
    pub fn candidates(
        &self,
        policy_id: &[u8; POLICY_ID_LEN],
    ) -> Result<Option<Vec<DirectoryCandidate>>, DecodeError> {
        let mut at = DIRECTORY_HEADER_LEN;
        for _ in 0..self.header.entry_count {
            let entry = self.entry_at(&mut at)?;
            if &entry.policy_id == policy_id {
                return Ok(Some(entry.candidates));
            }
            if entry.policy_id > *policy_id {
                return Ok(None);
            }
        }
        Ok(None)
    }

    /// Every entry, in stored order — the builder-side verify and the inspect
    /// tooling read this; the read path uses [`Self::candidates_for`].
    pub fn entries(&self) -> Result<Vec<DirectoryEntry>, DecodeError> {
        let mut out = Vec::with_capacity(self.header.entry_count as usize);
        let mut at = DIRECTORY_HEADER_LEN;
        for _ in 0..self.header.entry_count {
            out.push(self.entry_at(&mut at)?);
        }
        Ok(out)
    }

    /// One entry at the cursor, in whichever layout the header names.
    fn entry_at(&self, at: &mut usize) -> Result<DirectoryEntry, DecodeError> {
        let policy_id = array_at::<POLICY_ID_LEN>(self.bytes, at)?;
        let count = u16_at(self.bytes, at)?;
        let mut candidates = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let chunk = u16_at(self.bytes, at)?;
            let filter = match self.header.version {
                // `parse` admits only V1 and V2, so the other arm is v2.
                DIRECTORY_V1 => Vec::new(),
                _ => take_u16_prefixed(self.bytes, at)?.to_vec(),
            };
            candidates.push(DirectoryCandidate { chunk, filter });
        }
        Ok(DirectoryEntry {
            policy_id,
            candidates,
        })
    }
}

/// Assemble one shard's bytes. Builder-side.
pub struct DirectoryShard {
    /// The entry-layout version to encode — [`DIRECTORY_V1`] (chunk lists
    /// only) or [`DIRECTORY_V2`] (with per-candidate name filters).
    pub version: u8,
    /// Network byte.
    pub network: u8,
    /// Shard ordinal (the policy id first byte this shard covers).
    pub shard: u8,
    /// The chunk the shard is complete through.
    pub directory_through_chunk: u16,
    /// Entries; sorted defensively on encode, because the reader's early-exit
    /// scan depends on order.
    pub entries: Vec<DirectoryEntry>,
}

impl DirectoryShard {
    /// Encode header + entries.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        if self.version != DIRECTORY_V1 && self.version != DIRECTORY_V2 {
            return Err(EncodeError::UnsupportedVersion(self.version));
        }
        let mut entries = self.entries.clone();
        entries.sort_by_key(|e| e.policy_id);
        for entry in &mut entries {
            // Ascending by chunk; a duplicate chunk keeps the filtered copy
            // over the unfiltered one, so the defensive dedup cannot silently
            // drop a filter.
            entry.candidates.sort_by(|a, b| {
                a.chunk
                    .cmp(&b.chunk)
                    .then_with(|| a.filter.is_empty().cmp(&b.filter.is_empty()))
            });
            entry.candidates.dedup_by_key(|candidate| candidate.chunk);
        }
        let entry_count = u32::try_from(entries.len()).map_err(|_| EncodeError::FieldTooLong {
            field: "entry_count",
            len: entries.len(),
        })?;
        let header = DirectoryHeader {
            version: self.version,
            network: self.network,
            shard: self.shard,
            entry_count,
            directory_through_chunk: self.directory_through_chunk,
        };
        let mut out = Vec::new();
        out.extend_from_slice(&header.encode());
        for entry in &entries {
            out.extend_from_slice(&entry.policy_id);
            let count =
                u16::try_from(entry.candidates.len()).map_err(|_| EncodeError::FieldTooLong {
                    field: "candidate_count",
                    len: entry.candidates.len(),
                })?;
            out.extend_from_slice(&count.to_le_bytes());
            for candidate in &entry.candidates {
                out.extend_from_slice(&candidate.chunk.to_le_bytes());
                match self.version {
                    DIRECTORY_V2 => {
                        crate::bytes::put_u16_prefixed(&mut out, "filter_len", &candidate.filter)?;
                    }
                    _ => {
                        if !candidate.filter.is_empty() {
                            return Err(EncodeError::FilterInV1);
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(byte: u8) -> [u8; POLICY_ID_LEN] {
        [byte; POLICY_ID_LEN]
    }

    fn candidate(chunk: u16) -> DirectoryCandidate {
        DirectoryCandidate {
            chunk,
            filter: Vec::new(),
        }
    }

    fn filtered(names: &[&[u8]]) -> Vec<u8> {
        let mut builder = crate::FilterBuilder::new(names.len());
        for name in names {
            builder.insert(name);
        }
        builder.finish()
    }

    fn shard() -> Vec<u8> {
        DirectoryShard {
            version: DIRECTORY_V1,
            network: crate::NETWORK_MAINNET,
            shard: 0x02,
            directory_through_chunk: 900,
            entries: vec![
                DirectoryEntry {
                    policy_id: policy(0x02),
                    candidates: vec![candidate(7), candidate(2)],
                },
                DirectoryEntry {
                    policy_id: policy(0x01),
                    candidates: vec![candidate(3)],
                },
            ],
        }
        .encode()
        .expect("encodes")
    }

    fn v2_shard() -> Vec<u8> {
        DirectoryShard {
            version: DIRECTORY_V2,
            network: crate::NETWORK_MAINNET,
            shard: 0x02,
            directory_through_chunk: 900,
            entries: vec![DirectoryEntry {
                policy_id: policy(0x02),
                candidates: vec![
                    DirectoryCandidate {
                        chunk: 7,
                        filter: filtered(&[b"alpha", b"a1", b"a2"]),
                    },
                    DirectoryCandidate {
                        chunk: 2,
                        filter: filtered(&[b"beta", b"b1", b"b2"]),
                    },
                    candidate(9),
                ],
            }],
        }
        .encode()
        .expect("encodes")
    }

    fn hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    #[test]
    fn a_shard_round_trips_and_sorts() {
        let bytes = shard();
        let view = DirectoryView::parse(&bytes).expect("parses");
        assert_eq!(view.header.version, DIRECTORY_V1);
        assert_eq!(view.header.shard, 0x02);
        assert_eq!(view.header.directory_through_chunk, 900);

        let entries = view.entries().expect("entries");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].policy_id, policy(0x01));
        assert_eq!(entries[1].candidates, vec![candidate(2), candidate(7)]);
        assert!(entries[1].candidates.iter().all(|c| c.filter.is_empty()));

        assert_eq!(
            view.chunks_for(&policy(0x01)).expect("parses"),
            Some(vec![3])
        );
        assert_eq!(view.chunks_for(&policy(0x09)).expect("parses"), None);
        // A v1 shard has no filters: every name asks for every candidate.
        assert_eq!(
            view.candidates_for(&policy(0x01), b"anything")
                .expect("parses"),
            Some(vec![3])
        );
    }

    #[test]
    fn a_v2_shard_round_trips_and_prunes_by_name() {
        let bytes = v2_shard();
        let view = DirectoryView::parse(&bytes).expect("parses");
        assert_eq!(view.header.version, DIRECTORY_V2);

        let candidates = view
            .candidates(&policy(0x02))
            .expect("parses")
            .expect("entry");
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].chunk, 2);
        assert_eq!(candidates[0].filter, filtered(&[b"beta", b"b1", b"b2"]));
        assert_eq!(candidates[1].chunk, 7);
        assert_eq!(candidates[1].filter, filtered(&[b"alpha", b"a1", b"a2"]));
        assert_eq!(candidates[2].chunk, 9);
        assert!(candidates[2].filter.is_empty());

        // The unfiltered candidate always passes; the filtered ones answer
        // only for their own name. The queries are fixed, so the exact sets
        // are pinned — a false positive of a small filter would show up here
        // as an extra chunk, which is precisely the (harmless) behaviour the
        // format accepts.
        assert_eq!(
            view.candidates_for(&policy(0x02), b"alpha")
                .expect("parses"),
            Some(vec![7, 9])
        );
        assert_eq!(
            view.candidates_for(&policy(0x02), b"beta").expect("parses"),
            Some(vec![2, 9])
        );
        assert_eq!(
            view.candidates_for(&policy(0x02), b"gamma")
                .expect("parses"),
            Some(vec![9])
        );

        // The chunk list view is unchanged by the filters.
        assert_eq!(
            view.chunks_for(&policy(0x02)).expect("parses"),
            Some(vec![2, 7, 9])
        );
    }

    #[test]
    fn a_v1_shard_refuses_a_filter() {
        let shard = DirectoryShard {
            version: DIRECTORY_V1,
            network: crate::NETWORK_MAINNET,
            shard: 0x02,
            directory_through_chunk: 900,
            entries: vec![DirectoryEntry {
                policy_id: policy(0x02),
                candidates: vec![DirectoryCandidate {
                    chunk: 7,
                    filter: filtered(&[b"alpha"]),
                }],
            }],
        };
        assert_eq!(shard.encode(), Err(EncodeError::FilterInV1));
    }

    #[test]
    fn an_unencodable_version_is_refused() {
        let shard = DirectoryShard {
            version: 3,
            network: crate::NETWORK_MAINNET,
            shard: 0x02,
            directory_through_chunk: 900,
            entries: vec![],
        };
        assert_eq!(shard.encode(), Err(EncodeError::UnsupportedVersion(3)));
    }

    /// The scan may stop at the first entry greater than the probe — that is
    /// what makes a whole-shard read cheap. Proven by probing a policy that
    /// sorts before a malformed tail the scan must never reach.
    #[test]
    fn the_scan_stops_early() {
        let mut bytes = shard();
        bytes.extend_from_slice(&[0xFF; 8]); // junk that would fail if parsed
        let view = DirectoryView::parse(&bytes).expect("parses");
        assert_eq!(view.chunks_for(&policy(0x00)).expect("parses"), None);
    }

    #[test]
    fn bent_shards_are_refused() {
        let bytes = shard();
        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert!(matches!(
            DirectoryView::parse(&bad_magic),
            Err(DecodeError::BadMagic { .. })
        ));
        let mut bad_version = bytes.clone();
        bad_version[4] = 9;
        assert!(matches!(
            DirectoryView::parse(&bad_version),
            Err(DecodeError::UnsupportedVersion(9))
        ));
    }

    #[test]
    fn a_v1_shard_encodes_to_the_committed_bytes() {
        assert_eq!(
            hex(&shard()),
            "414d584401000200020000008403000000000000000000000000000000000000010101010101010101010101010101010101010101010101010101010100030002020202020202020202020202020202020202020202020202020202020002000700",
        );
    }

    #[test]
    fn a_v2_shard_encodes_to_the_committed_bytes() {
        assert_eq!(
            hex(&v2_shard()),
            "414d584402000200010000008403000000000000000000000000000000000000020202020202020202020202020202020202020202020202020202020300020008000db00ad044ab64120700080020100f24eb93045809000000",
        );
    }
}
