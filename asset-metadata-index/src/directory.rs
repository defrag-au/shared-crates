//! The directory: which chunks hold a policy's records.
//!
//! One object per policy-id first byte (256 shards), so a shard only changes
//! when a policy inside it mints or updates — a stable collection's shard is
//! untouched for as long as the collection is. Entries are additive: a reader
//! that sees a stale shard scans the frontier segments named by the pointer,
//! so staleness can cost work but never produce a wrong answer.
//!
//! Shards are read whole (hundreds of entries) and scanned; v1 has no
//! intra-shard index.

use crate::bytes::{array_at, u8_at, u16_at, u32_at};
use crate::error::{DecodeError, EncodeError};
use crate::{DIRECTORY_MAGIC, FORMAT_VERSION, POLICY_ID_LEN};

/// Fixed header length.
pub const DIRECTORY_HEADER_LEN: usize = 32;

/// A shard's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryHeader {
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
        if version != FORMAT_VERSION {
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
        out[4] = FORMAT_VERSION;
        out[5] = self.network;
        out[6] = self.shard;
        out[8..12].copy_from_slice(&self.entry_count.to_le_bytes());
        out[12..14].copy_from_slice(&self.directory_through_chunk.to_le_bytes());
        out
    }
}

/// One policy's directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// The policy.
    pub policy_id: [u8; POLICY_ID_LEN],
    /// Chunks holding the policy's records, strictly ascending.
    pub chunks: Vec<u16>,
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
    /// policy has no entry. Entries are sorted, so the scan can stop early.
    pub fn chunks_for(
        &self,
        policy_id: &[u8; POLICY_ID_LEN],
    ) -> Result<Option<Vec<u16>>, DecodeError> {
        let mut at = DIRECTORY_HEADER_LEN;
        for _ in 0..self.header.entry_count {
            let id = array_at::<POLICY_ID_LEN>(self.bytes, &mut at)?;
            let count = u16_at(self.bytes, &mut at)?;
            let mut chunks = Vec::with_capacity(count as usize);
            for _ in 0..count {
                chunks.push(u16_at(self.bytes, &mut at)?);
            }
            if &id == policy_id {
                return Ok(Some(chunks));
            }
            if id > *policy_id {
                return Ok(None);
            }
        }
        Ok(None)
    }

    /// Every entry, in stored order — the builder-side verify and the inspect
    /// tooling read this; the read path uses [`Self::chunks_for`].
    pub fn entries(&self) -> Result<Vec<DirectoryEntry>, DecodeError> {
        let mut out = Vec::with_capacity(self.header.entry_count as usize);
        let mut at = DIRECTORY_HEADER_LEN;
        for _ in 0..self.header.entry_count {
            let policy_id = array_at::<POLICY_ID_LEN>(self.bytes, &mut at)?;
            let count = u16_at(self.bytes, &mut at)?;
            let mut chunks = Vec::with_capacity(count as usize);
            for _ in 0..count {
                chunks.push(u16_at(self.bytes, &mut at)?);
            }
            out.push(DirectoryEntry { policy_id, chunks });
        }
        Ok(out)
    }
}

/// Assemble one shard's bytes. Builder-side.
pub struct DirectoryShard {
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
        let mut entries = self.entries.clone();
        entries.sort_by_key(|e| e.policy_id);
        for entry in &mut entries {
            entry.chunks.sort_unstable();
            entry.chunks.dedup();
        }
        let entry_count = u32::try_from(entries.len()).map_err(|_| EncodeError::FieldTooLong {
            field: "entry_count",
            len: entries.len(),
        })?;
        let header = DirectoryHeader {
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
                u16::try_from(entry.chunks.len()).map_err(|_| EncodeError::FieldTooLong {
                    field: "chunk_count",
                    len: entry.chunks.len(),
                })?;
            out.extend_from_slice(&count.to_le_bytes());
            for chunk in &entry.chunks {
                out.extend_from_slice(&chunk.to_le_bytes());
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

    fn shard() -> Vec<u8> {
        DirectoryShard {
            network: crate::NETWORK_MAINNET,
            shard: 0x02,
            directory_through_chunk: 900,
            entries: vec![
                DirectoryEntry {
                    policy_id: policy(0x02),
                    chunks: vec![7, 2],
                },
                DirectoryEntry {
                    policy_id: policy(0x01),
                    chunks: vec![3],
                },
            ],
        }
        .encode()
        .expect("encodes")
    }

    #[test]
    fn a_shard_round_trips_and_sorts() {
        let bytes = shard();
        let view = DirectoryView::parse(&bytes).expect("parses");
        assert_eq!(view.header.shard, 0x02);
        assert_eq!(view.header.directory_through_chunk, 900);

        let entries = view.entries().expect("entries");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].policy_id, policy(0x01));
        assert_eq!(entries[1].chunks, vec![2, 7]);

        assert_eq!(
            view.chunks_for(&policy(0x01)).expect("parses"),
            Some(vec![3])
        );
        assert_eq!(view.chunks_for(&policy(0x09)).expect("parses"), None);
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
}
