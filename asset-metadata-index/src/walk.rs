//! The read walk: `(policy, name)` → the current record, over one build.
//!
//! Four rounds of reads, each batch issued through a [`RangeSource`]:
//!
//! 1. the pointer and the policy's directory shard (whole objects — the two
//!    keys the worker mirrors into KV);
//! 2. segment headers, 64 bytes each, for every candidate chunk;
//! 3. the index region each header names;
//! 4. the record slice of each chunk whose index holds the policy.
//!
//! Candidates are the chunks the shard lists for the policy **plus** the
//! frontier — chunks newer than the shard's own coverage, bounded by the
//! pointer's. A stale shard can cost reads; it can never produce a wrong
//! answer.
//!
//! The walk owns no transport: a native harness implements [`RangeSource`]
//! over files, the edge worker over KV and R2. The bytes asked for, and the
//! order they are asked in, are the whole contract.

use std::fmt;

use crate::error::DecodeError;
use crate::record::{Record, ResolvedAsset, resolve};
use crate::segment::SegmentView;
use crate::{DirectoryView, POLICY_ID_LEN, Pointer};

/// One read's outcome, keeping its object key for error context.
type Fetched<E> = (String, Result<Vec<u8>, E>);

/// The artifact's default prefix under the bucket.
pub const DEFAULT_PREFIX: &str = "index/asset-meta/v1";

/// The pointer's key — the build manifest, read first.
#[must_use]
pub fn pointer_key(prefix: &str) -> String {
    under(prefix, "current")
}

/// The key of the shard covering `shard` (a policy id's first byte).
#[must_use]
pub fn directory_key(prefix: &str, shard: u8) -> String {
    under(prefix, &format!("directory/{shard:02x}.bin"))
}

/// The key of the segment for a sealed chunk.
#[must_use]
pub fn segment_key(prefix: &str, chunk: u16) -> String {
    under(prefix, &format!("segments/{chunk:05}.bin"))
}

fn under(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        path.to_string()
    } else {
        format!("{prefix}/{path}")
    }
}

/// One object read: a whole object, or a byte window of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// The object key, from one of the key builders in this module.
    pub key: String,
    /// First byte wanted.
    pub offset: u64,
    /// Bytes wanted; `None` reads from `offset` to the end of the object.
    pub len: Option<u64>,
}

impl Read {
    /// The whole object.
    #[must_use]
    pub fn whole(key: String) -> Self {
        Self {
            key,
            offset: 0,
            len: None,
        }
    }

    /// The byte window at `offset..offset + len`.
    #[must_use]
    pub fn range(key: String, offset: u64, len: u64) -> Self {
        Self {
            key,
            offset,
            len: Some(len),
        }
    }
}

/// A batched byte source.
///
/// Implementations decide how a batch is fetched — the native harness loops
/// over files; the worker fans out KV and R2 reads and awaits them together —
/// but each read must come back as the object's bytes at exactly the range
/// asked for ([`Read::whole`] reads to the end), and the result vector must
/// hold one entry per read, in order. The walk refuses anything else.
pub trait RangeSource {
    /// The source's failure type.
    type Error: fmt::Debug + fmt::Display;

    /// Fetch every read, in order.
    fn fetch(
        &self,
        reads: &[Read],
    ) -> impl std::future::Future<Output = Vec<Result<Vec<u8>, Self::Error>>>;
}

/// A failed walk. Every variant is a refusal — the walk never guesses.
#[derive(Debug)]
pub enum WalkError<E> {
    /// The source could not produce a read's bytes.
    Read {
        /// The object being read.
        key: String,
        /// The source's own error.
        cause: E,
    },
    /// Fetched bytes are not the object they claim to be.
    Decode {
        /// The object being decoded.
        key: String,
        /// What was wrong with it.
        cause: DecodeError,
    },
    /// The source returned a result count that does not match the request.
    ResultCount {
        /// Reads requested.
        expected: usize,
        /// Results returned.
        got: usize,
    },
}

impl<E: fmt::Display> fmt::Display for WalkError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { key, cause } => write!(f, "reading {key}: {cause}"),
            Self::Decode { key, cause } => write!(f, "decoding {key}: {cause}"),
            Self::ResultCount { expected, got } => {
                write!(f, "the source returned {got} results for {expected} reads")
            }
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for WalkError<E> {}

