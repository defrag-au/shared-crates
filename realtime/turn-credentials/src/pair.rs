//! The `id:token` pair a Cloudflare Realtime TURN key is minted with.

use std::fmt;

/// What separates the two halves, as the secret store holds them:
/// `TURN_HODLCROFT_TOKEN_PAIR=id:token`.
const SEPARATOR: char = ':';

/// Which half a [`PairError`] is about.
///
/// Named rather than shown: an error that echoed the value it refused would put
/// a token in a log, which is the one thing this type exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    Id,
    Token,
}

impl fmt::Display for Half {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Half::Id => "key id",
            Half::Token => "API token",
        })
    }
}

/// Why a raw secret is not a pair. Every message names the half and no message
/// carries its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairError {
    /// No `:` at all, so neither half can even be named.
    NotAPair,
    /// One half is empty.
    Empty(Half),
    /// One half has whitespace inside it — a wrapped value, or two values
    /// pasted together.
    Whitespace(Half),
}

impl fmt::Display for PairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PairError::NotAPair => f.write_str("no `:` between the key id and the API token"),
            PairError::Empty(half) => write!(f, "the TURN {half} is empty"),
            PairError::Whitespace(half) => write!(f, "the TURN {half} contains whitespace"),
        }
    }
}

impl std::error::Error for PairError {}

/// A TURN key id and its API token, as one value.
///
/// The token is reachable — [`TokenPair::token`] is how the mint signs its
/// request — but it takes a deliberate call, and `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenPair {
    id: String,
    token: String,
}

impl TokenPair {
    /// Split `raw` on its first `:`.
    ///
    /// The whole value is trimmed, then each half, because a secret pasted into
    /// a store or a `.dev.vars` line usually arrives with a trailing newline.
    /// A `:` inside the token is kept: Cloudflare's tokens do not contain one,
    /// but truncating at the second would turn a future format into a silent
    /// authentication failure.
    pub fn parse(raw: &str) -> Result<Self, PairError> {
        let (id, token) = raw
            .trim()
            .split_once(SEPARATOR)
            .ok_or(PairError::NotAPair)?;
        let (id, token) = (id.trim(), token.trim());
        for (half, value) in [(Half::Id, id), (Half::Token, token)] {
            if value.is_empty() {
                return Err(PairError::Empty(half));
            }
            if value.chars().any(char::is_whitespace) {
                return Err(PairError::Whitespace(half));
            }
        }
        Ok(Self {
            id: id.to_string(),
            token: token.to_string(),
        })
    }

    /// The key id: which Cloudflare key a mint is for. Safe to log.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The API token, for signing the mint request. Do not log it.
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for TokenPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenPair")
            .field("id", &self.id)
            .field("token", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(raw: &str) -> TokenPair {
        TokenPair::parse(raw).expect("a pair")
    }

    #[test]
    fn the_pair_splits_on_the_first_colon() {
        let parsed = pair("abc123:def:ghi");
        assert_eq!(parsed.id(), "abc123");
        assert_eq!(parsed.token(), "def:ghi");
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let parsed = pair("  abc123:def\n");
        assert_eq!(parsed.id(), "abc123");
        assert_eq!(parsed.token(), "def");
    }

    #[test]
    fn a_pair_without_a_separator_is_refused() {
        assert_eq!(TokenPair::parse("abcdef"), Err(PairError::NotAPair));
    }

    #[test]
    fn an_empty_half_is_named_rather_than_printed() {
        let error = TokenPair::parse(":secret-value").expect_err("no key id");
        assert_eq!(error, PairError::Empty(Half::Id));
        assert!(error.to_string().contains("key id"));
        assert!(!error.to_string().contains("secret-value"));
    }

    #[test]
    fn a_token_with_a_space_in_it_is_refused() {
        assert_eq!(
            TokenPair::parse("abc123:de f"),
            Err(PairError::Whitespace(Half::Token))
        );
    }

    #[test]
    fn debug_prints_the_id_and_not_the_token() {
        let printed = format!("{:?}", pair("abc123:super-secret"));
        assert!(printed.contains("abc123"));
        assert!(!printed.contains("super-secret"));
    }
}
