//! Statuses: conditions that are true **at a slot**, not things that happened.
//!
//! An achievement is earned once and remembered. A status is asked every time. A campaign needs
//! both: the score that opens a phase comes from the fold, and the gate — "holds a Perp", "holds
//! a leader role", "holds a partner collection" — is state, re-read at the moment of the claim.
//!
//! # Why this is not a [`Predicate`](crate::Predicate)
//!
//! Every predicate in the achievement grammar answers a question about history: a lot that
//! existed, a span that elapsed, a drawdown that happened. Two of them look like state — a
//! holding count and a trait — but the fold answers them *as at the reading slot*, which is the
//! bundle's build time and not the claim's. That is the right answer for a badge and the wrong
//! answer for a gate.
//!
//! So a status is deliberately narrow, and is always evaluated against a [`StatusView`] the
//! caller assembled from a read it just made: a `POST /account_assets` for a Cardano stake, a
//! `balanceOf` and a trait lookup for an EVM wallet, a row in a DO. This module knows nothing
//! about where the read came from, which is what lets one vocabulary serve a worker alarm, a
//! user's on-demand check and a marketing preview.
//!
//! # What it deliberately cannot ask
//!
//! Nothing about the past. There is no "ever held" here, no span, no drawdown: those are
//! achievements, and a status that could ask them would be a second, silently different way to
//! score the same history.

use serde::{Deserialize, Serialize};

use chain_events::TokenRef;

/// A condition on what an account holds, at one instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusPredicate {
    /// Holds at least this many units of the collection.
    HoldsOf {
        /// Which collection the read must have covered.
        collection: TokenRef,
        /// Whole units, inclusive. An NFT is one unit.
        at_least: u64,
    },
    /// Holds an asset carrying the trait.
    ///
    /// `value: None` accepts any non-empty value the category has, which is what "holds a
    /// leader Perp" means: the category is the role, the value is which leader. `"None"` is
    /// how the trait files spell an absent trait, and is never a match.
    HoldsTrait {
        /// Which collection the read must have covered.
        collection: TokenRef,
        /// The trait category — `Leader`, `Multiverse Leader`, `Faction`.
        category: String,
        /// A specific value, or `None` for any.
        value: Option<String>,
    },
}

impl StatusPredicate {
    /// The collection this status is asked about.
    pub fn collection(&self) -> &TokenRef {
        match self {
            Self::HoldsOf { collection, .. } | Self::HoldsTrait { collection, .. } => collection,
        }
    }
}

/// What a status is asked about: one account's holdings, at one instant, for one collection.
#[derive(Debug, Clone, Copy)]
pub struct StatusView<'a> {
    /// The collection the read covered, in the same CAIP-19 form a predicate names.
    pub collection: &'a str,
    /// Units of that collection held at the read.
    pub units: u64,
    /// The traits on the units held at the read, as `(category, value)`.
    pub traits: &'a [(String, String)],
}

impl StatusView<'_> {
    /// A read that saw no holdings — the state a wallet that sold everything is in.
    pub fn empty(collection: &str) -> StatusView<'_> {
        StatusView {
            collection,
            units: 0,
            traits: &[],
        }
    }
}

/// Why a status could not be answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusError {
    /// The predicate names a collection the read did not cover.
    ///
    /// Not `false`. A caller that folded this into `false` would gate every identity out of a
    /// campaign over a missing read, which is the failure that looks like "nobody qualifies".
    OtherCollection {
        /// What the status asked about.
        asked: String,
        /// What the read actually covered.
        read: String,
    },
}

impl std::fmt::Display for StatusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OtherCollection { asked, read } => write!(
                formatter,
                "the status asks about {asked}, but the read covered {read}"
            ),
        }
    }
}

impl std::error::Error for StatusError {}

/// Is the status true, given one account's holdings at one instant?
pub fn status_holds(
    predicate: &StatusPredicate,
    view: &StatusView<'_>,
) -> Result<bool, StatusError> {
    let collection = predicate.collection();
    if collection.to_string() != view.collection {
        return Err(StatusError::OtherCollection {
            asked: collection.to_string(),
            read: view.collection.to_string(),
        });
    }

    Ok(match predicate {
        StatusPredicate::HoldsOf { at_least, .. } => view.units >= *at_least,
        StatusPredicate::HoldsTrait {
            category, value, ..
        } => view.traits.iter().any(|(held_category, held_value)| {
            held_category == category
                && held_value != "None"
                && value.as_ref().is_none_or(|wanted| wanted == held_value)
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const PERPS: &str =
        "cardano:mainnet/cnt:e6ba9c0ff27be029442c32533c6efd956a60d15ecb976acbb64c4de0";

    fn perps() -> TokenRef {
        TokenRef::from_str(PERPS).unwrap()
    }

    fn view<'a>(units: u64, traits: &'a [(String, String)]) -> StatusView<'a> {
        StatusView {
            collection: PERPS,
            units,
            traits,
        }
    }

    #[test]
    fn holds_of_is_inclusive_at_the_boundary() {
        let predicate = StatusPredicate::HoldsOf {
            collection: perps(),
            at_least: 1,
        };
        assert!(status_holds(&predicate, &view(1, &[])).unwrap());
        assert!(!status_holds(&predicate, &view(0, &[])).unwrap());
    }

    #[test]
    fn a_read_of_another_collection_is_an_error_and_never_false() {
        let predicate = StatusPredicate::HoldsOf {
            collection: TokenRef::from_str(
                "cardano:mainnet/cnt:d9d59e71eb99e653a493956a6826a406060706bd8c9a2427b3564f64/5045525020434f494e",
            )
            .unwrap(),
            at_least: 1,
        };
        let error = status_holds(&predicate, &view(5, &[])).unwrap_err();
        assert!(matches!(error, StatusError::OtherCollection { .. }));
    }

    #[test]
    fn a_trait_status_accepts_any_value_when_none_is_named() {
        let predicate = StatusPredicate::HoldsTrait {
            collection: perps(),
            category: "Leader".to_string(),
            value: None,
        };
        let held = [("Leader".to_string(), "AI AI".to_string())];
        assert!(status_holds(&predicate, &view(1, &held)).unwrap());
        assert!(!status_holds(&predicate, &view(1, &[])).unwrap());
    }

    #[test]
    fn a_named_trait_value_has_to_match() {
        let predicate = StatusPredicate::HoldsTrait {
            collection: perps(),
            category: "Multiverse Leader".to_string(),
            value: Some("Natanael".to_string()),
        };
        let natanael = [("Multiverse Leader".to_string(), "Natanael".to_string())];
        let other = [("Multiverse Leader".to_string(), "Someone Else".to_string())];
        assert!(status_holds(&predicate, &view(1, &natanael)).unwrap());
        assert!(!status_holds(&predicate, &view(1, &other)).unwrap());
    }

    #[test]
    fn the_none_value_the_trait_files_write_is_not_a_leader() {
        let predicate = StatusPredicate::HoldsTrait {
            collection: perps(),
            category: "Leader".to_string(),
            value: None,
        };
        let absent = [("Leader".to_string(), "None".to_string())];
        assert!(!status_holds(&predicate, &view(1, &absent)).unwrap());
    }
}
