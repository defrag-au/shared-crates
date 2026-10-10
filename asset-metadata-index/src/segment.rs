//! One segment per sealed chunk: the resolved records, immutable forever.
//!
//! # Why per chunk
//!
//! Chunks seal and never change, so a segment written once is immutable by
//! construction. That is the churn property the whole design rests on: a
//! minted-out collection's segment — and its directory entry — never move
//! again. New mints append a new segment; nothing is ever rewritten.
//!
//! # The per-policy index
//!
//! Records are grouped by policy and sorted by `(policy_id, name)`. A fixed
//! [`IndexEntry`] per policy gives the slice's byte range, so a reader fetches
//! the index region once, binary-searches it, and range-reads exactly the
//! policy's slice — no whole-object fetch, no scan of other policies.

use crate::bytes::{array_at, u8_at, u16_at, u32_at};
use crate::error::{DecodeError, EncodeError};
use crate::record::{Record, Records, records};
use crate::{FORMAT_VERSION, POLICY_ID_LEN, SEGMENT_MAGIC};

/// Fixed header length.
pub const SEGMENT_HEADER_LEN: usize = 64;

/// Fixed index entry length.
pub const INDEX_ENTRY_LEN: usize = 36;

/// A segment's header — offsets are absolute within the object, so a range
/// read against R2 uses them verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentHeader {
    /// Network byte ([`crate::NETWORK_MAINNET`] / [`crate::NETWORK_PREPROD`]).
    pub network: u8,
    /// Chunk number in the immutable log. Segments are named for it.
    pub chunk: u16,
    /// Total records in the segment.
    pub record_count: u32,
    /// Offset of the index region.
    pub index_off: u32,
    /// Number of index entries (one per policy present).
    pub index_count: u32,
    /// Offset of the records region.
    pub records_off: u32,
    /// blake2b-256 over the records region — the verifier checks it before
    /// trusting a segment.
    pub records_hash: [u8; 32],
}

impl SegmentHeader {
    /// Parse a header from the front of `bytes`. Tolerates a longer slice
    /// (the caller has the whole object); requires at least a header.
    pub fn parse(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut at = 0usize;
        let magic = array_at::<4>(bytes, &mut at)?;
        if magic != SEGMENT_MAGIC {
            return Err(DecodeError::BadMagic {
                expected: SEGMENT_MAGIC,
                found: magic,
            });
        }
        let version = u8_at(bytes, &mut at)?;
        if version != FORMAT_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let network = u8_at(bytes, &mut at)?;
        let chunk = u16_at(bytes, &mut at)?;
        let flags = u16_at(bytes, &mut at)?;
        if flags != 0 {
            return Err(DecodeError::ReservedNotZero { at: at - 2 });
        }
        let record_count = u32_at(bytes, &mut at)?;
        let index_off = u32_at(bytes, &mut at)?;
        let index_count = u32_at(bytes, &mut at)?;
        let records_off = u32_at(bytes, &mut at)?;
        let records_hash = array_at::<32>(bytes, &mut at)?;
        for _ in 0..6 {
            if u8_at(bytes, &mut at)? != 0 {
                return Err(DecodeError::ReservedNotZero { at: at - 1 });
            }
        }
        Ok(Self {
            network,
            chunk,
            record_count,
            index_off,
            index_count,
            records_off,
            records_hash,
        })
    }

    /// Encode the 64-byte header. Padding bytes are written zero.
    #[must_use]
    pub fn encode(&self) -> [u8; SEGMENT_HEADER_LEN] {
        let mut out = [0u8; SEGMENT_HEADER_LEN];
        out[0..4].copy_from_slice(&SEGMENT_MAGIC);
        out[4] = FORMAT_VERSION;
        out[5] = self.network;
        out[6..8].copy_from_slice(&self.chunk.to_le_bytes());
        // 8..10 flags, zero
        out[10..14].copy_from_slice(&self.record_count.to_le_bytes());
        out[14..18].copy_from_slice(&self.index_off.to_le_bytes());
        out[18..22].copy_from_slice(&self.index_count.to_le_bytes());
        out[22..26].copy_from_slice(&self.records_off.to_le_bytes());
        out[26..58].copy_from_slice(&self.records_hash);
        out
    }
}

