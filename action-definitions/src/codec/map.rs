//! Structs are maps keyed by integer field id; enums are the same map with
//! the variant tag at key `0` (schema §2.2, §2.3).
//!
//! Three properties this module exists to guarantee:
//!
//! 1. **Encoding is canonical.** Fields are written in ascending id order,
//!    so two encoders of the same value produce identical bytes and the
//!    verifier can compare them.
//! 2. **Absent means default.** A field with a declared default is simply
//!    not written, which is what keeps a datum small and what lets a field
//!    be added later without touching an older encoder's output.
//! 3. **Unknown means preserved.** Ids this build does not know are carried
//!    in [`UnknownFields`] and written back in their right place, so an old
//!    reader re-emits a newer datum intact rather than silently deleting
//!    the parts it could not read.

use std::collections::BTreeMap;

use pallas_primitives::PlutusData;

use super::DecodeError;
use super::scalar::{PlutusCodec, as_i128, as_map, map_data};

/// Field ids a decode did not recognise, with their values.
///
/// Kept sorted by id so a re-encode is canonical regardless of the order
/// they arrived in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnknownFields(Vec<(i64, PlutusData)>);

impl UnknownFields {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.0.iter().map(|(id, _)| *id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(i64, PlutusData)> {
        self.0.iter()
    }

    fn push(&mut self, id: i64, value: PlutusData) {
        self.0.push((id, value));
    }
}

/// Serde skips this entirely — it is an on-chain concern (schema §2.7).
impl serde::Serialize for UnknownFields {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_unit()
    }
}

impl<'de> serde::Deserialize<'de> for UnknownFields {
    fn deserialize<D: serde::Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Ok(Self::default())
    }
}

// ── writing ────────────────────────────────────────────────────────────────

/// Builds one integer-keyed map.
#[derive(Debug, Default)]
pub struct MapWriter {
    entries: Vec<(i64, PlutusData)>,
}

impl MapWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A required field, always written.
    pub fn field<T: PlutusCodec>(&mut self, id: i64, value: &T) -> &mut Self {
        self.entries.push((id, value.to_data()));
        self
    }

    /// The variant tag of an enum — key `0` by the format's rule, which is
    /// why variant field ids start at 1.
    pub fn tag(&mut self, tag: i64) -> &mut Self {
        self.entries
            .push((0, super::scalar::int_data(i128::from(tag))));
        self
    }

    /// `None` is absence, never an explicit null (§2.4).
    pub fn opt<T: PlutusCodec>(&mut self, id: i64, value: &Option<T>) -> &mut Self {
        if let Some(inner) = value {
            self.entries.push((id, inner.to_data()));
        }
        self
    }

    /// A field with a declared default is written only when it differs.
    ///
    /// This is also how a list is written: `Vec<T>`'s default is empty, so
    /// an empty list is simply absent. There is deliberately no separate
    /// `list()` — one mechanism, and a `Vec<u8>` stays *bytes* (its own
    /// [`PlutusCodec`] impl) rather than becoming an array of integers.
    pub fn with_default<T: PlutusCodec + PartialEq>(
        &mut self,
        id: i64,
        value: &T,
        default: &T,
    ) -> &mut Self {
        if value != default {
            self.entries.push((id, value.to_data()));
        }
        self
    }

    /// Carry through what a previous reader did not understand.
    pub fn unknown(&mut self, unknown: &UnknownFields) -> &mut Self {
        for (id, value) in unknown.iter() {
            self.entries.push((*id, value.clone()));
        }
        self
    }

    /// Canonical: ascending id, one entry per id.
    pub fn finish(mut self) -> PlutusData {
        self.entries.sort_by_key(|(id, _)| *id);
        map_data(
            self.entries
                .into_iter()
                .map(|(id, value)| (super::scalar::int_data(i128::from(id)), value))
                .collect(),
        )
    }
}

// ── reading ────────────────────────────────────────────────────────────────

/// Reads one integer-keyed map, splitting known ids from unknown ones.
#[derive(Debug)]
pub struct MapReader {
    known: BTreeMap<i64, PlutusData>,
    unknown: UnknownFields,
}

