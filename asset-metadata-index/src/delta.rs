//! The delta derivation: fold the chunks that sealed since a cutoff into a
//! published v2 directory, without re-reading the corpus.
//!
//! The full derivation (`derive_directory`) walks every segment. A daily
//! sweep walks only the segments newer than the directory's own coverage —
//! each new segment names the `(policy, chunk)` pairs it contributes, and for
//! those pairs the filter is built the same way the full derivation builds
//! it. Everything else in a shard is preserved byte for byte, so a shard's
//! content changes only where the chain moved it; only the header's coverage
//! claim (and, for touched shards, the merged entries) moves otherwise.
//!
//! [`patch_shard`] is the merge for one shard: decode, insert the new
//! candidates, bump the header's coverage, re-encode. The merge is
//! **idempotent** — a chunk already listed with the same filter bytes is
//! skipped — which is what makes a re-run after a partial sweep a no-op
//! rather than corruption, and what lets the sweep advance coverage
//! shard-by-shard without a transaction.

use std::collections::BTreeMap;
use std::fmt;

use crate::directory::{DirectoryShard, DirectoryView};
use crate::error::{DecodeError, EncodeError};
use crate::{DIRECTORY_V2, DirectoryCandidate, DirectoryEntry, POLICY_ID_LEN};

/// A patched shard.
#[derive(Debug)]
pub struct PatchedShard {
    /// The re-encoded shard bytes.
    pub bytes: Vec<u8>,
    /// Net candidates added — extended entries and new entries alike. A
    /// candidate skipped as a duplicate counts zero.
    pub added: usize,
}

/// Why a shard patch was refused.
#[derive(Debug)]
pub enum DeltaError {
    /// The shard's bytes do not decode.
    Decode(DecodeError),
    /// The merged shard does not encode.
    Encode(EncodeError),
    /// The shard is not v2; the delta derivation patches v2 only.
    NotV2 {
        /// The version byte the shard carries.
        version: u8,
    },
    /// An addition's policy id does not fall into this shard.
    WrongShard {
        /// The shard's ordinal.
        shard: u8,
        /// The offending policy.
        policy_id: [u8; POLICY_ID_LEN],
    },
    /// A chunk is already listed for the policy with different filter bytes
    /// — its segment was rebuilt. Folding that in is a full derivation.
    RebuiltSegment {
        /// The policy.
        policy_id: [u8; POLICY_ID_LEN],
        /// The chunk.
        chunk: u16,
    },
    /// The requested coverage is behind the shard's own claim — a reader's
    /// frontier must never move backwards.
    CoverageRegressed {
        /// What the shard claims.
        existing: u16,
        /// What the caller asked for.
        asked: u16,
    },
}

impl fmt::Display for DeltaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(cause) => write!(f, "decoding the shard: {cause}"),
            Self::Encode(cause) => write!(f, "encoding the merged shard: {cause}"),
            Self::NotV2 { version } => {
                write!(
                    f,
                    "the shard is v{version} — the delta derivation patches v2 only"
                )
            }
            Self::WrongShard { shard, policy_id } => write!(
                f,
                "policy {} does not fall into shard {shard:02x}",
                hex(policy_id)
            ),
            Self::RebuiltSegment { policy_id, chunk } => write!(
                f,
                "chunk {chunk} is already listed for policy {} with different filter bytes — its segment was rebuilt; run the full derivation",
                hex(policy_id)
            ),
            Self::CoverageRegressed { existing, asked } => write!(
                f,
                "coverage {asked} is behind the shard's own claim of {existing} — never move a reader's frontier backwards"
            ),
        }
    }
}

impl std::error::Error for DeltaError {}

impl From<DecodeError> for DeltaError {
    fn from(cause: DecodeError) -> Self {
        Self::Decode(cause)
    }
}

impl From<EncodeError> for DeltaError {
    fn from(cause: EncodeError) -> Self {
        Self::Encode(cause)
    }
}