/// Resolve `(policy_id, name)` against the build at `prefix`.
///
/// Each candidate segment is fetched as **one whole object** — the shape gate
/// 3 settled (2026-10-10): an R2 read costs the round trip, not the bytes, so
/// one whole-segment read beats the header → index → slice chain it replaced
/// (warm p50 274-296 ms against 806-814 ms on the probe's fixture).
///
/// `Ok(None)` is the caller's fall-through signal: the build holds no record
/// for the pair — a fresh mint ahead of the build, or an asset no covered
/// chunk states metadata for. `Ok(Some(asset))` is the answer;
/// [`ResolvedAsset::burned`] says the latest event was a burn.
pub async fn resolve_asset<S: RangeSource>(
    source: &S,
    prefix: &str,
    policy_id: &[u8; POLICY_ID_LEN],
    name: &[u8],
) -> Result<Option<ResolvedAsset>, WalkError<S::Error>> {
    // Round 1 — the pointer and the shard, whole objects.
    let first_reads = vec![
        Read::whole(pointer_key(prefix)),
        Read::whole(directory_key(prefix, policy_id[0])),
    ];
    let first = fetch_checked(source, &first_reads).await?;
    let [(pointer_key, pointer_result), (shard_key, shard_result)] = first_reads
        .into_iter()
        .zip(first)
        .map(|(read, result)| (read.key, result))
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|batch: Vec<Fetched<S::Error>>| WalkError::ResultCount {
            expected: 2,
            got: batch.len(),
        })?;
    let pointer_bytes = pointer_result.map_err(|cause| WalkError::Read {
        key: pointer_key.clone(),
        cause,
    })?;
    let shard_bytes = shard_result.map_err(|cause| WalkError::Read {
        key: shard_key.clone(),
        cause,
    })?;
    let pointer = Pointer::parse(&pointer_bytes).map_err(|cause| WalkError::Decode {
        key: pointer_key,
        cause,
    })?;
    let directory = DirectoryView::parse(&shard_bytes).map_err(|cause| WalkError::Decode {
        key: shard_key.clone(),
        cause,
    })?;

    // Candidates: what the shard lists, plus the frontier newer than the older
    // of the two "complete through" claims. Reading a listed chunk twice costs
    // a read; skipping one that a stale shard failed to list would cost the
    // answer.
    let mut candidates = directory
        .chunks_for(policy_id)
        .map_err(|cause| WalkError::Decode {
            key: shard_key,
            cause,
        })?
        .unwrap_or_default();
    let frontier_from = directory
        .header
        .directory_through_chunk
        .min(pointer.directory_through_chunk);
    candidates.extend(frontier_from.saturating_add(1)..=pointer.built_through_chunk);
    candidates.sort_unstable();
    candidates.dedup();

    if candidates.is_empty() {
        return Ok(None);
    }

    let keys: Vec<String> = candidates
        .iter()
        .map(|chunk| segment_key(prefix, *chunk))
        .collect();
    let found = read_segments(source, &keys, policy_id, name).await?;

    Ok(resolve(found.iter(), pointer.built_through_chunk))
}

/// Fetch each candidate segment whole (round 2) and collect the records the
/// policy holds in it.
async fn read_segments<S: RangeSource>(
    source: &S,
    keys: &[String],
    policy_id: &[u8; POLICY_ID_LEN],
    name: &[u8],
) -> Result<Vec<Record>, WalkError<S::Error>> {
    let reads: Vec<Read> = keys.iter().map(|key| Read::whole(key.clone())).collect();
    let results = fetch_checked(source, &reads).await?;
    let mut found: Vec<Record> = Vec::new();
    for (key, result) in keys.iter().zip(results) {
        let bytes = result.map_err(|cause| WalkError::Read {
            key: key.clone(),
            cause,
        })?;
        let view = SegmentView::parse(&bytes).map_err(|cause| WalkError::Decode {
            key: key.clone(),
            cause,
        })?;
        let Some(region) = view
            .records_for(policy_id)
            .map_err(|cause| WalkError::Decode {
                key: key.clone(),
                cause,
            })?
        else {
            continue;
        };
        for record in region {
            let record = record.map_err(|cause| WalkError::Decode {
                key: key.clone(),
                cause,
            })?;
            if record.name.as_slice() == name {
                found.push(record);
            }
        }
    }
    Ok(found)
}

