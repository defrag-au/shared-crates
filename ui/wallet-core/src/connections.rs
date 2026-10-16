//! The set of active wallet connections — one per chain, keyed by family.
//!
//! ## Why a set and not a slot
//!
//! A reader can have a Cardano wallet and an EVM wallet connected at the same
//! time, and the two are independent: a stake-key signature comes from one, a
//! `personal_sign` from the other, and disconnecting either says nothing about
//! the other. A single connection slot cannot express that — it makes
//! "connected" mean "a CIP-30 wallet is connected", which is a claim about
//! Cardano dressed up as a claim about wallets.
//!
//! ## The key is the chain, and the payload carries it
//!
//! Nothing here takes a `(family, connection)` pair: a connection knows its own
//! family ([`ChainConnection::family`]) and the inserters derive the key from
//! it, so a Cardano session cannot be filed under EVM. That is the whole reason
//! [`Self::set_connected`] does not take a family argument.
//!
//! ## At most one connection per chain
//!
//! Which is the granularity an *action* resolves at — an action needs a chain,
//! and within it the wallet the reader connected. Two EVM wallets live at once
//! is a disambiguation no action site could answer, so it is not representable
//! here on purpose. If it ever needs to be, the key grows a wallet id and the
//! display projection ([`ConnectedIdentity`]) does not change.
//!
//! ## Supersedes `ConnectionState`
//!
//! [`crate::ConnectionState`] is the single-slot, Cardano-shaped predecessor of
//! this type. It is still what the macroquad widgets (`ui-components`) and
//! `wallet-leptos` hold, and migrating them is its own change; nothing new
//! should be built on it.

use std::collections::BTreeMap;
use std::fmt;

pub use chains::ChainFamily;

use crate::cip30::WalletApi;
use crate::evm::EvmWalletInfo;
use crate::types::{Network, WalletProvider};

/// The chains this crate can actually connect, in the order they are offered.
///
/// Midnight is a [`ChainFamily`] with no bridge behind it — it joins this list
/// the day there is a wallet to connect on it, and every surface that reads
/// this list (a picker, a cluster, a capability check) learns about it at once.
pub const CONNECTABLE: [ChainFamily; 2] = [ChainFamily::Cardano, ChainFamily::Evm];

/// One chain's connection, as only that family can describe it.
///
/// The enum is what signing has to match on — the families differ exactly there
/// — while anything that merely *displays* a connection goes through
/// [`ChainConnection::identity`] and never looks inside.
#[derive(Clone, Debug)]
pub enum ChainConnection {
    /// A CIP-30 session: what `enable()` and a change address produced.
    Cardano(CardanoConnection),
    /// An EIP-1193 session: what `eth_requestAccounts` and `personal_sign` work
    /// against.
    Evm(EvmConnection),
}

impl ChainConnection {
    /// Which chain this is a connection to. The same answer as the map key.
    pub fn family(&self) -> ChainFamily {
        match self {
            Self::Cardano(_) => ChainFamily::Cardano,
            Self::Evm(_) => ChainFamily::Evm,
        }
    }

    /// What any surface can show: which chain, which wallet, which account.
    pub fn identity(&self) -> ConnectedIdentity<'_> {
        match self {
            Self::Cardano(cardano) => cardano.identity(),
            Self::Evm(evm) => evm.identity(),
        }
    }

    /// This connection as a CIP-30 session, when it is one.
    pub fn cardano(&self) -> Option<&CardanoConnection> {
        match self {
            Self::Cardano(cardano) => Some(cardano),
            Self::Evm(_) => None,
        }
    }

    /// This connection as an EIP-1193 session, when it is one.
    pub fn evm(&self) -> Option<&EvmConnection> {
        match self {
            Self::Evm(evm) => Some(evm),
            Self::Cardano(_) => None,
        }
    }
}

/// A CIP-30 session.
#[derive(Clone)]
pub struct CardanoConnection {
    /// The wallet that answered.
    pub provider: WalletProvider,
    /// The change address, hex — what the connection's UTxOs pay to.
    pub address_hex: String,
    /// The bech32 stake address, when the change address carries a stake
    /// credential. `None` is a real answer: an enterprise address has no stake
    /// part, and a signature from it proves the payment key, not a stake key.
    pub stake_address: Option<String>,
    pub network: Network,
    /// The handle this connection signs with — `signData`, `signTx`, UTxO
    /// queries. `None` when nothing kept it: display does not need it, signing
    /// does.
    pub api: Option<WalletApi>,
    /// The extension's own icon, as it supplied it (a data URL).
    pub icon: Option<String>,
}

