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
//! | 4 | 1 | format version ([`crate::LISTING_VERSION`]) |
//! | 5 | 1 | network byte |
//! | 6 | 2 | reserved, zero |
//! | 8 | 28 | `policy_id` |
//! | 36 | 2 | `built_through` — the coverage these entries reflect |
//! | 38 | 4 | `asset_count` |
//! | 42 | 4 | `display_count` — v3 only; reserved on a v2 object |
//! | 46 | 18 | reserved, zero |
//!
//! Then `asset_count` entries, **ascending by raw name bytes**, each:
//!
//! | size | field |
//! |---|---|
//! | 1 | `form` — v3 only ([`Form`]: 0 bare, 1 CIP-25, 2 CIP-68 reference, 3 CIP-68 other) |
//! | 1 | `name_len` (on-chain asset names are ≤ 32 bytes) |
//! | n | `name` — raw bytes, exactly as minted |
//! | 8 | `supply` — the copies in existence, LE; always above zero |
//! | 2 | `image_len` |
//! | n | `image` — the URI exactly as the chain states it; 0-length = none |
//! | 2 | `media_type_len` |
//! | n | `media_type`; 0-length = none |
//!
//! Sortedness is load-bearing: it makes the object deterministic and lets a
//! pager resume from the last name it served. The builder sorts defensively
//! and refuses a duplicate name.
//!
//! # The form, and the display set
//!
//! A CIP-68 asset is **two standing names**: the reference token (`100`,
//! [`CIP68_REFERENCE_LABEL`]) that carries the datum, and the user form
//! (`222`/`333`/`444`) a holder holds — the same suffix under different
//! labels, each with a record so a caller can resolve whichever form it has.
//! The `form` byte states which is which, once, at build time: the reference
//! token is the metadata carrier, not a collection's tile, and the
//! `display_count` is how many entries a collection view keeps. Consumers
//! never parse a name prefix, and a CIP-25 name that happens to begin with
//! label bytes cannot be misread — the byte is a fact the builder wrote, not
//! a guess a reader makes.
//!
//! # Version 2
//!
//! A v2 object parses — entries state no form ([`ListingEntry::form`] is
//! `None`) and the header no `display_count` — so a reader keeps serving the
//! tree the reseed has not rewritten yet. v2 cannot be *encoded*: every write
//! is the current layout.
//!
//! # Standing
//!
//! One entry per asset currently standing: `supply` is what the asset's
//! records sum to ([`crate::standing`]), and a burn takes an entry out only
//! when it takes the last copy. The arithmetic lives here, not in the
//! emitter: [`ListingEntry::apply`] is the write-side twin of
//! [`crate::standing`], so the listing a sweep emits and the answer the walk
//! gives cannot drift. An asset with no image still has an entry: `image` is
//! empty, not absent. An asset with more than one copy is **one** entry
//! stating that supply, never several entries.
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

use crate::bytes::{
    array_at, put_u16_prefixed, take, take_u16_prefixed, u8_at, u16_at, u32_at, u64_at,
};
use crate::error::{DecodeError, EncodeError};
use crate::record::{Event, Family, Record};
use crate::{LISTING_MAGIC, LISTING_V2, LISTING_VERSION, POLICY_ID_LEN};

/// Fixed header length.
pub const LISTING_HEADER_LEN: usize = 64;

/// The largest entry the format can state: the `form` byte, `name_len` a `u8`
/// (≤ 255), `supply` 8, `image_len` and `media_type_len` `u16`s (≤ 65,535),
/// plus the three length prefixes. A paged reader sizes its window from this
/// so one ranged read always reaches the end of the page's last entry.
pub const LISTING_MAX_ENTRY_LEN: usize = 1 + 255 + 8 + 65_535 + 65_535 + 5;

/// The CIP-67 label prefix a CIP-68 reference token's name carries — label
/// `100`, the reference NFT. It is the one name form a collection view folds
/// out: the name carries the datum, and it is not what a holder holds.
pub const CIP68_REFERENCE_LABEL: [u8; 4] = [0x00, 0x06, 0x43, 0xb0];

