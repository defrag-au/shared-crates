//! A published achievements bundle — one file, sorted by account, read without decoding.
//!
//! # Why one file and not one file per stake
//!
//! A per-stake object would make a profile page a single fetch, and make everything else
//! worse: the grid needs per-achievement rarity, the allocation iterates every subject, and
//! a "who earned this" list reads across all of them. At 700 subjects a per-stake layout is
//! 700 objects per rebuild — an object explosion for one query shape. This layout is one
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
//! keys        `credential_count` × 28 bytes, ascending — the binary-search index
//! others      `other_count` × (u8 len + bytes), ascending — accounts with no credential
//! offsets     (`subject_count` + 1) × u32 into `records`
//! rarity      `achievement_count` × (u32 holders_with, u32 population)
//! records     variable: earned entries, then lots
//! ```
//!
//! Keys come first and are fixed width, so [`Bundle::find`] is a binary search over a
//! contiguous array and never parses anything. A record is reached by slicing between two
//! consecutive offsets — no length prefixes to walk, no allocation.
//!
//! # Why the lots are in it
//!
//! The earned set is a **snapshot** and goes stale monotonically: an achievement that is
//! missing is one that has not been observed yet, never one that was revoked. Progress is
//! the opposite — `"213/365 days"` that is a day old looks broken — so it is not stored.
//! The lots are, and [`crate::evaluate`] over [`BundleStore`] derives progress from them and
//! the reader's own clock. Adding a predicate therefore needs no new bundle.

use chain_events::{AccountRef, Coverage};
use chains::{TokenRef, ChainRef};

use crate::evaluate::{Assessment, Definition, Outcome, evaluate};
use crate::state::{AcquiredBy, Holding, LotId, NewLot, Slot, Store, SubjectId, UnitId};

/// Identifies the format. The trailing zeros are padding so a truncated read cannot
/// accidentally match.
pub const MAGIC: [u8; 8] = *b"HBABv1\0\0";

/// The layout revision. A reader refuses a bundle it does not know rather than guessing.
pub const VERSION: u16 = 1;

/// Bytes before the first section.
const HEADER: usize = 80;

/// Where an open interval ends, in the bundle and in [`Holding::disposed_at`] alike.
///
/// A chain height that large is not reachable, so it cannot collide with a real disposal —
/// which `0` could, since `0` is a legal coordinate in a synthetic source.
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
    pub population: u32,
}

