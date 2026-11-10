//! A published achievements bundle — one file, sorted by account, read without decoding.
//!
//! # Why one file and not one file per stake
//!
//! A per-stake object would make a profile page a single fetch, and make everything else
//! worse: the grid needs per-achievement rarity, the allocation iterates every subject, and
//! a "who earned this" list reads across all of them. At 769 subjects a per-stake layout is
//! 769 objects per rebuild — an object explosion for one query shape. This layout is one
//! object, and a profile is a binary search plus one slice.
//!
//! The whole bundle is under a megabyte, so a client downloads it once and searches in
//! memory; the byte-range protocol is there for a website that would rather fetch one
//! record, and costs nothing for a game that will not.
//!
//! # The layout
//!
//! ```text
//! header      80 bytes, fixed
//! collection  `collection_len` bytes — the CAIP-19 reference this file scores
//! offsets     (`subject_count` + 1) × u32 into `records`
//! rarity      `achievement_count` × (u32 holders_with, u32 population)
//! records     one per subject: u8 key_len, key bytes, earned entries, lots
//! ```
//!
//! One offset table serves both the key index and the records: a record begins with its
//! key, so [`Bundle::find`] binary-searches the offsets and compares key bytes, and
//! [`Bundle::subject_at`] reaches a record the same way. There is no per-format index — a
//! 28-byte Cardano credential and a 20-byte EVM address are both just a key string, and
//! both are found in `log(subject_count)` comparisons rather than a scan.
//!
//! Keys are stored as the text the fold interns (a hex stake credential, or the address
//! itself), so a caller searches with the same string the catalogue and the store use.

use chain_events::{AccountRef, Coverage};
use chains::{ChainRef, TokenRef};

use crate::evaluate::{Assessment, Definition, Outcome, Standing, evaluate};
use crate::state::{
    AcquiredBy, Holding, LotId, MarketHistory, MarketSide, MintFact, MintSummary, NewFill, NewLot,
    Slot, Store, SubjectId, UnitId,
};

/// Identifies the format. The trailing zeros are padding so a truncated read cannot
/// accidentally match.
pub const MAGIC: [u8; 8] = *b"HBABv2\0\0";

/// The layout revision this build writes. A reader refuses a bundle it does not know rather
/// than guessing.
///
/// Version 3 adds one byte to each earned entry, its [`Standing`]; version 4 two more, the pieces
/// held now. An older file still reads: a version 2 entry as [`Standing::Record`], and an entry
/// with no count as one piece when it is held, so it scores and grants as it did when written.
pub const VERSION: u16 = 4;

/// The oldest revision this build still reads.
const VERSION_MIN: u16 = 2;

/// Bytes of one earned entry in a revision: achievement u16, valid_from u64, then from
/// version 3 a standing byte, and from version 4 a u16 of pieces held.
const fn earned_len(version: u16) -> usize {
    match version {
        2 => 10,
        3 => 11,
        _ => 13,
    }
}

/// Bytes before the first section.
const HEADER: usize = 80;

/// Bytes of one encoding of [`Lot`]: unit u32, two i64, four u64, one bool.
const LOT: usize = 53;

/// Where an open interval ends, in the bundle and in [`Holding::disposed_at`]/
/// [`Holding::disposed_unix`] alike.
///
/// A coordinate that large is not reachable on either axis, so it cannot collide with a real
/// disposal — which `0` could, since `0` is a legal coordinate in a synthetic source.
const OPEN: u64 = u64::MAX;

/// Something the bundle could not be read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleError {
    /// Fewer bytes than even a header.
    TooShort,
    /// The magic does not match — this is not a bundle, or the leading bytes were lost.
    BadMagic,
    /// A revision this reader does not know.
    UnsupportedVersion(u16),
    /// The header's CAIP-19 collection reference is not UTF-8, or its CAIP-2 chain prefix is
    /// not a chain this build knows. Either way the file cannot say what it scores.
    BadCollection,
    /// A section runs past the end of the buffer.
    Truncated,
    /// A count or offset inside the file disagrees with its neighbours.
    Inconsistent,
    /// Writing through a published bundle. It is derived; rebuild it instead.
    ReadOnly,
}

impl core::fmt::Display for BundleError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort => formatter.write_str("shorter than a bundle header"),
            Self::BadMagic => formatter.write_str("not an achievements bundle"),
            Self::UnsupportedVersion(version) => write!(formatter, "bundle version {version}"),
            Self::BadCollection => {
                formatter.write_str("collection reference is not a valid CAIP-19 id")
            }
            Self::Truncated => formatter.write_str("a section runs past the end of the buffer"),
            Self::Inconsistent => formatter.write_str("a count and an offset disagree"),
            Self::ReadOnly => formatter.write_str("a published bundle is read-only"),
        }
    }
}

impl std::error::Error for BundleError {}

/// How many subjects hold an achievement, out of how many ever could.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rarity {
    /// Subjects whose snapshot carries this achievement.
    pub holders_with: u32,
    /// Every subject in the bundle — the eligible population, not the current holders.
    ///
    /// **Zero means the reading did not evaluate this achievement.** It is not "nobody
    /// earned it": a builder that could not answer a predicate must leave the population
    /// empty rather than claim a population-wide miss. A client renders that as unmeasured.
    pub population: u32,
}

/// One earned achievement, as the snapshot observed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Earned {
    /// Index into the catalogue the bundle was built against.
    pub achievement: u16,
    /// The chain height it was first true at, or the height of the reading that saw it.
    pub valid_from: Slot,
    /// For a holding, whether it was still held at the reading.
    pub standing: Standing,
    /// For a holding held now, how many pieces satisfy it. Zero otherwise.
    pub held: u16,
}

