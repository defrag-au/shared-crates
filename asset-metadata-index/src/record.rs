//! One resolved record: an asset's metadata as a defining event stated it.
//!
//! # The payload is the chain's document
//!
//! `payload` is the **raw on-chain metadata document** — for CIP-25 the
//! asset's own `721` entry, for CIP-68 the reference datum — exactly the bytes
//! the ledger holds. Re-modelling it here would create a second schema to
//! drift and would drop every field the model forgot; keeping it raw means
//! decoding is one call to `cardano-assets` (`cip25_metadata_value` /
//! `cip68::decode_cip68_datum`), the same functions the live modules already
//! run. `family` selects the decoder — callers never do.
//!
//! # Never truncate
//!
//! Every writer in this module returns [`crate::EncodeError`] rather than
//! shortening a value the format cannot state. A truncated name parses back
//! as a shorter, plausible name — the one output this format must not
//! produce.
//!
//! # Standing is a supply
//!
//! Records are one-per-defining-event, appended across segments, each stating
//! the signed quantity its transaction moved. An asset stands while those
//! quantities **sum above zero** ([`standing`]): a mint adds, a burn removes,
//! an update moves none. The metadata in force is the greatest
//! `(chunk, body_offset)` among the non-burn records — chain order, with the
//! within-chunk position as the tie-break.
//!
//! Summing rather than "latest wins" is load-bearing, not fastidious. A burn
//! is a supply decrement, not a deletion: CIP-25's dynamic-update trick mints
//! the new copy and *then* burns the previous one, so the newest record is a
//! burn while the asset still exists — and for a native token a quantity is
//! the only answer there is.
//!
//! The sum runs in chain order and **floors at zero**: the index never saw a
//! burn's copy minted if the mint predates its coverage, and such a burn must
//! not erase an asset (it can only fail to restore one).

use crate::POLICY_ID_LEN;
use crate::bytes::{
    array_at, i64_at, put_u16_prefixed, put_u32_prefixed, take, take_u16_prefixed,
    take_u32_prefixed, u8_at, u16_at, u32_at,
};
use crate::error::{DecodeError, EncodeError};

/// Fixed head length: everything before the first variable-length field.
///
/// `record_len` 4 · `family` 1 · `event` 1 · flags 2 · `qty` 8 ·
/// `policy_id` 28 · provenance 14 · `name_len` 1.
pub const RECORD_HEAD_LEN: usize = 59;

/// Which metadata standard the chain stated. Informational — stored so the
/// data is complete, never branched on by a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Family {
    /// Nothing standard was found on this mint (a bare token, or metadata
    /// the decoder did not recognise). The record still exists, so "minted
    /// with no metadata" is distinguishable from "not minted".
    None = 0,
    /// CIP-25: metadata in the mint transaction's auxiliary data.
    Cip25 = 1,
    /// CIP-68: metadata in the reference token's datum.
    Cip68 = 2,
}

impl TryFrom<u8> for Family {
    type Error = DecodeError;

    fn try_from(b: u8) -> Result<Self, DecodeError> {
        Ok(match b {
            0 => Self::None,
            1 => Self::Cip25,
            2 => Self::Cip68,
            other => return Err(DecodeError::BadFamily(other)),
        })
    }
}

impl From<Family> for u8 {
    fn from(f: Family) -> Self {
        f as Self
    }
}

/// The kind of defining event a record captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Event {
    /// Created, or re-created after a burn.
    Mint = 1,
    /// Destroyed. Clears the asset when latest.
    Burn = 2,
    /// The metadata moved without a mint or burn — a CIP-68 reference-token
    /// respend (reveal, trait shop, seasonal variant).
    Update = 3,
}

impl TryFrom<u8> for Event {
    type Error = DecodeError;

    fn try_from(b: u8) -> Result<Self, DecodeError> {
        Ok(match b {
            1 => Self::Mint,
            2 => Self::Burn,
            3 => Self::Update,
            other => return Err(DecodeError::BadEvent(other)),
        })
    }
}

