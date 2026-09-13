//! The one codec trait, and the scalar rules of schema §2.4.
//!
//! One trait covers scalars, structs and enums. The schema doc sketches a
//! separate `PlutusScalar` for primitives; a single [`PlutusCodec`] is the
//! same thing with one fewer name to keep in sync, and the derive macro
//! emits impls of exactly this trait.
//!
//! Everything goes through [`pallas_primitives::PlutusData`], so anything
//! this module produces is ledger-valid by construction — including the
//! chunking of long bytestrings, which pallas's `BoundedBytes` does for us:
//! anything over 64 bytes becomes an indefinite bytestring of 64-byte
//! chunks, matching the Haskell implementation, and decode rejoins them.
//! **The codec must not chunk a second time.**

use pallas_codec::utils::{Int, KeyValuePairs, MaybeIndefArray};
use pallas_primitives::{BigInt, BoundedBytes, Constr, PlutusData};

use super::DecodeError;

/// A type that reads and writes itself as PlutusData.
pub trait PlutusCodec: Sized {
    fn to_data(&self) -> PlutusData;
    fn from_data(data: &PlutusData) -> Result<Self, DecodeError>;
}

// ── shape helpers ──────────────────────────────────────────────────────────

/// The name of a `PlutusData`'s shape, for error copy.
pub const fn shape_of(data: &PlutusData) -> &'static str {
    match data {
        PlutusData::Constr(_) => "constr",
        PlutusData::Map(_) => "map",
        PlutusData::Array(_) => "array",
        PlutusData::BigInt(_) => "int",
        PlutusData::BoundedBytes(_) => "bytes",
    }
}

fn wrong(expected: &'static str, found: &PlutusData) -> DecodeError {
    DecodeError::WrongShape {
        expected,
        found: shape_of(found),
    }
}

pub fn as_bytes(data: &PlutusData) -> Result<&[u8], DecodeError> {
    match data {
        PlutusData::BoundedBytes(b) => Ok(b.as_ref()),
        other => Err(wrong("bytes", other)),
    }
}

pub fn as_array(data: &PlutusData) -> Result<&[PlutusData], DecodeError> {
    match data {
        PlutusData::Array(a) => Ok(a.as_ref()),
        other => Err(wrong("array", other)),
    }
}

pub fn as_map(data: &PlutusData) -> Result<&[(PlutusData, PlutusData)], DecodeError> {
    match data {
        PlutusData::Map(m) => Ok(m.as_ref()),
        other => Err(wrong("map", other)),
    }
}

pub fn as_constr(data: &PlutusData) -> Result<&Constr<PlutusData>, DecodeError> {
    match data {
        PlutusData::Constr(c) => Ok(c),
        other => Err(wrong("constr", other)),
    }
}

// ── integers ───────────────────────────────────────────────────────────────

/// Encode an integer. Total: values outside minicbor's `Int` range fall back
/// to the CBOR bignum forms, which is what the ledger expects for them.
pub fn int_data(value: i128) -> PlutusData {
    match Int::try_from(value) {
        Ok(int) => PlutusData::BigInt(BigInt::Int(int)),
        Err(_) => {
            let magnitude = if value < 0 {
                // CBOR negative bignum encodes -1 - n.
                (-1 - value) as u128
            } else {
                value as u128
            };
            let bytes = trim_leading_zeros(&magnitude.to_be_bytes());
            let bounded = BoundedBytes::from(bytes);
            if value < 0 {
                PlutusData::BigInt(BigInt::BigNInt(bounded))
            } else {
                PlutusData::BigInt(BigInt::BigUInt(bounded))
            }
        }
    }
}

fn trim_leading_zeros(bytes: &[u8]) -> Vec<u8> {
    let first = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len() - 1);
    bytes[first..].to_vec()
}

/// Read any integer form as `i128`, the widest thing every declared type
/// range-checks out of. A bignum too large for `i128` is `OutOfRange` rather
/// than a truncation (§2.4).
pub fn as_i128(data: &PlutusData) -> Result<i128, DecodeError> {
    let big = match data {
        PlutusData::BigInt(b) => b,
        other => return Err(wrong("int", other)),
    };
    match big {
        BigInt::Int(int) => Ok(i128::from(*int)),
        BigInt::BigUInt(bytes) => big_magnitude(bytes.as_ref()).map(|m| m as i128),
        BigInt::BigNInt(bytes) => big_magnitude(bytes.as_ref()).map(|m| -1 - (m as i128)),
    }
}