fn standing_byte(standing: Standing) -> u8 {
    match standing {
        Standing::Record => 0,
        Standing::Now => 1,
        Standing::Past => 2,
        Standing::Listed => 3,
    }
}

fn standing_of(byte: u8) -> Standing {
    match byte {
        1 => Standing::Now,
        2 => Standing::Past,
        3 => Standing::Listed,
        _ => Standing::Record,
    }
}

/// One acquisition, as the snapshot recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lot {
    /// Interned asset id within this collection.
    pub unit: u32,
    /// How many units this acquisition covers.
    pub quantity: i64,
    /// How many are still held.
    pub open: i64,
    /// Chain height the lot arrived at.
    pub acquired_at: Slot,
    /// Unix seconds the lot arrived at — the clock tenure is measured on.
    pub acquired_unix: u64,
    /// Chain height it finished leaving, or `None` while any of it is held.
    pub disposed_at: Option<Slot>,
    /// Unix seconds it finished leaving, beside [`Lot::disposed_at`].
    pub disposed_unix: Option<u64>,
    /// `true` when the lot arrived at the primary mint rather than from another party.
    pub from_mint: bool,
}

// ============================================================================
// Writing
// ============================================================================

/// One subject's row, ready to encode.
#[derive(Debug, Clone, Default)]
pub struct SubjectRecord {
    /// The account key the fold uses: a hex stake credential, or the address itself when
    /// it has none. Must be shorter than 256 bytes, which every address form is.
    pub key: String,
    /// Earned achievements. Sorted by `achievement` before writing.
    pub earned: Vec<Earned>,
    /// Every lot the subject has had, open and closed.
    pub lots: Vec<Lot>,
}

/// Everything a bundle states, before it is bytes.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// Unix seconds the reading was taken.
    pub built_at: u64,
    /// The collection this file scores, as CAIP-19 without a token half.
    ///
    /// A bundle is one collection's projection. Without this a reader can only infer the
    /// collection from the keys, and a file built for one collection can be opened as
    /// though it were another.
    pub collection: String,
    /// Whether the walk that produced this snapshot proved it saw the whole holder family.
    ///
    /// Stated by the builder from its own report, never inferred by the reader.
    pub coverage: Coverage,
    /// The last holder-family slot folded.
    pub holder_cursor: Slot,
    /// The last market-family slot folded.
    pub market_cursor: Slot,
    /// A change detector over the catalogue's definition ids, in order.
    ///
    /// Not a security hash: it exists so a bundle built against one catalogue is not read
    /// as though it were built against another. See [`catalogue_digest`].
    pub catalogue_digest: u64,
    /// How many achievements the catalogue has. Every `Earned::achievement` is below it.
    pub achievement_count: u16,
    /// One entry per achievement, in index order. A zero `population` marks one this
    /// reading did not evaluate — see [`Rarity::population`].
    pub rarity: Vec<Rarity>,
    /// Every subject, in any order — the writer sorts them.
    pub subjects: Vec<SubjectRecord>,
}

/// FNV-1a over `bytes`. A change detector, chosen because it needs no dependency.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The digest of a catalogue: its definition ids, in order, each `\0`-terminated.
///
/// Order matters, because the index is the contract — a catalogue reordered without a
/// change of ids would still be a different bundle.
pub fn catalogue_digest<'a>(ids: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut joined = Vec::new();
    for id in ids {
        joined.extend_from_slice(id.as_bytes());
        joined.push(0);
    }
    fnv1a(&joined)
}

/// Encodes a snapshot. The output is deterministic: the same snapshot always writes the
/// same bytes, which is what makes a sealed bundle reproducible.
pub fn write(snapshot: &Snapshot) -> Vec<u8> {
    write_as(snapshot, VERSION)
}

/// Encodes a snapshot in one revision's layout. Only the current one is published; the tests
/// write the older one to prove it still reads.
fn write_as(snapshot: &Snapshot, version: u16) -> Vec<u8> {
    let mut subjects = snapshot.subjects.clone();
    subjects.sort_by(|a, b| a.key.as_bytes().cmp(b.key.as_bytes()));
    subjects.dedup_by(|a, b| a.key == b.key);

    let mut records = Vec::new();
    let mut offsets: Vec<u32> = Vec::with_capacity(subjects.len() + 1);
    for record in &subjects {
        offsets.push(records.len() as u32);
        encode_record(record, &mut records, version);
    }
    offsets.push(records.len() as u32);

    let collection = snapshot.collection.as_bytes();
    let offsets_len = offsets.len() * 4;
    let rarity_len = snapshot.rarity.len() * 8;

    let mut out =
        Vec::with_capacity(HEADER + collection.len() + offsets_len + rarity_len + records.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&snapshot.achievement_count.to_le_bytes());
    out.extend_from_slice(&(subjects.len() as u32).to_le_bytes());
    out.extend_from_slice(&snapshot.built_at.to_le_bytes());
    out.extend_from_slice(&snapshot.holder_cursor.to_le_bytes());
    out.extend_from_slice(&snapshot.market_cursor.to_le_bytes());
    out.extend_from_slice(&snapshot.catalogue_digest.to_le_bytes());
    out.extend_from_slice(&(collection.len() as u32).to_le_bytes());
    out.push(coverage_byte(snapshot.coverage));
    out.extend_from_slice(&[0u8; HEADER - 53]);
    debug_assert_eq!(out.len(), HEADER);

    out.extend_from_slice(collection);
    for offset in &offsets {
        out.extend_from_slice(&offset.to_le_bytes());
    }
    for rarity in &snapshot.rarity {
        out.extend_from_slice(&rarity.holders_with.to_le_bytes());
        out.extend_from_slice(&rarity.population.to_le_bytes());
    }
    out.extend_from_slice(&records);
    out
}

