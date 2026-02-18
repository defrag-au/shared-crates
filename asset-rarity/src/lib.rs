//! Pluggable NFT rarity scoring.
//!
//! Provides a common [`Scorer`] trait with implementations for:
//! - [`MagicEdenScorer`] — statistical rarity (product of trait probabilities)
//! - [`ICScorer`] — OpenRarity information content (entropy-normalized IC)
//!
//! # Usage
//! ```
//! use asset_rarity::{Token, Attribute, score_and_rank, MagicEdenScorer};
//!
//! let tokens = vec![
//!     Token::new("1", vec![Attribute::new("hat", "red"), Attribute::new("body", "blue")]),
//!     Token::new("2", vec![Attribute::new("hat", "red"), Attribute::new("body", "green")]),
//!     Token::new("3", vec![Attribute::new("hat", "gold"), Attribute::new("body", "blue")]),
//! ];
//!
//! let ranked = score_and_rank(&MagicEdenScorer, &tokens);
//! assert_eq!(ranked[0].rank, 1); // rarest token
//! ```

mod collection;
mod information_content;
mod magic_eden;
mod ranker;

pub use collection::{build_collection, Collection};
pub use information_content::ICScorer;
pub use magic_eden::MagicEdenScorer;

/// A single trait_type/value attribute (Solana Metaplex format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub trait_type: String,
    pub value: String,
}

impl Attribute {
    pub fn new(trait_type: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            trait_type: trait_type.into(),
            value: value.into(),
        }
    }
}

/// A token with its id and flat attribute list.
#[derive(Debug, Clone)]
pub struct Token {
    pub id: String,
    pub attributes: Vec<Attribute>,
}

impl Token {
    pub fn new(id: impl Into<String>, attributes: Vec<Attribute>) -> Self {
        Self {
            id: id.into(),
            attributes,
        }
    }

    /// Count of non-null attributes on this token.
    pub fn trait_count(&self) -> usize {
        self.attributes.len()
    }
}

/// A scored and ranked token.
#[derive(Debug, Clone)]
pub struct RankedToken {
    pub id: String,
    pub score: f64,
    pub rank: usize,
}

/// Pluggable scoring algorithm.
pub trait Scorer {
    /// Score all tokens against collection statistics.
    /// Returns `(token_id, score)` pairs.
    fn score(&self, collection: &Collection, tokens: &[Token]) -> Vec<(String, f64)>;

    /// Whether lower scores indicate rarer tokens.
    /// - `true` for Magic Eden (product of probabilities)
    /// - `false` for IC (higher information = rarer)
    fn lower_is_rarer(&self) -> bool;

    /// Human-readable name for this algorithm.
    fn name(&self) -> &str;
}

/// Build collection stats and score+rank all tokens with the given algorithm.
pub fn score_and_rank(scorer: &dyn Scorer, tokens: &[Token]) -> Vec<RankedToken> {
    let collection = build_collection(tokens);
    let scores = scorer.score(&collection, tokens);
    ranker::rank(scores, scorer.lower_is_rarer())
}
