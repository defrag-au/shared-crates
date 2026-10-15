//! The read walk: `(policy, name)` → the current record, over one build.
//!
//! Two rounds of reads, each batch issued through a [`RangeSource`]:
//!
//! 1. the pointer and the policy's directory shard (whole objects — the two
//!    keys the worker mirrors into KV);
//! 2. every candidate chunk's segment, as one cap-probe read each — a short
//!    read is the whole object; a read still at the cap falls back to the
//!    header → index → slice chain.
//!
//! Candidates are the chunks the shard admits for `name` **plus** the
//! frontier — chunks newer than the shard's own coverage, bounded by the
//! pointer's. A v1 shard admits every listed chunk; a v2 shard prunes to the
//! ones whose name filter could hold the asset
//! ([`DirectoryView::candidates_for`] — Bloom filters have no false
//! negatives, so pruning never hides an answer). A stale shard can cost
//! reads; it can never produce a wrong answer.
//!
//! The walk owns no transport: a native harness implements [`RangeSource`]
//! over files, the edge worker over KV and R2. The bytes asked for, and the
//! order they are asked in, are the whole contract.

use std::fmt;

use crate::error::DecodeError;
use crate::filter::{FilterVerdict, verdict};
use crate::record::{Record, ResolvedAsset, records, resolve};
use crate::segment::{
    INDEX_ENTRY_LEN, SEGMENT_HEADER_LEN, SegmentHeader, SegmentView, find_index_entry,
};
use crate::{DirectoryView, POLICY_ID_LEN, Pointer};

/// One read's outcome, keeping its object key for error context.
type Fetched<E> = (String, Result<Vec<u8>, E>);

/// Round 1's two reads — the pointer and the policy's directory shard — as an
/// array once the fetch has checked its result count.
type FirstReads<E> = [Fetched<E>; 2];

/// What `try_into` hands back on the impossible short side, named so the
/// conversion's type stays readable.
type ShortBatch<E> = Vec<Fetched<E>>;

/// The artifact's default prefix under the bucket. A `v1` tree carries no
/// record quantities and no listing supplies; the reader walks the `v2` build.
pub const DEFAULT_PREFIX: &str = "index/asset-meta/v2";

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

/// The key of a policy's collection listing — every standing asset of the
/// policy, name-sorted, for enumeration.
#[must_use]
pub fn listing_key(prefix: &str, policy_id: &[u8; POLICY_ID_LEN]) -> String {
    under(prefix, &format!("collections/{}.bin", hex_lower(policy_id)))
}

/// Lowercase hex, for the listing's key. The crate takes no dependencies;
/// five lines of it beat a crate that exists to do this.
fn hex_lower(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
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

/// What a walk did — the diagnostic twin of its answer.
///
/// Returned alongside the answer by [`resolve_asset_reported`], and filled by
/// the same code [`resolve_asset`] runs, so a report cannot disagree with the
/// walk it describes about which chunks were read, in what order, and what
/// the directory's filters said. `/diagnose` on the edge is this type's
/// consumer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalkReport {
    /// Every candidate the directory lists for the policy, in stored (chunk)
    /// order, with the requested name's filter verdict.
    pub candidates: Vec<(u16, FilterVerdict)>,
    /// Frontier chunks — newer than the directory's own coverage — added to
    /// the read set unconditionally.
    pub frontier: Vec<u16>,
    /// The chunks the walk read, ascending (admitted candidates ∪ frontier).
    pub read_chunks: Vec<u16>,
    /// Matching records found, per chunk read, in read order.
    pub matches: Vec<(u16, u32)>,
}

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
    resolve_asset_reported(source, prefix, policy_id, name)
        .await
        .0
}

