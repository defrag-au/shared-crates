//! The value types. Deliberately small: a transaction reduces to inputs and
//! outputs keyed by *party*, and everything downstream is derived from that.

use serde::{Deserialize, Serialize};

/// Which chain a view came from. Carried on the data rather than inferred,
/// because the strength of a provenance walk differs per chain and the UI has
/// to say which it is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chain {
    Cardano,
    Solana,
}

impl Chain {
    /// Whether a per-unit custody trace is possible at all.
    ///
    /// UTxO chains link each input to the exact output it consumes, so a walk
    /// is proof. Account chains have no such edge — anything resembling a trace
    /// is reconstructed from instruction ordering, which is inference.
    pub fn has_utxo_provenance(self) -> bool {
        matches!(self, Chain::Cardano)
    }
}

/// A counterparty identity, normalised per chain.
///
/// Cardano resolves to the stake key when there is one, else the payment
/// address — and the *absence* of a stake key is signal, not a missing value:
/// enterprise addresses are the shape an off-ramp takes. A resolver that
/// silently falls back to the payment address while presenting it as a stake
/// key erases that distinction, which is how every stakeless counterparty in an
/// early Mekka run ended up misclassified as an ordinary wallet.
///
/// Solana resolves to the *owner* of a token account, never the token account
/// itself — the account-per-mint layout otherwise splits one wallet into many
/// apparent parties.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Party {
    pub chain: Chain,
    pub key: String,
    /// False when the key is a bare payment/enterprise address with no staking
    /// credential. Kept explicit so `is_stakeless()` is a fact about the
    /// address rather than a string-prefix guess at the call site.
    pub has_stake_credential: bool,
}

impl Party {
    pub fn cardano_stake(key: impl Into<String>) -> Self {
        Self {
            chain: Chain::Cardano,
            key: key.into(),
            has_stake_credential: true,
        }
    }

    /// A Cardano payment/enterprise address with no staking credential.
    pub fn cardano_enterprise(key: impl Into<String>) -> Self {
        Self {
            chain: Chain::Cardano,
            key: key.into(),
            has_stake_credential: false,
        }
    }

    pub fn solana_owner(key: impl Into<String>) -> Self {
        Self {
            chain: Chain::Solana,
            key: key.into(),
            has_stake_credential: true,
        }
    }

    /// Off-ramp *shape* — never an assertion about origin. Exchange withdrawals
    /// and OTC legs land on stakeless addresses, but so do plenty of ordinary
    /// things; this reports the shape and nothing more.
    pub fn is_stakeless(&self) -> bool {
        !self.has_stake_credential
    }
}

/// How firmly an identity is known. The field that keeps an assertion from
/// being read as an observation.
///
/// A per-machine deposit figure supplied in conversation, reconciled against an
/// on-chain total by solving for the exchange rate that made it fit, spent
/// three days recorded as established fact before a real rate check falsified
/// it. Nothing in that workflow distinguished the two, so this is a required
/// field on every label rather than an optional annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Read off the chain: balance, transaction count, absence of a stake key.
    Observed,
    /// Supplied from outside the chain — a person, a document, a screenshot.
    Asserted,
    /// Follows from another recorded claim.
    Derived,
}

impl Basis {
    /// Whether the chain alone supports this. Used by the UI to decide whether
    /// a label may be presented without a visible source.
    pub fn is_self_supporting(self) -> bool {
        matches!(self, Basis::Observed)
    }

    /// Stored form. Same rule as [`AliasKind`]: one enum at both ends of the
    /// store, so a writer and a reader cannot drift apart.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::Derived => "derived",
            Self::Asserted => "asserted",
        }
    }

    /// Parse a stored basis, defaulting to the WEAKEST reading.
    ///
    /// Deliberately infallible and deliberately pessimistic: a value we cannot
    /// parse must never be promoted to `Observed`, because "the chain says so"
    /// is the one claim a reader is entitled to take without a source. An
    /// unknown string is somebody's assertion until proven otherwise.
    pub fn parse(s: &str) -> Self {
        match s {
            "observed" => Self::Observed,
            "derived" => Self::Derived,
            _ => Self::Asserted,
        }
    }

    /// Weakest last, so a picker built from this reads in the direction of
    /// decreasing confidence.
    pub const ALL: [Self; 3] = [Self::Observed, Self::Derived, Self::Asserted];
}