fn encode_record(record: &SubjectRecord, out: &mut Vec<u8>, version: u16) {
    let key = record.key.as_bytes();
    out.push(u8::try_from(key.len()).expect("an account key is shorter than 256 bytes"));
    out.extend_from_slice(key);

    let mut earned = record.earned.clone();
    earned.sort_by_key(|entry| entry.achievement);
    out.extend_from_slice(&(earned.len() as u16).to_le_bytes());
    for entry in &earned {
        out.extend_from_slice(&entry.achievement.to_le_bytes());
        out.extend_from_slice(&entry.valid_from.to_le_bytes());
        if earned_len(version) > 10 {
            out.push(standing_byte(entry.standing));
        }
        if earned_len(version) > 11 {
            out.extend_from_slice(&entry.held.to_le_bytes());
        }
    }
    out.extend_from_slice(&(record.lots.len() as u16).to_le_bytes());
    for lot in &record.lots {
        out.extend_from_slice(&lot.unit.to_le_bytes());
        out.extend_from_slice(&lot.quantity.to_le_bytes());
        out.extend_from_slice(&lot.open.to_le_bytes());
        out.extend_from_slice(&lot.acquired_at.to_le_bytes());
        out.extend_from_slice(&lot.acquired_unix.to_le_bytes());
        out.extend_from_slice(&lot.disposed_at.unwrap_or(OPEN).to_le_bytes());
        out.extend_from_slice(&lot.disposed_unix.unwrap_or(OPEN).to_le_bytes());
        out.push(u8::from(lot.from_mint));
    }
}

fn coverage_byte(coverage: Coverage) -> u8 {
    match coverage {
        Coverage::Complete => 0,
        Coverage::Partial => 1,
    }
}

// ============================================================================
// Reading
// ============================================================================

/// A bundle over borrowed bytes. No copy, and no allocation to open.
#[derive(Debug, Clone)]
pub struct Bundle<'a> {
    bytes: &'a [u8],
    version: u16,
    achievement_count: u16,
    subject_count: u32,
    built_at: u64,
    collection: &'a str,
    chain: ChainRef,
    coverage: Coverage,
    holder_cursor: Slot,
    market_cursor: Slot,
    catalogue_digest: u64,
    offsets_at: usize,
    rarity_at: usize,
    records_at: usize,
}

impl<'a> Bundle<'a> {
    /// Opens a bundle, validating every section length before anything is read.
    pub fn open(bytes: &'a [u8]) -> Result<Self, BundleError> {
        if bytes.len() < HEADER {
            return Err(BundleError::TooShort);
        }
        if bytes[..8] != MAGIC {
            return Err(BundleError::BadMagic);
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if !(VERSION_MIN..=VERSION).contains(&version) {
            return Err(BundleError::UnsupportedVersion(version));
        }
        let achievement_count = u16::from_le_bytes([bytes[10], bytes[11]]);
        let subject_count = u32::from_le_bytes(quad(bytes, 12));
        let built_at = u64::from_le_bytes(octet(bytes, 16));
        let holder_cursor = u64::from_le_bytes(octet(bytes, 24));
        let market_cursor = u64::from_le_bytes(octet(bytes, 32));
        let catalogue_digest = u64::from_le_bytes(octet(bytes, 40));
        let collection_len = u32::from_le_bytes(quad(bytes, 48)) as usize;
        let coverage = match bytes[52] {
            0 => Coverage::Complete,
            1 => Coverage::Partial,
            _ => return Err(BundleError::Inconsistent),
        };

        let collection_end = HEADER
            .checked_add(collection_len)
            .ok_or(BundleError::Truncated)?;
        let collection = bytes
            .get(HEADER..collection_end)
            .ok_or(BundleError::Truncated)?;
        let collection =
            core::str::from_utf8(collection).map_err(|_| BundleError::BadCollection)?;
        // The CAIP-19 reference is `chain/asset-type`, so the chain half is the CAIP-2 id a
        // reader needs to build an `AccountRef` from a key.
        let caip2 = collection.split('/').next().unwrap_or(collection);
        let chain = caip2.parse().map_err(|_| BundleError::BadCollection)?;

        let offsets_at = collection_end;
        let rarity_at = offsets_at
            .checked_add((subject_count as usize + 1) * 4)
            .ok_or(BundleError::Truncated)?;
        let records_at = rarity_at
            .checked_add(achievement_count as usize * 8)
            .ok_or(BundleError::Truncated)?;
        if records_at > bytes.len() {
            return Err(BundleError::Truncated);
        }

        let bundle = Self {
            bytes,
            version,
            achievement_count,
            subject_count,
            built_at,
            collection,
            chain,
            coverage,
            holder_cursor,
            market_cursor,
            catalogue_digest,
            offsets_at,
            rarity_at,
            records_at,
        };
        // The last offset must land exactly on the end of the buffer: anything else is a
        // file that was truncated after its counts were written.
        let last = bundle.offset(subject_count as usize)?;
        if records_at + last as usize != bytes.len() {
            return Err(BundleError::Inconsistent);
        }
        Ok(bundle)
    }

    /// Unix seconds the reading was taken. A reader should show this, not hide it.
    pub fn built_at(&self) -> u64 {
        self.built_at
    }

    /// The collection this file scores, as CAIP-19 without a token half.
    pub fn collection(&self) -> &'a str {
        self.collection
    }