/// Merge `additions` into the v2 shard `bytes`, setting the header's
/// `directory_through_chunk` to `through`.
///
/// `additions` may hold a policy more than once and candidates in any order;
/// the merge normalises both. A candidate whose chunk is already listed is
/// skipped when its filter matches and refused when it does not — the one
/// case the delta cannot absorb is a rebuilt segment.
///
/// The result equals what a full derivation over the same data encodes; the
/// tests pin that equality, because it is the whole correctness argument for
/// the sweep.
pub fn patch_shard(
    bytes: &[u8],
    additions: &[DirectoryEntry],
    through: u16,
) -> Result<PatchedShard, DeltaError> {
    let view = DirectoryView::parse(bytes)?;
    let header = view.header;
    if header.version != DIRECTORY_V2 {
        return Err(DeltaError::NotV2 {
            version: header.version,
        });
    }
    if through < header.directory_through_chunk {
        return Err(DeltaError::CoverageRegressed {
            existing: header.directory_through_chunk,
            asked: through,
        });
    }

    let mut by_policy: BTreeMap<[u8; POLICY_ID_LEN], Vec<DirectoryCandidate>> = BTreeMap::new();
    for entry in additions {
        if entry.policy_id[0] != header.shard {
            return Err(DeltaError::WrongShard {
                shard: header.shard,
                policy_id: entry.policy_id,
            });
        }
        by_policy
            .entry(entry.policy_id)
            .or_default()
            .extend(entry.candidates.iter().cloned());
    }

    let mut added = 0usize;
    let mut entries = view.entries()?;
    for entry in &mut entries {
        if let Some(candidates) = by_policy.remove(&entry.policy_id) {
            added += merge_candidates(entry, &candidates)?;
        }
    }
    for (policy_id, candidates) in by_policy {
        let mut entry = DirectoryEntry {
            policy_id,
            candidates: Vec::new(),
        };
        added += merge_candidates(&mut entry, &candidates)?;
        entries.push(entry);
    }

    let merged = DirectoryShard {
        version: header.version,
        network: header.network,
        shard: header.shard,
        directory_through_chunk: through,
        entries,
    }
    .encode()?;

    Ok(PatchedShard {
        bytes: merged,
        added,
    })
}

