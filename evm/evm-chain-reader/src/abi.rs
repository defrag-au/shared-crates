//! Just enough ABI to read a return value.
//!
//! Every call this crate makes returns **one static 32-byte word** — an address, a
//! `uint256`, or a `bool` — so there is nothing here that decodes a dynamic type,
//! no offset following, and no length prefix. That is not a limitation to grow out
//! of: it is why the reader needs no ABI crate. A future call that returns
//! `string` or `bytes` is the point at which that changes, and it should be a
//! deliberate decision then rather than an import now.

use std::fmt;

use crate::address::{Address, AddressError};
use crate::hex::{self, HexError};
use crate::log::HexData;
use crate::uint256::U256;

/// How many bytes an ABI word is.
pub const WORD: usize = 32;

/// Why a return value was not the shape the call promised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiError {
    /// The call returned no data at all. On most nodes this is what a call to a
    /// selector the contract does not implement produces.
    NoData,
    /// The data was not the length the call's return type requires.
    WrongLength { expected: usize, got: usize },
    /// A return that should be an address carried bits above its low twenty bytes.
    /// Almost always a `uint256` return decoded as an address.
    NotAnAddress,
    /// A log had the wrong number of indexed words for the event.
    WrongTopics { expected: usize, got: usize },
    /// A log's first indexed word was not the event this decoder reads.
    WrongEvent { expected: &'static str, got: String },
    /// Malformed hex.
    Hex(HexError),
    /// A malformed address.
    Address(AddressError),
}

impl fmt::Display for AbiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoData => formatter.write_str("the call returned no data"),
            Self::WrongLength { expected, got } => {
                write!(formatter, "expected {expected} bytes, got {got}")
            }
            Self::NotAnAddress => formatter
                .write_str("the word is not an address: bits above the low 20 bytes are set"),
            Self::WrongTopics { expected, got } => write!(
                formatter,
                "expected {expected} indexed words for this event, got {got}"
            ),
            Self::WrongEvent { expected, got } => {
                write!(formatter, "not a {expected}: topics[0] is {got}")
            }
            Self::Hex(error) => write!(formatter, "{error}"),
            Self::Address(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for AbiError {}

impl From<HexError> for AbiError {
    fn from(error: HexError) -> Self {
        Self::Hex(error)
    }
}

impl From<AddressError> for AbiError {
    fn from(error: AddressError) -> Self {
        Self::Address(error)
    }
}

/// Builds calldata from a four-byte selector and one word.
///
/// Every call this crate makes takes exactly one argument, so this is the only
/// encoder it needs — no head/tail, no dynamic types.
pub(crate) fn calldata(selector: &str, word: &str) -> Result<HexData, AbiError> {
    HexData::new(format!("{selector}{}", &word[2..])).map_err(AbiError::from)
}

/// Decodes a call's result into its single word.
pub(crate) fn single_word(data: &HexData) -> Result<[u8; WORD], AbiError> {
    let bytes = data.decode()?;
    if bytes.is_empty() {
        return Err(AbiError::NoData);
    }
    if bytes.len() != WORD {
        return Err(AbiError::WrongLength {
            expected: WORD,
            got: bytes.len(),
        });
    }

    let mut word = [0u8; WORD];
    word.copy_from_slice(&bytes);
    Ok(word)
}

/// Reads the little-endian-agnostic big-endian word at a position.
pub(crate) fn word_at(bytes: &[u8], index: usize) -> Result<[u8; WORD], AbiError> {
    let start = index * WORD;
    let end = start + WORD;
    if bytes.len() < end {
        return Err(AbiError::WrongLength {
            expected: end,
            got: bytes.len(),
        });
    }

    let mut word = [0u8; WORD];
    word.copy_from_slice(&bytes[start..end]);
    Ok(word)
}

/// Reads an address from a word, refusing one with bits above its low twenty bytes.
///
/// Strict on purpose: a lenient read is what lets a `uint256` return be mistaken
/// for an address without anything saying so.
pub(crate) fn address(word: &[u8; WORD]) -> Result<Address, AbiError> {
    if word[..WORD - 20].iter().any(|byte| *byte != 0) {
        return Err(AbiError::NotAnAddress);
    }
    Address::parse(&hex::to_hex(&word[WORD - 20..])).map_err(AbiError::from)
}

/// Reads a `uint256` from a word.
pub(crate) fn uint(word: &[u8; WORD]) -> U256 {
    U256::from_word(*word)
}

/// Reads a `bool` from a word. Anything non-zero is true, as Solidity's decoder
/// does.
pub(crate) fn boolean(word: &[u8; WORD]) -> bool {
    word.iter().any(|byte| *byte != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word_from(hex_digits: &str) -> [u8; WORD] {
        let bytes = hex::decode(&format!("0x{hex_digits:0>64}")).unwrap();
        let mut word = [0u8; WORD];
        word.copy_from_slice(&bytes);
        word
    }

    #[test]
    fn an_address_comes_out_of_its_word() {
        let word = word_from("7980aa64093853cb78c927e05b88fed96e945f81");
        assert_eq!(
            address(&word).unwrap().as_str(),
            "0x7980aa64093853cb78c927e05b88fed96e945f81"
        );
    }

    #[test]
    fn a_word_with_bits_above_the_low_twenty_bytes_is_not_an_address() {
        // A `uint256` return read as an address. There is no address in it, and
        // saying so is the point of the check.
        let word = word_from("1000000000000000000000000000000000000000000000000000000000000000");
        assert_eq!(address(&word).unwrap_err(), AbiError::NotAnAddress);
    }

    #[test]
    fn a_uint_and_a_bool_come_out_of_their_words() {
        assert_eq!(uint(&word_from("4d2")).to_decimal(), "1234");
        assert!(boolean(&word_from("1")));
        assert!(!boolean(&word_from("0")));
    }

    #[test]
    fn a_call_with_no_data_says_so() {
        assert_eq!(
            single_word(&HexData::new("0x").unwrap()).unwrap_err(),
            AbiError::NoData
        );
    }

    #[test]
    fn a_call_with_the_wrong_length_says_how_long_it_was() {
        let data = HexData::new("0x0011").unwrap();
        assert_eq!(
            single_word(&data).unwrap_err(),
            AbiError::WrongLength {
                expected: 32,
                got: 2
            }
        );
    }
}