impl From<Event> for u8 {
    fn from(e: Event) -> Self {
        e as Self
    }
}

/// Where the event's bytes sit in the Mithril chunk log.
///
/// For the verifier and for audit — deliberately **not** the read path. A
/// reader that needs the original transaction can find it from these five
/// numbers; the resolved fields are enough to serve without ever looking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provenance {
    /// Chunk number in the immutable log.
    pub chunk: u16,
    /// Offset of the transaction body within the chunk file.
    pub body_offset: u32,
    /// Length of the transaction body.
    pub body_len: u16,
    /// Offset of the auxiliary data within the chunk file; copy of
    /// `policy-index`'s span convention (`0` when absent).
    pub aux_offset: u32,
    /// Length of the auxiliary data (`0` when absent).
    pub aux_len: u16,
}

/// One entry of the asset's media — a CIP-25/68 `files[]` element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLink {
    /// The link as the chain states it (`ipfs://…`, `https://…`).
    pub src: Vec<u8>,
    /// Zero-length means absent.
    pub media_type: Vec<u8>,
    /// Zero-length means absent.
    pub name: Vec<u8>,
}

/// An asset's metadata as one defining event stated it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Which standard the chain stated.
    pub family: Family,
    /// Mint, burn or update.
    pub event: Event,
    /// The signed quantity this event moved: positive for a mint, negative for
    /// a burn, zero for an update. Stored as the ledger states it — the sum
    /// over a name's records is what decides standing.
    pub qty: i64,
    /// 28-byte policy id.
    pub policy_id: [u8; POLICY_ID_LEN],
    /// Where the event's bytes are, for the verifier.
    pub provenance: Provenance,
    /// The asset name **exactly as minted**, raw bytes.
    pub name: Vec<u8>,
    /// The image URI as the chain states it; empty when none.
    pub image: Vec<u8>,
    /// Top-level media type; empty when none.
    pub media_type: Vec<u8>,
    /// `files[]` beyond the image.
    pub media: Vec<MediaLink>,
    /// The raw on-chain metadata document; empty when none.
    pub payload: Vec<u8>,
}

