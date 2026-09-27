//! Chain identity — *which* chain, and *which network* on it.
//!
//! The wire form is CAIP-2: `cardano:mainnet`, `eip155:4663`. That is not a new
//! decision — `wallet-core`'s `Network::as_chain_str` already called these "the
//! CAIP-style `chain:network` wire form workers speak", and
//! `cnft.dev-workers/docs/MULTICHAIN_NETWORK_SUPPORT.md` settled the same format
//! in 2026-05. This crate gives that vocabulary one home and one parser.
//!
//! It also carries CAIP-19, which answers the next question down — *which asset*
//! on such a chain ([`AssetType`], [`TokenRef`]). Same reasoning and the same family
//! of standard: one opaque string on the wire, a structure in code.
//!
//! ## Why a nested enum rather than `{ chain, network }`
//!
//! A `Chain` + `Network` pair describes Cardano well and EVM badly, because the
//! second component means different things on each: `cardano:preprod` names one
//! of a closed set of Cardano networks, while `eip155:4663` is a chain id —
//! open, numeric, and with no notion of "mainnet" at all. Sub-variants let each
//! family keep its own shape, and [`ChainRef::family`] then answers the question
//! that genuinely is shared: which *mechanics* apply (address format, signature
//! scheme, monogram).
//!
//! ## Never guess
//!
//! An unknown namespace is an error; an unknown EIP-155 id is a value
//! ([`EvmChain::Id`]). Nothing is ever defaulted to mainnet. `wallet-core`
//! established that rule for Cardano networks and it holds here — a chain we
//! cannot name is a chain we must not claim.
//!
//! ## Relationship to `shared_types::ChainNetwork`
//!
//! Deliberately not a replacement yet. `ChainNetwork` is persisted in D1 and
//! **cannot represent an EIP-155 id**, so this crate is what the UI and the EVM
//! work use now, and the worker-side unification is a separate change with the
//! migration in hand. Until that lands, a lossless conversion belongs at the
//! boundary on the worker side — it cannot live here, because this crate is
//! upstream of it.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

mod asset;

pub use asset::{AssetNamespace, AssetRefError, AssetType, TokenRef};

// ============================================================================
// Family
// ============================================================================

/// The axis that decides *mechanics*, as distinct from identity.
///
/// Two chains can be far apart (chain 4663 and chain 1) and still answer the
/// same way to "how is an address validated, how does a signature look" — both
/// are EVM. Two networks on the *same* chain can answer differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChainFamily {
    Cardano,
    Evm,
}

impl ChainFamily {
    pub const ALL: [Self; 2] = [Self::Cardano, Self::Evm];

    /// The CAIP-2 namespace this family's chains are written under.
    pub const fn namespace(self) -> &'static str {
        match self {
            Self::Cardano => "cardano",
            Self::Evm => "eip155",
        }
    }
}

// ============================================================================
// Cardano
// ============================================================================

/// The Cardano networks this workspace speaks to.
///
/// Moved here from `wallet-core`, where it lived as `Network`. It was never
/// wasm-specific — a pure enum inside a wasm-bindgen crate is why
/// `egui-widgets` could not name a chain without turning on its `cardano`
/// feature. `wallet-core` re-exports it under the old name.
///
/// ⚠️ **Serialised lower-case** — `"mainnet"`, `"preprod"`, `"preview"` — which is
/// the CAIP-2 component form [`ChainRef`] writes, so there is one spelling of a
/// network rather than two. This changed on 2026-09-27: the type used to serialise
/// its variant names (`"Mainnet"`, …), because that is what `wallet_core::Network`
/// had always written and `stake_session` persists a session as JSON in localStorage.
/// The **reader** is what makes the change safe — it accepts both casings and the
/// legacy `testnet` spelling — so a session saved before the change still loads. Only
/// a downgrade (new lowercase value, old code) would fail, and that is the direction
/// a reader cannot fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardanoNetwork {
    Mainnet,
    Preprod,
    Preview,
}

impl Serialize for CardanoNetwork {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.network_name())
    }
}

