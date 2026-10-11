//! The collection listing: one policy's standing assets, name-sorted.
//!
//! The directory answers *which chunks* hold a policy's records; this answers
//! *what the policy holds* — every standing asset, as the card fields a grid
//! needs. One object per policy, so "browse a collection" is one read where
//! the walk answers one asset at a time.
//!
//! # Layout
//!
//! Header, fixed [`LISTING_HEADER_LEN`] bytes (all integers little-endian):
//!
//! | off | size | field |
//! |---|---|---|
//! | 0 | 4 | magic `AMXL` |
//! | 4 | 1 | format version ([`crate::FORMAT_VERSION`]) |
//! | 5 | 1 | network byte |
//! | 6 | 2 | reserved, zero |
//! | 8 | 28 | `policy_id` |
//! | 36 | 2 | `built_through` — the coverage these entries reflect |
//! | 38 | 4 | `asset_count` |
//! | 42 | 22 | reserved, zero |
//!
//! Then `asset_count` entries, **ascending by raw name bytes**, each:
//!
//! | size | field |
//! |---|---|
//! | 1 | `name_len` (on-chain asset names are ≤ 32 bytes) |
//! | n | `name` — raw bytes, exactly as minted |
//! | 2 | `image_len` |
//! | n | `image` — the URI exactly as the chain states it; 0-length = none |
//! | 2 | `media_type_len` |
//! | n | `media_type`; 0-length = none |
//!
//! Sortedness is load-bearing: it makes the object deterministic and lets a
//! pager resume from the last name it served. The builder sorts defensively
//! and refuses a duplicate name.
//!
//! # Standing
//!
//! One entry per asset currently standing — the latest event per name
//! ([`crate::latest`]), a burn clearing it. Deciding standing is the
//! emitter's call; this module stores what it was handed. An asset with no
//! image still has an entry: `image` is empty, not absent.
//!
//! # Walking
//!
//! [`entry_at`] parses one entry from any boundary of the entry region — the
//! paged read (`ASSET_METADATA_INDEX.md` §Collection listing reads) cuts
//! windows into pages with it, and a client parsing a `format=bin` window
//! walks the same call.
//!
//! # Churn
//!
//! Mutable, like the directory: the emitter rewrites a policy's listing when
//! its standing set changes. Read it through the R2 binding, never the CDN —
//! a cached copy of a key a later publish changed outlives the publish.

use crate::bytes::{array_at, put_u16_prefixed, take, take_u16_prefixed, u8_at, u16_at, u32_at};
use crate::error::{DecodeError, EncodeError};
use crate::record::Record;
use crate::{FORMAT_VERSION, LISTING_MAGIC, POLICY_ID_LEN};

/// Fixed header length.
pub const LISTING_HEADER_LEN: usize = 64;

/// The largest entry the format can state: `name_len` is a `u8` (≤ 255),
/// `image_len` and `media_type_len` are `u16`s (≤ 65,535), plus the three
/// length prefixes. A paged reader sizes its window from this so one ranged
/// read always reaches the end of the page's last entry.
pub const LISTING_MAX_ENTRY_LEN: usize = 255 + 65_535 + 65_535 + 5;

/// A listing's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListingHeader {
    /// Network byte ([`crate::NETWORK_MAINNET`] / [`crate::NETWORK_PREPROD`]).
    pub network: u8,
    /// The policy every entry belongs to.
    pub policy_id: [u8; POLICY_ID_LEN],
    /// The build coverage these entries reflect — the pointer's
    /// `built_through` when the listing was last written. Per object: a
    /// listing the sweep did not touch keeps the coverage it was written at.
    pub built_through: u16,
    /// Number of standing assets that follow.
    pub asset_count: u32,
}

impl ListingHeader {
    /// Parse a header from the front of `bytes`. Tolerates a longer slice
    /// (the caller has the whole object); requires at least a header.
    pub fn parse(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut at = 0usize;
        let magic = array_at::<4>(bytes, &mut at)?;
        if magic != LISTING_MAGIC {
            return Err(DecodeError::BadMagic {
                expected: LISTING_MAGIC,
                found: magic,
            });
        }
        let version = u8_at(bytes, &mut at)?;
        if version != FORMAT_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let network = u8_at(bytes, &mut at)?;
        let reserved = u16_at(bytes, &mut at)?;
        if reserved != 0 {
            return Err(DecodeError::ReservedNotZero { at: at - 2 });
        }
        let policy_id = array_at::<POLICY_ID_LEN>(bytes, &mut at)?;
        let built_through = u16_at(bytes, &mut at)?;
        let asset_count = u32_at(bytes, &mut at)?;
        for _ in 0..22 {
            if u8_at(bytes, &mut at)? != 0 {
                return Err(DecodeError::ReservedNotZero { at: at - 1 });
            }
        }
        Ok(Self {
            network,
            policy_id,
            built_through,
            asset_count,
        })
    }