impl Record {
    /// The record's byte length once encoded, without encoding it.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        RECORD_HEAD_LEN
            + self.name.len()
            + 2
            + self.image.len()
            + 2
            + self.media_type.len()
            + 2
            + self
                .media
                .iter()
                .map(|m| 6 + m.src.len() + m.media_type.len() + m.name.len())
                .sum::<usize>()
            + 4
            + self.payload.len()
    }

    /// Encode to a fresh buffer.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let mut out = Vec::with_capacity(self.encoded_len());
        self.encode_into(&mut out)?;
        Ok(out)
    }

    /// Append this record's bytes.
    pub fn encode_into(&self, out: &mut Vec<u8>) -> Result<(), EncodeError> {
        // The sign is the event's. A quantity that contradicts it would move
        // a supply the wrong way, so it is refused rather than written.
        let sign_matches = match self.event {
            Event::Mint => self.qty > 0,
            Event::Burn => self.qty < 0,
            Event::Update => self.qty == 0,
        };
        if !sign_matches {
            return Err(EncodeError::BadQuantity {
                event: self.event,
                qty: self.qty,
            });
        }
        let start = out.len();
        out.extend_from_slice(&[0; 4]); // record_len, patched last
        out.push(u8::from(self.family));
        out.push(u8::from(self.event));
        out.extend_from_slice(&[0; 2]); // flags, reserved
        out.extend_from_slice(&self.qty.to_le_bytes());
        out.extend_from_slice(&self.policy_id);
        out.extend_from_slice(&self.provenance.chunk.to_le_bytes());
        out.extend_from_slice(&self.provenance.body_offset.to_le_bytes());
        out.extend_from_slice(&self.provenance.body_len.to_le_bytes());
        out.extend_from_slice(&self.provenance.aux_offset.to_le_bytes());
        out.extend_from_slice(&self.provenance.aux_len.to_le_bytes());
        let name_len = u8::try_from(self.name.len()).map_err(|_| EncodeError::FieldTooLong {
            field: "name",
            len: self.name.len(),
        })?;
        out.push(name_len);
        out.extend_from_slice(&self.name);
        put_u16_prefixed(out, "image", &self.image)?;
        put_u16_prefixed(out, "media_type", &self.media_type)?;
        let count = u16::try_from(self.media.len()).map_err(|_| EncodeError::FieldTooLong {
            field: "media_count",
            len: self.media.len(),
        })?;
        out.extend_from_slice(&count.to_le_bytes());
        for m in &self.media {
            put_u16_prefixed(out, "media.src", &m.src)?;
            put_u16_prefixed(out, "media.media_type", &m.media_type)?;
            put_u16_prefixed(out, "media.name", &m.name)?;
        }
        put_u32_prefixed(out, "payload", &self.payload)?;
        let total = u32::try_from(out.len() - start).map_err(|_| EncodeError::FieldTooLong {
            field: "record",
            len: out.len() - start,
        })?;
        out[start..start + 4].copy_from_slice(&total.to_le_bytes());
        Ok(())
    }

    /// Decode one record from the front of `bytes`. Returns the record and how
    /// many bytes it consumed.
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), DecodeError> {
        let mut at = 0usize;
        let declared = u32_at(bytes, &mut at)? as usize;
        if declared < RECORD_HEAD_LEN || declared > bytes.len() {
            return Err(DecodeError::Truncated {
                at: 0,
                want: declared,
            });
        }
        let b = &bytes[..declared];
        let family = Family::try_from(u8_at(b, &mut at)?)?;
        let event = Event::try_from(u8_at(b, &mut at)?)?;
        let flags = u16_at(b, &mut at)?;
        if flags != 0 {
            return Err(DecodeError::ReservedNotZero { at: at - 2 });
        }
        // Read permissively: a sign that contradicts the event is still a
        // number a reader can sum honestly, and refusing it would fail a
        // whole segment over one bad record. The builder refuses it instead
        // (`EncodeError::BadQuantity`).
        let qty = i64_at(b, &mut at)?;
        let policy_id = array_at::<POLICY_ID_LEN>(b, &mut at)?;
        let provenance = Provenance {
            chunk: u16_at(b, &mut at)?,
            body_offset: u32_at(b, &mut at)?,
            body_len: u16_at(b, &mut at)?,
            aux_offset: u32_at(b, &mut at)?,
            aux_len: u16_at(b, &mut at)?,
        };
        let name_len = u8_at(b, &mut at)? as usize;
        let name = take(b, &mut at, name_len)?.to_vec();
        let image = take_u16_prefixed(b, &mut at)?.to_vec();
        let media_type = take_u16_prefixed(b, &mut at)?.to_vec();
        let media_count = u16_at(b, &mut at)?;
        let mut media = Vec::with_capacity(media_count as usize);
        for _ in 0..media_count {
            let src = take_u16_prefixed(b, &mut at)?.to_vec();
            let media_type = take_u16_prefixed(b, &mut at)?.to_vec();
            let name = take_u16_prefixed(b, &mut at)?.to_vec();
            media.push(MediaLink {
                src,
                media_type,
                name,
            });
        }
        let payload = take_u32_prefixed(b, &mut at)?.to_vec();
        if at != declared {
            return Err(DecodeError::RecordLengthMismatch {
                declared,
                consumed: at,
            });
        }
        Ok((
            Self {
                family,
                event,
                qty,
                policy_id,
                provenance,
                name,
                image,
                media_type,
                media,
                payload,
            },
            declared,
        ))
    }
}

/// Iterator over back-to-back records in a slice. Stops at the first refusal
/// — a failed record has no trustworthy length, so continuing would guess.
pub struct Records<'a> {
    bytes: &'a [u8],
}

impl Iterator for Records<'_> {
    type Item = Result<Record, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }
        match Record::decode(self.bytes) {
            Ok((record, consumed)) => {
                self.bytes = &self.bytes[consumed..];
                Some(Ok(record))
            }
            Err(e) => {
                self.bytes = &[];
                Some(Err(e))
            }
        }
    }
}