/// What a standing name is: which standard carries its metadata, and — for
/// CIP-68 — whether the name is the reference token that carries the datum or
/// a form a holder holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Form {
    /// Nothing standard was found on the mint (a bare token, or metadata the
    /// builder did not recognise).
    Bare = 0,
    /// CIP-25: metadata in the mint transaction's auxiliary data.
    Cip25 = 1,
    /// CIP-68 reference token (label `100`): the name carries the datum, and
    /// a collection view folds it out ([`ListingHeader::display_count`]).
    Cip68Reference = 2,
    /// A CIP-68 name that is not the reference token — the `222`/`333`/`444`
    /// user forms a holder holds, and any labelled name whose label is not
    /// the reference's (the fail-open side: a form the builder cannot place
    /// is shown, not hidden).
    Cip68User = 3,
}

impl TryFrom<u8> for Form {
    type Error = DecodeError;

    fn try_from(b: u8) -> Result<Self, DecodeError> {
        Ok(match b {
            0 => Self::Bare,
            1 => Self::Cip25,
            2 => Self::Cip68Reference,
            3 => Self::Cip68User,
            other => return Err(DecodeError::BadForm(other)),
        })
    }
}

impl From<Form> for u8 {
    fn from(form: Form) -> Self {
        form as Self
    }
}

impl Form {
    /// Classify a record's name: the standard the record states, with CIP-68
    /// split by the reference label.
    #[must_use]
    pub fn of(family: Family, name: &[u8]) -> Self {
        match family {
            Family::None => Self::Bare,
            Family::Cip25 => Self::Cip25,
            Family::Cip68 if name.starts_with(&CIP68_REFERENCE_LABEL) => Self::Cip68Reference,
            Family::Cip68 => Self::Cip68User,
        }
    }
}

/// A listing's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListingHeader {
    /// The layout the object states: [`crate::LISTING_VERSION`], or
    /// [`crate::LISTING_V2`] for an object the reseed has not rewritten.
    pub version: u8,
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
    /// How many of them a collection view keeps — every entry whose form is
    /// not [`Form::Cip68Reference`]. `None` on a v2 object, whose entries
    /// state no form.
    pub display_count: Option<u32>,
}

impl ListingHeader {
    /// Parse a header from the front of `bytes`. Tolerates a longer slice
    /// (the caller has the whole object); requires at least a header. A v2
    /// object parses — entries state no form — so a reader keeps serving the
    /// tree the reseed has not rewritten yet.
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
        if version != LISTING_VERSION && version != LISTING_V2 {
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
        let display_count = if version == LISTING_VERSION {
            Some(u32_at(bytes, &mut at)?)
        } else {
            None
        };
        let reserved = LISTING_HEADER_LEN
            .checked_sub(at)
            .expect("the header parse never runs past the header");
        for _ in 0..reserved {
            if u8_at(bytes, &mut at)? != 0 {
                return Err(DecodeError::ReservedNotZero { at: at - 1 });
            }
        }
        Ok(Self {
            version,
            network,
            policy_id,
            built_through,
            asset_count,
            display_count,
        })
    }

    /// Encode the 64-byte v3 header. Padding bytes are written zero.
    ///
    /// A header parsed from a v2 object states no `display_count` and cannot
    /// be re-encoded ([`EncodeError::MissingForm`] is the entry-side twin):
    /// every write is the current layout.
    pub fn encode(&self) -> Result<[u8; LISTING_HEADER_LEN], EncodeError> {
        let Some(display_count) = self.display_count else {
            return Err(EncodeError::MissingForm);
        };
        let mut out = [0u8; LISTING_HEADER_LEN];
        out[0..4].copy_from_slice(&LISTING_MAGIC);
        out[4] = LISTING_VERSION;
        out[5] = self.network;
        // 6..8 reserved, zero
        out[8..36].copy_from_slice(&self.policy_id);
        out[36..38].copy_from_slice(&self.built_through.to_le_bytes());
        out[38..42].copy_from_slice(&self.asset_count.to_le_bytes());
        out[42..46].copy_from_slice(&display_count.to_le_bytes());
        // 46..64 reserved, zero
        Ok(out)
    }
}