impl CardanoConnection {
    pub fn identity(&self) -> ConnectedIdentity<'_> {
        ConnectedIdentity {
            family: ChainFamily::Cardano,
            wallet: self.provider.display_name(),
            // The stake address when there is one: it is the account a proof
            // names, and the payment address of a base address restates it.
            account: self.stake_address.as_deref().unwrap_or(&self.address_hex),
            icon: self.icon.as_deref(),
        }
    }
}

/// Manual, because the handle in here is a live `JsValue`: it has no `Debug`,
/// and if it did it would print nothing a reader could use.
impl fmt::Debug for CardanoConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CardanoConnection")
            .field("provider", &self.provider)
            .field("address_hex", &self.address_hex)
            .field("stake_address", &self.stake_address)
            .field("network", &self.network)
            .field("api", &self.api.is_some())
            .finish()
    }
}

/// An EIP-1193 session — `eth_requestAccounts` answered, and this is what it
/// said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmConnection {
    /// The wallet that answered. Its [`id`](EvmWalletInfo::id) is the identity
    /// to reach the provider by again.
    pub wallet: EvmWalletInfo,
    /// The account it is signed in as — an `0x…` address, as the provider gave
    /// it. EIP-55 checksumming is the provider's business; storing what it said
    /// is what makes a later "this wallet does not sign with that address"
    /// answerable.
    pub address: String,
}

impl EvmConnection {
    pub fn identity(&self) -> ConnectedIdentity<'_> {
        ConnectedIdentity {
            family: ChainFamily::Evm,
            wallet: &self.wallet.name,
            account: &self.address,
            icon: self.wallet.icon.as_deref(),
        }
    }
}

/// What one chain's slot is doing.
#[derive(Clone, Debug)]
pub enum ConnectionPhase {
    /// A connect is in flight and nothing has answered yet.
    Connecting,
    /// The chain has a wallet, and this is it.
    Connected(ChainConnection),
    /// The last attempt on this chain failed. The string is what to show the
    /// reader — the wallet's own words, or the bridge's.
    Error(String),
}

/// Any connection, as a surface that does not care which chain it is shows it.
///
/// The account cluster, a header button and a connect flow's receipt all want
/// the same four things, and none of them should have to know how a family
/// spells an address to get them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectedIdentity<'a> {
    pub family: ChainFamily,
    /// The wallet's own name — "Eternl", "Rabby".
    pub wallet: &'a str,
    /// The account it is signed in as: a bech32 `stake1…` on Cardano, a hex
    /// `0x…` on EVM.
    pub account: &'a str,
    /// The wallet's icon, when it supplied one.
    pub icon: Option<&'a str>,
}

/// The set of active connections: at most one per chain.
#[derive(Clone, Debug, Default)]
pub struct WalletConnections {
    by_chain: BTreeMap<ChainFamily, ConnectionPhase>,
}

impl WalletConnections {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every chain with something to say, in [`ChainFamily::ALL`] order.
    ///
    /// The order is the vocabulary's, not the map's, so a cluster of pills
    /// cannot reshuffle when a second chain connects.
    pub fn iter(&self) -> impl Iterator<Item = (ChainFamily, &ConnectionPhase)> {
        ChainFamily::ALL
            .into_iter()
            .filter_map(|family| self.by_chain.get(&family).map(|phase| (family, phase)))
    }

    pub fn phase(&self, family: ChainFamily) -> Option<&ConnectionPhase> {
        self.by_chain.get(&family)
    }

    /// The connection on `family`, when that chain has one.
    pub fn connection(&self, family: ChainFamily) -> Option<&ChainConnection> {
        match self.by_chain.get(&family) {
            Some(ConnectionPhase::Connected(connection)) => Some(connection),
            _ => None,
        }
    }

    /// Every connected chain, in [`ChainFamily::ALL`] order.
    pub fn connected(&self) -> impl Iterator<Item = &ChainConnection> {
        self.iter().filter_map(|(_, phase)| match phase {
            ConnectionPhase::Connected(connection) => Some(connection),
            _ => None,
        })
    }