/// Reads either case, and `testnet` as preprod.
///
/// Delegates to [`CardanoNetwork::from_chain_str`] rather than matching again: one
/// parser is what stops two from disagreeing, and it takes the same tolerance the
/// chain-qualified form takes — so a stored value's spelling does not depend on which
/// door it came in by.
impl<'de> Deserialize<'de> for CardanoNetwork {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NetworkVisitor;

        impl serde::de::Visitor<'_> for NetworkVisitor {
            type Value = CardanoNetwork;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a Cardano network: mainnet, preprod or preview")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<CardanoNetwork, E> {
                CardanoNetwork::from_chain_str(value).ok_or_else(|| {
                    serde::de::Error::custom(format!("not a cardano network: {value:?}"))
                })
            }
        }

        deserializer.deserialize_str(NetworkVisitor)
    }
}

impl CardanoNetwork {
    /// The CIP-19 network id — 1 for mainnet, 0 for testnets.
    ///
    /// Note that preprod and preview share `0`, so this is a protocol field and
    /// not an identity: it cannot tell the two testnets apart.
    pub const fn network_id(self) -> u8 {
        match self {
            Self::Mainnet => 1,
            Self::Preprod | Self::Preview => 0,
        }
    }

    /// The bare network name — `mainnet`, `preprod`, `preview`.
    ///
    /// The serde form, and the half of [`CardanoNetwork::as_chain_str`] that follows
    /// the chain — kept here so the two cannot drift.
    pub const fn network_name(self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Preprod => "preprod",
            Self::Preview => "preview",
        }
    }

    /// The CAIP-style `chain:network` wire form workers speak.
    ///
    /// This enum existed without it, so five places parsed the string by hand
    /// instead — `expected.contains("mainnet")` in `stake_session`, a
    /// `strip_prefix("cardano:")` in `collection_list`, a full-string match in
    /// `image-core`, and two more. Each was right about a different subset.
    pub const fn as_chain_str(self) -> &'static str {
        match self {
            Self::Mainnet => "cardano:mainnet",
            Self::Preprod => "cardano:preprod",
            Self::Preview => "cardano:preview",
        }
    }

    /// Parse the `chain:network` wire form.
    ///
    /// Tolerant of a bare network name (`"preprod"`) because some callers store
    /// it stripped, and of case because nothing guarantees it. Also tolerant of
    /// `"testnet"`, a legacy spelling for preprod — see the arm below for why that
    /// one is not a guess. `None` for anything else: **deliberately not a mainnet
    /// default**, since guessing mainnet for an unknown string is how a preprod wallet
    /// gets told it is on the wrong network, or worse, how a mainnet check silently
    /// passes.
    ///
    /// Cardano-only. For a whole chain, including EVM, use [`ChainRef`], which
    /// additionally requires the namespace.
    pub fn from_chain_str(s: &str) -> Option<Self> {
        // Lowercase BEFORE stripping: the other order fails on `Cardano:PREPROD`
        // because the prefix no longer matches, and the whole string then fails
        // the arm too. Caught by `case_does_not_matter`.
        let lower = s.to_ascii_lowercase();
        let bare = lower.strip_prefix("cardano:").unwrap_or(&lower);
        match bare {
            "mainnet" => Some(Self::Mainnet),
            "preprod" => Some(Self::Preprod),
            "preview" => Some(Self::Preview),
            // Legacy, and the only tolerated spelling that is not a real Cardano
            // environment. Cardano has mainnet, preprod and preview — a network id of
            // 0 covers both testnets and cannot tell them apart — but this workspace's
            // older `ChainNetwork` accepted `cardano:testnet`, so a persisted value may
            // still say it. It reads as **preprod**, which is the network the sites
            // that wrote it meant. Normalising rather than echoing: a value that comes
            // in as `cardano:testnet` leaves as `cardano:preprod`, so the alias cannot
            // spread by round trip.
            "testnet" => Some(Self::Preprod),
            _ => None,
        }
    }
}

// ============================================================================
// EVM
// ============================================================================

