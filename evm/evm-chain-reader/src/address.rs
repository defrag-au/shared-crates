//! An EVM account address.
//!
//! Lowercase, `0x`-prefixed, twenty bytes — the form a node writes and the form a
//! log topic carries once its twelve zero bytes are stripped. Not checksummed:
//! EIP-55 needs Keccak-256, and an address here is an identifier to compare, not
//! one to display. Normalising on the way in is what makes a comparison a string
//! comparison, so an address read from a topic word and the same address typed
//! into a config file compare equal.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::hex::{self, HexError};

/// How many bytes an address is.
pub const ADDRESS_LEN: usize = 20;

/// How many hex digits an address is.
const ADDRESS_DIGITS: usize = ADDRESS_LEN * 2;

/// Why a string was not an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressError {
    /// Not twenty bytes.
    WrongLength(usize),
    /// Malformed hex.
    Hex(HexError),
    /// The all-zero address, which is a sentinel rather than an account.
    ///
    /// A burn is `to == 0x0…0` and a mint is `from == 0x0…0`, so this is a
    /// *value* the crate has to carry. It is an error only where a caller asks for
    /// a real account, never on the way in — see [`Address::is_zero`].
    Zero,
}

impl fmt::Display for AddressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength(length) => {
                write!(formatter, "expected 20 bytes of address, got {length}")
            }
            Self::Hex(error) => write!(formatter, "{error}"),
            Self::Zero => formatter.write_str("the zero address is a sentinel, not an account"),
        }
    }
}

impl std::error::Error for AddressError {}

impl From<HexError> for AddressError {
    fn from(error: HexError) -> Self {
        Self::Hex(error)
    }
}

/// An EVM account address, normalised to lower-case hex.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Address(String);

impl Address {
    /// The zero address — a burn's destination and a mint's source.
    pub const ZERO: &'static str = "0x0000000000000000000000000000000000000000";

    /// Reads an address, with or without the `0x`, in either case.
    pub fn parse(raw: &str) -> Result<Self, AddressError> {
        let digits = raw
            .strip_prefix("0x")
            .or_else(|| raw.strip_prefix("0X"))
            .unwrap_or(raw);

        if digits.len() != ADDRESS_DIGITS {
            return Err(AddressError::WrongLength(digits.len().div_ceil(2)));
        }

        let bytes = hex::decode(&format!("0x{digits}"))?;
        Ok(Self(format!("0x{}", hex::encode(&bytes))))
    }

    /// Reads an address from a 32-byte log topic, ignoring its twelve zero bytes.
    ///
    /// A word shorter than 64 digits is left-padded rather than refused, because a
    /// provider that trims a leading run of zeros is sending the same value. Empty
    /// is still an error: that is not a trimmed word, it is a missing one.
    pub fn from_topic(topic: &str) -> Result<Self, AddressError> {
        let digits = topic
            .strip_prefix("0x")
            .or_else(|| topic.strip_prefix("0X"))
            .unwrap_or(topic);

        if digits.is_empty() || digits.len() > ADDRESS_DIGITS * 2 {
            return Err(AddressError::WrongLength(digits.len().div_ceil(2)));
        }

        // A word is the address in its last twenty bytes, so a word longer than an
        // address is shortened from the front and a shorter one is padded there.
        let tail = if digits.len() > ADDRESS_DIGITS {
            &digits[digits.len() - ADDRESS_DIGITS..]
        } else {
            digits
        };

        Self::parse(&format!("{tail:0>40}"))
    }

    /// The address as the ABI moves it — a 32-byte word, left-padded.
    pub fn encode_word(&self) -> String {
        format!("0x{}{}", "0".repeat(64 - ADDRESS_DIGITS), &self.0[2..])
    }

    /// The lower-case, `0x`-prefixed form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is the zero address — a burn or a mint, not an account.
    pub fn is_zero(&self) -> bool {
        self.0 == Self::ZERO
    }

    /// Whether this is a real account, i.e. not the zero address.
    pub fn is_account(&self) -> bool {
        !self.is_zero()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for Address {
    type Error = AddressError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(&raw)
    }
}

impl From<Address> for String {
    fn from(address: Address) -> Self {
        address.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MADJACKET: &str = "0x7980aa64093853cb78c927e05b88fed96e945f81";

    #[test]
    fn an_address_round_trips_through_its_wire_form() {
        let address = Address::parse(MADJACKET).unwrap();
        assert_eq!(address.as_str(), MADJACKET);
        assert_eq!(address.to_string(), MADJACKET);
    }

    #[test]
    fn case_and_prefix_do_not_matter_on_the_way_in() {
        let checksummed = Address::parse("0x7980AA64093853CB78C927E05B88FED96E945F81").unwrap();
        let bare = Address::parse("7980aa64093853cb78c927e05b88fed96e945f81").unwrap();
        assert_eq!(checksummed, bare);
        assert_eq!(checksummed.as_str(), MADJACKET);
    }

    #[test]
    fn a_log_topic_gives_up_its_address() {
        let topic = format!("0x{}{}", "0".repeat(24), &MADJACKET[2..]);
        assert_eq!(Address::from_topic(&topic).unwrap().as_str(), MADJACKET);
    }

    #[test]
    fn a_topic_that_was_trimmed_of_its_zeros_is_the_same_address() {
        // The same word with the node's leading zero run removed, and one with the
        // address's own leading zeros removed too.
        let trimmed = format!("0x{}", &MADJACKET[2..]);
        assert_eq!(Address::from_topic(&trimmed).unwrap().as_str(), MADJACKET);

        let address = "0x00000000000000000000000000000000000000ab";
        assert_eq!(Address::from_topic("0xab").unwrap().as_str(), address);
    }

    #[test]
    fn an_empty_topic_is_refused_rather_than_read_as_zero() {
        assert!(Address::from_topic("0x").is_err());
    }

    #[test]
    fn an_address_encodes_as_a_left_padded_word() {
        assert_eq!(
            Address::parse(MADJACKET).unwrap().encode_word(),
            format!("0x{}{}", "0".repeat(24), &MADJACKET[2..])
        );
    }

    #[test]
    fn the_zero_address_is_carried_and_recognised() {
        let zero = Address::parse(Address::ZERO).unwrap();
        assert!(zero.is_zero());
        assert!(!zero.is_account());
        assert!(!Address::parse(MADJACKET).unwrap().is_zero());
    }

    #[test]
    fn the_wrong_length_is_refused() {
        assert_eq!(
            Address::parse("0x7980aa").unwrap_err(),
            AddressError::WrongLength(3)
        );
    }
}