/// Insert `candidates` into `entry`, keeping the chunk order and applying the
/// duplicate rule; returns how many were added.
fn merge_candidates(
    entry: &mut DirectoryEntry,
    candidates: &[DirectoryCandidate],
) -> Result<usize, DeltaError> {
    let mut added = 0usize;
    for candidate in candidates {
        match entry
            .candidates
            .binary_search_by_key(&candidate.chunk, |existing| existing.chunk)
        {
            Ok(at) => {
                if entry.candidates[at].filter != candidate.filter {
                    return Err(DeltaError::RebuiltSegment {
                        policy_id: entry.policy_id,
                        chunk: candidate.chunk,
                    });
                }
            }
            Err(at) => {
                entry.candidates.insert(at, candidate.clone());
                added += 1;
            }
        }
    }
    Ok(added)
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Event, Family, Provenance, Record};
    use crate::segment::SegmentBuilder;
    use crate::{FilterBuilder, NETWORK_MAINNET, SegmentView, records};

    fn policy(byte: u8, ordinal: u8) -> [u8; POLICY_ID_LEN] {
        let mut id = [byte; POLICY_ID_LEN];
        id[1] = ordinal;
        id
    }

    fn record(policy_id: [u8; POLICY_ID_LEN], chunk: u16, offset: u32, name: &[u8]) -> Record {
        Record {
            family: Family::Cip25,
            event: Event::Mint,
            qty: 1,
            policy_id,
            provenance: Provenance {
                chunk,
                body_offset: offset,
                body_len: 10,
                aux_offset: 0,
                aux_len: 0,
            },
            name: name.to_vec(),
            image: b"ipfs://i".to_vec(),
            media_type: b"image/png".to_vec(),
            media: Vec::new(),
            payload: b"{}".to_vec(),
        }
    }

    fn segment(chunk: u16, entries: Vec<([u8; POLICY_ID_LEN], Vec<Record>)>) -> Vec<u8> {
        SegmentBuilder {
            network: NETWORK_MAINNET,
            chunk,
            entries,
        }
        .encode()
        .expect("encodes")
    }

    fn filter(names: &[&[u8]]) -> Vec<u8> {
        let mut builder = FilterBuilder::new(names.len());
        for name in names {
            builder.insert(name);
        }
        builder.finish()
    }

    fn shard_with(chunk: u16, filter: Vec<u8>) -> Vec<u8> {
        DirectoryShard {
            version: DIRECTORY_V2,
            network: NETWORK_MAINNET,
            shard: 0x02,
            directory_through_chunk: 100,
            entries: vec![DirectoryEntry {
                policy_id: policy(0x02, 0x01),
                candidates: vec![DirectoryCandidate { chunk, filter }],
            }],
        }
        .encode()
        .expect("encodes")
    }

    /// The full derivation, test-local — every `(policy, chunk)` the segments
    /// name, one filter per pair over that pair's record names, headers at the
    /// newest chunk. The oracle the delta is compared against.
    fn full_derive(segments: &[(u16, Vec<u8>)]) -> BTreeMap<u8, Vec<u8>> {
        let mut tree: BTreeMap<[u8; POLICY_ID_LEN], BTreeMap<u16, Vec<Vec<u8>>>> = BTreeMap::new();
        let mut frontier = 0u16;
        for (chunk, bytes) in segments {
            frontier = frontier.max(*chunk);
            let view = SegmentView::parse(bytes).expect("parses");
            for entry in view.entries().expect("entries") {
                let slice = view
                    .slice(&entry.policy_id)
                    .expect("parses")
                    .expect("slice");
                let names = tree
                    .entry(entry.policy_id)
                    .or_default()
                    .entry(*chunk)
                    .or_default();
                for record in records(slice) {
                    names.push(record.expect("record").name);
                }
            }
        }

        let mut shards = BTreeMap::new();
        for shard in 0u8..=255 {
            let entries: Vec<DirectoryEntry> = tree
                .iter()
                .filter(|(policy_id, _)| policy_id[0] == shard)
                .map(|(policy_id, chunks)| DirectoryEntry {
                    policy_id: *policy_id,
                    candidates: chunks
                        .iter()
                        .map(|(chunk, names)| {
                            let mut builder = FilterBuilder::new(names.len());
                            for name in names {
                                builder.insert(name);
                            }
                            DirectoryCandidate {
                                chunk: *chunk,
                                filter: builder.finish(),
                            }
                        })
                        .collect(),
                })
                .collect();
            if entries.is_empty() {
                continue;
            }
            let bytes = DirectoryShard {
                version: DIRECTORY_V2,
                network: NETWORK_MAINNET,
                shard,
                directory_through_chunk: frontier,
                entries,
            }
            .encode()
            .expect("encodes");
            shards.insert(shard, bytes);
        }
        shards
    }

    /// The delta side of the same collection: additions grouped by shard.
    fn additions_by_shard(segments: &[(u16, Vec<u8>)]) -> BTreeMap<u8, Vec<DirectoryEntry>> {
        let mut tree: BTreeMap<[u8; POLICY_ID_LEN], BTreeMap<u16, Vec<Vec<u8>>>> = BTreeMap::new();
        for (chunk, bytes) in segments {
            let view = SegmentView::parse(bytes).expect("parses");
            for entry in view.entries().expect("entries") {
                let slice = view
                    .slice(&entry.policy_id)
                    .expect("parses")
                    .expect("slice");
                let names = tree
                    .entry(entry.policy_id)
                    .or_default()
                    .entry(*chunk)
                    .or_default();
                for record in records(slice) {
                    names.push(record.expect("record").name);
                }
            }
        }
        let mut out: BTreeMap<u8, Vec<DirectoryEntry>> = BTreeMap::new();
        for (policy_id, chunks) in tree {
            let entry = DirectoryEntry {
                policy_id,
                candidates: chunks
                    .iter()
                    .map(|(chunk, names)| {
                        let mut builder = FilterBuilder::new(names.len());
                        for name in names {
                            builder.insert(name);
                        }
                        DirectoryCandidate {
                            chunk: *chunk,
                            filter: builder.finish(),
                        }
                    })
                    .collect(),
            };
            out.entry(policy_id[0]).or_default().push(entry);
        }
        out
    }

    #[test]
    fn a_patch_with_no_additions_round_trips() {
        let bytes = shard_with(7, filter(&[b"alpha", b"a1"]));
        let patched = patch_shard(&bytes, &[], 100).expect("patches");
        assert_eq!(patched.bytes, bytes);
        assert_eq!(patched.added, 0);

        let bumped = patch_shard(&bytes, &[], 105).expect("patches");
        assert_eq!(bumped.bytes[12..14], 105u16.to_le_bytes());
        assert_eq!(bumped.bytes[..12], bytes[..12]);
        assert_eq!(bumped.bytes[14..], bytes[14..]);
    }

    #[test]
    fn the_same_chunk_with_equal_bytes_is_skipped() {
        let bytes = shard_with(7, filter(&[b"alpha", b"a1"]));
        let additions = vec![DirectoryEntry {
            policy_id: policy(0x02, 0x01),
            candidates: vec![DirectoryCandidate {
                chunk: 7,
                filter: filter(&[b"alpha", b"a1"]),
            }],
        }];
        let patched = patch_shard(&bytes, &additions, 100).expect("patches");
        assert_eq!(patched.added, 0);
        assert_eq!(patched.bytes, bytes);
    }

    #[test]
    fn the_same_chunk_with_different_bytes_is_refused() {
        let bytes = shard_with(7, filter(&[b"alpha", b"a1"]));
        let additions = vec![DirectoryEntry {
            policy_id: policy(0x02, 0x01),
            candidates: vec![DirectoryCandidate {
                chunk: 7,
                filter: filter(&[b"other"]),
            }],
        }];
        assert!(matches!(
            patch_shard(&bytes, &additions, 100),
            Err(DeltaError::RebuiltSegment { chunk: 7, .. })
        ));
    }

    #[test]
    fn a_policy_from_another_shard_is_refused() {
        let bytes = shard_with(7, filter(&[b"alpha"]));
        let additions = vec![DirectoryEntry {
            policy_id: policy(0x03, 0x01),
            candidates: vec![DirectoryCandidate {
                chunk: 8,
                filter: filter(&[b"beta"]),
            }],
        }];
        assert!(matches!(
            patch_shard(&bytes, &additions, 100),
            Err(DeltaError::WrongShard { shard: 0x02, .. })
        ));
    }

    #[test]
    fn a_v1_shard_is_refused() {
        let bytes = DirectoryShard {
            version: crate::DIRECTORY_V1,
            network: NETWORK_MAINNET,
            shard: 0x02,
            directory_through_chunk: 100,
            entries: vec![DirectoryEntry {
                policy_id: policy(0x02, 0x01),
                candidates: vec![DirectoryCandidate {
                    chunk: 7,
                    filter: Vec::new(),
                }],
            }],
        }
        .encode()
        .expect("encodes");
        assert!(matches!(
            patch_shard(&bytes, &[], 100),
            Err(DeltaError::NotV2 { version: 1 })
        ));
    }

    #[test]
    fn coverage_cannot_regress() {
        let bytes = shard_with(7, filter(&[b"alpha"]));
        assert!(matches!(
            patch_shard(&bytes, &[], 99),
            Err(DeltaError::CoverageRegressed {
                existing: 100,
                asked: 99
            })
        ));
    }

    /// The sweep's whole correctness argument: folding the segments that
    /// sealed after a cutoff into the cutoff's directory — merges, coverage
    /// bumps, a shard created from nothing — produces byte-for-byte what a
    /// full derivation of all of them produces.
    #[test]
    fn a_delta_from_a_cutoff_directory_equals_a_full_derive() {
        let policy_spanning = policy(0x11, 0x02);
        let policy_before = policy(0x11, 0x01);
        let policy_quiet = policy(0x44, 0x01);
        let policy_after = policy(0x33, 0x01);

        let before = vec![
            (
                100u16,
                segment(
                    100,
                    vec![
                        (
                            policy_before,
                            vec![record(policy_before, 100, 0, b"Alpha 1")],
                        ),
                        (
                            policy_spanning,
                            vec![
                                record(policy_spanning, 100, 50, b"Bravo 1"),
                                record(policy_spanning, 100, 90, b"Bravo 1"),
                            ],
                        ),
                        (policy_quiet, vec![record(policy_quiet, 100, 0, b"Delta")]),
                    ],
                ),
            ),
            (
                101u16,
                segment(
                    101,
                    vec![(
                        policy_spanning,
                        vec![record(policy_spanning, 101, 0, b"Bravo 2")],
                    )],
                ),
            ),
        ];
        let after = vec![
            (
                102u16,
                segment(
                    102,
                    vec![
                        (
                            policy_spanning,
                            vec![record(policy_spanning, 102, 0, b"Bravo 1")],
                        ),
                        (policy_after, vec![record(policy_after, 102, 0, b"Charlie")]),
                    ],
                ),
            ),
            (
                103u16,
                segment(
                    103,
                    vec![(
                        policy_before,
                        vec![record(policy_before, 103, 0, b"Alpha 2")],
                    )],
                ),
            ),
        ];

        let mut all = before.clone();
        all.extend(after.clone());
        let cutoff = full_derive(&before);
        let reference = full_derive(&all);
        let adds = additions_by_shard(&after);

        for shard in 0u8..=255 {
            match (cutoff.get(&shard), adds.get(&shard)) {
                (Some(bytes), Some(entries)) => {
                    let patched = patch_shard(bytes, entries, 103).expect("patches");
                    assert_eq!(
                        patched.bytes, reference[&shard],
                        "shard {shard:02x} patched differs from the full derivation"
                    );
                }
                (Some(bytes), None) => {
                    let mut bumped = bytes.clone();
                    bumped[12..14].copy_from_slice(&103u16.to_le_bytes());
                    assert_eq!(
                        bumped, reference[&shard],
                        "shard {shard:02x} coverage bump differs from the full derivation"
                    );
                }
                (None, Some(entries)) => {
                    let bytes = DirectoryShard {
                        version: DIRECTORY_V2,
                        network: NETWORK_MAINNET,
                        shard,
                        directory_through_chunk: 103,
                        entries: entries.clone(),
                    }
                    .encode()
                    .expect("encodes");
                    assert_eq!(
                        bytes, reference[&shard],
                        "shard {shard:02x} created from nothing differs from the full derivation"
                    );
                }
                (None, None) => assert!(!reference.contains_key(&shard)),
            }
        }

        assert!(
            reference.contains_key(&0x11),
            "0x11 holds the before and spanning policies"
        );
        assert!(
            reference.contains_key(&0x33),
            "0x33 is created entirely after the cutoff"
        );
        assert!(
            cutoff.contains_key(&0x44) && !adds.contains_key(&0x44),
            "0x44 exists at the cutoff and gains nothing after — the coverage-bump branch"
        );
    }
}