impl MapReader {
    /// `known_ids` is the set this build understands; everything else is
    /// preserved. The derive macro passes the type's declared ids.
    pub fn new(data: &PlutusData, known_ids: &[i64]) -> Result<Self, DecodeError> {
        let entries = as_map(data)?;
        let mut known = BTreeMap::new();
        let mut unknown = UnknownFields::default();
        let mut seen = BTreeMap::<i64, ()>::new();

        for (key, value) in entries {
            let id = i64::try_from(as_i128(key)?).map_err(|_| DecodeError::OutOfRange {
                value: as_i128(key).unwrap_or_default(),
                target: "i64",
            })?;
            if seen.insert(id, ()).is_some() {
                return Err(DecodeError::DuplicateField { id });
            }
            if known_ids.contains(&id) {
                known.insert(id, value.clone());
            } else {
                unknown.push(id, value.clone());
            }
        }
        // The writer sorts, so a decode-then-encode of an unknown block is
        // canonical too.
        unknown.0.sort_by_key(|(id, _)| *id);

        Ok(Self { known, unknown })
    }

    /// The enum tag at key `0`.
    pub fn tag(&self) -> Result<i64, DecodeError> {
        let data = self.known.get(&0).ok_or(DecodeError::MissingTag)?;
        i64::try_from(as_i128(data)?).map_err(|_| DecodeError::OutOfRange {
            value: as_i128(data).unwrap_or_default(),
            target: "i64",
        })
    }

    pub fn has(&self, id: i64) -> bool {
        self.known.contains_key(&id)
    }

    /// A field with no default: absent is an error, never a silent zero.
    pub fn required<T: PlutusCodec>(&self, id: i64) -> Result<T, DecodeError> {
        let data = self
            .known
            .get(&id)
            .ok_or(DecodeError::MissingField { id })?;
        T::from_data(data)
    }

    pub fn optional<T: PlutusCodec>(&self, id: i64) -> Result<Option<T>, DecodeError> {
        match self.known.get(&id) {
            Some(data) => T::from_data(data).map(Some),
            None => Ok(None),
        }
    }

    pub fn list<T: PlutusCodec>(&self, id: i64) -> Result<Vec<T>, DecodeError> {
        match self.known.get(&id) {
            Some(data) => Vec::<T>::from_data(data),
            None => Ok(Vec::new()),
        }
    }

    pub fn or_default<T: PlutusCodec + Default>(&self, id: i64) -> Result<T, DecodeError> {
        match self.known.get(&id) {
            Some(data) => T::from_data(data),
            None => Ok(T::default()),
        }
    }

    /// A field whose declared default is not `Default::default()`.
    pub fn or_else<T: PlutusCodec>(&self, id: i64, default: T) -> Result<T, DecodeError> {
        match self.known.get(&id) {
            Some(data) => T::from_data(data),
            None => Ok(default),
        }
    }

    /// Everything this build did not recognise. Store it on the value.
    pub fn into_unknown(self) -> UnknownFields {
        self.unknown
    }