/// One standing asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingEntry {
    /// What the name is ([`Form`]) — `None` on an entry parsed from a v2
    /// object, which records no form.
    pub form: Option<Form>,
    /// The asset name **exactly as minted**, raw bytes.
    pub name: Vec<u8>,
    /// The copies in existence when the entry was written — the quantity the
    /// asset's records sum to ([`crate::standing`]), above zero.
    pub supply: u64,
    /// The image URI as the chain states it; empty when none.
    pub image: Vec<u8>,
    /// Top-level media type; empty when none.
    pub media_type: Vec<u8>,
}

impl ListingEntry {
    /// The card fields of a standing record, with the supply the emission has
    /// summed to — the record's own quantity plus whatever the listing
    /// already held for this name. A burn has no entry in a listing.
    #[must_use]
    pub fn from_record(record: &Record, supply: u64) -> Self {
        Self {
            form: Some(Form::of(record.family, &record.name)),
            name: record.name.clone(),
            supply,
            image: record.image.clone(),
            media_type: record.media_type.clone(),
        }
    }

    /// Apply one defining record to a standing entry — the write-side twin of
    /// [`crate::standing`], deliberately the same arithmetic: quantities sum in
    /// chain order, floored at zero, so a burn takes the entry down only as far
    /// as the copies it accounts for. A mint or update replaces the metadata in
    /// force; a burn leaves it alone.
    ///
    /// `false` means the entry no longer stands and the caller must drop it.
    #[must_use]
    pub fn apply(&mut self, record: &Record) -> bool {
        let supply = i64::try_from(self.supply)
            .unwrap_or(i64::MAX)
            .saturating_add(record.qty);
        if supply <= 0 {
            return false;
        }
        if record.event != Event::Burn {
            self.form = Some(Form::of(record.family, &record.name));
            self.image = record.image.clone();
            self.media_type = record.media_type.clone();
        }
        self.supply = u64::try_from(supply).expect("supply is above zero and below u64::MAX here");
        true
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
            let (entry, next) = entry_from(self.bytes, at, self.header.version)?;
            out.push(entry);
            at = next;
        }
        Ok(out)
    }
}

/// Parse the entry at `at` — the paged walk's step. `version` is a parsed
/// header's, which is what selects the entry layout (v2 entries state no
/// form).
///
/// `at` must be an entry boundary: [`LISTING_HEADER_LEN`] at the front of the
/// object, or a previous call's returned offset. `Ok(None)` means `at` is
/// exactly the end of `bytes`; a shortfall mid-entry is
/// [`DecodeError::Truncated`] — how a reader walking a window learns the
/// entry runs past it.
pub fn entry_at(
    bytes: &[u8],
    at: usize,
    version: u8,
) -> Result<Option<(ListingEntry, usize)>, DecodeError> {
    if at == bytes.len() {
        return Ok(None);
    }
    entry_from(bytes, at, version).map(Some)
}

