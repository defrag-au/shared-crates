//! Row types, one per table in [`crate::schema`].
//!
//! Plain data with no driver types in sight, so the same records come back from
//! rusqlite on the desktop and from a Durable Object's SQL on the edge.
//!
//! Enum-ish columns (`basis`, `status`, `chain`) are stored as `String` rather
//! than a Rust enum. A case file outlives the build that wrote it, and a strict
//! enum turns an unrecognised value from a newer version into a hard parse
//! failure over the whole case — where a string lets the reader keep the row,
//! show it, and refuse only to reason about it. Helpers below do the mapping for
//! the values this build knows.

use serde::{Deserialize, Serialize};

use chain_ledger::{Basis, Chain};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyRecord {
    pub key: String,
    pub chain: String,
    pub has_stake: bool,
    pub label: Option<String>,
    pub basis: String,
    pub source: Option<String>,
    pub note: Option<String>,
    pub updated_at: i64,
}

impl PartyRecord {
    /// The stored basis, or [`Basis::Asserted`] when it is not a value this
    /// build knows.
    ///
    /// Unknown falls to the *weakest* reading on purpose. If a future version
    /// adds a basis, treating it as strong would let an unknown claim inherit
    /// authority; treating it as weak is wrong in the cautious direction.
    pub fn basis(&self) -> Basis {
        match self.basis.as_str() {
            "observed" => Basis::Observed,
            "derived" => Basis::Derived,
            _ => Basis::Asserted,
        }
    }

    pub fn chain(&self) -> Option<Chain> {
        match self.chain.as_str() {
            "cardano" => Some(Chain::Cardano),
            "solana" => Some(Chain::Solana),
            _ => None,
        }
    }

    /// An identity somebody asserted with nothing to attribute it to.
    pub fn is_unsourced_assertion(&self) -> bool {
        self.basis() == Basis::Asserted && self.source.as_deref().unwrap_or("").is_empty()
    }
}

pub fn basis_str(b: Basis) -> &'static str {
    match b {
        Basis::Observed => "observed",
        Basis::Asserted => "asserted",
        Basis::Derived => "derived",
    }
}

pub fn chain_str(c: Chain) -> &'static str {
    match c {
        Chain::Cardano => "cardano",
        Chain::Solana => "solana",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterRecord {
    pub name: String,
    pub color: Option<String>,
    pub note: Option<String>,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportRecord {
    pub summary: String,
    pub basis: String,
    pub source: Option<String>,
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimRecord {
    pub id: String,
    pub statement: String,
    /// Absent until someone writes one. Capture is free; the falsifier gates
    /// promotion, not creation.
    pub falsifier: Option<String>,
    pub status: String,
    pub outcome: Option<String>,
    pub support: Vec<SupportRecord>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ClaimRecord {
    /// Whether this claim may be cited.
    ///
    /// Deliberately strict about *both* halves: a status of `survived` with no
    /// falsifier and no outcome recorded is a badge with nothing behind it, and
    /// an unknown status is not load-bearing either.
    pub fn is_load_bearing(&self) -> bool {
        self.status == "survived" && self.falsifier.is_some() && self.outcome.is_some()
    }

    /// A verdict claimed without the test that produced it.
    pub fn is_unevidenced_verdict(&self) -> bool {
        matches!(self.status.as_str(), "survived" | "refuted")
            && (self.falsifier.is_none() || self.outcome.is_none())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRecord {
    pub id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub body: String,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(status: &str, falsifier: Option<&str>, outcome: Option<&str>) -> ClaimRecord {
        ClaimRecord {
            id: "c1".into(),
            statement: "s".into(),
            falsifier: falsifier.map(Into::into),
            status: status.into(),
            outcome: outcome.map(Into::into),
            support: vec![],
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn only_a_fully_evidenced_survived_claim_is_citable() {
        assert!(claim("survived", Some("f"), Some("o")).is_load_bearing());
        assert!(!claim("survived", None, Some("o")).is_load_bearing());
        assert!(!claim("survived", Some("f"), None).is_load_bearing());
        assert!(!claim("untested", None, None).is_load_bearing());
        assert!(!claim("refuted", Some("f"), Some("o")).is_load_bearing());
    }

    #[test]
    fn a_verdict_missing_its_test_is_flagged() {
        assert!(claim("survived", None, Some("o")).is_unevidenced_verdict());
        assert!(claim("refuted", Some("f"), None).is_unevidenced_verdict());
        assert!(!claim("survived", Some("f"), Some("o")).is_unevidenced_verdict());
        // A provisional claim is not expected to have one.
        assert!(!claim("untested", None, None).is_unevidenced_verdict());
    }

    /// A value from a newer build must not be read as strong.
    #[test]
    fn an_unknown_basis_falls_to_the_weakest_reading() {
        let p = PartyRecord {
            key: "k".into(),
            chain: "cardano".into(),
            has_stake: true,
            label: None,
            basis: "corroborated-by-three-oracles".into(),
            source: None,
            note: None,
            updated_at: 0,
        };
        assert_eq!(p.basis(), Basis::Asserted);
        assert!(p.is_unsourced_assertion());
    }

    /// An unrecognised status is likewise not citable.
    #[test]
    fn an_unknown_status_is_not_load_bearing() {
        assert!(!claim("mostly-confirmed", Some("f"), Some("o")).is_load_bearing());
    }

    #[test]
    fn basis_and_chain_round_trip_through_their_strings() {
        for b in [Basis::Observed, Basis::Asserted, Basis::Derived] {
            let p = PartyRecord {
                key: "k".into(),
                chain: "cardano".into(),
                has_stake: true,
                label: None,
                basis: basis_str(b).into(),
                source: Some("x".into()),
                note: None,
                updated_at: 0,
            };
            assert_eq!(p.basis(), b);
        }
        for c in [Chain::Cardano, Chain::Solana] {
            let p = PartyRecord {
                key: "k".into(),
                chain: chain_str(c).into(),
                has_stake: true,
                label: None,
                basis: "observed".into(),
                source: None,
                note: None,
                updated_at: 0,
            };
            assert_eq!(p.chain(), Some(c));
        }
    }

    /// An empty string is not attribution.
    #[test]
    fn an_empty_source_does_not_count_as_attributed() {
        let mk = |src: Option<&str>| PartyRecord {
            key: "k".into(),
            chain: "cardano".into(),
            has_stake: true,
            label: None,
            basis: "asserted".into(),
            source: src.map(Into::into),
            note: None,
            updated_at: 0,
        };
        assert!(mk(None).is_unsourced_assertion());
        assert!(mk(Some("")).is_unsourced_assertion());
        assert!(!mk(Some("operator, 2026-08-12")).is_unsourced_assertion());
    }
}