    /// The chain the collection is on, parsed from the CAIP-19 reference.
    ///
    /// A reader needs it to turn a row's key into an account, and taking it from the file is
    /// what lets one reader serve a Cardano bundle and an EVM one.
    pub fn chain(&self) -> ChainRef {
        self.chain
    }

    /// Whether the walk behind this file proved it saw the whole holder family.
    ///
    /// A reader that evaluates progress from the lots must pass this to [`Assessment`]: a
    /// span read over a partial walk is unavailable, not short.
    pub fn coverage(&self) -> Coverage {
        self.coverage
    }

    /// The last holder-family slot folded into this bundle.
    pub fn holder_cursor(&self) -> Slot {
        self.holder_cursor
    }

    /// The last market-family slot folded into this bundle.
    pub fn market_cursor(&self) -> Slot {
        self.market_cursor
    }

    /// The catalogue this bundle was built against.
    pub fn catalogue_digest(&self) -> u64 {
        self.catalogue_digest
    }

    /// How many subjects the bundle carries.
    pub fn subject_count(&self) -> u32 {
        self.subject_count
    }

    /// How many achievements the catalogue has.
    pub fn achievement_count(&self) -> u16 {
        self.achievement_count
    }

    /// How many subjects hold `achievement`.
    pub fn rarity(&self, achievement: u16) -> Option<Rarity> {
        if achievement >= self.achievement_count {
            return None;
        }
        let at = self.rarity_at + achievement as usize * 8;
        Some(Rarity {
            holders_with: u32::from_le_bytes(quad(self.bytes, at)),
            population: u32::from_le_bytes(quad(self.bytes, at + 4)),
        })
    }

    /// The subject at ordinal `index`, in the bundle's own order.
    pub fn subject_at(&self, index: u32) -> Result<SubjectView<'a>, BundleError> {
        if index >= self.subject_count {
            return Err(BundleError::Inconsistent);
        }
        let start = self.offset(index as usize)? as usize;
        let end = self.offset(index as usize + 1)? as usize;
        if end < start {
            return Err(BundleError::Inconsistent);
        }
        let record = self
            .bytes
            .get(self.records_at + start..self.records_at + end)
            .ok_or(BundleError::Truncated)?;
        let key = key_of(record)?;
        let body = record.get(1 + key.len()..).ok_or(BundleError::Truncated)?;
        Ok(SubjectView {
            key,
            record: body,
            entry: earned_len(self.version),
        })
    }

    /// The subject whose account key is `key`, or `None`.
    ///
    /// One binary search over the offset table, for every key format: the record begins
    /// with its key, so a comparison needs no separate index and no knowledge of whether
    /// the key is a Cardano credential or an EVM address.
    pub fn find(&self, key: &str) -> Result<Option<SubjectView<'a>>, BundleError> {
        let wanted = key.as_bytes();
        let mut low = 0usize;
        let mut high = self.subject_count as usize;
        while low < high {
            let mid = (low + high) / 2;
            match self.key_at(mid)?.cmp(wanted) {
                core::cmp::Ordering::Less => low = mid + 1,
                core::cmp::Ordering::Greater => high = mid,
                core::cmp::Ordering::Equal => return self.subject_at(mid as u32).map(Some),
            }
        }
        Ok(None)
    }

    /// Every subject, in the bundle's order — the eligible population for a rarity count.
    pub fn subjects(&self) -> impl Iterator<Item = Result<SubjectView<'a>, BundleError>> + '_ {
        (0..self.subject_count).map(|index| self.subject_at(index))
    }

    fn offset(&self, index: usize) -> Result<u32, BundleError> {
        let at = self.offsets_at + index * 4;
        self.bytes
            .get(at..at + 4)
            .map(|slice| u32::from_le_bytes(quad(slice, 0)))
            .ok_or(BundleError::Truncated)
    }

    fn key_at(&self, index: usize) -> Result<&'a [u8], BundleError> {
        let start = self.offset(index)? as usize;
        let end = self.offset(index + 1)? as usize;
        if end < start {
            return Err(BundleError::Inconsistent);
        }
        // Bounded by the record, not by the rest of the buffer: a corrupt key length then
        // fails rather than comparing bytes that belong to the next subject.
        let record = self
            .bytes
            .get(self.records_at + start..self.records_at + end)
            .ok_or(BundleError::Truncated)?;
        key_of(record)
    }
}

/// The key at the front of one encoded record.
fn key_of(record: &[u8]) -> Result<&[u8], BundleError> {
    let length = *record.first().ok_or(BundleError::Truncated)? as usize;
    record.get(1..1 + length).ok_or(BundleError::Truncated)
}

/// One subject's row, as slices into the bundle.
#[derive(Debug, Clone, Copy)]
pub struct SubjectView<'a> {
    key: &'a [u8],
    record: &'a [u8],
    /// Bytes per earned entry in this file's revision.
    entry: usize,
}