/// An EIP-155 chain.
///
/// Deliberately short. [`Self::Robinhood`] is here because its id was **verified
/// against the live chain** (`eth_chainId` on the public RPC returned `0x1237`).
/// Every other chain is reachable as [`Self::Id`] and parses back unchanged, so
/// naming one later is purely additive — whereas naming one now, from memory,
/// would be writing down a constant nobody has checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvmChain {
    /// Robinhood Chain — `eip155:4663`.
    Robinhood,
    /// Any EIP-155 id with no name here. Not an error case: meeting a chain we
    /// have not named yet is normal, and must survive a round trip unaltered.
    Id(u64),
}

impl EvmChain {
    pub const fn chain_id(self) -> u64 {
        match self {
            Self::Robinhood => 4663,
            Self::Id(id) => id,
        }
    }

    /// The inverse of [`Self::chain_id`], so parsing a known id yields the named
    /// variant rather than `Id`. Without this, `Id(4663)` and `Robinhood` would
    /// be the same chain with two spellings, and comparing them would fail.
    pub const fn from_chain_id(id: u64) -> Self {
        match id {
            4663 => Self::Robinhood,
            other => Self::Id(other),
        }
    }

    /// The chain's name, where it has one.
    pub const fn name(self) -> Option<&'static str> {
        match self {
            Self::Robinhood => Some("Robinhood Chain"),
            Self::Id(_) => None,
        }
    }
}

// ============================================================================
// ChainRef
// ============================================================================

/// A chain, as identity.
///
/// Serialises as the CAIP-2 string — `"cardano:mainnet"`, `"eip155:4663"` —
/// rather than as a nested enum, whose derived form would be
/// `{"Evm":{"Robinhood":null}}`. These values are persisted (D1 columns, DO
/// storage, queue payloads), so the wire shape has to be one a database can hold
/// as one column and a human can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ChainRef {
    Cardano(CardanoNetwork),
    Evm(EvmChain),
}

impl ChainRef {
    /// Which mechanics apply. See [`ChainFamily`].
    pub const fn family(self) -> ChainFamily {
        match self {
            Self::Cardano(_) => ChainFamily::Cardano,
            Self::Evm(_) => ChainFamily::Evm,
        }
    }

    /// The CAIP-2 namespace half — `cardano`, `eip155`.
    pub const fn namespace(self) -> &'static str {
        self.family().namespace()
    }

    /// The CAIP-2 reference half — a network name, or a chain id in decimal.
    ///
    /// Taken from [`CardanoNetwork::as_chain_str`] rather than written out a
    /// second time, so the two cannot drift.
    pub fn reference(self) -> String {
        match self {
            Self::Cardano(n) => n.as_chain_str().trim_start_matches("cardano:").to_string(),
            Self::Evm(c) => c.chain_id().to_string(),
        }
    }

    /// The CAIP-2 form — the only spelling to persist.
    pub fn as_caip2(self) -> String {
        format!("{}:{}", self.namespace(), self.reference())
    }

    pub const fn is_cardano(self) -> bool {
        matches!(self, Self::Cardano(_))
    }

    /// This chain as a Cardano network, where it is one.
    ///
    /// `None` for an EVM chain rather than an invented network — and it keeps all
    /// three Cardano networks, which the `shared_types::ChainNetwork` this replaces
    /// could not: its `Testnet` collapsed preprod and preview together, so nothing
    /// downstream could tell them apart.
    pub const fn cardano_network(self) -> Option<CardanoNetwork> {
        match self {
            Self::Cardano(network) => Some(network),
            Self::Evm(_) => None,
        }
    }

    /// Cardano mainnet.
    ///
    /// A convenience for the spelling `ChainRef::Cardano(CardanoNetwork::Mainnet)`,
    /// which is what almost every call site means. There is deliberately **no
    /// `cardano_testnet()`**: Cardano has mainnet, preprod and preview, and "testnet"
    /// is not one of them — a network id of `0` covers both testnets and cannot tell
    /// them apart, so a constructor named after it could only pick one silently.
    pub const fn cardano_mainnet() -> Self {
        Self::Cardano(CardanoNetwork::Mainnet)
    }

    /// Cardano preprod.
    pub const fn cardano_preprod() -> Self {
        Self::Cardano(CardanoNetwork::Preprod)
    }

    /// Cardano preview.
    pub const fn cardano_preview() -> Self {
        Self::Cardano(CardanoNetwork::Preview)
    }

    /// The short label for the chain mark — drawn in a chip by the UI.
    ///
    /// Derived here for the same reason [`CardanoNetwork::as_chain_str`] is: a
    /// call site that picks its own string is a call site that can disagree with
    /// the identity beside it.
    pub const fn monogram(self) -> &'static str {
        match self {
            Self::Cardano(_) => "ADA",
            Self::Evm(EvmChain::Robinhood) => "RH",
            Self::Evm(EvmChain::Id(_)) => "EVM",
        }
    }
}