    /// Encode the 64-byte header. Padding bytes are written zero.
    #[must_use]
    pub fn encode(&self) -> [u8; LISTING_HEADER_LEN] {
        let mut out = [0u8; LISTING_HEADER_LEN];
        out[0..4].copy_from_slice(&LISTING_MAGIC);
        out[4] = FORMAT_VERSION;
        out[5] = self.network;
        // 6..8 reserved, zero
        out[8..36].copy_from_slice(&self.policy_id);
        out[36..38].copy_from_slice(&self.built_through.to_le_bytes());
        out[38..42].copy_from_slice(&self.asset_count.to_le_bytes());
        // 42..64 reserved, zero
        out
    }
}

/// One standing asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingEntry {
    /// The asset name **exactly as minted**, raw bytes.
    pub name: Vec<u8>,
    /// The image URI as the chain states it; empty when none.
    pub image: Vec<u8>,
    /// Top-level media type; empty when none.
    pub media_type: Vec<u8>,
}

impl ListingEntry {
    /// The card fields of a standing record. The caller passes the winner of
    /// [`crate::latest`]; a burn has no entry in a listing.
    #[must_use]
    pub fn from_record(record: &Record) -> Self {
        Self {
            name: record.name.clone(),
            image: record.image.clone(),
            media_type: record.media_type.clone(),
        }
    }
}

/// A whole listing, borrowed.
pub struct ListingView<'a> {
    /// The parsed header.
    pub header: ListingHeader,
    bytes: &'a [u8],
}

impl<'a> ListingView<'a> {
    /// Parse a whole listing object.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            header: ListingHeader::parse(bytes)?,
            bytes,
        })
    }

    /// Every entry, in stored (name-ascending) order.
    pub fn entries(&self) -> Result<Vec<ListingEntry>, DecodeError> {
        let mut at = LISTING_HEADER_LEN;
        let mut out = Vec::with_capacity(self.header.asset_count as usize);
        for _ in 0..self.header.asset_count {
            let (entry, next) = entry_from(self.bytes, at)?;
            out.push(entry);
            at = next;
        }
        Ok(out)
    }
}

/// Parse the entry at `at` — the paged walk's step.
///
/// `at` must be an entry boundary: [`LISTING_HEADER_LEN`] at the front of the
/// object, or a previous call's returned offset. `Ok(None)` means `at` is
/// exactly the end of `bytes`; a shortfall mid-entry is
/// [`DecodeError::Truncated`] — how a reader walking a window learns the
/// entry runs past it.
pub fn entry_at(bytes: &[u8], at: usize) -> Result<Option<(ListingEntry, usize)>, DecodeError> {
    if at == bytes.len() {
        return Ok(None);
    }
    entry_from(bytes, at).map(Some)
}

/// The entry parse `entries` and `entry_at` share.
fn entry_from(bytes: &[u8], at: usize) -> Result<(ListingEntry, usize), DecodeError> {
    let mut cursor = at;
    let name_len = u8_at(bytes, &mut cursor)? as usize;
    let name = take(bytes, &mut cursor, name_len)?.to_vec();
    let image = take_u16_prefixed(bytes, &mut cursor)?.to_vec();
    let media_type = take_u16_prefixed(bytes, &mut cursor)?.to_vec();
    Ok((
        ListingEntry {
            name,
            image,
            media_type,
        },
        cursor,
    ))
}

/// Assemble one policy's listing bytes. Builder-side.
pub struct ListingBuilder {
    /// Network byte.
    pub network: u8,
    /// The policy the entries belong to.
    pub policy_id: [u8; POLICY_ID_LEN],
    /// The coverage to claim for these entries.
    pub built_through: u16,
    /// Standing assets; sorted by name on encode, duplicates refused.
    pub entries: Vec<ListingEntry>,
}