/// One policy's slice in a segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    /// The policy this slice belongs to.
    pub policy_id: [u8; POLICY_ID_LEN],
    /// Absolute offset of the policy's first record.
    pub records_off: u32,
    /// Byte length of the policy's records.
    pub records_len: u32,
}

impl IndexEntry {
    /// Encode as the fixed 36-byte index entry.
    #[must_use]
    pub fn encode(&self) -> [u8; INDEX_ENTRY_LEN] {
        let mut out = [0u8; INDEX_ENTRY_LEN];
        out[0..POLICY_ID_LEN].copy_from_slice(&self.policy_id);
        out[POLICY_ID_LEN..POLICY_ID_LEN + 4].copy_from_slice(&self.records_off.to_le_bytes());
        out[POLICY_ID_LEN + 4..POLICY_ID_LEN + 8].copy_from_slice(&self.records_len.to_le_bytes());
        out
    }

    fn parse_at(bytes: &[u8], at: usize) -> Result<Self, DecodeError> {
        let mut at = at;
        Ok(Self {
            policy_id: array_at::<POLICY_ID_LEN>(bytes, &mut at)?,
            records_off: u32_at(bytes, &mut at)?,
            records_len: u32_at(bytes, &mut at)?,
        })
    }
}

/// A whole segment, borrowed. `bytes` must be the complete object — the index
/// offsets are relative to its start, which is also what a range reader sees.
pub struct SegmentView<'a> {
    /// The parsed header.
    pub header: SegmentHeader,
    bytes: &'a [u8],
}

impl<'a> SegmentView<'a> {
    /// Parse a whole segment object.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let header = SegmentHeader::parse(bytes)?;
        let index_end = (header.index_off as usize)
            .checked_add(header.index_count as usize * INDEX_ENTRY_LEN)
            .ok_or(DecodeError::BadLength)?;
        if index_end > bytes.len() || header.records_off as usize > bytes.len() {
            return Err(DecodeError::BadLength);
        }
        Ok(Self { header, bytes })
    }

    /// The index entry for `policy_id`, by binary search over the fixed-size
    /// index region.
    pub fn entry(
        &self,
        policy_id: &[u8; POLICY_ID_LEN],
    ) -> Result<Option<IndexEntry>, DecodeError> {
        find_index_entry(
            &self.bytes[self.header.index_off as usize..],
            self.header.index_count,
            policy_id,
        )
    }

    /// Every index entry, in stored order — the inspect and verify tooling
    /// reads this; the read path binary-searches [`Self::entry`] instead.
    pub fn entries(&self) -> Result<Vec<IndexEntry>, DecodeError> {
        let mut out = Vec::with_capacity(self.header.index_count as usize);
        for i in 0..self.header.index_count as usize {
            out.push(self.entry_at(i)?);
        }
        Ok(out)
    }

    fn entry_at(&self, i: usize) -> Result<IndexEntry, DecodeError> {
        let at = self.header.index_off as usize + i * INDEX_ENTRY_LEN;
        IndexEntry::parse_at(self.bytes, at)
    }

    /// The policy's record slice, or `None` when the policy has no records in
    /// this segment.
    pub fn slice(&self, policy_id: &[u8; POLICY_ID_LEN]) -> Result<Option<&'a [u8]>, DecodeError> {
        let Some(entry) = self.entry(policy_id)? else {
            return Ok(None);
        };
        let start = entry.records_off as usize;
        let end = start
            .checked_add(entry.records_len as usize)
            .ok_or(DecodeError::BadLength)?;
        if end > self.bytes.len() {
            return Err(DecodeError::BadLength);
        }
        Ok(Some(&self.bytes[start..end]))
    }

    /// Iterate the policy's records in this segment.
    pub fn records_for(
        &self,
        policy_id: &[u8; POLICY_ID_LEN],
    ) -> Result<Option<Records<'a>>, DecodeError> {
        Ok(self.slice(policy_id)?.map(records))
    }
}