fn big_magnitude(bytes: &[u8]) -> Result<u128, DecodeError> {
    let significant = bytes.iter().skip_while(|b| **b == 0).count();
    if significant > 16 {
        return Err(DecodeError::OutOfRange {
            // The value cannot be represented, so report the width instead of
            // a wrong number.
            value: i128::MAX,
            target: "i128",
        });
    }
    let mut acc: u128 = 0;
    for byte in bytes {
        acc = (acc << 8) | u128::from(*byte);
    }
    // A 16-byte magnitude can still exceed i128::MAX once signed.
    if acc > i128::MAX as u128 {
        return Err(DecodeError::OutOfRange {
            value: i128::MAX,
            target: "i128",
        });
    }
    Ok(acc)
}

macro_rules! int_codec {
    ($($ty:ty),+ $(,)?) => {$(
        impl PlutusCodec for $ty {
            fn to_data(&self) -> PlutusData {
                int_data(i128::from(*self))
            }

            fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
                let raw = as_i128(data)?;
                <$ty>::try_from(raw).map_err(|_| DecodeError::OutOfRange {
                    value: raw,
                    target: stringify!($ty),
                })
            }
        }
    )+};
}

int_codec!(u8, u16, u32, u64, i32, i64);

// ── bool ───────────────────────────────────────────────────────────────────

impl PlutusCodec for bool {
    fn to_data(&self) -> PlutusData {
        int_data(i128::from(*self))
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        match as_i128(data)? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(DecodeError::InvalidBool { value }),
        }
    }
}

// ── bytes and text ─────────────────────────────────────────────────────────

/// A variable-length bytestring.
///
/// Bytes are a newtype rather than a bare `Vec<u8>` because `Vec<T>` encodes
/// as a PlutusData **array** and bytes encode as a **bytestring** — and Rust
/// has no specialisation, so one blanket impl cannot serve both. Making the
/// distinction a type rather than a convention means "asset name" can never
/// silently ship as a list of 32 small integers. The derive macro refuses a
/// `Vec<u8>` field and points here.
///
/// JSON form is a hex string, which is what every surface in the repo
/// already shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl From<&[u8]> for Bytes {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl PlutusCodec for Bytes {
    fn to_data(&self) -> PlutusData {
        PlutusData::BoundedBytes(BoundedBytes::from(self.0.clone()))
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        Ok(Self(as_bytes(data)?.to_vec()))
    }
}

impl serde::Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(&self.0))
    }
}

impl<'de> serde::Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        hex::decode(&text)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

impl<const N: usize> PlutusCodec for [u8; N] {
    fn to_data(&self) -> PlutusData {
        PlutusData::BoundedBytes(BoundedBytes::from(self.to_vec()))
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        let bytes = as_bytes(data)?;
        <[u8; N]>::try_from(bytes).map_err(|_| DecodeError::BadLength {
            target: "fixed-width bytes",
            expected: N,
            actual: bytes.len(),
        })
    }
}

impl PlutusCodec for String {
    fn to_data(&self) -> PlutusData {
        // pallas chunks >64 bytes into an indefinite bytestring for us.
        PlutusData::BoundedBytes(BoundedBytes::from(self.as_bytes().to_vec()))
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        let bytes = as_bytes(data)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeError::InvalidUtf8)
    }
}

// ── containers ─────────────────────────────────────────────────────────────

impl<T: PlutusCodec> PlutusCodec for Vec<T> {
    fn to_data(&self) -> PlutusData {
        PlutusData::Array(MaybeIndefArray::Def(
            self.iter().map(PlutusCodec::to_data).collect(),
        ))
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        as_array(data)?.iter().map(T::from_data).collect()
    }
}

/// Identity — for `body: Data` and for anything carried through unread.
impl PlutusCodec for PlutusData {
    fn to_data(&self) -> PlutusData {
        self.clone()
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        Ok(data.clone())
    }
}

// ── canonical helpers used by the map layer ────────────────────────────────