impl<'a> SubjectView<'a> {
    /// The account key as stored: the text the fold interns.
    pub fn key(&self) -> &'a [u8] {
        self.key
    }

    /// The account key as text — a hex stake credential, or the address itself.
    ///
    /// This is the form the fold interns and the catalogue names, so it is what a caller
    /// hashes, displays or compares.
    pub fn key_str(&self) -> String {
        String::from_utf8_lossy(self.key).into_owned()
    }

    /// The earned achievements, ascending by index.
    pub fn earned(&self) -> impl Iterator<Item = Earned> + 'a {
        let count = self
            .record
            .get(0..2)
            .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
            .unwrap_or(0) as usize;
        let record = self.record;
        let entry = self.entry;
        (0..count).filter_map(move |index| {
            let at = 2 + index * entry;
            let slice = record.get(at..at + entry)?;
            Some(Earned {
                achievement: u16::from_le_bytes([slice[0], slice[1]]),
                valid_from: u64::from_le_bytes(octet(slice, 2)),
                standing: slice
                    .get(10)
                    .map_or(Standing::Record, |byte| standing_of(*byte)),
                // A file with no count read one piece for anything held, which is what it scored.
                held: match slice.get(11..13) {
                    Some(pair) => u16::from_le_bytes([pair[0], pair[1]]),
                    None => {
                        u16::from(slice.get(10).map(|b| standing_of(*b)) == Some(Standing::Now))
                    }
                },
            })
        })
    }

    /// Every lot, open and closed.
    pub fn lots(&self) -> impl Iterator<Item = Lot> + 'a {
        let earned = self
            .record
            .get(0..2)
            .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
            .unwrap_or(0) as usize;
        let at = 2 + earned * self.entry;
        let count = self
            .record
            .get(at..at + 2)
            .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
            .unwrap_or(0) as usize;
        let record = self.record;
        (0..count).filter_map(move |index| {
            let start = at + 2 + index * LOT;
            let slice = record.get(start..start + LOT)?;
            let disposed_at = u64::from_le_bytes(octet(slice, 36));
            let disposed_unix = u64::from_le_bytes(octet(slice, 44));
            Some(Lot {
                unit: u32::from_le_bytes(quad(slice, 0)),
                quantity: i64::from_le_bytes(octet(slice, 4)),
                open: i64::from_le_bytes(octet(slice, 12)),
                acquired_at: u64::from_le_bytes(octet(slice, 20)),
                acquired_unix: u64::from_le_bytes(octet(slice, 28)),
                disposed_at: (disposed_at != OPEN).then_some(disposed_at),
                disposed_unix: (disposed_unix != OPEN).then_some(disposed_unix),
                from_mint: slice[52] != 0,
            })
        })
    }
}

fn quad(bytes: &[u8], at: usize) -> [u8; 4] {
    let mut out = [0u8; 4];
    out.copy_from_slice(&bytes[at..at + 4]);
    out
}

fn octet(bytes: &[u8], at: usize) -> [u8; 8] {
    let mut out = [0u8; 8];
    out.copy_from_slice(&bytes[at..at + 8]);
    out
}

// ============================================================================
// Evaluating a bundle
// ============================================================================

/// A [`Store`] over one subject's row in a bundle.
///
/// This is what makes a published bundle usable by the same evaluator the batch and the
/// Durable Object run: `evaluate` needs a store, and a row is a store for one subject.
/// Everything the fold would write is [`BundleError::ReadOnly`] — the bundle is derived, so
/// the answer to "record this" is "rebuild, do not mutate".
pub struct BundleStore<'a> {
    view: SubjectView<'a>,
    chain: ChainRef,
}

impl<'a> BundleStore<'a> {
    /// A store for one row.
    pub fn new(view: SubjectView<'a>, chain: ChainRef) -> Self {
        Self { view, chain }
    }

    /// The account key, as text — what the catalogue and a caller compare.
    pub fn key_str(&self) -> String {
        self.view.key_str()
    }
}