impl fmt::Display for ChainRef {
    /// The CAIP-2 form — the same text [`ChainRef::as_caip2`] writes, and the same
    /// shape this crate's other types render (`AssetType`, `TokenRef`).
    ///
    /// It was missing, which meant every caller that wanted to print a chain either
    /// reached for `as_caip2` explicitly or formatted the nested enum — the second of
    /// which writes `Evm(Robinhood)` rather than a chain anyone can read.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_caip2())
    }
}

impl FromStr for ChainRef {
    type Err = ChainRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        let lower = trimmed.to_ascii_lowercase();
        let Some((namespace, reference)) = lower.split_once(':') else {
            return Err(ChainRefError::NotCaip2(trimmed.to_string()));
        };
        // A third component is CAIP-10 (`namespace:reference:account`), not
        // CAIP-2. An account is not a chain, and reading the first two parts
        // would make one look like the other.
        if reference.contains(':') {
            return Err(ChainRefError::NotCaip2(trimmed.to_string()));
        }
        match namespace {
            "cardano" => CardanoNetwork::from_chain_str(reference)
                .map(Self::Cardano)
                .ok_or_else(|| ChainRefError::UnknownReference {
                    namespace: "cardano",
                    reference: reference.to_string(),
                }),
            "eip155" => reference
                .parse::<u64>()
                .map(|id| Self::Evm(EvmChain::from_chain_id(id)))
                .map_err(|_| ChainRefError::UnknownReference {
                    namespace: "eip155",
                    reference: reference.to_string(),
                }),
            other => Err(ChainRefError::UnknownNamespace(other.to_string())),
        }
    }
}

// `serde(try_from = "String", into = "String")` needs these two, and they
// delegate to the one parser above rather than repeating it — two parsers with
// two sets of rules is the drift this codebase keeps having to undo.

impl TryFrom<String> for ChainRef {
    type Error = ChainRefError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<ChainRef> for String {
    fn from(chain: ChainRef) -> Self {
        chain.as_caip2()
    }
}

/// A Cardano network as the chain that carries it.
///
/// Total, where the reverse is not: every Cardano network is a chain, and no EVM
/// chain is a network.
impl From<CardanoNetwork> for ChainRef {
    fn from(network: CardanoNetwork) -> Self {
        Self::Cardano(network)
    }
}

/// Why a string was not a chain.
///
/// Hand-written rather than pulled from `thiserror`, so this crate's only
/// dependency stays `serde` — it is consumed by wasm frontends, macroquad games
/// and workers, and every dep it takes is one all of them take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainRefError {
    /// Not `namespace:reference` at all — no colon, or a third component.
    NotCaip2(String),
    /// A namespace this crate does not speak.
    UnknownNamespace(String),
    /// A namespace we do speak, with a reference it does not recognise.
    UnknownReference {
        namespace: &'static str,
        reference: String,
    },
}

impl fmt::Display for ChainRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCaip2(s) => write!(f, "not a chain:network reference: {s}"),
            Self::UnknownNamespace(ns) => write!(f, "unknown chain namespace: {ns}"),
            Self::UnknownReference {
                namespace,
                reference,
            } => write!(f, "unknown {namespace} reference: {reference}"),
        }
    }
}