    /// The unknown block without consuming the reader (for an enum, whose
    /// variant arm reads fields after the tag).
    pub fn unknown(&self) -> UnknownFields {
        self.unknown.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pallas_primitives::Fragment;

    const KNOWN: &[i64] = &[0, 1, 2];

    fn sample() -> PlutusData {
        let mut w = MapWriter::new();
        w.field(0, &7u64).field(2, &"hi".to_string());
        w.finish()
    }

    #[test]
    fn fields_are_written_in_ascending_id_order() {
        let mut w = MapWriter::new();
        // Deliberately out of order.
        w.field(5, &1u64).field(0, &2u64).field(3, &3u64);
        let data = w.finish();
        let ids: Vec<i128> = as_map(&data)
            .unwrap()
            .iter()
            .map(|(k, _)| as_i128(k).unwrap())
            .collect();
        assert_eq!(ids, vec![0, 3, 5]);
    }

    #[test]
    fn absent_fields_read_as_their_default() {
        let reader = MapReader::new(&sample(), KNOWN).unwrap();
        assert_eq!(reader.required::<u64>(0).unwrap(), 7);
        assert_eq!(reader.optional::<u64>(1).unwrap(), None);
        assert_eq!(reader.or_default::<u64>(1).unwrap(), 0);
        assert_eq!(reader.or_else(1, 300u64).unwrap(), 300);
        assert!(reader.list::<u64>(1).unwrap().is_empty());
    }

    #[test]
    fn a_required_field_that_is_absent_is_an_error_not_a_zero() {
        let reader = MapReader::new(&sample(), KNOWN).unwrap();
        assert_eq!(
            reader.required::<u64>(1).unwrap_err(),
            DecodeError::MissingField { id: 1 }
        );
    }

    #[test]
    fn unknown_ids_are_preserved_and_re_encode_byte_identically() {
        // A datum from a newer writer: ids 0 and 2 are known here, 9 is not.
        let mut w = MapWriter::new();
        w.field(0, &1u64).field(2, &2u64).field(9, &vec![3u8, 4]);
        let newer = w.finish();
        let original = newer.encode_fragment().unwrap();

        let reader = MapReader::new(&newer, KNOWN).unwrap();
        let unknown = reader.unknown();
        assert_eq!(unknown.ids().collect::<Vec<_>>(), vec![9]);

        let mut w = MapWriter::new();
        w.field(0, &1u64).field(2, &2u64).unknown(&unknown);
        let round_tripped = w.finish().encode_fragment().unwrap();

        assert_eq!(
            original, round_tripped,
            "an old reader must not eat new fields"
        );
    }

    #[test]
    fn a_duplicate_id_is_rejected_rather_than_last_write_wins() {
        // Hand-built, because MapWriter would never produce it.
        let entries = vec![
            (super::super::scalar::int_data(1), 1u64.to_data()),
            (super::super::scalar::int_data(1), 2u64.to_data()),
        ];
        let data = map_data(entries);
        assert_eq!(
            MapReader::new(&data, &[1]).unwrap_err(),
            DecodeError::DuplicateField { id: 1 }
        );
    }

    #[test]
    fn an_empty_list_is_absent_on_the_wire() {
        let mut w = MapWriter::new();
        w.with_default(1, &Vec::<u64>::new(), &Vec::new());
        assert!(as_map(&w.finish()).unwrap().is_empty());

        // …and a non-empty one is written as an array.
        let mut w = MapWriter::new();
        w.with_default(1, &vec![5u64], &Vec::new());
        let reader = MapReader::new(&w.finish(), &[1]).unwrap();
        assert_eq!(reader.list::<u64>(1).unwrap(), vec![5]);
    }

    #[test]
    fn bytes_are_a_bytestring_and_a_vec_is_an_array() {
        // `Bytes` exists because these two cannot share one impl, and the
        // derive macro refuses a `Vec<u8>` field so the wrong one can never
        // be picked by accident.
        let mut w = MapWriter::new();
        w.field(1, &super::super::scalar::Bytes::from(vec![1u8, 2, 3]));
        w.field(2, &vec![1u64, 2, 3]);
        let data = w.finish();
        let entries = as_map(&data).unwrap();
        assert_eq!(super::super::scalar::shape_of(&entries[0].1), "bytes");
        assert_eq!(super::super::scalar::shape_of(&entries[1].1), "array");
    }

    #[test]
    fn a_value_equal_to_its_default_is_not_written() {
        let mut w = MapWriter::new();
        w.with_default(1, &300u64, &300u64)
            .with_default(2, &7u64, &300u64);
        let data = w.finish();
        let ids: Vec<i128> = as_map(&data)
            .unwrap()
            .iter()
            .map(|(k, _)| as_i128(k).unwrap())
            .collect();
        assert_eq!(ids, vec![2]);
    }

    #[test]
    fn the_tag_lives_at_key_zero() {
        let mut w = MapWriter::new();
        w.tag(3).field(1, &9u64);
        let data = w.finish();
        let reader = MapReader::new(&data, &[0, 1]).unwrap();
        assert_eq!(reader.tag().unwrap(), 3);
        assert_eq!(reader.required::<u64>(1).unwrap(), 9);
    }

    #[test]
    fn a_map_with_no_tag_is_missing_tag() {
        let reader = MapReader::new(&sample(), KNOWN).unwrap();
        let _ = reader.required::<u64>(0);
        let mut w = MapWriter::new();
        w.field(1, &1u64);
        let reader = MapReader::new(&w.finish(), &[0, 1]).unwrap();
        assert_eq!(reader.tag().unwrap_err(), DecodeError::MissingTag);
    }
}