/// Iterate the records in a policy's slice.
#[must_use]
pub fn records(slice: &[u8]) -> Records<'_> {
    Records { bytes: slice }
}

/// The chain-order key: chunk first, then position within it. Strictly the
/// defining events' order, which is what "latest" means.
#[must_use]
pub fn recency(record: &Record) -> (u16, u32) {
    (record.provenance.chunk, record.provenance.body_offset)
}

/// An asset's current state, decided by [`standing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing<'a> {
    /// Supply is above zero: the asset exists. `record` is the newest mint or
    /// update — the metadata in force.
    Active {
        /// The newest non-burn record: where the metadata comes from.
        record: &'a Record,
        /// The copies the index can account for; above zero here.
        supply: i64,
    },
    /// Supply is zero: the asset is gone. `record` is the newest record of any
    /// kind — on a complete history, the burn that took the last copy.
    Burned {
        /// The newest record.
        record: &'a Record,
        /// The copies the index can account for; zero here (the sum floors
        /// there).
        supply: i64,
    },
}

/// Which of an asset's records is in force, and how many copies the index can
/// account for: quantities summed **in chain order**, floored at zero, with the
/// newest non-burn record carrying the metadata.
///
/// A burn decrements, so it clears the asset only when it takes the last
/// accounted copy. A CIP-25 dynamic update — mint the new copy, then burn the
/// old — therefore stays standing and shows the new copy's metadata. A burn
/// with no minted copy behind it (a mint outside the build's coverage) floors
/// at zero rather than going negative: an incomplete history may understate a
/// supply, it must never erase an asset. The same rule for both families.
///
/// Records are ordered here defensively — the walk and the emitter both
/// produce chain order — so the answer does not depend on the caller's
/// iteration order. Ties cannot happen for one asset (two events at the same
/// `(chunk, offset)` would be the same transaction); the last wins if one
/// ever does.
#[must_use]
pub fn standing<'a, I>(records: I) -> Option<Standing<'a>>
where
    I: IntoIterator<Item = &'a Record>,
{
    let mut ordered: Vec<&'a Record> = records.into_iter().collect();
    ordered.sort_by_key(|record| recency(record));

    let mut supply: i64 = 0;
    let mut newest_non_burn: Option<&'a Record> = None;
    for record in &ordered {
        // Saturating, not wrapping: corrupt data must not flip a standing
        // asset out of its listing by overflowing the sum.
        supply = supply.saturating_add(record.qty).max(0);
        if record.event != Event::Burn {
            newest_non_burn = Some(record);
        }
    }
    let newest = ordered.last().copied()?;
    Some(match newest_non_burn {
        Some(record) if supply > 0 => Standing::Active { record, supply },
        // A positive supply with no mint or update behind it is bytes no
        // writer can produce; report it burned rather than claim metadata
        // that is not there.
        _ => Standing::Burned {
            record: newest,
            supply,
        },
    })
}

/// What a reader hands upward: the current state of one asset, owned — no
/// lifetimes for the caller to carry past the fetch.
///
/// The `payload` stays the chain's CBOR; decoding it into traits is one call
/// to `cardano-assets` (`cip25_metadata_value` / `cip68::decode_cip68_datum`,
/// selected by `family`), made where a caller actually wants traits. This
/// crate never decodes it, so the format has no metadata schema to drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAsset {
    /// The asset's policy.
    pub policy_id: [u8; POLICY_ID_LEN],
    /// The asset name, exactly as minted.
    pub name: Vec<u8>,
    /// Which standard the chain stated. Informational.
    pub family: Family,
    /// True when the asset is not standing — the index accounts for no copies
    /// of it. The media fields are empty for a cleared asset.
    pub burned: bool,
    /// The copies the index can account for: quantities summed in chain order,
    /// floored at zero. Above zero for a standing asset (its supply); zero
    /// once cleared.
    pub supply: i64,
    /// The defining event the answer came from.
    pub event: Event,
    /// The image URI; empty when none.
    pub image: Vec<u8>,
    /// Top-level media type; empty when none.
    pub media_type: Vec<u8>,
    /// `files[]` beyond the image.
    pub media: Vec<MediaLink>,
    /// The raw on-chain metadata document; empty when none.
    pub payload: Vec<u8>,
    /// Where the event's bytes are.
    pub provenance: Provenance,
    /// The build coverage the answer came from (the pointer's
    /// `built_through`), so a caller can state how fresh it is.
    pub as_of_chunk: u16,
}

