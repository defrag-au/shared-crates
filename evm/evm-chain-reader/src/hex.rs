//! Hex, as JSON-RPC and the ABI spell it.
//!
//! Two things share the `0x` prefix and mean different things: a **quantity**
//! (JSON-RPC's `"0x1237"` — minimal, no leading zeros) and **data** (an
//! even-length byte string, leading zeros significant). [`Quantity`] is the first;
//! [`decode`] and [`encode`] are the second. Reading one as the other is a bug
//! that produces a wrong number rather than an error, so they are separate types
//! here rather than separate conventions in a caller's head.

use std::fmt;

use serde::{Deserialize, Serialize};

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Why a string was not hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HexError {
    /// No `0x` prefix. JSON-RPC always sends one.
    MissingPrefix,
    /// An odd number of digits, so not whole bytes.
    OddLength(usize),
    /// More hex digits than the value can hold — a word longer than 32 bytes.
    TooLong(usize),
    /// A character outside `0-9a-fA-F`.
    NotHex(char),
    /// A quantity with nothing after the prefix.
    Empty,
    /// A quantity too large for a `u64`. Not reachable for a chain id, a block
    /// number or a log index — all of which is why they are `u64` here — but a
    /// caller who hands one of these functions a foreign string needs a reason,
    /// not a panic.
    TooLarge,
}

impl fmt::Display for HexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => formatter.write_str("not 0x-prefixed hex"),
            Self::OddLength(length) => write!(formatter, "odd hex length: {length} digits"),
            Self::TooLong(digits) => write!(formatter, "too many hex digits: {digits}"),
            Self::NotHex(character) => write!(formatter, "not a hex digit: {character:?}"),
            Self::Empty => formatter.write_str("no digits after 0x"),
            Self::TooLarge => formatter.write_str("a quantity larger than 64 bits"),
        }
    }
}

impl std::error::Error for HexError {}

/// Strips the `0x` prefix, accepting an upper-case `0X` as some nodes write it.
fn digits(raw: &str) -> Result<&str, HexError> {
    raw.strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .ok_or(HexError::MissingPrefix)
}

fn nibble(byte: u8) -> Result<u8, HexError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        other => Err(HexError::NotHex(other as char)),
    }
}

/// Decodes `0x`-prefixed hex into bytes, requiring whole bytes.
pub fn decode(raw: &str) -> Result<Vec<u8>, HexError> {
    let digits = digits(raw)?;
    if digits.len() % 2 != 0 {
        return Err(HexError::OddLength(digits.len()));
    }

    let bytes = digits.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks(2) {
        out.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    Ok(out)
}

/// Encodes bytes as lower-case hex, with no prefix.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Encodes bytes as lower-case hex, `0x`-prefixed — the form a JSON-RPC request
/// wants.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push_str("0x");
    out.push_str(&encode(bytes));
    out
}

/// Reads a JSON-RPC quantity as its number.
pub fn quantity(raw: &str) -> Result<u64, HexError> {
    let digits = digits(raw)?;
    if digits.is_empty() {
        return Err(HexError::Empty);
    }
    if !digits.bytes().all(|byte| nibble(byte).is_ok()) {
        return Err(HexError::NotHex(
            digits
                .chars()
                .find(|character| character.to_digit(16).is_none())
                .unwrap_or('?'),
        ));
    }
    u64::from_str_radix(digits, 16).map_err(|_| HexError::TooLarge)
}

/// Writes a number as a JSON-RPC quantity — minimal, lower-case, prefixed.
pub fn quantity_string(value: u64) -> String {
    format!("0x{value:x}")
}

/// A JSON-RPC quantity as a type — a number that is a hex string on the wire.
///
/// Deserialising through `String` rather than `u64` is the whole point: a node
/// sends `"0x1237"`, and a derived `u64` would need `0x1237` to be a JSON number,
/// which it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Quantity(u64);

impl Quantity {
    /// The number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl From<Quantity> for u64 {
    fn from(quantity: Quantity) -> Self {
        quantity.0
    }
}

impl From<u64> for Quantity {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl TryFrom<String> for Quantity {
    type Error = HexError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        quantity(&raw).map(Self)
    }
}

impl From<Quantity> for String {
    fn from(quantity: Quantity) -> Self {
        quantity_string(quantity.0)
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&quantity_string(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_through_its_wire_form() {
        let bytes = [0x00, 0x0f, 0xf0, 0xff, 0x42];
        assert_eq!(to_hex(&bytes), "0x000ff0ff42");
        assert_eq!(decode("0x000ff0ff42").unwrap(), bytes);
    }

    #[test]
    fn an_empty_data_blob_is_a_valid_empty_byte_string() {
        // What a node returns for a call to a selector the contract does not
        // implement. It has to be representable, so the caller can say "no data"
        // rather than "malformed".
        assert_eq!(decode("0x").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn a_quantity_is_read_as_a_number() {
        assert_eq!(quantity("0x1237").unwrap(), 4663);
        assert_eq!(quantity("0x0").unwrap(), 0);
        assert_eq!(quantity("0x").unwrap_err(), HexError::Empty);
        assert_eq!(quantity("1237").unwrap_err(), HexError::MissingPrefix);
        assert_eq!(quantity("0xzz").unwrap_err(), HexError::NotHex('z'));
    }

    #[test]
    fn a_quantity_too_large_for_sixty_four_bits_is_an_error_not_a_wrap() {
        assert_eq!(
            quantity("0x10000000000000000").unwrap_err(),
            HexError::TooLarge
        );
    }

    #[test]
    fn a_quantity_writes_back_without_leading_zeros() {
        assert_eq!(quantity_string(0), "0x0");
        assert_eq!(quantity_string(4663), "0x1237");
        assert_eq!(Quantity(4663).to_string(), "0x1237");
    }

    #[test]
    fn a_quantity_deserialises_from_a_hex_string_not_a_number() {
        let parsed: Quantity = serde_json::from_str("\"0x1237\"").unwrap();
        assert_eq!(parsed.get(), 4663);

        // A bare JSON number is not the wire form and is refused rather than
        // silently accepted, so the two spellings cannot drift apart.
        assert!(serde_json::from_str::<Quantity>("4663").is_err());
    }

    #[test]
    fn data_with_an_odd_digit_count_is_refused() {
        assert_eq!(decode("0xabc").unwrap_err(), HexError::OddLength(3));
    }
}