impl std::error::Error for ChainRefError {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // ── The Cardano network tests, moved with the type ──────────────────────
    //
    // Assertions are unchanged from `wallet_core`; only the type name follows
    // the move. They live here now because this is where the implementation is.

    #[test]
    fn the_wire_form_round_trips() {
        for n in [
            CardanoNetwork::Mainnet,
            CardanoNetwork::Preprod,
            CardanoNetwork::Preview,
        ] {
            assert_eq!(CardanoNetwork::from_chain_str(n.as_chain_str()), Some(n));
        }
    }

    #[test]
    fn a_bare_network_name_parses_too() {
        // Some callers store the stripped form.
        assert_eq!(
            CardanoNetwork::from_chain_str("preprod"),
            Some(CardanoNetwork::Preprod)
        );
        assert_eq!(
            CardanoNetwork::from_chain_str("mainnet"),
            Some(CardanoNetwork::Mainnet)
        );
    }

    #[test]
    fn case_does_not_matter() {
        assert_eq!(
            CardanoNetwork::from_chain_str("Cardano:PREPROD"),
            Some(CardanoNetwork::Preprod)
        );
    }

    #[test]
    fn an_unknown_network_is_none_and_never_mainnet() {
        // The dangerous default. If this ever returns `Mainnet`, a wrong-network
        // pre-check passes silently and a preprod wallet signs a mainnet
        // challenge.
        assert_eq!(CardanoNetwork::from_chain_str(""), None);
        assert_eq!(CardanoNetwork::from_chain_str("cardano:"), None);
        assert_eq!(CardanoNetwork::from_chain_str("ethereum:1"), None);
        assert_eq!(CardanoNetwork::from_chain_str("sanchonet"), None);
    }

    #[test]
    fn only_mainnet_has_network_id_one() {
        assert_eq!(CardanoNetwork::Mainnet.network_id(), 1);
        assert_eq!(CardanoNetwork::Preprod.network_id(), 0);
        assert_eq!(CardanoNetwork::Preview.network_id(), 0);
    }

    #[test]
    fn a_cardano_network_becomes_a_chain_and_back_without_collapsing_two_of_them() {
        for network in [
            CardanoNetwork::Mainnet,
            CardanoNetwork::Preprod,
            CardanoNetwork::Preview,
        ] {
            assert_eq!(ChainRef::from(network).cardano_network(), Some(network));
        }

        // An EVM chain is not a Cardano network, and saying so beats inventing one.
        assert_eq!(ChainRef::Evm(EvmChain::Robinhood).cardano_network(), None);
    }

    #[test]
    fn a_cardano_network_is_written_lower_case_and_the_old_spelling_still_reads() {
        // ⚠️ This test asserted the OPPOSITE until 2026-09-27: `"Preprod"`, because
        // `stake_session` persists a session as JSON in localStorage and that is what
        // it had always written. Settled since: one spelling, the CAIP-2 component
        // form, matching `cardano:preprod`.
        //
        // What makes the change safe is the READER, not the writer — a session saved
        // before it still loads, so only a downgrade would break, and that is the
        // direction a reader cannot fix.
        assert_eq!(
            serde_json::to_string(&CardanoNetwork::Preprod).unwrap(),
            "\"preprod\""
        );
        assert_eq!(
            serde_json::from_str::<CardanoNetwork>("\"Preprod\"").unwrap(),
            CardanoNetwork::Preprod
        );
    }

    // ── ChainRef ────────────────────────────────────────────────────────────

    #[test]
    fn the_caip2_form_round_trips() {
        for chain in [
            ChainRef::Cardano(CardanoNetwork::Mainnet),
            ChainRef::Cardano(CardanoNetwork::Preprod),
            ChainRef::Cardano(CardanoNetwork::Preview),
            ChainRef::Evm(EvmChain::Robinhood),
            ChainRef::Evm(EvmChain::Id(8453)),
        ] {
            let s = chain.as_caip2();
            assert_eq!(
                ChainRef::from_str(&s),
                Ok(chain),
                "round trip failed for {s}"
            );
        }
    }