/// [`resolve_asset`], with the walk's reasoning attached — the debugging
/// surface. The answer is the same code's; the report states the candidate
/// list and its filter verdicts, the frontier, the chunks read and the
/// records each produced, so "not in the index" can be told apart from "read
/// and empty" and from "every filter rejected the name".
pub async fn resolve_asset_reported<S: RangeSource>(
    source: &S,
    prefix: &str,
    policy_id: &[u8; POLICY_ID_LEN],
    name: &[u8],
) -> (
    Result<Option<ResolvedAsset>, WalkError<S::Error>>,
    WalkReport,
) {
    let mut report = WalkReport::default();
    // Round 1 — the pointer and the shard, whole objects.
    let first_reads = vec![
        Read::whole(pointer_key(prefix)),
        Read::whole(directory_key(prefix, policy_id[0])),
    ];
    let first = match fetch_checked(source, &first_reads).await {
        Ok(first) => first,
        Err(error) => return (Err(error), report),
    };
    let pairs: Vec<Fetched<S::Error>> = first_reads
        .into_iter()
        .zip(first)
        .map(|(read, result)| (read.key, result))
        .collect();
    let pair: Result<FirstReads<S::Error>, ShortBatch<S::Error>> = pairs.try_into();
    let [(pointer_key, pointer_result), (shard_key, shard_result)] = match pair {
        Ok(pair) => pair,
        Err(batch) => {
            return (
                Err(WalkError::ResultCount {
                    expected: 2,
                    got: batch.len(),
                }),
                report,
            );
        }
    };
    let pointer_bytes = match pointer_result {
        Ok(bytes) => bytes,
        Err(cause) => {
            return (
                Err(WalkError::Read {
                    key: pointer_key.clone(),
                    cause,
                }),
                report,
            );
        }
    };
    let shard_bytes = match shard_result {
        Ok(bytes) => bytes,
        Err(cause) => {
            return (
                Err(WalkError::Read {
                    key: shard_key.clone(),
                    cause,
                }),
                report,
            );
        }
    };
    let pointer = match Pointer::parse(&pointer_bytes) {
        Ok(pointer) => pointer,
        Err(cause) => {
            return (
                Err(WalkError::Decode {
                    key: pointer_key,
                    cause,
                }),
                report,
            );
        }
    };
    let directory = match DirectoryView::parse(&shard_bytes) {
        Ok(directory) => directory,
        Err(cause) => {
            return (
                Err(WalkError::Decode {
                    key: shard_key.clone(),
                    cause,
                }),
                report,
            );
        }
    };

    // Candidates: what the shard lists, plus the frontier newer than the older
    // of the two "complete through" claims. Reading a listed chunk twice costs
    // a read; skipping one that a stale shard failed to list would cost the
    // answer.
    let listed = match directory.candidates(policy_id) {
        Ok(listed) => listed,
        Err(cause) => {
            return (
                Err(WalkError::Decode {
                    key: shard_key,
                    cause,
                }),
                report,
            );
        }
    };
    let mut candidates: Vec<u16> = Vec::new();
    if let Some(listed) = listed {
        for candidate in &listed {
            let verdict = verdict(&candidate.filter, name);
            report.candidates.push((candidate.chunk, verdict));
            if verdict != FilterVerdict::Rejected {
                candidates.push(candidate.chunk);
            }
        }
    }
    let frontier_from = directory
        .header
        .directory_through_chunk
        .min(pointer.directory_through_chunk);
    report.frontier = (frontier_from.saturating_add(1)..=pointer.built_through_chunk).collect();
    candidates.extend(report.frontier.iter().copied());
    candidates.sort_unstable();
    candidates.dedup();
    report.read_chunks = candidates.clone();

    if candidates.is_empty() {
        return (Ok(None), report);
    }

    let found = match read_segments(source, &candidates, prefix, policy_id, name, &mut report).await
    {
        Ok(found) => found,
        Err(error) => return (Err(error), report),
    };

    (
        Ok(resolve(found.iter(), pointer.built_through_chunk)),
        report,
    )
}

/// The largest segment fetched whole. MEASURED: the full build's segments
/// reach 59.4 MiB (mean 1.2 MiB) — fetched whole, one fat segment is a large
/// fraction of a Worker's 128 MiB of Wasm memory. Segments above this use
/// the header → index → slice chain instead.
pub const WHOLE_SEGMENT_CAP: u64 = 8 << 20;