pub(crate) fn map_data(entries: Vec<(PlutusData, PlutusData)>) -> PlutusData {
    PlutusData::Map(KeyValuePairs::Def(entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pallas_primitives::Fragment;

    fn round_trip<T: PlutusCodec + PartialEq + std::fmt::Debug>(value: T) {
        let data = value.to_data();
        let back = T::from_data(&data).expect("decode");
        assert_eq!(value, back);
        // And it survives a real CBOR round trip — the ledger's view.
        let bytes = data.encode_fragment().expect("encode");
        let reparsed = PlutusData::decode_fragment(&bytes).expect("reparse");
        assert_eq!(data, reparsed);
    }

    #[test]
    fn integers_round_trip_at_their_boundaries() {
        round_trip(0u64);
        round_trip(u64::MAX);
        round_trip(u32::MAX);
        round_trip(i64::MIN);
        round_trip(i64::MAX);
        round_trip(0u8);
        round_trip(255u8);
    }

    #[test]
    fn an_integer_too_large_for_the_target_is_out_of_range_never_truncated() {
        let data = int_data(i128::from(u32::MAX) + 1);
        let err = u32::from_data(&data).unwrap_err();
        assert_eq!(
            err,
            DecodeError::OutOfRange {
                value: i128::from(u32::MAX) + 1,
                target: "u32"
            }
        );
    }

    #[test]
    fn a_negative_value_never_becomes_a_large_unsigned_one() {
        let data = int_data(-1);
        assert!(matches!(
            u64::from_data(&data),
            Err(DecodeError::OutOfRange { value: -1, .. })
        ));
    }

    #[test]
    fn bools_are_zero_and_one_and_nothing_else() {
        round_trip(true);
        round_trip(false);
        assert_eq!(
            bool::from_data(&int_data(2)).unwrap_err(),
            DecodeError::InvalidBool { value: 2 }
        );
    }

    #[test]
    fn pallas_chunks_long_bytestrings_for_us() {
        // Schema §6 Q2, answered against pallas-primitives 1.1.1: a
        // definite bytestring longer than 64 bytes is invalid Plutus data,
        // and `BoundedBytes`'s own Encode splits at 64 into an indefinite
        // bytestring. If this ever stops being true the codec has to chunk
        // itself, and this test is where that shows up.
        let long = "x".repeat(200);
        round_trip(long.clone());

        let bytes = long.to_data().encode_fragment().unwrap();
        // 0x5f = indefinite-length byte string.
        assert_eq!(bytes[0], 0x5f, "expected an indefinite bytestring header");
        assert_eq!(*bytes.last().unwrap(), 0xff, "expected a break byte");

        let short = "x".repeat(64);
        let bytes = short.to_data().encode_fragment().unwrap();
        assert_eq!(bytes[0], 0x58, "64 bytes should stay definite");
    }

    #[test]
    fn fixed_width_bytes_check_their_length() {
        round_trip([7u8; 28]);
        let wrong_width = Bytes::from(vec![0u8; 27]).to_data();
        assert!(matches!(
            <[u8; 28]>::from_data(&wrong_width),
            Err(DecodeError::BadLength {
                expected: 28,
                actual: 27,
                ..
            })
        ));
    }

    #[test]
    fn a_wrong_shape_names_both_sides() {
        let err = u64::from_data(&Bytes::default().to_data()).unwrap_err();
        assert_eq!(
            err,
            DecodeError::WrongShape {
                expected: "int",
                found: "bytes"
            }
        );
    }

    #[test]
    fn vectors_round_trip() {
        round_trip(vec![1u64, 2, 3]);
        round_trip(Vec::<u64>::new());
        round_trip(vec![Bytes::from(vec![1u8, 2]), Bytes::from(vec![3])]);
    }

    #[test]
    fn bytes_are_a_bytestring_and_a_list_is_an_array() {
        assert_eq!(shape_of(&Bytes::from(vec![1u8, 2]).to_data()), "bytes");
        assert_eq!(shape_of(&vec![1u64, 2].to_data()), "array");
    }

    #[test]
    fn bytes_are_hex_in_json() {
        let bytes = Bytes::from(vec![0xde, 0xad]);
        let json = serde_json::to_string(&bytes).unwrap();
        assert_eq!(json, "\"dead\"");
        assert_eq!(serde_json::from_str::<Bytes>(&json).unwrap(), bytes);
    }
}