    #[test]
    fn the_convenience_constructors_name_the_chain_they_say() {
        assert_eq!(ChainRef::cardano_mainnet().as_caip2(), "cardano:mainnet");
        assert_eq!(ChainRef::cardano_preprod().as_caip2(), "cardano:preprod");
        assert_eq!(ChainRef::cardano_preview().as_caip2(), "cardano:preview");
    }

    #[test]
    fn display_writes_the_caip2_form_and_not_the_nested_enum() {
        // The gap this closes: `{}` on the derived `Debug` shape writes
        // `Evm(Robinhood)`, which is not a chain anyone can read or parse back.
        assert_eq!(ChainRef::cardano_preprod().to_string(), "cardano:preprod");
        assert_eq!(
            ChainRef::Evm(EvmChain::Robinhood).to_string(),
            "eip155:4663"
        );
    }

    #[test]
    fn a_network_is_written_lower_case_and_read_in_either_case() {
        // One spelling on the way out — the CAIP-2 component form — so a network name
        // does not depend on which type carries it.
        assert_eq!(
            serde_json::to_string(&CardanoNetwork::Preprod).unwrap(),
            "\"preprod\""
        );

        // The tolerance on the way in is what makes that change safe: the PascalCase
        // this type used to write is still accepted, so a session saved before the
        // change still loads.
        assert_eq!(
            serde_json::from_str::<CardanoNetwork>("\"Preprod\"").unwrap(),
            CardanoNetwork::Preprod
        );
        assert_eq!(
            serde_json::from_str::<CardanoNetwork>("\"mainnet\"").unwrap(),
            CardanoNetwork::Mainnet
        );
        // And the legacy spelling, as preprod.
        assert_eq!(
            serde_json::from_str::<CardanoNetwork>("\"testnet\"").unwrap(),
            CardanoNetwork::Preprod
        );
    }

    #[test]
    fn a_network_that_is_not_one_is_refused_rather_than_defaulted() {
        assert!(serde_json::from_str::<CardanoNetwork>("\"stagenet\"").is_err());
    }

    #[test]
    fn the_legacy_testnet_spelling_reads_as_preprod_and_does_not_survive_a_round_trip() {
        // Back-compat for values this workspace's old `ChainNetwork` could write.
        // It normalises: what comes back out is the real environment, so the legacy
        // word cannot propagate by being echoed.
        assert_eq!(
            CardanoNetwork::from_chain_str("cardano:testnet"),
            Some(CardanoNetwork::Preprod)
        );

        let parsed: ChainRef = "cardano:testnet"
            .parse()
            .expect("the legacy spelling reads");
        assert_eq!(parsed, ChainRef::cardano_preprod());
        assert_eq!(parsed.as_caip2(), "cardano:preprod");
    }

    #[test]
    fn robinhood_is_eip155_4663() {
        // Verified against the live chain: `eth_chainId` on
        // `rpc.mainnet.chain.robinhood.com` returns `0x1237`.
        assert_eq!(EvmChain::Robinhood.chain_id(), 4663);
        assert_eq!(
            ChainRef::from_str("eip155:4663").unwrap().as_caip2(),
            "eip155:4663"
        );
    }

    #[test]
    fn a_named_chain_and_its_raw_id_are_one_value_not_two() {
        // Otherwise `Id(4663)` and `Robinhood` would be the same chain with two
        // spellings, and an equality check would quietly fail.
        assert_eq!(EvmChain::from_chain_id(4663), EvmChain::Robinhood);
        assert_eq!(
            ChainRef::from_str("eip155:4663").unwrap(),
            ChainRef::Evm(EvmChain::Robinhood)
        );
    }

    #[test]
    fn an_unnamed_eip155_id_is_representable_and_survives_intact() {
        // Meeting a chain we have not named is normal. It must not error, and it
        // must not be rewritten to a chain we do know.
        let parsed = ChainRef::from_str("eip155:8453").unwrap();
        assert_eq!(parsed, ChainRef::Evm(EvmChain::Id(8453)));
        assert_eq!(parsed.as_caip2(), "eip155:8453");
        assert_eq!(parsed.monogram(), "EVM");
        assert_eq!(parsed.family(), ChainFamily::Evm);
    }