/// Fetch each candidate segment (round 2) and collect the records the policy
/// holds in it: whole when it fits [`WHOLE_SEGMENT_CAP`], as ranges when it
/// does not. Every chunk read appends its match count to the report.
async fn read_segments<S: RangeSource>(
    source: &S,
    chunks: &[u16],
    prefix: &str,
    policy_id: &[u8; POLICY_ID_LEN],
    name: &[u8],
    report: &mut WalkReport,
) -> Result<Vec<Record>, WalkError<S::Error>> {
    let keys: Vec<String> = chunks
        .iter()
        .map(|chunk| segment_key(prefix, *chunk))
        .collect();
    // One byte past the cap distinguishes "the whole object" from "needs
    // ranges" without a second round trip: a short read means it ended.
    let reads: Vec<Read> = keys
        .iter()
        .map(|key| Read::range(key.clone(), 0, WHOLE_SEGMENT_CAP + 1))
        .collect();
    let results = fetch_checked(source, &reads).await?;
    let mut found: Vec<Record> = Vec::new();
    for ((chunk, key), result) in chunks.iter().zip(&keys).zip(results) {
        let bytes = result.map_err(|cause| WalkError::Read {
            key: key.clone(),
            cause,
        })?;
        let mut matched = 0u32;
        if bytes.len() as u64 > WHOLE_SEGMENT_CAP {
            read_ranges(source, key, policy_id, name, &mut found, &mut matched).await?;
            report.matches.push((*chunk, matched));
            continue;
        }
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
            report.matches.push((*chunk, 0));
            continue;
        };
        for record in region {
            let record = record.map_err(|cause| WalkError::Decode {
                key: key.clone(),
                cause,
            })?;
            if record.name.as_slice() == name {
                found.push(record);
                matched += 1;
            }
        }
        report.matches.push((*chunk, matched));
    }
    Ok(found)
}

/// One candidate over [`WHOLE_SEGMENT_CAP`] — the header → index → slice
/// chain, three dependent reads, the shape gate 3 measured before the whole
/// read replaced it.
async fn read_ranges<S: RangeSource>(
    source: &S,
    key: &str,
    policy_id: &[u8; POLICY_ID_LEN],
    name: &[u8],
    found: &mut Vec<Record>,
    matched: &mut u32,
) -> Result<(), WalkError<S::Error>> {
    let header_bytes = oneshot(source, key, 0, SEGMENT_HEADER_LEN as u64).await?;
    let header = SegmentHeader::parse(&header_bytes).map_err(|cause| WalkError::Decode {
        key: key.to_string(),
        cause,
    })?;

    let index = oneshot(
        source,
        key,
        u64::from(header.index_off),
        header.index_count as u64 * INDEX_ENTRY_LEN as u64,
    )
    .await?;
    let want = header.index_count as usize * INDEX_ENTRY_LEN;
    if index.len() < want {
        return Err(WalkError::Decode {
            key: key.to_string(),
            cause: DecodeError::Truncated {
                at: index.len(),
                want,
            },
        });
    }
    let Some(entry) = find_index_entry(&index, header.index_count, policy_id).map_err(|cause| {
        WalkError::Decode {
            key: key.to_string(),
            cause,
        }
    })?
    else {
        return Ok(());
    };

    let slice = oneshot(
        source,
        key,
        u64::from(entry.records_off),
        u64::from(entry.records_len),
    )
    .await?;
    let want = entry.records_len as usize;
    if slice.len() < want {
        return Err(WalkError::Decode {
            key: key.to_string(),
            cause: DecodeError::Truncated {
                at: slice.len(),
                want,
            },
        });
    }
    for record in records(&slice[..want]) {
        let record = record.map_err(|cause| WalkError::Decode {
            key: key.to_string(),
            cause,
        })?;
        if record.name.as_slice() == name {
            found.push(record);
            *matched += 1;
        }
    }
    Ok(())
}