async fn fetch_checked<S: RangeSource>(
    source: &S,
    reads: &[Read],
) -> Result<Vec<Result<Vec<u8>, S::Error>>, WalkError<S::Error>> {
    let results = source.fetch(reads).await;
    if results.len() != reads.len() {
        return Err(WalkError::ResultCount {
            expected: reads.len(),
            got: results.len(),
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Event, Family, Provenance};
    use crate::segment::{SEGMENT_HEADER_LEN, SegmentBuilder};
    use crate::{DirectoryEntry, DirectoryShard, NETWORK_MAINNET};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::future::Future;

    const PREFIX: &str = "test/v1";

    fn block_on<F: Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Waker};

        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn policy(byte: u8) -> [u8; POLICY_ID_LEN] {
        [byte; POLICY_ID_LEN]
    }

    fn record(policy_byte: u8, name: &[u8], event: Event, chunk: u16, offset: u32) -> Record {
        Record {
            family: Family::Cip25,
            event,
            policy_id: policy(policy_byte),
            provenance: Provenance {
                chunk,
                body_offset: offset,
                body_len: 10,
                aux_offset: 0,
                aux_len: 0,
            },
            name: name.to_vec(),
            image: b"ipfs://img".to_vec(),
            media_type: b"image/png".to_vec(),
            media: Vec::new(),
            payload: b"meta".to_vec(),
        }
    }

    /// An in-memory build, recording every read it serves.
    struct MapSource {
        objects: HashMap<String, Vec<u8>>,
        seen: RefCell<Vec<Read>>,
    }

    impl MapSource {
        fn new() -> Self {
            Self {
                objects: HashMap::new(),
                seen: RefCell::new(Vec::new()),
            }
        }

        fn put_segment(&mut self, chunk: u16, entries: Vec<([u8; POLICY_ID_LEN], Vec<Record>)>) {
            let bytes = SegmentBuilder {
                network: NETWORK_MAINNET,
                chunk,
                entries,
            }
            .encode()
            .expect("encodes");
            self.objects.insert(segment_key(PREFIX, chunk), bytes);
        }

        fn put_shard(&mut self, shard: u8, through: u16, entries: Vec<DirectoryEntry>) {
            let bytes = DirectoryShard {
                network: NETWORK_MAINNET,
                shard,
                directory_through_chunk: through,
                entries,
            }
            .encode()
            .expect("encodes");
            self.objects.insert(directory_key(PREFIX, shard), bytes);
        }

        fn put_pointer(&mut self, built: u16, directory: u16) {
            let text = Pointer {
                network: NETWORK_MAINNET,
                built_through_chunk: built,
                directory_through_chunk: directory,
                record_count: 0,
                published_at: 0,
            }
            .encode();
            self.objects.insert(pointer_key(PREFIX), text.into_bytes());
        }

        fn reads(&self) -> Vec<Read> {
            self.seen.borrow().clone()
        }
    }

    impl RangeSource for MapSource {
        type Error = String;

        async fn fetch(&self, reads: &[Read]) -> Vec<Result<Vec<u8>, String>> {
            reads
                .iter()
                .map(|read| {
                    self.seen.borrow_mut().push(read.clone());
                    match self.objects.get(&read.key) {
                        None => Err(format!("missing {}", read.key)),
                        Some(bytes) => {
                            let start = read.offset as usize;
                            let end = match read.len {
                                None => bytes.len(),
                                Some(len) => start + len as usize,
                            };
                            bytes
                                .get(start..end)
                                .map(<[u8]>::to_vec)
                                .ok_or_else(|| format!("short {}", read.key))
                        }
                    }
                })
                .collect()
        }
    }

    fn one_mint_build() -> MapSource {
        let mut source = MapSource::new();
        source.put_segment(
            100,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Mint, 100, 10)])],
        );
        source.put_shard(
            0x02,
            100,
            vec![DirectoryEntry {
                policy_id: policy(0x02),
                chunks: vec![100],
            }],
        );
        source.put_pointer(100, 100);
        source
    }

    #[test]
    fn keys_follow_the_layout() {
        assert_eq!(pointer_key(""), "current");
        assert_eq!(pointer_key(DEFAULT_PREFIX), "index/asset-meta/v1/current");
        assert_eq!(
            directory_key(DEFAULT_PREFIX, 0x0a),
            "index/asset-meta/v1/directory/0a.bin"
        );
        assert_eq!(
            segment_key(DEFAULT_PREFIX, 5242),
            "index/asset-meta/v1/segments/05242.bin"
        );
    }

    #[test]
    fn a_mint_resolves_with_its_provenance() {
        let source = one_mint_build();
        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");
        assert!(!asset.burned);
        assert_eq!(asset.event, Event::Mint);
        assert_eq!(asset.provenance.chunk, 100);
        assert_eq!(asset.provenance.body_offset, 10);
        assert_eq!(asset.image, b"ipfs://img");
        assert_eq!(asset.as_of_chunk, 100);
    }

    #[test]
    fn the_latest_event_across_chunks_wins_and_a_burn_clears() {
        let mut source = MapSource::new();
        source.put_segment(
            100,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Mint, 100, 10)])],
        );
        source.put_segment(
            105,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Burn, 105, 5)])],
        );
        source.put_shard(
            0x02,
            105,
            vec![DirectoryEntry {
                policy_id: policy(0x02),
                chunks: vec![100, 105],
            }],
        );
        source.put_pointer(105, 105);

        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");
        assert!(asset.burned);
        assert_eq!(asset.event, Event::Burn);
        assert_eq!(asset.provenance.chunk, 105);
    }

    #[test]
    fn a_remint_after_a_burn_is_active() {
        let mut source = MapSource::new();
        source.put_segment(
            105,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Burn, 105, 5)])],
        );
        source.put_segment(
            110,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Mint, 110, 3)])],
        );
        source.put_shard(
            0x02,
            110,
            vec![DirectoryEntry {
                policy_id: policy(0x02),
                chunks: vec![105, 110],
            }],
        );
        source.put_pointer(110, 110);

        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");
        assert!(!asset.burned);
        assert_eq!(asset.provenance.chunk, 110);
        assert_eq!(asset.provenance.body_offset, 3);
    }

    #[test]
    fn within_a_chunk_the_greater_offset_wins() {
        let mut source = MapSource::new();
        source.put_segment(
            100,
            vec![(
                policy(0x02),
                vec![
                    record(0x02, b"A", Event::Mint, 100, 1),
                    record(0x02, b"A", Event::Mint, 100, 9),
                ],
            )],
        );
        source.put_shard(
            0x02,
            100,
            vec![DirectoryEntry {
                policy_id: policy(0x02),
                chunks: vec![100],
            }],
        );
        source.put_pointer(100, 100);

        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");
        assert_eq!(asset.provenance.body_offset, 9);
    }

    #[test]
    fn an_absent_name_is_not_covered() {
        let source = one_mint_build();
        let outcome = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"Z")).expect("walks");
        assert!(outcome.is_none());
        assert_eq!(source.reads().len(), 3, "pointer, shard, whole segment");
    }

    #[test]
    fn an_absent_policy_reads_only_pointer_and_shard() {
        let mut source = one_mint_build();
        source.put_shard(0x07, 100, Vec::new());
        let outcome = block_on(resolve_asset(&source, PREFIX, &policy(0x07), b"A")).expect("walks");
        assert!(outcome.is_none());
        assert_eq!(source.reads().len(), 2, "nothing beyond the first round");
    }

    #[test]
    fn the_frontier_between_shard_and_pointer_is_scanned() {
        let mut source = MapSource::new();
        source.put_segment(
            90,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Mint, 90, 1)])],
        );
        source.put_segment(
            105,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Mint, 105, 7)])],
        );
        // The shard's own coverage stops at 104 and it lists only chunk 90.
        source.put_shard(
            0x02,
            104,
            vec![DirectoryEntry {
                policy_id: policy(0x02),
                chunks: vec![90],
            }],
        );
        source.put_pointer(105, 104);

        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");
        assert_eq!(asset.provenance.chunk, 105);
        assert_eq!(asset.as_of_chunk, 105);
    }

    #[test]
    fn a_missing_listed_segment_fails_loudly() {
        let mut source = MapSource::new();
        source.put_shard(
            0x02,
            100,
            vec![DirectoryEntry {
                policy_id: policy(0x02),
                chunks: vec![100],
            }],
        );
        source.put_pointer(100, 100);
        // No segment 100 exists.

        let error =
            block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A")).expect_err("refuses");
        match error {
            WalkError::Read { key, .. } => assert_eq!(key, segment_key(PREFIX, 100)),
            other => panic!("expected a read failure, got {other:?}"),
        }
    }

    #[test]
    fn a_bent_segment_is_refused() {
        let mut source = one_mint_build();
        source
            .objects
            .insert(segment_key(PREFIX, 100), vec![0u8; SEGMENT_HEADER_LEN]);

        let error =
            block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A")).expect_err("refuses");
        assert!(matches!(error, WalkError::Decode { .. }));
    }

    #[test]
    fn the_read_shape_is_pointer_shard_then_per_chunk() {
        let source = one_mint_build();
        let _ = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A")).expect("walks");

        let reads = source.reads();
        assert_eq!(reads.len(), 3, "pointer, shard, one whole segment");
        assert_eq!(reads[0], Read::whole(pointer_key(PREFIX)));
        assert_eq!(reads[1], Read::whole(directory_key(PREFIX, 0x02)));
        assert_eq!(reads[2], Read::whole(segment_key(PREFIX, 100)));
    }
}