    /// One line per connected chain — what a cluster shows.
    pub fn identities(&self) -> impl Iterator<Item = ConnectedIdentity<'_>> {
        self.connected().map(ChainConnection::identity)
    }

    pub fn is_connected(&self, family: ChainFamily) -> bool {
        self.connection(family).is_some()
    }

    pub fn is_connecting(&self, family: ChainFamily) -> bool {
        matches!(self.phase(family), Some(ConnectionPhase::Connecting))
    }

    pub fn error(&self, family: ChainFamily) -> Option<&str> {
        match self.phase(family) {
            Some(ConnectionPhase::Error(error)) => Some(error),
            _ => None,
        }
    }

    /// Whether any chain at all is connected — the question a header asks before
    /// it decides to draw a connect button.
    pub fn any_connected(&self) -> bool {
        self.connected().next().is_some()
    }

    /// Whether any chain is mid-connect.
    pub fn any_connecting(&self) -> bool {
        self.iter()
            .any(|(_, phase)| matches!(phase, ConnectionPhase::Connecting))
    }

    /// A connect has started on `family`. Whatever was there is replaced: the
    /// chain is mid-answer, so an older answer is no longer the truth.
    pub fn set_connecting(&mut self, family: ChainFamily) {
        self.by_chain.insert(family, ConnectionPhase::Connecting);
    }

    /// A chain answered. The key comes *from* the connection.
    pub fn set_connected(&mut self, connection: ChainConnection) {
        self.by_chain
            .insert(connection.family(), ConnectionPhase::Connected(connection));
    }

    /// The last attempt on `family` failed.
    pub fn set_error(&mut self, family: ChainFamily, error: impl Into<String>) {
        self.by_chain
            .insert(family, ConnectionPhase::Error(error.into()));
    }

    /// Drop a chain's phase.
    ///
    /// Disconnecting one chain says nothing about another — which is exactly
    /// what a single-slot model could not express.
    pub fn remove(&mut self, family: ChainFamily) -> Option<ConnectionPhase> {
        self.by_chain.remove(&family)
    }

    pub fn clear(&mut self) {
        self.by_chain.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eternl() -> CardanoConnection {
        CardanoConnection {
            provider: WalletProvider::Eternl,
            address_hex: format!("01{}", "ab".repeat(28)),
            stake_address: Some(
                "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9".into(),
            ),
            network: Network::Mainnet,
            api: None,
            icon: None,
        }
    }

    fn rabby() -> EvmConnection {
        EvmConnection {
            wallet: EvmWalletInfo {
                id: "io.rabby".to_string().into(),
                name: "Rabby".into(),
                icon: None,
                rdns: Some("io.rabby".into()),
            },
            address: format!("0x{}", "1f".repeat(20)),
        }
    }

    /// The whole point of the type: two chains are connected at once, and each
    /// one's removal leaves the other alone.
    #[test]
    fn two_chains_connect_and_disconnect_independently() {
        let mut connections = WalletConnections::new();
        connections.set_connected(ChainConnection::Cardano(eternl()));
        connections.set_connected(ChainConnection::Evm(rabby()));

        assert!(connections.is_connected(ChainFamily::Cardano));
        assert!(connections.is_connected(ChainFamily::Evm));
        assert_eq!(connections.connected().count(), 2);

        connections.remove(ChainFamily::Evm);
        assert!(connections.is_connected(ChainFamily::Cardano));
        assert!(!connections.is_connected(ChainFamily::Evm));
    }

    /// A Cardano session cannot be filed under EVM: the family is the
    /// connection's own, and `set_connected` takes no key.
    #[test]
    fn the_key_comes_from_the_connection() {
        let mut connections = WalletConnections::new();
        connections.set_connected(ChainConnection::Cardano(eternl()));
        assert!(connections.phase(ChainFamily::Cardano).is_some());
        assert!(connections.phase(ChainFamily::Evm).is_none());
    }

    /// Reading order is the vocabulary's, so a cluster cannot reshuffle — EVM
    /// is inserted first and still comes out after Cardano.
    #[test]
    fn iteration_is_in_chain_family_order() {
        let mut connections = WalletConnections::new();
        connections.set_connected(ChainConnection::Evm(rabby()));
        connections.set_connected(ChainConnection::Cardano(eternl()));

        let families: Vec<_> = connections.iter().map(|(family, _)| family).collect();
        assert_eq!(families, [ChainFamily::Cardano, ChainFamily::Evm]);
    }

    /// The display projection answers the same four questions on both chains.
    #[test]
    fn every_connection_projects_to_an_identity() {
        let cardano = ChainConnection::Cardano(eternl());
        assert_eq!(cardano.identity().family, ChainFamily::Cardano);
        assert_eq!(cardano.identity().wallet, "Eternl");
        assert!(cardano.identity().account.starts_with("stake1"));

        let evm = ChainConnection::Evm(rabby());
        assert_eq!(evm.identity().family, ChainFamily::Evm);
        assert_eq!(evm.identity().wallet, "Rabby");
        assert!(evm.identity().account.starts_with("0x"));
    }
}