/// One range read, checked.
async fn oneshot<S: RangeSource>(
    source: &S,
    key: &str,
    offset: u64,
    len: u64,
) -> Result<Vec<u8>, WalkError<S::Error>> {
    let reads = vec![Read::range(key.to_string(), offset, len)];
    let mut results = fetch_checked(source, &reads).await?;
    results
        .pop()
        .ok_or(WalkError::ResultCount {
            expected: 1,
            got: 0,
        })?
        .map_err(|cause| WalkError::Read {
            key: key.to_string(),
            cause,
        })
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
    use crate::{
        DIRECTORY_V1, DIRECTORY_V2, DirectoryCandidate, DirectoryEntry, DirectoryShard,
        FilterBuilder, NETWORK_MAINNET,
    };
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
        // The quantity the event moves, as the chain states it: a mint adds a
        // copy, a burn removes one, an update moves none.
        let qty = match event {
            Event::Mint => 1,
            Event::Burn => -1,
            Event::Update => 0,
        };
        Record {
            family: Family::Cip25,
            event,
            qty,
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

    fn candidate(chunk: u16) -> DirectoryCandidate {
        DirectoryCandidate {
            chunk,
            filter: Vec::new(),
        }
    }

    fn filtered(chunk: u16, names: &[&[u8]]) -> DirectoryCandidate {
        let mut builder = FilterBuilder::new(names.len());
        for name in names {
            builder.insert(name);
        }
        DirectoryCandidate {
            chunk,
            filter: builder.finish(),
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
            self.put_shard_at(DIRECTORY_V1, shard, through, entries);
        }

        fn put_shard_v2(&mut self, shard: u8, through: u16, entries: Vec<DirectoryEntry>) {
            self.put_shard_at(DIRECTORY_V2, shard, through, entries);
        }

        fn put_shard_at(
            &mut self,
            version: u8,
            shard: u8,
            through: u16,
            entries: Vec<DirectoryEntry>,
        ) {
            let bytes = DirectoryShard {
                version,
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
                                // A real object store answers an over-long
                                // range with what it has; the walk relies on
                                // the short read to mean "the object ended".
                                Some(len) => start.saturating_add(len as usize).min(bytes.len()),
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
                candidates: vec![candidate(100)],
            }],
        );
        source.put_pointer(100, 100);
        source
    }

    #[test]
    fn keys_follow_the_layout() {
        assert_eq!(pointer_key(""), "current");
        assert_eq!(pointer_key(DEFAULT_PREFIX), "index/asset-meta/v2/current");
        assert_eq!(
            directory_key(DEFAULT_PREFIX, 0x0a),
            "index/asset-meta/v2/directory/0a.bin"
        );
        assert_eq!(
            segment_key(DEFAULT_PREFIX, 5242),
            "index/asset-meta/v2/segments/05242.bin"
        );
        assert_eq!(
            listing_key(DEFAULT_PREFIX, &[0x93; POLICY_ID_LEN]),
            concat!(
                "index/asset-meta/v2/collections/",
                "93939393",
                "93939393",
                "93939393",
                "93939393",
                "93939393",
                "93939393",
                "93939393",
                ".bin"
            )
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

    /// One copy minted, one burned: the supply is zero and the asset is gone.
    #[test]
    fn the_last_copy_burned_clears_the_asset() {
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
                candidates: vec![candidate(100), candidate(105)],
            }],
        );
        source.put_pointer(105, 105);

        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");
        assert!(asset.burned);
        assert_eq!(asset.supply, 0);
        assert_eq!(asset.event, Event::Burn);
        assert_eq!(asset.provenance.chunk, 105);
    }

    /// A burn with no minted copy behind it — the mint predates the build's
    /// coverage — floors at zero instead of going negative, so a re-mint after
    /// it still stands.
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
                candidates: vec![candidate(105), candidate(110)],
            }],
        );
        source.put_pointer(110, 110);

        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");
        assert!(!asset.burned);
        assert_eq!(asset.supply, 1);
        assert_eq!(asset.provenance.chunk, 110);
        assert_eq!(asset.provenance.body_offset, 3);
    }

    /// The CIP-25 dynamic-update trick, from the walk's side: the replacement
    /// mint lands first, the burn of the previous copy second, and the asset
    /// still stands at one copy with the replacement's metadata.
    #[test]
    fn a_replacement_then_a_burn_of_the_previous_copy_stays_active() {
        let mut source = MapSource::new();
        source.put_segment(
            108,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Mint, 108, 4)])],
        );
        source.put_segment(
            110,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Mint, 110, 3)])],
        );
        source.put_segment(
            112,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Burn, 112, 1)])],
        );
        source.put_shard(
            0x02,
            112,
            vec![DirectoryEntry {
                policy_id: policy(0x02),
                candidates: vec![candidate(108), candidate(110), candidate(112)],
            }],
        );
        source.put_pointer(112, 112);

        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");
        assert!(!asset.burned);
        assert_eq!(asset.supply, 1);
        assert_eq!(asset.event, Event::Mint);
        assert_eq!(asset.provenance.chunk, 110);
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
                candidates: vec![candidate(100)],
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
                candidates: vec![candidate(90)],
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
                candidates: vec![candidate(100)],
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
        assert_eq!(
            reads[2],
            Read::range(segment_key(PREFIX, 100), 0, WHOLE_SEGMENT_CAP + 1)
        );
    }

    #[test]
    fn a_fat_segment_falls_back_to_ranges() {
        let mut source = one_mint_build();
        // Pad the segment past the whole-read cap: the walk must switch to
        // the header → index → slice chain for it.
        let key = segment_key(PREFIX, 100);
        let mut bytes = source.objects.get(&key).expect("built").clone();
        bytes.resize(WHOLE_SEGMENT_CAP as usize + 2, 0);
        source.objects.insert(key.clone(), bytes);

        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");

        let reads = source.reads();
        assert_eq!(reads[0], Read::whole(pointer_key(PREFIX)));
        assert_eq!(reads[1], Read::whole(directory_key(PREFIX, 0x02)));
        assert_eq!(reads[2], Read::range(key.clone(), 0, WHOLE_SEGMENT_CAP + 1));
        assert_eq!(
            reads[3],
            Read::range(key.clone(), 0, SEGMENT_HEADER_LEN as u64)
        );
        assert_eq!(reads[4].len, Some(INDEX_ENTRY_LEN as u64));
        assert_eq!(reads[5].key, key);
        assert_eq!(asset.image, b"ipfs://img");
        assert_eq!(asset.event, Event::Mint);
    }

    #[test]
    fn a_v2_filter_prunes_the_candidates_that_cannot_hold_the_name() {
        let mut source = MapSource::new();
        source.put_segment(
            100,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Mint, 100, 10)])],
        );
        source.put_shard_v2(
            0x02,
            100,
            vec![DirectoryEntry {
                policy_id: policy(0x02),
                candidates: vec![filtered(100, &[b"A"]), filtered(101, &[b"B", b"C"])],
            }],
        );
        source.put_pointer(100, 100);
        // No segment 101 exists: if the filter failed to prune it, the walk
        // would fail on the missing object instead of resolving.

        let asset = block_on(resolve_asset(&source, PREFIX, &policy(0x02), b"A"))
            .expect("walks")
            .expect("found");
        assert_eq!(asset.provenance.chunk, 100);

        let reads = source.reads();
        assert_eq!(
            reads.len(),
            3,
            "pointer, shard, and only the passing candidate"
        );
        assert_eq!(reads[2].key, segment_key(PREFIX, 100));
    }

    #[test]
    fn a_burn_candidate_a_filter_admits_still_clears() {
        // The burn-inclusion rule, from the walk's side: both chunks carry the
        // name, so both pass the filter (the builder's job) and the merge
        // behaves exactly as v1's.
        let mut source = MapSource::new();
        source.put_segment(
            100,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Mint, 100, 10)])],
        );
        source.put_segment(
            105,
            vec![(policy(0x02), vec![record(0x02, b"A", Event::Burn, 105, 5)])],
        );
        source.put_shard_v2(
            0x02,
            105,
            vec![DirectoryEntry {
                policy_id: policy(0x02),
                candidates: vec![filtered(100, &[b"A"]), filtered(105, &[b"A"])],
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

    /// The report is the walk's own account, so it must agree with what the
    /// walk read: a rejected candidate is not read, an admitted one is, and a
    /// match lands on the chunk that holds it. `/diagnose` renders this.
    #[test]
    fn the_report_states_the_filter_verdicts_and_what_was_read() {
        let mut source = MapSource::new();
        source.put_pointer(5, 5);
        source.put_shard_v2(
            0xaa,
            5,
            vec![DirectoryEntry {
                policy_id: policy(0xaa),
                candidates: vec![filtered(3, &[b"held"]), filtered(4, &[b"other"])],
            }],
        );
        source.put_segment(
            3,
            vec![(policy(0xaa), vec![record(0xaa, b"held", Event::Mint, 3, 0)])],
        );

        let (answer, report) = block_on(resolve_asset_reported(
            &source,
            PREFIX,
            &policy(0xaa),
            b"held",
        ));
        assert!(answer.expect("walks").is_some());
        assert_eq!(
            report.candidates,
            vec![(3, FilterVerdict::Admitted), (4, FilterVerdict::Rejected)]
        );
        assert_eq!(report.read_chunks, vec![3]);
        assert_eq!(report.matches, vec![(3, 1)]);
        assert!(report.frontier.is_empty());

        // A name no filter admits: no reads at all, and the report says so —
        // the distinction a bare `NOT_IN_INDEX` cannot carry.
        let (answer, report) = block_on(resolve_asset_reported(
            &source,
            PREFIX,
            &policy(0xaa),
            b"absent",
        ));
        assert!(answer.expect("walks").is_none());
        assert!(report.read_chunks.is_empty());
        assert!(report.matches.is_empty());
        assert!(
            report
                .candidates
                .iter()
                .all(|(_, verdict)| *verdict == FilterVerdict::Rejected),
            "{report:?}"
        );
    }
}
