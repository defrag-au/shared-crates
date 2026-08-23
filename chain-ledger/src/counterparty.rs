//! What a counterparty IS — a participant, or a service with capabilities.
//!
//! ## Why a flat label was not enough
//!
//! The first version of this carried one string per counterparty: `exchange`,
//! `marketplace`, `service`. It broke on the first real case. `bank.pillar` is
//! a **minting provider AND an airdrop payer** — one label had to be chosen and
//! the other fact discarded. Meanwhile two wallets were plainly exchange hot
//! wallets by shape (thousands of payees, three senders) but nobody could say
//! WHICH exchange, so `name` and `capability` were being forced into one field
//! that could only hold one of them.
//!
//! So: identity and function are separate. A provider may be unnamed and still
//! have known capabilities; it may be named and have several.
//!
//! ## Why the distinction earns its keep on a chart
//!
//! A **CEX** is a BOUNDARY — value crossing it leaves this project's story, and
//! whatever happens beyond is somebody else's business. A **minting provider**
//! is an intermediary the project USED: its onward payments are the project's
//! own distribution. Drawn identically, the second one lies.
//!
//! Both must still be frozen by the frontier — neither should recruit — so
//! "should it expand" and "what is it" are genuinely different questions, and
//! this type answers only the second.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// One thing a provider does. A provider may do several.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    /// Centralised exchange. The hard boundary: deposits arrive at per-user
    /// addresses and withdrawals leave a hot wallet, so on-chain lineage ends
    /// here in both directions.
    Cex,
    /// On-chain AMM or order book. Unlike a CEX this is NOT a boundary — a swap
    /// is a round trip, and value returning from one is the wallet's own money.
    Dex,
    /// Routes across venues rather than holding liquidity itself.
    Aggregator,
    /// NFT marketplace: listings, offers, sales.
    Marketplace,
    /// Runs mints on a project's behalf and receives part of the fund split.
    Minting,
    /// Distributes to a holder set on a project's behalf.
    Airdrop,
    /// Lending, borrowing, or yield.
    Lending,
    /// Stake pool or delegation service.
    Staking,
    /// Moves value to or from another chain. Like [`Self::Cex`], a boundary.
    Bridge,
    /// Holds assets for others — an escrow or custodial wallet.
    Custody,
    /// A PER-CUSTOMER chain exit — the deposit-address shape: stakeless, one
    /// (or very few) payers, strictly one-way. Money in never comes back.
    ///
    /// Distinct from [`Self::Cex`] on purpose: a CEX hot wallet is SHARED
    /// infrastructure (pays thousands, boundary in both directions), while an
    /// off-ramp is one wallet's private door out — usually an exchange deposit
    /// address, whose exchange the chain cannot name. Same boundary
    /// consequence, different investigative meaning: an off-ramp's sole payer
    /// IS an identification.
    Offramp,
}

impl ProviderCapability {
    /// Value arriving from here is the wallet's OWN money coming back, not
    /// income — a swap's return leg, a withdrawal from custody.
    ///
    /// The distinction that stops a round trip being booked as revenue: on one
    /// real ledger that was 58,150 ₳ of a supposed 132,590 ₳ of unexplained
    /// inbound. A CEX is deliberately NOT in this list — money arriving from an
    /// exchange came from OUTSIDE, and calling it a round trip would erase the
    /// most interesting inflow a project has.
    pub fn is_round_trip(self) -> bool {
        matches!(
            self,
            Self::Dex | Self::Aggregator | Self::Lending | Self::Staking
        )
    }

    /// Value crossing here leaves on-chain traceability entirely.
    pub fn is_boundary(self) -> bool {
        matches!(self, Self::Cex | Self::Bridge | Self::Offramp)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cex => "cex",
            Self::Dex => "dex",
            Self::Aggregator => "aggregator",
            Self::Marketplace => "marketplace",
            Self::Minting => "minting",
            Self::Airdrop => "airdrop",
            Self::Lending => "lending",
            Self::Staking => "staking",
            Self::Bridge => "bridge",
            Self::Custody => "custody",
            Self::Offramp => "offramp",
        }
    }

    pub const ALL: [Self; 11] = [
        Self::Cex,
        Self::Dex,
        Self::Aggregator,
        Self::Marketplace,
        Self::Minting,
        Self::Airdrop,
        Self::Lending,
        Self::Staking,
        Self::Bridge,
        Self::Custody,
        Self::Offramp,
    ];
}

impl fmt::Display for ProviderCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProviderCapability {
    type Err = UnknownCapability;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| UnknownCapability(s.to_string()))
    }
}

/// A capability string that is not one we know.
///
/// An error rather than a silent skip: a stored capability nobody recognises
/// means the vocabulary moved on and a reader is being shown less than the data
/// holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownCapability(pub String);

impl fmt::Display for UnknownCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown provider capability: {}", self.0)
    }
}

impl std::error::Error for UnknownCapability {}

/// What a counterparty is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CounterpartyKind {
    /// An ordinary participant — a person, or another project's wallet. The
    /// default reading, and the one that must NOT be applied on suspicion: a
    /// participant wrongly called a provider has its payments erased as round
    /// trips.
    Participant,
    /// A service other people use.
    ///
    /// `name` is optional on purpose. A wallet can be unmistakably a CEX hot
    /// wallet by shape — thousands of distinct payees, almost no senders — while
    /// which exchange it belongs to is unknowable from chain data alone. Forcing
    /// a name would mean either inventing one or discarding the capability.
    Provider {
        name: Option<String>,
        capabilities: BTreeSet<ProviderCapability>,
    },
}