/// Binary-search an index region — the shared search behind
/// [`SegmentView::entry`], and the fat-segment walk's middle read, where the
/// header is known but the rest of the object was never fetched. A region
/// shorter than `index_count` claims refuses rather than reporting an absent
/// policy.
pub(crate) fn find_index_entry(
    index: &[u8],
    index_count: u32,
    policy_id: &[u8; POLICY_ID_LEN],
) -> Result<Option<IndexEntry>, DecodeError> {
    let mut lo = 0usize;
    let mut hi = index_count as usize;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let entry = IndexEntry::parse_at(index, mid * INDEX_ENTRY_LEN)?;
        match entry.policy_id.cmp(policy_id) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => return Ok(Some(entry)),
        }
    }
    Ok(None)
}

/// Assemble a segment's bytes. Builder-side; the reader only ever parses.
///
/// Records must be grouped by policy, and policies sorted by `policy_id` —
/// the reader's binary search depends on it. This constructor sorts a copy
/// defensively rather than trusting the caller, because an unsorted segment
/// is a wrong answer waiting for the right hash.
pub struct SegmentBuilder {
    /// Network byte.
    pub network: u8,
    /// Chunk number.
    pub chunk: u16,
    /// One entry per policy, records in chain order within the policy.
    pub entries: Vec<([u8; POLICY_ID_LEN], Vec<Record>)>,
}

impl SegmentBuilder {
    /// Encode header + index + records.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let mut entries = self.entries.clone();
        entries.sort_by_key(|(policy_id, _)| *policy_id);
        for (_, records) in &mut entries {
            records.sort_by_key(|r| (r.provenance.chunk, r.provenance.body_offset));
        }

        let mut records_region = Vec::new();
        let index_count = u32::try_from(entries.len()).map_err(|_| EncodeError::FieldTooLong {
            field: "index_count",
            len: entries.len(),
        })?;
        let records_off = (SEGMENT_HEADER_LEN + entries.len() * INDEX_ENTRY_LEN) as u32;
        let mut index = Vec::with_capacity(entries.len() * INDEX_ENTRY_LEN);
        let mut record_count = 0usize;
        for (policy_id, records) in &entries {
            let start = records_region.len() as u32;
            for record in records {
                record.encode_into(&mut records_region)?;
            }
            record_count += records.len();
            index.extend_from_slice(
                &IndexEntry {
                    policy_id: *policy_id,
                    records_off: records_off + start,
                    records_len: (records_region.len() as u32) - start,
                }
                .encode(),
            );
        }