impl std::fmt::Display for Basis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A kind of name a party goes by, beyond its key.
///
/// A wallet is a stake key to the model, but people hold a payment address or
/// a `$handle`. Both are observable on-chain, so an importer can record them
/// and a reader can resolve whatever the user has to hand. Persisted by its
/// [`AliasKind::as_str`] form; parsed back with `FromStr`. One enum at both
/// ends of the store, so the two cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasKind {
    /// A payment address (bech32 `addr1…`) resolved to this party.
    Address,
    /// An ADA Handle held by this party, stored WITHOUT the `$`.
    Handle,
}

impl AliasKind {
    pub const ALL: [Self; 2] = [Self::Address, Self::Handle];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Address => "address",
            Self::Handle => "handle",
        }
    }
}

impl std::fmt::Display for AliasKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An alias kind string the model does not know. Kept as an error rather than
/// mapped to a default so an unknown row is *reported*, never silently binned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownAliasKind(pub String);

impl std::fmt::Display for UnknownAliasKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown alias kind `{}`", self.0)
    }
}

impl std::error::Error for UnknownAliasKind {}

impl std::str::FromStr for AliasKind {
    type Err = UnknownAliasKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| UnknownAliasKind(s.to_string()))
    }
}

/// Index of a party within a [`TxView`], so deltas and movements can refer to
/// parties without cloning keys around.
pub type PartyRef = usize;

/// One consumed input. `source` is the transaction that created the output
/// being spent — present only on UTxO chains, and the edge a provenance walk
/// follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxInput {
    pub party: PartyRef,
    pub value: i128,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxOutput {
    pub party: PartyRef,
    pub value: i128,
}

/// A normalised transaction: who put value in, who took value out.
///
/// There is intentionally no accessor returning raw outputs. Every consumer
/// goes through [`crate::net_deltas`], which is what makes gross attribution
/// unavailable rather than merely discouraged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxView {
    pub chain: Chain,
    pub tx_id: String,
    /// Seconds since the Unix epoch.
    pub timestamp: i64,
    pub parties: Vec<Party>,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
}

impl TxView {
    pub fn party(&self, r: PartyRef) -> Option<&Party> {
        self.parties.get(r)
    }

    /// Position of `party` in this view, if it took part at all.
    pub fn find(&self, party: &Party) -> Option<PartyRef> {
        self.parties.iter().position(|p| p == party)
    }
}

/// A party's net movement in one transaction — the only currency of this crate.
///
/// Positive means value arrived. Note that a *negative* delta does not imply
/// the counterparty received the money: a wallet paying 400 out of a 4,000 UTxO
/// also creates a 3,600 change output, and treating that change as the payee's
/// inverts the trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxDelta {
    pub tx_id: String,
    pub timestamp: i64,
    pub party: Party,
    pub delta: i128,
}

impl TxDelta {
    pub fn is_inflow(&self) -> bool {
        self.delta > 0
    }
}

/// Directed value between two parties in one transaction, attributed pro-rata
/// across the net-opposite side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movement {
    pub tx_id: String,
    pub timestamp: i64,
    pub from: Party,
    pub to: Party,
    pub value: i128,
}

/// Whether a per-unit custody trace is meaningful for a given view, and why
/// not when it isn't. Surfaced so a UI can badge a walk "proven" or "inferred"
/// instead of showing both the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub chain: Chain,
    pub available: bool,
}

impl Provenance {
    pub fn for_chain(chain: Chain) -> Self {
        Self {
            chain,
            available: chain.has_utxo_provenance(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stored form must round-trip, and an unreadable one must degrade
    /// DOWNWARD. Promoting an unknown string to `Observed` would let a claim
    /// nobody can support present itself as chain fact.
    #[test]
    fn basis_round_trips_and_degrades_to_the_weakest_reading() {
        for b in Basis::ALL {
            assert_eq!(Basis::parse(b.as_str()), b, "{b}");
        }
        assert_eq!(Basis::parse("who knows"), Basis::Asserted);
        assert_eq!(Basis::parse(""), Basis::Asserted);
        assert!(Basis::Observed.is_self_supporting());
        assert!(!Basis::Derived.is_self_supporting());
        assert!(!Basis::Asserted.is_self_supporting());
    }

    #[test]
    fn alias_kind_round_trips_through_its_stored_form() {
        for k in AliasKind::ALL {
            let s = k.as_str();
            assert_eq!(s.parse::<AliasKind>(), Ok(k), "{s}");
            assert_eq!(k.to_string(), s);
        }
        // An unknown kind is an ERROR, not a silent default.
        assert_eq!(
            "twitter".parse::<AliasKind>(),
            Err(UnknownAliasKind("twitter".into()))
        );
    }
}