impl CounterpartyKind {
    /// A provider with no capabilities yet established — known to be a service
    /// by its shape, function unknown.
    pub fn unknown_provider() -> Self {
        Self::Provider {
            name: None,
            capabilities: BTreeSet::new(),
        }
    }

    pub fn provider(
        name: impl Into<String>,
        caps: impl IntoIterator<Item = ProviderCapability>,
    ) -> Self {
        Self::Provider {
            name: Some(name.into()),
            capabilities: caps.into_iter().collect(),
        }
    }

    pub fn capabilities(&self) -> &BTreeSet<ProviderCapability> {
        static NONE: std::sync::OnceLock<BTreeSet<ProviderCapability>> = std::sync::OnceLock::new();
        match self {
            Self::Participant => NONE.get_or_init(BTreeSet::new),
            Self::Provider { capabilities, .. } => capabilities,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Participant => None,
            Self::Provider { name, .. } => name.as_deref(),
        }
    }

    /// Add a capability, keeping any already established. Returns whether it
    /// was new.
    ///
    /// Additive because capabilities arrive from DIFFERENT evidence at
    /// different times — `Minting` from a mint's fund split, `Dex` from an
    /// address registry, `Airdrop` from CIP-20 tags — and the second source
    /// must not erase the first. That erasure is exactly what the flat label
    /// did to `bank.pillar`.
    pub fn add_capability(&mut self, cap: ProviderCapability) -> bool {
        match self {
            Self::Participant => {
                *self = Self::Provider {
                    name: None,
                    capabilities: BTreeSet::from([cap]),
                };
                true
            }
            Self::Provider { capabilities, .. } => capabilities.insert(cap),
        }
    }

    /// Name it, without disturbing what it does.
    pub fn set_name(&mut self, new: impl Into<String>) {
        match self {
            Self::Participant => {
                *self = Self::Provider {
                    name: Some(new.into()),
                    capabilities: BTreeSet::new(),
                }
            }
            Self::Provider { name, .. } => *name = Some(new.into()),
        }
    }

    /// Value arriving from this counterparty is the wallet's own, returning.
    pub fn is_round_trip(&self) -> bool {
        self.capabilities().iter().any(|c| c.is_round_trip())
    }

    /// On-chain lineage ends here.
    pub fn is_boundary(&self) -> bool {
        self.capabilities().iter().any(|c| c.is_boundary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case that broke the flat label: one entity, two functions.
    #[test]
    fn a_provider_can_hold_several_capabilities() {
        let mut pillar = CounterpartyKind::provider("bank.pillar", [ProviderCapability::Minting]);
        assert!(pillar.add_capability(ProviderCapability::Airdrop));
        assert!(
            !pillar.add_capability(ProviderCapability::Airdrop),
            "idempotent"
        );

        assert_eq!(pillar.name(), Some("bank.pillar"));
        assert_eq!(pillar.capabilities().len(), 2);
        // A minting/airdrop provider is neither a round trip nor a boundary:
        // its onward payments ARE the project's distribution.
        assert!(!pillar.is_round_trip());
        assert!(!pillar.is_boundary());
    }

    /// The other case: capability known, identity not.
    #[test]
    fn a_provider_may_be_unnamed_and_still_classified() {
        let mut hot = CounterpartyKind::unknown_provider();
        hot.add_capability(ProviderCapability::Cex);
        assert_eq!(hot.name(), None);
        assert!(hot.is_boundary(), "a CEX ends on-chain lineage");
        assert!(
            !hot.is_round_trip(),
            "money from an exchange came from OUTSIDE — treating it as a round trip \
             would erase the most interesting inflow a project has"
        );
    }

    /// A swap's return leg is the wallet's own money; an exchange withdrawal is
    /// not. Conflating them was worth 58,150 ADA on one real ledger.
    #[test]
    fn only_on_chain_services_are_round_trips() {
        assert!(ProviderCapability::Dex.is_round_trip());
        assert!(ProviderCapability::Aggregator.is_round_trip());
        assert!(!ProviderCapability::Cex.is_round_trip());
        assert!(!ProviderCapability::Minting.is_round_trip());
        assert!(ProviderCapability::Cex.is_boundary());
        assert!(ProviderCapability::Bridge.is_boundary());
        assert!(!ProviderCapability::Dex.is_boundary());
    }

    /// Capabilities round-trip through their stored form, and an unrecognised
    /// one is an ERROR rather than a silent drop.
    #[test]
    fn capabilities_round_trip_and_reject_the_unknown() {
        for c in ProviderCapability::ALL {
            assert_eq!(c.as_str().parse::<ProviderCapability>().unwrap(), c);
        }
        assert!("teleporter".parse::<ProviderCapability>().is_err());
    }

    /// Promoting a participant must not lose the capability that prompted it.
    #[test]
    fn adding_a_capability_promotes_a_participant() {
        let mut k = CounterpartyKind::Participant;
        assert!(k.add_capability(ProviderCapability::Dex));
        assert_eq!(k.capabilities().len(), 1);
        assert!(k.is_round_trip());
        k.set_name("Minswap");
        assert_eq!(k.name(), Some("Minswap"));
        assert!(k.is_round_trip(), "naming must not disturb capabilities");
    }
}