impl ResolvedAsset {
    /// Materialise the winner of [`standing`].
    #[must_use]
    pub fn from_record(record: &Record, supply: i64, as_of_chunk: u16) -> Self {
        Self {
            policy_id: record.policy_id,
            name: record.name.clone(),
            family: record.family,
            // Standing is the supply, not the newest event: a burn that took
            // one of several copies leaves the asset in place.
            burned: supply == 0,
            supply,
            event: record.event,
            image: record.image.clone(),
            media_type: record.media_type.clone(),
            media: record.media.clone(),
            payload: record.payload.clone(),
            provenance: record.provenance,
            as_of_chunk,
        }
    }
}

/// The whole resolve, in one call: sum the quantities, take the metadata in
/// force, materialise it. `None` when the asset has no records at all.
#[must_use]
pub fn resolve<'a, I>(records: I, as_of_chunk: u16) -> Option<ResolvedAsset>
where
    I: IntoIterator<Item = &'a Record>,
{
    let (record, supply) = match standing(records)? {
        Standing::Active { record, supply } | Standing::Burned { record, supply } => {
            (record, supply)
        }
    };
    Some(ResolvedAsset::from_record(record, supply, as_of_chunk))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(clean.len().is_multiple_of(2), "odd hex length");
        (0..clean.len() / 2)
            .map(|i| u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16).expect("hex"))
            .collect()
    }

    fn policy(byte: u8) -> [u8; POLICY_ID_LEN] {
        [byte; POLICY_ID_LEN]
    }

    fn sample() -> Record {
        Record {
            family: Family::Cip25,
            event: Event::Mint,
            qty: 1,
            policy_id: policy(0xAA),
            provenance: Provenance {
                chunk: 7,
                body_offset: 1000,
                body_len: 500,
                aux_offset: 2000,
                aux_len: 300,
            },
            name: b"NFT".to_vec(),
            image: b"ipfs://x".to_vec(),
            media_type: b"image/png".to_vec(),
            media: vec![MediaLink {
                src: b"https://a".to_vec(),
                media_type: Vec::new(),
                name: Vec::new(),
            }],
            payload: b"xyz".to_vec(),
        }
    }

    /// BYTE STABILITY. The committed bytes are the contract the box-side
    /// builder and the edge-side reader meet at; a change here is a format
    /// change and must be deliberate.
    #[test]
    fn a_record_encodes_to_the_committed_bytes() {
        let expected = unhex(
            "6b000000 01 01 0000 0100000000000000
             aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
             0700 e8030000 f401 d0070000 2c01
             03 4e4654
             0800 697066733a2f2f78
             0900 696d6167652f706e67
             0100
                 0900 68747470733a2f2f61 0000 0000
             03000000 78797a",
        );
        assert_eq!(sample().encode().expect("encodes"), expected);
        assert_eq!(sample().encoded_len(), expected.len());
    }

    #[test]
    fn a_record_round_trips_with_every_field() {
        let record = sample();
        let bytes = record.encode().expect("encodes");
        let (back, consumed) = Record::decode(&bytes).expect("decodes");
        assert_eq!(back, record);
        assert_eq!(consumed, bytes.len());
    }

    /// Empty optional fields are representable: a burn has no image, media or
    /// payload, and the shape still round-trips.
    #[test]
    fn a_burn_round_trips_with_empty_optional_fields() {
        let record = Record {
            family: Family::None,
            event: Event::Burn,
            qty: -1,
            policy_id: policy(0x01),
            provenance: Provenance {
                chunk: 1,
                body_offset: 0,
                body_len: 1,
                aux_offset: 0,
                aux_len: 0,
            },
            name: vec![0x00, 0x06, 0x43, 0xb0, b'x'],
            image: Vec::new(),
            media_type: Vec::new(),
            media: Vec::new(),
            payload: Vec::new(),
        };
        let bytes = record.encode().expect("encodes");
        let (back, _) = Record::decode(&bytes).expect("decodes");
        assert_eq!(back, record);
    }

    #[test]
    fn names_longer_than_u8_are_refused_not_truncated() {
        let mut record = sample();
        record.name = vec![b'x'; 256];
        assert!(matches!(
            record.encode(),
            Err(EncodeError::FieldTooLong { field: "name", .. })
        ));
    }

    #[test]
    fn a_truncated_record_refuses() {
        let bytes = sample().encode().expect("encodes");
        for cut in [0usize, 3, 10, RECORD_HEAD_LEN, bytes.len() - 1] {
            assert!(
                Record::decode(&bytes[..cut]).is_err(),
                "decoding {cut} bytes must fail"
            );
        }
    }

    #[test]
    fn an_unknown_family_or_event_refuses() {
        let bytes = sample().encode().expect("encodes");
        let mut bad_family = bytes.clone();
        bad_family[4] = 9;
        assert!(matches!(
            Record::decode(&bad_family),
            Err(DecodeError::BadFamily(9))
        ));
        let mut bad_event = bytes.clone();
        bad_event[5] = 9;
        assert!(matches!(
            Record::decode(&bad_event),
            Err(DecodeError::BadEvent(9))
        ));
    }

    #[test]
    fn records_iterates_back_to_back_and_stops_at_a_refusal() {
        let mut buffer = Vec::new();
        sample().encode_into(&mut buffer).expect("encodes");
        sample().encode_into(&mut buffer).expect("encodes");
        let count = records(&buffer).count();
        assert_eq!(count, 2);

        buffer.extend_from_slice(&[0xFF, 0x00]); // a record_len that cannot fit
        let items: Vec<_> = records(&buffer).collect();
        assert_eq!(items.len(), 3);
        assert!(items[2].is_err());
    }

    /// A quantity per record, summed: the metadata in force is the newest
    /// non-burn, and a burn clears only the copy it took.
    #[test]
    fn standing_sums_quantities_and_keeps_the_newest_metadata() {
        let mut mint = sample();
        mint.provenance = Provenance {
            chunk: 10,
            body_offset: 100,
            body_len: 1,
            aux_offset: 0,
            aux_len: 0,
        };
        let mut update = mint.clone();
        update.event = Event::Update;
        update.qty = 0;
        update.image = b"ipfs://updated".to_vec();
        update.provenance.body_offset = 200;

        // One copy, updated in place: the update's metadata is in force.
        match standing([&mint, &update]) {
            Some(Standing::Active { record, supply }) => {
                assert_eq!(supply, 1);
                assert_eq!(record.image, b"ipfs://updated");
            }
            other => panic!("expected active on the update, got {other:?}"),
        }

        // Burning that copy clears the asset.
        let mut burn = update.clone();
        burn.event = Event::Burn;
        burn.qty = -1;
        burn.image = Vec::new();
        burn.provenance.chunk = 11;
        match standing([&mint, &update, &burn]) {
            Some(Standing::Burned { record, supply }) => {
                assert_eq!(supply, 0);
                assert_eq!(*record, burn);
            }
            other => panic!("expected burned, got {other:?}"),
        }
    }

    /// The CIP-25 dynamic-update trick: mint the new copy, then burn the old.
    /// The newest record is a burn and the asset still stands — one copy, with
    /// the new copy's metadata.
    #[test]
    fn a_burn_after_a_replacement_mint_leaves_the_asset_standing() {
        let mut previous = sample();
        previous.provenance = Provenance {
            chunk: 10,
            body_offset: 100,
            body_len: 1,
            aux_offset: 0,
            aux_len: 0,
        };
        previous.image = b"ipfs://season-1".to_vec();
        let mut replacement = previous.clone();
        replacement.provenance.chunk = 11;
        replacement.image = b"ipfs://season-2".to_vec();
        let mut burn = replacement.clone();
        burn.event = Event::Burn;
        burn.qty = -1;
        burn.image = Vec::new();
        burn.provenance.chunk = 12;

        match standing([&previous, &replacement, &burn]) {
            Some(Standing::Active { record, supply }) => {
                assert_eq!(supply, 1);
                assert_eq!(record.image, b"ipfs://season-2");
            }
            other => panic!("expected one copy standing on the replacement, got {other:?}"),
        }
    }

    /// A native token: five copies minted, three burned, two still stand.
    #[test]
    fn a_partial_burn_leaves_the_remaining_copies_standing() {
        let mut mint = sample();
        mint.qty = 5;
        let mut burn = mint.clone();
        burn.event = Event::Burn;
        burn.qty = -3;
        burn.provenance.chunk = 11;
        match standing([&mint, &burn]) {
            Some(Standing::Active { supply, .. }) => assert_eq!(supply, 2),
            other => panic!("expected two copies standing, got {other:?}"),
        }
    }

    /// The builder refuses a quantity whose sign contradicts its event.
    #[test]
    fn a_quantity_that_contradicts_its_event_is_refused() {
        let mut mint = sample();
        mint.qty = 0;
        assert!(matches!(
            mint.encode(),
            Err(EncodeError::BadQuantity { qty: 0, .. })
        ));

        let mut burn = sample();
        burn.event = Event::Burn;
        burn.qty = 1;
        assert!(matches!(
            burn.encode(),
            Err(EncodeError::BadQuantity { qty: 1, .. })
        ));

        let mut update = sample();
        update.event = Event::Update;
        update.qty = -2;
        assert!(matches!(
            update.encode(),
            Err(EncodeError::BadQuantity { qty: -2, .. })
        ));
    }

    /// The tie-break is within-chunk position, not arrival order.
    #[test]
    fn standing_uses_within_chunk_order_for_the_metadata_in_force() {
        let mut first = sample();
        first.provenance = Provenance {
            chunk: 5,
            body_offset: 10,
            body_len: 1,
            aux_offset: 0,
            aux_len: 0,
        };
        let mut second = first.clone();
        second.provenance.body_offset = 20;
        second.event = Event::Update;
        second.qty = 0;
        match standing([&second, &first]) {
            Some(Standing::Active { record, .. }) => {
                assert_eq!(record.provenance.body_offset, 20)
            }
            other => panic!("expected offset 20 to win, got {other:?}"),
        }
    }

    /// The one-call read path: the metadata in force, materialised, with the
    /// standing supply and the build's coverage stated.
    #[test]
    fn resolve_materialises_the_metadata_in_force() {
        let mut first = sample();
        first.provenance = Provenance {
            chunk: 5,
            body_offset: 10,
            body_len: 1,
            aux_offset: 0,
            aux_len: 0,
        };
        assert!(resolve([] as [&Record; 0], 9).is_none());

        let mut second = first.clone();
        second.event = Event::Update;
        second.qty = 0;
        second.image = b"ipfs://new".to_vec();
        second.provenance.body_offset = 20;
        let got = resolve([&first, &second], 42).expect("resolves");
        assert_eq!(got.image, b"ipfs://new");
        assert!(!got.burned);
        assert_eq!(got.supply, 1);
        assert_eq!(got.event, Event::Update);
        assert_eq!(got.as_of_chunk, 42);

        let mut burn = second.clone();
        burn.event = Event::Burn;
        burn.qty = -1;
        burn.image = Vec::new();
        burn.provenance.chunk = 6;
        let got = resolve([&first, &second, &burn], 42).expect("resolves");
        assert!(got.burned);
        assert_eq!(got.supply, 0);
        assert!(got.image.is_empty());
    }
}
