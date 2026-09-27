//! A 256-bit unsigned integer, as the EVM uses for token ids and balances.
//!
//! Deliberately not a bignum library. An ERC-721 `tokenId` and an `ownerOf`
//! argument are 32 bytes, and the only arithmetic this crate needs is the one
//! conversion between the two spellings a token id has: the **decimal string** it
//! is written as everywhere identity is concerned (`…/1234`, our CAIP-19 token
//! reference) and the **32-byte big-endian word** the ABI moves it in. Both
//! directions are a repeated ×10 / ÷10 over a fixed `[u8; 32]`, so there is no
//! allocation and no dependency for a type that exists to be a token id.
//!
//! Serde uses the decimal form, because that is the one that appears in an
//! identified asset. [`U256::encode_word`] and [`U256::decode_word`] are the ABI's.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::hex::{self, HexError};

/// Thirty-two bytes, big-endian — the ABI's word.
pub const WORD_LEN: usize = 32;

/// Why a string was not a 256-bit integer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum U256Error {
    /// No digits at all.
    Empty,
    /// A character outside `0-9`.
    NotADigit(char),
    /// More than 256 bits.
    Overflow,
    /// The hex was malformed.
    Hex(HexError),
    /// An ABI word that is not 32 bytes.
    WrongLength(usize),
}

impl fmt::Display for U256Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("no digits"),
            Self::NotADigit(character) => write!(formatter, "not a decimal digit: {character:?}"),
            Self::Overflow => formatter.write_str("more than 256 bits"),
            Self::Hex(error) => write!(formatter, "{error}"),
            Self::WrongLength(length) => write!(formatter, "expected 32 bytes, got {length}"),
        }
    }
}

impl std::error::Error for U256Error {}

impl From<HexError> for U256Error {
    fn from(error: HexError) -> Self {
        Self::Hex(error)
    }
}

/// A 256-bit unsigned integer, big-endian.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct U256([u8; WORD_LEN]);

impl U256 {
    /// Zero.
    pub const ZERO: Self = Self([0; WORD_LEN]);

    /// Parses a decimal string — the form a token id is written in.
    ///
    /// Leading zeros are accepted (`"007"` is `7`), because a token id read back
    /// from a stored reference may carry them and refusing them would be a parse
    /// error for a value that is not ambiguous.
    pub fn from_decimal(raw: &str) -> Result<Self, U256Error> {
        if raw.is_empty() {
            return Err(U256Error::Empty);
        }

        let mut bytes = [0u8; WORD_LEN];
        for character in raw.chars() {
            let digit = character
                .to_digit(10)
                .ok_or(U256Error::NotADigit(character))? as u8;
            if !multiply_add(&mut bytes, 10, digit) {
                return Err(U256Error::Overflow);
            }
        }
        Ok(Self(bytes))
    }

    /// The decimal string — the form identity uses, and the inverse of
    /// [`U256::from_decimal`].
    pub fn to_decimal(&self) -> String {
        let mut bytes = self.0;
        let mut digits = Vec::with_capacity(78);
        while bytes.iter().any(|byte| *byte != 0) {
            digits.push(divide_small(&mut bytes, 10));
        }
        if digits.is_empty() {
            return "0".to_owned();
        }

        let mut out = String::with_capacity(digits.len());
        for digit in digits.iter().rev() {
            out.push((b'0' + digit) as char);
        }
        out
    }

    /// Reads a hex word — `0x`-prefixed, up to 64 digits, right-aligned.
    ///
    /// Fewer than 64 digits is accepted because a node may trim a leading run of
    /// zeros, and the value is the same either way.
    pub fn decode_word(raw: &str) -> Result<Self, U256Error> {
        let trimmed = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X"));
        let digits = trimmed.ok_or(HexError::MissingPrefix)?;
        if digits.is_empty() {
            return Err(U256Error::Empty);
        }
        if digits.len() > WORD_LEN * 2 {
            return Err(U256Error::Overflow);
        }

        let padded = format!("{:0>64}", digits);
        let bytes = hex::decode(&format!("0x{padded}"))?;
        let mut word = [0u8; WORD_LEN];
        word.copy_from_slice(&bytes);
        Ok(Self(word))
    }

    /// The ABI word — 32 bytes, `0x`-prefixed, always 64 digits.
    pub fn encode_word(&self) -> String {
        hex::to_hex(&self.0)
    }

    /// Wraps a 32-byte word, big-endian. Infallible, because a word is the type.
    pub const fn from_word(word: [u8; WORD_LEN]) -> Self {
        Self(word)
    }

    /// Widens a `u64` — always fits, so there is no failure to report.
    pub const fn from_u64(value: u64) -> Self {
        let mut word = [0u8; WORD_LEN];
        let bytes = value.to_be_bytes();
        let mut index = 0;
        while index < 8 {
            word[WORD_LEN - 8 + index] = bytes[index];
            index += 1;
        }
        Self(word)
    }

    /// Reads exactly 32 bytes, big-endian.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, U256Error> {
        let mut word = [0u8; WORD_LEN];
        match bytes.len() {
            WORD_LEN => word.copy_from_slice(bytes),
            length if length < WORD_LEN => {
                word[WORD_LEN - length..].copy_from_slice(bytes);
            }
            length => return Err(U256Error::WrongLength(length)),
        }
        Ok(Self(word))
    }

    /// The raw bytes.
    pub const fn as_bytes(&self) -> &[u8; WORD_LEN] {
        &self.0
    }

    /// Whether this is zero.
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }

    /// The value as a `u64`, if it fits — which a chain id or a block number
    /// does and a token id need not.
    pub fn to_u64(&self) -> Option<u64> {
        if self.0[..WORD_LEN - 8].iter().any(|byte| *byte != 0) {
            return None;
        }
        let mut value = 0u64;
        for byte in &self.0[WORD_LEN - 8..] {
            value = (value << 8) | u64::from(*byte);
        }
        Some(value)
    }
}