/// One earned achievement, as the snapshot observed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Earned {
    /// Index into the catalogue the bundle was built against.
    pub achievement: u16,
    /// The chain height it was first true at, or the height of the reading that saw it.
    pub valid_from: Slot,
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
    /// Chain height it finished leaving, or `None` while any of it is held.
    pub disposed_at: Option<Slot>,
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
    /// it has none.
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
    /// One entry per achievement, in index order.
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
    let mut subjects = snapshot.subjects.clone();
    subjects.sort_by(|a, b| a.key.cmp(&b.key));
    subjects.dedup_by(|a, b| a.key == b.key);

    let credentials: Vec<&SubjectRecord> = subjects
        .iter()
        .filter(|record| credential_of(&record.key).is_some())
        .collect();
    let others: Vec<&SubjectRecord> = subjects
        .iter()
        .filter(|record| credential_of(&record.key).is_none())
        .collect();

    // Every subject has a record slot; the key sections only index the ones they cover.
    let mut records = Vec::new();
    let mut offsets: Vec<u32> = Vec::with_capacity(subjects.len() + 1);
    for record in credentials.iter().chain(others.iter()) {
        offsets.push(records.len() as u32);
        encode_record(record, &mut records);
    }
    offsets.push(records.len() as u32);

    let keys_len = credentials.len() * 28;
    let others_len: usize = others.iter().map(|record| 1 + record.key.len()).sum();
    let offsets_len = offsets.len() * 4;
    let rarity_len = snapshot.rarity.len() * 8;

    let mut out = Vec::with_capacity(
        HEADER + keys_len + others_len + offsets_len + rarity_len + records.len(),
    );
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&snapshot.achievement_count.to_le_bytes());
    out.extend_from_slice(&(subjects.len() as u32).to_le_bytes());
    out.extend_from_slice(&snapshot.built_at.to_le_bytes());
    out.extend_from_slice(&snapshot.holder_cursor.to_le_bytes());
    out.extend_from_slice(&snapshot.market_cursor.to_le_bytes());
    out.extend_from_slice(&snapshot.catalogue_digest.to_le_bytes());
    out.extend_from_slice(&(credentials.len() as u32).to_le_bytes());
    out.extend_from_slice(&(others.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    debug_assert_eq!(out.len(), HEADER);

    for record in &credentials {
        out.extend_from_slice(&credential_of(&record.key).expect("filtered"));
    }
    for record in &others {
        out.push(record.key.len() as u8);
        out.extend_from_slice(record.key.as_bytes());
    }
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

fn encode_record(record: &SubjectRecord, out: &mut Vec<u8>) {
    let mut earned = record.earned.clone();
    earned.sort_by_key(|entry| entry.achievement);
    out.extend_from_slice(&(earned.len() as u16).to_le_bytes());
    for entry in &earned {
        out.extend_from_slice(&entry.achievement.to_le_bytes());
        out.extend_from_slice(&entry.valid_from.to_le_bytes());
    }
    out.extend_from_slice(&(record.lots.len() as u16).to_le_bytes());
    for lot in &record.lots {
        out.extend_from_slice(&lot.unit.to_le_bytes());
        out.extend_from_slice(&lot.quantity.to_le_bytes());
        out.extend_from_slice(&lot.open.to_le_bytes());
        out.extend_from_slice(&lot.acquired_at.to_le_bytes());
        out.extend_from_slice(&lot.disposed_at.unwrap_or(OPEN).to_le_bytes());
        out.push(u8::from(lot.from_mint));
    }
}

/// The 28 bytes of a stake credential, or `None` for an account that has none.
fn credential_of(key: &str) -> Option<[u8; 28]> {
    if key.len() != 56 {
        return None;
    }
    let mut out = [0u8; 28];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(key.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

// ============================================================================
// Reading
// ============================================================================

/// A bundle over borrowed bytes. No copy, and no allocation to open.
#[derive(Debug, Clone)]
pub struct Bundle<'a> {
    bytes: &'a [u8],
    achievement_count: u16,
    subject_count: u32,
    built_at: u64,
    holder_cursor: Slot,
    market_cursor: Slot,
    catalogue_digest: u64,
    credentials: usize,
    others: usize,
    keys_at: usize,
    others_at: usize,
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
        if version != VERSION {
            return Err(BundleError::UnsupportedVersion(version));
        }
        let achievement_count = u16::from_le_bytes([bytes[10], bytes[11]]);
        let subject_count = u32::from_le_bytes(quad(bytes, 12));
        let built_at = u64::from_le_bytes(octet(bytes, 16));
        let holder_cursor = u64::from_le_bytes(octet(bytes, 24));
        let market_cursor = u64::from_le_bytes(octet(bytes, 32));
        let catalogue_digest = u64::from_le_bytes(octet(bytes, 40));
        let credentials = u32::from_le_bytes(quad(bytes, 48)) as usize;
        let others = u32::from_le_bytes(quad(bytes, 52)) as usize;
        if credentials + others != subject_count as usize {
            return Err(BundleError::Inconsistent);
        }

        let keys_at = HEADER;
        let others_at = keys_at
            .checked_add(credentials * 28)
            .ok_or(BundleError::Truncated)?;
        let others_end = (0..others)
            .try_fold(others_at, |at, index| {
                let length = *bytes.get(at)? as usize;
                let end = at.checked_add(1 + length)?;
                // The remainder is checked below, once; this only walks the lengths.
                let _ = index;
                Some(end)
            })
            .ok_or(BundleError::Truncated)?;
        let offsets_at = others_end;
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
            achievement_count,
            subject_count,
            built_at,
            holder_cursor,
            market_cursor,
            catalogue_digest,
            credentials,
            others,
            keys_at,
            others_at,
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
        let key = self.key_at(index as usize)?;
        let record = self
            .bytes
            .get(self.records_at + start..self.records_at + end)
            .ok_or(BundleError::Truncated)?;
        Ok(SubjectView {
            key,
            record,
            is_credential: (index as usize) < self.credentials,
        })
    }

    /// The subject whose account key is `key`, or `None`.
    ///
    /// A hex stake credential is a binary search over a 28-byte array. Anything else — an
    /// enterprise address, which genuinely has no credential — is a linear scan of the
    /// handful of those and never touches the main index.
    pub fn find(&self, key: &str) -> Result<Option<SubjectView<'a>>, BundleError> {
        if let Some(credential) = credential_of(key) {
            let mut low = 0usize;
            let mut high = self.credentials;
            while low < high {
                let mid = (low + high) / 2;
                let at = self.keys_at + mid * 28;
                match self.bytes.get(at..at + 28) {
                    None => return Err(BundleError::Truncated),
                    Some(found) => match found.cmp(&credential[..]) {
                        core::cmp::Ordering::Less => low = mid + 1,
                        core::cmp::Ordering::Greater => high = mid,
                        core::cmp::Ordering::Equal => return self.subject_at(mid as u32).map(Some),
                    },
                }
            }
            return Ok(None);
        }

        let mut at = self.others_at;
        for index in 0..self.others {
            let length = *self.bytes.get(at).ok_or(BundleError::Truncated)? as usize;
            let start = at + 1;
            let end = start + length;
            let found = self
                .bytes
                .get(start..end)
                .ok_or(BundleError::Truncated)?;
            if found == key.as_bytes() {
                return self
                    .subject_at((self.credentials + index) as u32)
                    .map(Some);
            }
            at = end;
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
        if index < self.credentials {
            let at = self.keys_at + index * 28;
            return self
                .bytes
                .get(at..at + 28)
                .ok_or(BundleError::Truncated);
        }
        let mut at = self.others_at;
        for _ in 0..(index - self.credentials) {
            let length = *self.bytes.get(at).ok_or(BundleError::Truncated)? as usize;
            at += 1 + length;
        }
        let length = *self.bytes.get(at).ok_or(BundleError::Truncated)? as usize;
        self.bytes
            .get(at + 1..at + 1 + length)
            .ok_or(BundleError::Truncated)
    }
}

/// One subject's row, as slices into the bundle.
#[derive(Debug, Clone, Copy)]
pub struct SubjectView<'a> {
    key: &'a [u8],
    record: &'a [u8],
    is_credential: bool,
}

impl<'a> SubjectView<'a> {
    /// The account key as stored: 28 raw bytes for a credential, the address bytes otherwise.
    pub fn key(&self) -> &'a [u8] {
        self.key
    }

    /// The account key as text — lower-case hex for a credential, the address otherwise.
    ///
    /// This is the form the fold interns and the catalogue names, so it is what a caller
    /// hashes, displays or compares. It allocates: a credential is stored as bytes and
    /// hexing it is the only way to produce the text.
    pub fn key_str(&self) -> String {
        if self.is_credential {
            self.key.iter().map(|byte| format!("{byte:02x}")).collect()
        } else {
            String::from_utf8_lossy(self.key).into_owned()
        }
    }

    /// The earned achievements, ascending by index.
    pub fn earned(&self) -> impl Iterator<Item = Earned> + 'a {
        let count = self
            .record
            .get(0..2)
            .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
            .unwrap_or(0) as usize;
        let record = self.record;
        (0..count).filter_map(move |index| {
            let at = 2 + index * 10;
            let slice = record.get(at..at + 10)?;
            Some(Earned {
                achievement: u16::from_le_bytes([slice[0], slice[1]]),
                valid_from: u64::from_le_bytes(octet(slice, 2)),
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
        let at = 2 + earned * 10;
        let count = self
            .record
            .get(at..at + 2)
            .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
            .unwrap_or(0) as usize;
        let record = self.record;
        (0..count).filter_map(move |index| {
            let start = at + 2 + index * 37;
            let slice = record.get(start..start + 37)?;
            let disposed = u64::from_le_bytes(octet(slice, 28));
            Some(Lot {
                unit: u32::from_le_bytes(quad(slice, 0)),
                quantity: i64::from_le_bytes(octet(slice, 4)),
                open: i64::from_le_bytes(octet(slice, 12)),
                acquired_at: u64::from_le_bytes(octet(slice, 20)),
                disposed_at: (disposed != OPEN).then_some(disposed),
                from_mint: slice[36] != 0,
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

    fn open_lots_of(
        &self,
        _subject: SubjectId,
        unit: UnitId,
    ) -> Result<Vec<Holding>, Self::Error> {
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
            acquired_by: if lot.from_mint {
                AcquiredBy::Mint
            } else {
                AcquiredBy::Transfer
            },
            acquired_tx: None,
            disposed_at: lot.disposed_at,
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

/// The coverage a bundle can claim.
///
/// A bundle is built from a walk that reported its own completeness; anything a reader
/// wants to say about spans has to be true of that walk. Until the bundle carries the
/// coverage explicitly, a reader states it — see [`Assessment`].
pub const BUNDLE_COVERAGE: Coverage = Coverage::Complete;

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(byte: u8) -> String {
        format!("{byte:02x}").repeat(28)
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            built_at: 1_793_427_000,
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
                    }],
                    lots: vec![Lot {
                        unit: 7,
                        quantity: 1,
                        open: 1,
                        acquired_at: 170_960_000,
                        disposed_at: None,
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
                        disposed_at: Some(170_500_000),
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
        let lot = found.lots().next().unwrap();
        assert_eq!(lot.unit, 7);
        assert!(lot.from_mint);
        assert_eq!(lot.disposed_at, None);

        let missing = bundle.find(&credential(0xcc)).unwrap();
        assert!(missing.is_none(), "an absent credential is not a hit");
    }

    #[test]
    fn an_account_with_no_credential_is_found_by_its_address() {
        // The three enterprise addresses in the real Perps data take this path.
        let bytes = write(&snapshot());
        let bundle = Bundle::open(&bytes).unwrap();
        let key = "addr1v825u96vkgrr5lke2c36sgvxa72szq4d50wq3jdk4crylncgd0x55";
        let found = bundle.find(key).unwrap().expect("present");
        assert_eq!(found.key_str(), key);
        assert_eq!(found.lots().next().unwrap().disposed_at, Some(170_500_000));
        assert!(found.earned().next().is_none());
    }

    #[test]
    fn order_is_the_bundle_and_not_the_caller() {
        // Written bb, enterprise, aa; read back aa, bb, enterprise.
        let bytes = write(&snapshot());
        let bundle = Bundle::open(&bytes).unwrap();
        let keys: Vec<String> = bundle
            .subjects()
            .map(|row| row.unwrap().key_str())
            .collect();
        assert_eq!(keys[0], credential(0xaa));
        assert_eq!(keys[1], credential(0xbb));
        assert!(keys[2].starts_with("addr1v"));
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
    fn a_row_is_a_store_the_evaluator_can_read() {
        use crate::predicate::Predicate;

        let bytes = write(&snapshot());
        let bundle = Bundle::open(&bytes).unwrap();
        let view = bundle.find(&credential(0xbb)).unwrap().unwrap();
        let store = BundleStore::new(view, ChainRef::cardano_mainnet());
        let definitions = vec![
            Definition {
                id: "never-sold",
                predicate: Predicate::NeverDisposed,
            },
            Definition {
                id: "from-mint",
                predicate: Predicate::HeldSinceMint,
            },
        ];
        let outcomes = evaluate_row(&store, &Assessment::complete(201_753_328), &definitions)
            .expect("a published row evaluates");
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].id, "never-sold");
        assert_eq!(outcomes[0].progress, crate::Progress::Earned {
            since: crate::EarnedSince::Exact(170_960_000)
        });
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