/// The entry parse `entries` and `entry_at` share.
fn entry_from(bytes: &[u8], at: usize, version: u8) -> Result<(ListingEntry, usize), DecodeError> {
    let mut cursor = at;
    let form = if version == LISTING_VERSION {
        Some(Form::try_from(u8_at(bytes, &mut cursor)?)?)
    } else {
        None
    };
    let name_len = u8_at(bytes, &mut cursor)? as usize;
    let name = take(bytes, &mut cursor, name_len)?.to_vec();
    let supply = u64_at(bytes, &mut cursor)?;
    let image = take_u16_prefixed(bytes, &mut cursor)?.to_vec();
    let media_type = take_u16_prefixed(bytes, &mut cursor)?.to_vec();
    Ok((
        ListingEntry {
            form,
            name,
            supply,
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
        // Every v3 entry states its form; a v2-parsed entry cannot be
        // re-encoded (the reseed rebuilds those from the segments).
        if entries.iter().any(|entry| entry.form.is_none()) {
            return Err(EncodeError::MissingForm);
        }
        let asset_count = u32::try_from(entries.len()).map_err(|_| EncodeError::FieldTooLong {
            field: "asset_count",
            len: entries.len(),
        })?;
        let kept = entries
            .iter()
            .filter(|entry| entry.form != Some(Form::Cip68Reference))
            .count();
        let display_count = u32::try_from(kept).map_err(|_| EncodeError::FieldTooLong {
            field: "display_count",
            len: kept,
        })?;
        let header = ListingHeader {
            version: LISTING_VERSION,
            network: self.network,
            policy_id: self.policy_id,
            built_through: self.built_through,
            asset_count,
            display_count: Some(display_count),
        };
        let mut out = Vec::new();
        out.extend_from_slice(&header.encode()?);
        for entry in &entries {
            let form = entry
                .form
                .expect("the form check above refused every entry without one");
            out.push(form.into());
            let name_len =
                u8::try_from(entry.name.len()).map_err(|_| EncodeError::FieldTooLong {
                    field: "name",
                    len: entry.name.len(),
                })?;
            out.push(name_len);
            out.extend_from_slice(&entry.name);
            out.extend_from_slice(&entry.supply.to_le_bytes());
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
            form: Some(Form::Cip25),
            name: name.to_vec(),
            supply: 1,
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
    /// change and must be deliberate. v3: the version byte, the entry form
    /// bytes and the header's `display_count`.
    #[test]
    fn a_listing_encodes_to_the_committed_bytes() {
        let expected = unhex(
            "414d584c 03 00 0000
             93a06006
             00000000 00000000 00000000 00000000 00000000 00000000
             3412 02000000 02000000
             00000000 00000000 00000000 00000000 0000
             01 01 41 0100000000000000 0800 697066733a2f2f78 0900 696d6167652f706e67
             01 01 42 0100000000000000 0000 0000",
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
        assert_eq!(view.header.version, LISTING_VERSION);
        assert_eq!(view.header.network, NETWORK_MAINNET);
        assert_eq!(view.header.policy_id, policy());
        assert_eq!(view.header.built_through, 0x1234);
        assert_eq!(view.header.asset_count, 3);
        assert_eq!(view.header.display_count, Some(3));
        assert_eq!(
            view.entries().expect("entries"),
            vec![
                entry(b"A", b"ipfs://x", b"image/png"),
                entry(b"B", b"ipfs://y", b""),
                entry(b"C", b"", b""),
            ]
        );
    }

    /// The display count is the fold's one derived number: every entry that is
    /// not a CIP-68 reference token. The reference entries stay in the object
    /// — lookup by whatever form a caller holds — and the count says how many
    /// a collection view keeps. A v3 entry always states its form, and a v2
    /// entry never can.
    #[test]
    fn the_display_count_folds_out_reference_tokens() {
        let mut reference = entry(b"\x00\x06\x43\xb0USDM", b"", b"");
        reference.form = Some(Form::Cip68Reference);
        let mut user = entry(b"\x00\x14\xdf\x10USDM", b"", b"");
        user.form = Some(Form::Cip68User);
        user.supply = 13_579_705_760_000;
        let bytes = builder(vec![reference.clone(), user.clone()])
            .encode()
            .expect("encodes");
        let view = ListingView::parse(&bytes).expect("parses");
        assert_eq!(view.header.asset_count, 2);
        assert_eq!(view.header.display_count, Some(1));
        assert_eq!(view.entries().expect("entries"), vec![reference, user]);

        // A listing parsed from a v2 object states no form — and so cannot be
        // re-encoded: every write is the current layout, and a form cannot be
        // guessed back from a name.
        let v2 = legacy_v2_bytes(false);
        let view = ListingView::parse(&v2).expect("parses");
        assert_eq!(view.header.version, LISTING_V2);
        assert_eq!(view.header.display_count, None);
        assert!(
            view.entries()
                .expect("entries")
                .iter()
                .all(|entry| entry.form.is_none())
        );
        assert_eq!(
            builder(view.entries().expect("entries")).encode(),
            Err(EncodeError::MissingForm)
        );
    }

    /// A hand-built v2 object: the layout the reseed is rewriting — `supply`,
    /// no form byte, no `display_count`. `with_reference_name` picks the entry
    /// name so the v2/v3 entry walk can be compared byte for byte.
    fn legacy_v2_bytes(with_reference_name: bool) -> Vec<u8> {
        let mut out = vec![0u8; LISTING_HEADER_LEN];
        out[0..4].copy_from_slice(&LISTING_MAGIC);
        out[4] = LISTING_V2;
        out[8..36].copy_from_slice(&policy());
        out[36..38].copy_from_slice(&0x1234u16.to_le_bytes());
        out[38..42].copy_from_slice(&1u32.to_le_bytes());
        // No display_count on a v2 header — bytes 42..64 stay reserved.
        let name: &[u8] = if with_reference_name {
            b"\x00\x06\x43\xb0USDM"
        } else {
            b"A"
        };
        out.push(name.len() as u8);
        out.extend_from_slice(name);
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// The supply is per entry and stated exactly: a native token's copy count
    /// is one entry with a supply, never the entry repeated.
    #[test]
    fn a_listing_entry_states_its_supply() {
        let mut many = entry(b"CNT", b"", b"");
        many.supply = 10_000;
        let bytes = builder(vec![many.clone(), entry(b"ONE", b"", b"")])
            .encode()
            .expect("encodes");
        let view = ListingView::parse(&bytes).expect("parses");
        assert_eq!(
            view.entries().expect("entries"),
            vec![many, entry(b"ONE", b"", b"")]
        );
        let (parsed, next) = entry_at(&bytes, LISTING_HEADER_LEN, LISTING_VERSION)
            .expect("parses")
            .expect("an entry");
        assert_eq!(parsed.name, b"CNT".to_vec());
        assert_eq!(parsed.supply, 10_000);
        assert!(next > LISTING_HEADER_LEN);
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
        short.extend_from_slice(&[LISTING_VERSION, NETWORK_MAINNET]);
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

        // The superseded v1 layout states no entry supply, so it is refused
        // rather than misread — and so is a version from the future. v2 still
        // parses: the reseed is rewriting the tree in place, and the reader
        // must keep serving what it has not reached.
        let mut v1 = bytes.clone();
        v1[4] = 1;
        assert_eq!(
            ListingHeader::parse(&v1),
            Err(DecodeError::UnsupportedVersion(1))
        );
        let mut future = bytes.clone();
        future[4] = 4;
        assert_eq!(
            ListingHeader::parse(&future),
            Err(DecodeError::UnsupportedVersion(4))
        );
        let v2 = legacy_v2_bytes(false);
        assert_eq!(
            ListingHeader::parse(&v2).expect("parses").version,
            LISTING_V2
        );

        let mut reserved = bytes.clone();
        reserved[6] = 1;
        assert!(matches!(
            ListingHeader::parse(&reserved),
            Err(DecodeError::ReservedNotZero { .. })
        ));

        let mut tail = bytes;
        tail[46] = 1;
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
        while let Some((entry, next)) = entry_at(&bytes, at, LISTING_VERSION).expect("walks") {
            walked.push(entry);
            at = next;
        }
        assert_eq!(walked, view.entries().expect("entries"));
        assert_eq!(at, bytes.len());

        // From the second entry's own boundary the walk starts there.
        let (_, second) = entry_at(&bytes, LISTING_HEADER_LEN, LISTING_VERSION)
            .expect("walks")
            .expect("an entry");
        let (entry, _) = entry_at(&bytes, second, LISTING_VERSION)
            .expect("walks")
            .expect("an entry");
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
        let (_, second) = entry_at(&bytes, LISTING_HEADER_LEN, LISTING_VERSION)
            .expect("walks")
            .expect("an entry");
        let cut = &bytes[..bytes.len() - 1];
        assert!(matches!(
            entry_at(cut, second, LISTING_VERSION),
            Err(DecodeError::Truncated { .. })
        ));
        // Exactly at the end there is nothing left to parse.
        assert_eq!(
            entry_at(&bytes, bytes.len(), LISTING_VERSION).expect("walks"),
            None
        );
    }

    #[test]
    fn the_max_entry_len_is_reachable() {
        let max = ListingEntry {
            form: Some(Form::Bare),
            name: vec![b'a'; 255],
            supply: 1,
            image: vec![b'i'; 65_535],
            media_type: vec![b'm'; 65_535],
        };
        let bytes = builder(vec![max]).encode().expect("encodes");
        let (entry, next) = entry_at(&bytes, LISTING_HEADER_LEN, LISTING_VERSION)
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
            qty: 1,
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
            ListingEntry::from_record(&record, 1),
            entry(b"NFT", b"ipfs://x", b"image/png")
        );
    }

    /// The form the builder writes is a fact, not a guess: the record's family
    /// and — for CIP-68 — the reference label split the name once, at build
    /// time, so no reader parses a prefix. The fail-open side is deliberate:
    /// a labelled name that is *not* the reference is a form a holder can
    /// hold, shown rather than hidden.
    #[test]
    fn a_form_is_classified_from_the_label_once() {
        let bare = ListingEntry::from_record(&record(Family::None, b"bare"), 1);
        assert_eq!(bare.form, Some(Form::Bare));
        let cip25 = ListingEntry::from_record(&record(Family::Cip25, b"NFT"), 1);
        assert_eq!(cip25.form, Some(Form::Cip25));

        let reference =
            ListingEntry::from_record(&record(Family::Cip68, b"\x00\x06\x43\xb0USDM"), 1);
        assert_eq!(reference.form, Some(Form::Cip68Reference));
        for prefix in [&b"\x00\x14\xdf\x10"[..], &b"\x00\x0d\xe1\x40"[..]] {
            let mut name = prefix.to_vec();
            name.extend_from_slice(b"USDM");
            let user = ListingEntry::from_record(&record(Family::Cip68, &name), 1);
            assert_eq!(user.form, Some(Form::Cip68User), "{prefix:?}");
        }
        // A CIP-68 name carrying an unknown label is shown, not hidden.
        let other = ListingEntry::from_record(&record(Family::Cip68, b"no-label"), 1);
        assert_eq!(other.form, Some(Form::Cip68User));

        // A burn never rewrites the form: the standing metadata is the newest
        // *non-burn* record, and a burn arrives with none of its own.
        let mut entry =
            ListingEntry::from_record(&record(Family::Cip68, b"\x00\x06\x43\xb0USDM"), 3);
        assert!(entry.apply(&record(Family::None, b"\x00\x06\x43\xb0USDM")));
        assert_eq!(entry.form, Some(Form::Cip68Reference));
    }

    fn record(family: Family, name: &[u8]) -> Record {
        Record {
            family,
            event: Event::Burn,
            qty: -1,
            policy_id: policy(),
            provenance: Provenance {
                chunk: 7,
                body_offset: 10,
                body_len: 500,
                aux_offset: 0,
                aux_len: 0,
            },
            name: name.to_vec(),
            image: Vec::new(),
            media_type: Vec::new(),
            media: Vec::new(),
            payload: Vec::new(),
        }
    }

    /// The write-side twin of `standing`: the same sum, the same floor, the
    /// same metadata rule — so a listing a sweep emits and the answer a walk
    /// gives cannot disagree.
    #[test]
    fn applying_records_sums_quantities_and_floors_at_zero() {
        fn event(event: Event, qty: i64, image: &[u8]) -> Record {
            Record {
                family: Family::Cip25,
                event,
                qty,
                policy_id: policy(),
                provenance: Provenance {
                    chunk: 1,
                    body_offset: 0,
                    body_len: 1,
                    aux_offset: 0,
                    aux_len: 0,
                },
                name: b"A".to_vec(),
                image: image.to_vec(),
                media_type: b"image/png".to_vec(),
                media: Vec::new(),
                payload: Vec::new(),
            }
        }

        // Five copies minted, two burned: three stand, and a burn does not
        // clear the art.
        let mut entry = ListingEntry::from_record(&event(Event::Mint, 5, b"ipfs://a"), 5);
        assert!(entry.apply(&event(Event::Burn, -2, b"")));
        assert_eq!(entry.supply, 3);
        assert_eq!(entry.image, b"ipfs://a");

        // The replacement mint's metadata wins, and the burn of the previous
        // copy after it does not clear the entry.
        assert!(entry.apply(&event(Event::Mint, 1, b"ipfs://b")));
        assert!(entry.apply(&event(Event::Burn, -1, b"")));
        assert_eq!(entry.supply, 3);
        assert_eq!(entry.image, b"ipfs://b");

        // A burn past the accounted copies floors at zero: no longer standing.
        // The caller drops the entry — its state after a `false` is not
        // promised, the standing set is.
        assert!(!entry.apply(&event(Event::Burn, -3, b"")));
    }
}