        let header = SegmentHeader {
            network: self.network,
            chunk: self.chunk,
            record_count: u32::try_from(record_count).map_err(|_| EncodeError::FieldTooLong {
                field: "record_count",
                len: record_count,
            })?,
            index_off: SEGMENT_HEADER_LEN as u32,
            index_count,
            records_off,
            // The builder computes the real hash after encoding; a zero here
            // is a bug the verifier will catch, not a value to trust.
            records_hash: [0u8; 32],
        };
        let mut out = Vec::with_capacity(SEGMENT_HEADER_LEN + index.len() + records_region.len());
        out.extend_from_slice(&header.encode());
        out.extend_from_slice(&index);
        out.extend_from_slice(&records_region);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Event, Family, Provenance};

    fn policy(byte: u8) -> [u8; POLICY_ID_LEN] {
        [byte; POLICY_ID_LEN]
    }

    fn record(byte: u8, name: &[u8]) -> Record {
        Record {
            family: Family::Cip25,
            event: Event::Mint,
            policy_id: policy(byte),
            provenance: Provenance {
                chunk: 3,
                body_offset: 42,
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

    fn build() -> Vec<u8> {
        SegmentBuilder {
            network: crate::NETWORK_MAINNET,
            chunk: 42,
            entries: vec![
                (policy(0x02), vec![record(0x02, b"B")]),
                (policy(0x01), vec![record(0x01, b"A"), record(0x01, b"B")]),
            ],
        }
        .encode()
        .expect("encodes")
    }

    #[test]
    fn header_round_trips() {
        let header = SegmentHeader {
            network: crate::NETWORK_MAINNET,
            chunk: 9,
            record_count: 3,
            index_off: 64,
            index_count: 2,
            records_off: 136,
            records_hash: [7u8; 32],
        };
        let bytes = header.encode();
        assert_eq!(bytes.len(), SEGMENT_HEADER_LEN);
        assert_eq!(SegmentHeader::parse(&bytes).expect("parses"), header);
    }

    #[test]
    fn a_segment_round_trips_and_indexes_by_policy() {
        let bytes = build();
        let view = SegmentView::parse(&bytes).expect("parses");
        assert_eq!(view.header.chunk, 42);
        assert_eq!(view.header.index_count, 2);
        assert_eq!(view.header.record_count, 3);

        let one = view
            .records_for(&policy(0x01))
            .expect("parses")
            .expect("present")
            .collect::<Result<Vec<_>, _>>()
            .expect("records");
        assert_eq!(one.len(), 2);
        assert_eq!(one[0].name, b"A");
        assert_eq!(one[1].name, b"B");

        let two = view
            .records_for(&policy(0x02))
            .expect("parses")
            .expect("present")
            .collect::<Result<Vec<_>, _>>()
            .expect("records");
        assert_eq!(two.len(), 1);

        assert!(view.entry(&policy(0x09)).expect("parses").is_none());
        assert!(view.slice(&policy(0x09)).expect("parses").is_none());
    }

    /// The builder sorts defensively: an unsorted caller still produces a
    /// segment the binary search can read.
    #[test]
    fn the_builder_sorts_policies_and_within_policy_events() {
        let mut later = record(0x01, b"A");
        later.provenance.body_offset = 90;
        let bytes = SegmentBuilder {
            network: crate::NETWORK_MAINNET,
            chunk: 1,
            entries: vec![
                (policy(0x05), vec![record(0x05, b"Z")]),
                (policy(0x01), vec![later, record(0x01, b"A")]),
            ],
        }
        .encode()
        .expect("encodes");
        let view = SegmentView::parse(&bytes).expect("parses");
        let first = view.entry(&policy(0x01)).expect("parses").expect("present");
        assert!(
            first.records_off
                < view
                    .entry(&policy(0x05))
                    .expect("parses")
                    .expect("present")
                    .records_off
        );
        let records = view
            .records_for(&policy(0x01))
            .expect("parses")
            .expect("present")
            .collect::<Result<Vec<_>, _>>()
            .expect("records");
        assert_eq!(records[0].provenance.body_offset, 42);
    }

    #[test]
    fn bent_builders_are_refused() {
        let bytes = build();

        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert!(matches!(
            SegmentView::parse(&bad_magic),
            Err(DecodeError::BadMagic { .. })
        ));

        let mut bad_version = bytes.clone();
        bad_version[4] = 9;
        assert!(matches!(
            SegmentView::parse(&bad_version),
            Err(DecodeError::UnsupportedVersion(9))
        ));

        // index_off pointing past the end must refuse, not read.
        let mut bad_index = bytes.clone();
        bad_index[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            SegmentView::parse(&bad_index),
            Err(DecodeError::BadLength)
        ));
    }

    /// The search must work on the index region alone, without the header,
    /// and a region shorter than the header's count must surface as
    /// truncation — never as an absent policy.
    #[test]
    fn a_fetched_index_region_searches_alone() {
        let bytes = build();
        let view = SegmentView::parse(&bytes).expect("parses");
        assert_eq!(view.entries().expect("entries").len(), 2);

        let at = view.header.index_off as usize;
        let len = view.header.index_count as usize * INDEX_ENTRY_LEN;
        let region = &bytes[at..at + len];
        assert_eq!(
            find_index_entry(region, 2, &policy(0x02)).expect("searches"),
            Some(IndexEntry::parse_at(region, INDEX_ENTRY_LEN).expect("parses"))
        );
        assert!(
            find_index_entry(region, 2, &policy(0x09))
                .expect("searches")
                .is_none()
        );

        let short = &region[..INDEX_ENTRY_LEN];
        assert!(matches!(
            find_index_entry(short, 3, &policy(0x09)),
            Err(DecodeError::Truncated { .. })
        ));
    }
}