impl ListingBuilder {
    /// Encode header + entries.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let mut entries = self.entries.clone();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        if entries.windows(2).any(|pair| pair[0].name == pair[1].name) {
            return Err(EncodeError::DuplicateName);
        }
        let asset_count = u32::try_from(entries.len()).map_err(|_| EncodeError::FieldTooLong {
            field: "asset_count",
            len: entries.len(),
        })?;
        let header = ListingHeader {
            network: self.network,
            policy_id: self.policy_id,
            built_through: self.built_through,
            asset_count,
        };
        let mut out = Vec::new();
        out.extend_from_slice(&header.encode());
        for entry in &entries {
            let name_len =
                u8::try_from(entry.name.len()).map_err(|_| EncodeError::FieldTooLong {
                    field: "name",
                    len: entry.name.len(),
                })?;
            out.push(name_len);
            out.extend_from_slice(&entry.name);
            put_u16_prefixed(&mut out, "image", &entry.image)?;
            put_u16_prefixed(&mut out, "media_type", &entry.media_type)?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NETWORK_MAINNET;
    use crate::record::{Event, Family, Provenance};

    fn unhex(s: &str) -> Vec<u8> {
        let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(clean.len().is_multiple_of(2), "odd hex length");
        (0..clean.len() / 2)
            .map(|i| u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16).expect("hex"))
            .collect()
    }

    fn policy() -> [u8; POLICY_ID_LEN] {
        let mut policy = [0u8; POLICY_ID_LEN];
        policy[..4].copy_from_slice(&[0x93, 0xa0, 0x60, 0x06]);
        policy
    }

    fn entry(name: &[u8], image: &[u8], media_type: &[u8]) -> ListingEntry {
        ListingEntry {
            name: name.to_vec(),
            image: image.to_vec(),
            media_type: media_type.to_vec(),
        }
    }

    fn builder(entries: Vec<ListingEntry>) -> ListingBuilder {
        ListingBuilder {
            network: NETWORK_MAINNET,
            policy_id: policy(),
            built_through: 0x1234,
            entries,
        }
    }

    /// BYTE STABILITY. The committed bytes are the contract the box-side
    /// emitter and the edge-side reader meet at; a change here is a format
    /// change and must be deliberate.
    #[test]
    fn a_listing_encodes_to_the_committed_bytes() {
        let expected = unhex(
            "414d584c 01 00 0000
             93a06006
             00000000 00000000 00000000 00000000 00000000 00000000
             3412 02000000
             00000000 00000000 00000000 00000000 00000000 0000
             01 41 0800 697066733a2f2f78 0900 696d6167652f706e67
             01 42 0000 0000",
        );
        let listing = builder(vec![
            entry(b"A", b"ipfs://x", b"image/png"),
            entry(b"B", b"", b""),
        ]);
        assert_eq!(listing.encode().expect("encodes"), expected);
    }

    #[test]
    fn a_listing_round_trips_header_and_entries() {
        let listing = builder(vec![
            entry(b"B", b"ipfs://y", b""),
            entry(b"A", b"ipfs://x", b"image/png"),
            entry(b"C", b"", b""),
        ]);
        let bytes = listing.encode().expect("encodes");
        let view = ListingView::parse(&bytes).expect("parses");
        assert_eq!(view.header.network, NETWORK_MAINNET);
        assert_eq!(view.header.policy_id, policy());
        assert_eq!(view.header.built_through, 0x1234);
        assert_eq!(view.header.asset_count, 3);
        assert_eq!(
            view.entries().expect("entries"),
            vec![
                entry(b"A", b"ipfs://x", b"image/png"),
                entry(b"B", b"ipfs://y", b""),
                entry(b"C", b"", b""),
            ]
        );
    }

    #[test]
    fn entries_are_sorted_by_name_on_encode() {
        let bytes = builder(vec![entry(b"B", b"", b""), entry(b"A", b"", b"")])
            .encode()
            .expect("encodes");
        let view = ListingView::parse(&bytes).expect("parses");
        let names: Vec<Vec<u8>> = view
            .entries()
            .expect("entries")
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec![b"A".to_vec(), b"B".to_vec()]);
    }

    /// One entry per name is the contract. The refusal must not depend on the
    /// duplicates arriving adjacent, so the second case separates them.
    #[test]
    fn a_duplicate_name_is_refused() {
        assert_eq!(
            builder(vec![entry(b"A", b"", b""), entry(b"A", b"", b"")]).encode(),
            Err(EncodeError::DuplicateName)
        );
        assert_eq!(
            builder(vec![
                entry(b"A", b"", b""),
                entry(b"B", b"", b""),
                entry(b"A", b"", b""),
            ])
            .encode(),
            Err(EncodeError::DuplicateName)
        );
    }

    #[test]
    fn a_name_longer_than_u8_is_refused_not_truncated() {
        let long = b"A".repeat(300);
        assert!(matches!(
            builder(vec![entry(&long, b"", b"")]).encode(),
            Err(EncodeError::FieldTooLong { field: "name", .. })
        ));
    }

    #[test]
    fn an_empty_listing_round_trips() {
        let bytes = builder(Vec::new()).encode().expect("encodes");
        assert_eq!(bytes.len(), LISTING_HEADER_LEN);
        let view = ListingView::parse(&bytes).expect("parses");
        assert_eq!(view.header.asset_count, 0);
        assert!(view.entries().expect("entries").is_empty());
    }

    #[test]
    fn a_truncated_object_refuses() {
        let mut short = LISTING_MAGIC.to_vec();
        short.extend_from_slice(&[FORMAT_VERSION, NETWORK_MAINNET]);
        assert!(matches!(
            ListingHeader::parse(&short),
            Err(DecodeError::Truncated { .. })
        ));

        let bytes = builder(vec![entry(b"A", b"ipfs://x", b"")])
            .encode()
            .expect("encodes");
        let cut = &bytes[..bytes.len() - 1];
        let view = ListingView::parse(cut).expect("the header still parses");
        assert!(matches!(view.entries(), Err(DecodeError::Truncated { .. })));
    }

    #[test]
    fn a_bent_header_is_refused() {
        let bytes = builder(Vec::new()).encode().expect("encodes");

        let mut magic = bytes.clone();
        magic[0] ^= 0xff;
        assert!(matches!(
            ListingHeader::parse(&magic),
            Err(DecodeError::BadMagic { .. })
        ));

        let mut version = bytes.clone();
        version[4] = 2;
        assert_eq!(
            ListingHeader::parse(&version),
            Err(DecodeError::UnsupportedVersion(2))
        );

        let mut reserved = bytes.clone();
        reserved[6] = 1;
        assert!(matches!(
            ListingHeader::parse(&reserved),
            Err(DecodeError::ReservedNotZero { .. })
        ));

        let mut tail = bytes;
        tail[42] = 1;
        assert!(matches!(
            ListingHeader::parse(&tail),
            Err(DecodeError::ReservedNotZero { .. })
        ));
    }

    #[test]
    fn entry_at_walks_from_any_boundary_to_the_end() {
        let bytes = builder(vec![
            entry(b"A", b"ipfs://x", b"image/png"),
            entry(b"B", b"", b""),
            entry(b"Cc", b"ipfs://z", b""),
        ])
        .encode()
        .expect("encodes");
        let view = ListingView::parse(&bytes).expect("parses");

        let mut at = LISTING_HEADER_LEN;
        let mut walked = Vec::new();
        while let Some((entry, next)) = entry_at(&bytes, at).expect("walks") {
            walked.push(entry);
            at = next;
        }
        assert_eq!(walked, view.entries().expect("entries"));
        assert_eq!(at, bytes.len());

        // From the second entry's own boundary the walk starts there.
        let (_, second) = entry_at(&bytes, LISTING_HEADER_LEN)
            .expect("walks")
            .expect("an entry");
        let (entry, _) = entry_at(&bytes, second).expect("walks").expect("an entry");
        assert_eq!(entry.name, b"B");
    }

    #[test]
    fn a_window_cut_mid_entry_is_truncated_not_a_shorter_entry() {
        let bytes = builder(vec![
            entry(b"A", b"ipfs://x", b"image/png"),
            entry(b"B", b"", b""),
        ])
        .encode()
        .expect("encodes");
        let (_, second) = entry_at(&bytes, LISTING_HEADER_LEN)
            .expect("walks")
            .expect("an entry");
        let cut = &bytes[..bytes.len() - 1];
        assert!(matches!(
            entry_at(cut, second),
            Err(DecodeError::Truncated { .. })
        ));
        // Exactly at the end there is nothing left to parse.
        assert_eq!(entry_at(&bytes, bytes.len()).expect("walks"), None);
    }

    #[test]
    fn the_max_entry_len_is_reachable() {
        let max = ListingEntry {
            name: vec![b'a'; 255],
            image: vec![b'i'; 65_535],
            media_type: vec![b'm'; 65_535],
        };
        let bytes = builder(vec![max]).encode().expect("encodes");
        let (entry, next) = entry_at(&bytes, LISTING_HEADER_LEN)
            .expect("walks")
            .expect("an entry");
        assert_eq!(next - LISTING_HEADER_LEN, LISTING_MAX_ENTRY_LEN);
        assert_eq!(entry.name.len(), 255);
        assert_eq!(entry.image.len(), 65_535);
    }

    #[test]
    fn from_record_carries_the_card_fields() {
        let record = Record {
            family: Family::Cip25,
            event: Event::Mint,
            policy_id: policy(),
            provenance: Provenance {
                chunk: 7,
                body_offset: 10,
                body_len: 500,
                aux_offset: 0,
                aux_len: 0,
            },
            name: b"NFT".to_vec(),
            image: b"ipfs://x".to_vec(),
            media_type: b"image/png".to_vec(),
            media: Vec::new(),
            payload: b"xyz".to_vec(),
        };
        assert_eq!(
            ListingEntry::from_record(&record),
            entry(b"NFT", b"ipfs://x", b"image/png")
        );
    }
}