impl fmt::Debug for U256 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "U256({})", self.to_decimal())
    }
}

impl fmt::Display for U256 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_decimal())
    }
}

impl TryFrom<String> for U256 {
    type Error = U256Error;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::from_decimal(&raw)
    }
}

impl From<U256> for String {
    fn from(value: U256) -> Self {
        value.to_decimal()
    }
}

/// `bytes = bytes * multiplier + addend`, reporting overflow rather than wrapping.
fn multiply_add(bytes: &mut [u8; WORD_LEN], multiplier: u8, addend: u8) -> bool {
    let mut carry = u32::from(addend);
    for index in (0..WORD_LEN).rev() {
        let value = u32::from(bytes[index]) * u32::from(multiplier) + carry;
        bytes[index] = value as u8;
        carry = value >> 8;
    }
    carry == 0
}

/// `bytes = bytes / divisor`, returning the remainder.
fn divide_small(bytes: &mut [u8; WORD_LEN], divisor: u8) -> u8 {
    let mut remainder = 0u32;
    for byte in bytes.iter_mut() {
        let value = (remainder << 8) | u32::from(*byte);
        *byte = (value / u32::from(divisor)) as u8;
        remainder = value % u32::from(divisor);
    }
    remainder as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `2^256 - 1`, the largest value the type holds.
    const MAX_DECIMAL: &str =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935";

    /// `2^256`, one past it.
    const OVERFLOW_DECIMAL: &str =
        "115792089237316195423570985008687907853269984665640564039457584007913129639936";

    #[test]
    fn a_u64_widens_without_a_parse() {
        assert_eq!(U256::from_u64(42).to_decimal(), "42");
        assert_eq!(U256::from_u64(u64::MAX).to_decimal(), u64::MAX.to_string());
        assert_eq!(U256::from_u64(0), U256::ZERO);
    }

    #[test]
    fn a_decimal_token_id_round_trips() {
        let value = U256::from_decimal("1234").unwrap();
        assert_eq!(value.to_decimal(), "1234");
        assert_eq!(value.encode_word(), format!("0x{:0>64}", "4d2"));
    }

    #[test]
    fn the_largest_value_the_type_holds_round_trips() {
        let value = U256::from_decimal(MAX_DECIMAL).unwrap();
        assert_eq!(value.encode_word(), format!("0x{}", "f".repeat(64)));
        assert_eq!(value.to_decimal(), MAX_DECIMAL);
    }

    #[test]
    fn one_past_the_maximum_is_an_error_rather_than_a_wrap() {
        assert_eq!(
            U256::from_decimal(OVERFLOW_DECIMAL).unwrap_err(),
            U256Error::Overflow
        );
    }

    #[test]
    fn leading_zeros_are_accepted_and_dropped() {
        assert_eq!(U256::from_decimal("007").unwrap().to_decimal(), "7");
        assert_eq!(U256::from_decimal("0").unwrap(), U256::ZERO);
        assert_eq!(U256::ZERO.to_decimal(), "0");
    }

    #[test]
    fn a_hex_word_round_trips_both_ways() {
        let word = "0x00000000000000000000000000000000000000000000000000000000000004d2";
        let value = U256::decode_word(word).unwrap();
        assert_eq!(value.to_decimal(), "1234");
        assert_eq!(value.encode_word(), word);
    }

    #[test]
    fn a_trimmed_hex_word_is_the_same_value_as_a_padded_one() {
        assert_eq!(
            U256::decode_word("0x4d2").unwrap(),
            U256::decode_word("0x00000000000000000000000000000000000000000000000000000000000004d2")
                .unwrap()
        );
    }

    #[test]
    fn a_hex_word_longer_than_the_type_is_refused() {
        assert_eq!(
            U256::decode_word(&format!("0x{}", "1".repeat(65))).unwrap_err(),
            U256Error::Overflow
        );
    }

    #[test]
    fn thirty_two_bytes_are_read_big_endian() {
        let mut bytes = [0u8; WORD_LEN];
        bytes[31] = 0x2a;
        assert_eq!(U256::from_bytes(&bytes).unwrap().to_decimal(), "42");
        assert_eq!(
            U256::from_bytes(&[1u8; WORD_LEN + 1]).unwrap_err(),
            U256Error::WrongLength(WORD_LEN + 1)
        );
    }

    #[test]
    fn a_small_value_reads_back_as_a_u64_and_a_large_one_does_not() {
        assert_eq!(U256::from_decimal("42").unwrap().to_u64(), Some(42));
        assert_eq!(U256::from_decimal(MAX_DECIMAL).unwrap().to_u64(), None);
    }

    #[test]
    fn serde_uses_the_decimal_form() {
        let value = U256::from_decimal("1234").unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), "\"1234\"");
        let read_back: U256 = serde_json::from_str("\"1234\"").unwrap();
        assert_eq!(read_back, value);
        assert!(serde_json::from_str::<U256>("\"0x4d2\"").is_err());
    }
}