    #[test]
    fn an_unknown_namespace_is_an_error_and_never_cardano() {
        // The dangerous default, in its other form: an unrecognised namespace
        // must not fall back to the one chain this workspace knows best.
        for (input, namespace) in [
            ("solana:mainnet", "solana"),
            ("midnight:mainnet", "midnight"),
            ("sui:mainnet", "sui"),
        ] {
            assert!(
                matches!(
                    ChainRef::from_str(input),
                    Err(ChainRefError::UnknownNamespace(ns)) if ns == namespace
                ),
                "{input} should be an unknown namespace"
            );
        }
    }

    #[test]
    fn a_bare_network_name_is_not_a_chain() {
        // `cardano:mainnet` needs its namespace: `mainnet` alone does not say
        // which chain it is a mainnet *of*, and this crate will not pick one.
        for input in ["mainnet", "preprod", "preview"] {
            assert!(
                matches!(ChainRef::from_str(input), Err(ChainRefError::NotCaip2(_))),
                "{input} should not parse as a chain"
            );
        }
    }

    #[test]
    fn a_known_namespace_with_a_bad_reference_is_an_error() {
        for input in [
            "cardano:",
            "eip155:",
            "cardano:sanchonet",
            "eip155:abc",
            "eip155:-1",
            // CAIP-10, not CAIP-2 — an account is not a chain.
            "eip155:4663:0xe638f58c87258ece6c0eddc46f70327663635c69",
            "cardano:mainnet:stake1u8boef",
            // Whitespace inside is not trimmed away for the reader's benefit.
            "cardano: mainnet",
        ] {
            assert!(
                ChainRef::from_str(input).is_err(),
                "{input} should not parse"
            );
        }
    }

    #[test]
    fn serde_writes_the_caip2_string_and_never_defaults() {
        let chain = ChainRef::Evm(EvmChain::Robinhood);
        assert_eq!(serde_json::to_string(&chain).unwrap(), "\"eip155:4663\"");

        assert_eq!(
            serde_json::from_str::<ChainRef>("\"cardano:preprod\"").unwrap(),
            ChainRef::Cardano(CardanoNetwork::Preprod)
        );

        // The shape a derived nested enum would have written. If this ever
        // parses, the wire format has regressed to something a D1 column cannot
        // hold and a human cannot read.
        assert!(serde_json::from_str::<ChainRef>("{\"Evm\":{\"Robinhood\":null}}").is_err());

        // And it does not quietly accept a chain it cannot read.
        assert!(serde_json::from_str::<ChainRef>("\"cardano:sanchonet\"").is_err());
        assert!(serde_json::from_str::<ChainRef>("\"bitcoin:mainnet\"").is_err());
    }

    #[test]
    fn family_is_the_mechanical_axis_and_distinguishes_the_two() {
        assert_eq!(
            ChainRef::Cardano(CardanoNetwork::Mainnet).family(),
            ChainFamily::Cardano
        );
        assert_eq!(
            ChainRef::Evm(EvmChain::Robinhood).family(),
            ChainFamily::Evm
        );

        // Two chains far apart still share a family; two networks on one chain
        // do too. That is the point of the axis.
        assert_eq!(
            ChainRef::Evm(EvmChain::Robinhood).family(),
            ChainRef::Evm(EvmChain::Id(8453)).family()
        );
        assert_eq!(
            ChainRef::Cardano(CardanoNetwork::Mainnet).family(),
            ChainRef::Cardano(CardanoNetwork::Preview).family()
        );
    }

    #[test]
    fn the_namespace_comes_from_the_family_not_the_variant() {
        assert_eq!(
            ChainRef::Cardano(CardanoNetwork::Preview).namespace(),
            "cardano"
        );
        assert_eq!(ChainRef::Evm(EvmChain::Id(1)).namespace(), "eip155");
        for family in ChainFamily::ALL {
            assert!(!family.namespace().is_empty());
        }
    }
}