impl Store for BundleStore<'_> {
    type Error = BundleError;

    fn subject(&mut self, _account: &AccountRef) -> Result<SubjectId, Self::Error> {
        Ok(SubjectId(0))
    }

    fn unit(&mut self, _token: &TokenRef) -> Result<UnitId, Self::Error> {
        Err(BundleError::ReadOnly)
    }

    fn token_of(&self, _unit: UnitId) -> Result<Option<TokenRef>, Self::Error> {
        Ok(None)
    }

    fn account_of(&self, _subject: SubjectId) -> Result<Option<AccountRef>, Self::Error> {
        Ok(Some(AccountRef::new(self.chain, self.key_str())))
    }

    fn open_lot(&mut self, _lot: NewLot) -> Result<Holding, Self::Error> {
        Err(BundleError::ReadOnly)
    }

    fn save_lot(&mut self, _lot: &Holding) -> Result<(), Self::Error> {
        Err(BundleError::ReadOnly)
    }

    fn open_lots_of(&self, _subject: SubjectId, unit: UnitId) -> Result<Vec<Holding>, Self::Error> {
        Ok(self
            .lots()
            .filter(|lot| lot.unit == unit && lot.open > 0)
            .collect())
    }

    fn lots(&self, _subject: SubjectId) -> Result<Vec<Holding>, Self::Error> {
        Ok(self.lots().collect())
    }

    fn traits_of(&self, _unit: UnitId) -> Result<Vec<(String, String)>, Self::Error> {
        // Metadata is not in the bundle yet. Empty is "no rows recorded", which the
        // evaluator reports as `Unavailable(Gap::Traits)` rather than as a miss.
        Ok(Vec::new())
    }

    fn record_mint(
        &mut self,
        _unit: UnitId,
        _slot: Slot,
        _unix: u64,
    ) -> Result<MintFact, Self::Error> {
        Err(BundleError::ReadOnly)
    }

    fn mint_of(&self, _unit: UnitId) -> Result<Option<MintFact>, Self::Error> {
        // A row carries the lot's own `from_mint` and acquisition slot, not the collection's
        // mint order. Provenance predicates therefore read as a gap from a bundle; the
        // earned set in the file already answered them.
        Ok(None)
    }

    fn mint_summary(&self) -> Result<Option<MintSummary>, Self::Error> {
        Ok(None)
    }

    fn observe_market(&mut self) -> Result<(), Self::Error> {
        Err(BundleError::ReadOnly)
    }

    fn apply_book(
        &mut self,
        _change: crate::state::BookChange<'_>,
        _at: Option<Slot>,
        _unix: u64,
    ) -> Result<crate::state::BookOutcome, Self::Error> {
        Err(BundleError::ReadOnly)
    }

    fn record_movement(&mut self, _subject: SubjectId) -> Result<(), Self::Error> {
        Err(BundleError::ReadOnly)
    }

    fn movements_of(&self, _subject: SubjectId) -> Result<Option<u64>, Self::Error> {
        // A row carries lots, not a movement count, so a ratio predicate reads as a gap
        // from a bundle; the earned set in the file already answered it.
        Ok(None)
    }

    fn record_placement(
        &mut self,
        _subject: SubjectId,
        _order: &str,
        _side: MarketSide,
        _at: Option<Slot>,
        _unix: u64,
    ) -> Result<(), Self::Error> {
        Err(BundleError::ReadOnly)
    }

    fn record_fill(&mut self, _subject: SubjectId, _fill: NewFill) -> Result<(), Self::Error> {
        Err(BundleError::ReadOnly)
    }

    fn market_history(&self, _subject: SubjectId) -> Result<Option<MarketHistory>, Self::Error> {
        // A row carries lots, not marketplace facts. Market predicates therefore read as a
        // gap from a bundle; the earned set in the file already answered them.
        Ok(None)
    }

    fn floor_series(&self) -> Result<Option<crate::state::FloorSeries>, Self::Error> {
        // The floor is derived from the marketplace feed, which a row does not carry.
        Ok(None)
    }

    fn subjects(&self) -> Result<Vec<SubjectId>, Self::Error> {
        Ok(vec![SubjectId(0)])
    }
}

impl BundleStore<'_> {
    fn lots(&self) -> impl Iterator<Item = Holding> + '_ {
        self.view.lots().map(|lot| Holding {
            lot: LotId(0),
            subject: SubjectId(0),
            unit: UnitId(lot.unit),
            quantity: lot.quantity,
            open: lot.open,
            acquired_at: lot.acquired_at,
            acquired_unix: lot.acquired_unix,
            acquired_by: if lot.from_mint {
                AcquiredBy::Mint
            } else {
                AcquiredBy::Transfer
            },
            acquired_tx: None,
            disposed_at: lot.disposed_at,
            disposed_unix: lot.disposed_unix,
            disposed_tx: None,
        })
    }
}

/// Evaluates a catalogue against one row of a bundle.
///
/// The same [`evaluate`] the replay and the Durable Object call, so a profile served from a
/// bundle and a profile served from the live store cannot disagree about an outcome — only
/// about how much history each has seen, which is what `built_at` is for.
pub fn evaluate_row<'a>(
    store: &BundleStore<'_>,
    at: &Assessment,
    definitions: &[Definition<'a>],
) -> Result<Vec<Outcome<'a>>, BundleError> {
    evaluate(store, SubjectId(0), at, definitions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(byte: u8) -> String {
        format!("{byte:02x}").repeat(28)
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            built_at: 1_793_427_000,
            collection: crate::memory::fixtures::perps().to_string(),
            coverage: Coverage::Complete,
            holder_cursor: 201_753_328,
            market_cursor: 179_054_285,
            catalogue_digest: 0xdead_beef,
            achievement_count: 2,
            rarity: vec![
                Rarity {
                    holders_with: 1,
                    population: 2,
                },
                Rarity {
                    holders_with: 0,
                    population: 2,
                },
            ],
            subjects: vec![
                SubjectRecord {
                    key: credential(0xbb),
                    earned: vec![Earned {
                        achievement: 0,
                        valid_from: 170_960_000,
                        standing: Standing::Now,
                        held: 3,
                    }],
                    lots: vec![Lot {
                        unit: 7,
                        quantity: 1,
                        open: 1,
                        acquired_at: 170_960_000,
                        acquired_unix: 170_960_000,
                        disposed_at: None,
                        disposed_unix: None,
                        from_mint: true,
                    }],
                },
                SubjectRecord {
                    key: "addr1v825u96vkgrr5lke2c36sgvxa72szq4d50wq3jdk4crylncgd0x55".into(),
                    earned: Vec::new(),
                    lots: vec![Lot {
                        unit: 9,
                        quantity: 1,
                        open: 0,
                        acquired_at: 170_000_000,
                        acquired_unix: 170_000_010,
                        disposed_at: Some(170_500_000),
                        disposed_unix: Some(170_500_100),
                        from_mint: false,
                    }],
                },
                SubjectRecord {
                    key: credential(0xaa),
                    earned: Vec::new(),
                    lots: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn a_snapshot_round_trips_through_its_bytes() {
        let bytes = write(&snapshot());
        let bundle = Bundle::open(&bytes).expect("a written bundle opens");
        assert_eq!(bundle.built_at(), 1_793_427_000);
        assert_eq!(bundle.holder_cursor(), 201_753_328);
        assert_eq!(bundle.market_cursor(), 179_054_285);
        assert_eq!(bundle.catalogue_digest(), 0xdead_beef);
        assert_eq!(
            bundle.collection(),
            crate::memory::fixtures::perps().to_string()
        );
        assert_eq!(bundle.coverage(), Coverage::Complete);
        assert_eq!(bundle.subject_count(), 3);
        assert_eq!(bundle.achievement_count(), 2);
    }

    #[test]
    fn a_credential_is_found_by_binary_search_and_keeps_its_row() {
        let bytes = write(&snapshot());
        let bundle = Bundle::open(&bytes).unwrap();

        let found = bundle.find(&credential(0xbb)).unwrap().expect("present");
        assert_eq!(found.earned().count(), 1);
        assert_eq!(found.earned().next().unwrap().valid_from, 170_960_000);
        assert_eq!(found.earned().next().unwrap().standing, Standing::Now);
        assert_eq!(found.earned().next().unwrap().held, 3);
        let lot = found.lots().next().unwrap();
        assert_eq!(lot.unit, 7);
        assert_eq!(lot.acquired_unix, 170_960_000);
        assert!(lot.from_mint);
        assert_eq!(lot.disposed_at, None);

        let missing = bundle.find(&credential(0xcc)).unwrap();
        assert!(missing.is_none(), "an absent credential is not a hit");
    }

    #[test]
    fn a_version_two_file_still_reads_with_every_entry_a_record() {
        // Written before standings existed: ten bytes an entry, and the lots straight after.
        let bytes = write_as(&snapshot(), 2);
        let bundle = Bundle::open(&bytes).expect("the previous revision opens");
        let found = bundle.find(&credential(0xbb)).unwrap().expect("present");
        let earned: Vec<Earned> = found.earned().collect();
        assert_eq!(earned.len(), 1);
        assert_eq!(earned[0].valid_from, 170_960_000);
        assert_eq!(
            earned[0].standing,
            Standing::Record,
            "a file with no standing scores as it did"
        );
        assert_eq!(
            found.lots().next().unwrap().unit,
            7,
            "the lots are found past the narrower entries"
        );
    }

    #[test]
    fn a_version_three_file_reads_one_piece_for_a_holding_held() {
        // Standings but no counts: eleven bytes an entry. A holding held reads as one piece, which
        // is what it granted when it was written.
        let bytes = write_as(&snapshot(), 3);
        let bundle = Bundle::open(&bytes).expect("version 3 opens");
        let found = bundle.find(&credential(0xbb)).unwrap().expect("present");
        let earned: Vec<Earned> = found.earned().collect();
        assert_eq!(earned[0].standing, Standing::Now);
        assert_eq!(earned[0].held, 1);
        assert_eq!(found.lots().next().unwrap().unit, 7);
    }

    #[test]
    fn an_address_key_is_found_by_the_same_index() {
        // The three enterprise addresses in the real Perps data have no credential; they
        // are not a second index any more, just a key of a different length.
        let bytes = write(&snapshot());
        let bundle = Bundle::open(&bytes).unwrap();
        let key = "addr1v825u96vkgrr5lke2c36sgvxa72szq4d50wq3jdk4crylncgd0x55";
        let found = bundle.find(key).unwrap().expect("present");
        assert_eq!(found.key_str(), key);
        let lot = found.lots().next().unwrap();
        assert_eq!(lot.disposed_at, Some(170_500_000));
        assert_eq!(lot.disposed_unix, Some(170_500_100));
        assert!(found.earned().next().is_none());
    }

    #[test]
    fn an_evm_address_is_found_without_a_second_index() {
        // A 20-byte EVM address is not a 28-byte Cardano credential, and v1 would have
        // fallen back to a linear scan for it. The key section no longer has a format.
        let mut snapshot = snapshot();
        snapshot.collection =
            "eip155:4663/erc721:0x00000000000000000000000000000000000000aa".into();
        let evm = |byte: u8| format!("0x{}", format!("{byte:02x}").repeat(20));
        snapshot.subjects.push(SubjectRecord {
            key: evm(0x11),
            earned: Vec::new(),
            lots: Vec::new(),
        });
        snapshot.subjects.push(SubjectRecord {
            key: evm(0x22),
            earned: Vec::new(),
            lots: Vec::new(),
        });
        let bytes = write(&snapshot);
        let bundle = Bundle::open(&bytes).unwrap();
        assert_eq!(bundle.chain().as_caip2(), "eip155:4663");

        for key in [evm(0x11), evm(0x22)] {
            let found = bundle.find(&key).unwrap().expect("present");
            assert_eq!(found.key_str(), key);
        }
        assert!(bundle.find(&evm(0x33)).unwrap().is_none());
        assert_eq!(bundle.subject_count(), 5);
    }

    #[test]
    fn order_is_the_bundle_and_not_the_caller() {
        // Written bb, enterprise, aa; read back in ascending key order. One index covers
        // every key format, so the enterprise address sorts between the two credentials.
        let bytes = write(&snapshot());
        let bundle = Bundle::open(&bytes).unwrap();
        let keys: Vec<String> = bundle
            .subjects()
            .map(|row| row.unwrap().key_str())
            .collect();
        assert_eq!(keys[0], credential(0xaa));
        assert_eq!(
            keys[1],
            "addr1v825u96vkgrr5lke2c36sgvxa72szq4d50wq3jdk4crylncgd0x55"
        );
        assert_eq!(keys[2], credential(0xbb));
    }

    #[test]
    fn a_duplicate_key_is_written_once() {
        let mut snapshot = snapshot();
        snapshot.subjects.push(SubjectRecord {
            key: credential(0xaa),
            earned: Vec::new(),
            lots: Vec::new(),
        });
        let bundle_bytes = write(&snapshot);
        let bundle = Bundle::open(&bundle_bytes).unwrap();
        assert_eq!(bundle.subject_count(), 3);
    }

    #[test]
    fn the_same_snapshot_writes_the_same_bytes() {
        // The seal is only reproducible if encoding is deterministic.
        assert_eq!(write(&snapshot()), write(&snapshot()));
    }

    #[test]
    fn a_truncated_bundle_is_refused_rather_than_read_short() {
        let bytes = write(&snapshot());
        let short = &bytes[..bytes.len() - 1];
        assert_eq!(Bundle::open(short).unwrap_err(), BundleError::Inconsistent);
    }

    #[test]
    fn a_foreign_file_is_refused_by_its_magic() {
        let mut bytes = write(&snapshot());
        bytes[0] = b'X';
        assert_eq!(Bundle::open(&bytes).unwrap_err(), BundleError::BadMagic);
        assert_eq!(Bundle::open(&[0u8; 4]).unwrap_err(), BundleError::TooShort);
    }

    #[test]
    fn a_newer_version_is_refused_by_name() {
        let mut bytes = write(&snapshot());
        bytes[8..10].copy_from_slice(&99u16.to_le_bytes());
        assert_eq!(
            Bundle::open(&bytes).unwrap_err(),
            BundleError::UnsupportedVersion(99)
        );
    }

    #[test]
    fn the_header_states_the_collection_and_the_walks_coverage() {
        // Both are what a reader needs to say which collection it is looking at and whether
        // a span over it is answerable at all.
        let mut snapshot = snapshot();
        snapshot.coverage = Coverage::Partial;
        let bytes = write(&snapshot);
        let bundle = Bundle::open(&bytes).unwrap();
        assert_eq!(bundle.collection(), snapshot.collection);
        assert_eq!(bundle.coverage(), Coverage::Partial);
    }

    #[test]
    fn a_collection_that_is_not_utf8_is_refused() {
        let mut bytes = write(&snapshot());
        bytes[48..52].copy_from_slice(&1u32.to_le_bytes());
        bytes[HEADER] = 0xff;
        assert_eq!(
            Bundle::open(&bytes).unwrap_err(),
            BundleError::BadCollection
        );
    }

    #[test]
    fn an_unknown_coverage_byte_is_refused() {
        let mut bytes = write(&snapshot());
        bytes[52] = 9;
        assert_eq!(Bundle::open(&bytes).unwrap_err(), BundleError::Inconsistent);
    }

    #[test]
    fn a_collection_whose_chain_is_unknown_is_refused() {
        // The header names the collection, and the chain half of it has to be one this
        // build can build an `AccountRef` for; otherwise a reader would have to guess.
        let mut snapshot = snapshot();
        snapshot.collection = "not-a-chain/cnt:aa".into();
        assert_eq!(
            Bundle::open(&write(&snapshot)).unwrap_err(),
            BundleError::BadCollection
        );
    }

    #[test]
    fn a_row_is_a_store_the_evaluator_can_read() {
        use crate::predicate::Predicate;

        let bytes = write(&snapshot());
        let bundle = Bundle::open(&bytes).unwrap();
        assert_eq!(bundle.chain(), ChainRef::cardano_mainnet());
        let view = bundle.find(&credential(0xbb)).unwrap().unwrap();
        let store = BundleStore::new(view, bundle.chain());
        let definitions = vec![
            Definition {
                id: "never-sold",
                predicate: Predicate::NeverDisposed,
            },
            Definition {
                id: "from-mint",
                predicate: Predicate::HeldSinceMint,
            },
            Definition {
                id: "genesis",
                predicate: Predicate::MintRankAtMost { n: 1 },
            },
            Definition {
                id: "churner",
                predicate: Predicate::MovementRatioAtLeast { ratio: 10.0 },
            },
        ];
        let outcomes = evaluate_row(
            &store,
            &Assessment::complete(201_753_328, 1_793_244_235),
            &definitions,
        )
        .expect("a published row evaluates");
        assert_eq!(outcomes.len(), 4);
        assert_eq!(outcomes[0].id, "never-sold");
        assert_eq!(
            outcomes[0].progress,
            crate::Progress::Earned {
                since: crate::EarnedSince::Exact(170_960_000)
            }
        );
        // A row carries the lot's own `from_mint`, not the collection's mint order, so a
        // rank is a gap from a bundle — the earned set in the file is what answered it.
        assert_eq!(
            outcomes[2].progress,
            crate::Progress::Unavailable(crate::Gap::MintFacts)
        );
        // A row carries no movement count either, so a ratio is a gap for the same reason:
        // the earned set in the file already answered it.
        assert_eq!(
            outcomes[3].progress,
            crate::Progress::Unavailable(crate::Gap::MovementFacts)
        );
    }

    #[test]
    fn a_digest_changes_with_the_catalogue_and_its_order() {
        let one = catalogue_digest(["a", "b"]);
        let reordered = catalogue_digest(["b", "a"]);
        let added = catalogue_digest(["a", "b", "c"]);
        assert_ne!(one, reordered);
        assert_ne!(one, added);
        assert_eq!(one, catalogue_digest(["a", "b"]));
    }
}
