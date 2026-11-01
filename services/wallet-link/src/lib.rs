//! What auth-service says about a user's linked wallets.
//!
//! auth-service owns the Discord ↔ wallet mapping. This crate exists so the
//! services that *ask* about it — `/traits` in collection-ownership,
//! `/check-rank` in game-sessions — share one definition of the answer rather
//! than each keeping a private copy.
//!
//! That is not hypothetical tidiness. A hand-mirrored struct deserialises a
//! renamed or dropped field to `None`/absent rather than failing, so the
//! symptom is a user who appears to have no wallets — indistinguishable from a
//! user who genuinely has none. The same shape silently cost every asset its
//! rarity rank earlier in this codebase's life.

use serde::{Deserialize, Serialize};

/// Path auth-service serves the internal lookup on, appended with the Discord
/// user id. Internal-key authenticated: this maps an identity to wallets, which
/// must never become a public lookup.
pub const INTERNAL_WALLETS_PATH: &str = "/_internal/wallets";

/// One wallet a user has verified control of.
///
/// **Chain-qualified.** The mapping is not Cardano-only: an EVM account links
/// beside a stake address, so a consumer reads `chain` before it interprets
/// `address`. A stake-shaped filter fed an `0x…` account finds nothing; an EVM
/// ownership resolver fed a bech32 address does the same.
///
/// **No wallet is primary.** The list arrives in the order the wallets were
/// linked, which is stable and nothing more: users cannot reorder it, and no
/// entry is special. Treat it as a set. A consumer that one day needs a
/// particular wallet — a payout destination, say — needs a setting named for
/// that job, not the first entry here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedWallet {
    /// Where the proof holds, as `chains::ChainScope` writes it: a CAIP-2 chain
    /// when the proof is chain-specific, the bare namespace when it is not.
    ///
    /// | Link | `chain` | Why |
    /// |---|---|---|
    /// | Cardano | `cardano:mainnet` | a stake address encodes its network |
    /// | EVM | `eip155` | the account comes from a key alone, so a signature proves control on **every** EVM chain |
    /// | Midnight | `midnight:mainnet` | an unshielded address's HRP names its network (`mn_addr` on mainnet), and only mainnet is accepted |
    ///
    /// So an EVM link asserts no chain. A consumer that reads ownership on a
    /// particular EVM chain looks the address up *there*; the link does not, and
    /// cannot, say which chains the account is used on. Rows written before
    /// 2026-10-24 carried `eip155:4663`; migration 0046 rewrote them.
    pub chain: String,

    /// The address in the form its chain uses: bech32 `stake1…` on Cardano,
    /// lower-case `0x…` on EVM, bech32m `mn_addr…` on Midnight. Cardano links
    /// carry the **reward** address — the one credential a CIP-8 signature
    /// proves, so every payment address under it is covered.
    pub address: String,

    /// ADA handle, when the wallet has one. Cardano only — handles are Cardano
    /// assets — so absent on EVM rows. Display only: never an identifier,
    /// since handles are transferable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

/// Build the lookup path for a Discord user.
pub fn wallets_path(discord_user_id: &str) -> String {
    format!("{INTERNAL_WALLETS_PATH}/{discord_user_id}")
}

/// Path auth-service serves the BATCH reverse lookup on: given credentials, who
/// owns them.
///
/// Deliberately not a suffix of [`INTERNAL_WALLETS_PATH`] — that route takes a
/// user id, and a literal segment beside a `:id` parameter is a collision
/// waiting for whichever router rule happens to come first.
///
/// Internal-key authenticated for the same reason as the forward lookup: it is
/// the wallet↔identity mapping, and W14 keeps that out of any public read.
pub const INTERNAL_WALLET_OWNERS_PATH: &str = "/_internal/wallet-owners";

/// A set of credentials to resolve, in one call.
///
/// The caller sends the form its holder source gave it — the token ledger keys
/// holders by the 28-byte hex stake credential — and auth-service decides which
/// stored form that corresponds to. Doing the encoding on the identity side is
/// deliberate: how a credential is written into `user_wallet` is that service's
/// business, and a caller that mirrored it would break the day a legacy row
/// reaches it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletOwnerQuery {
    pub credentials: Vec<String>,
}

/// One credential that resolved to an owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletOwner {
    /// Echoed exactly as asked, so a caller can key its own aggregation without
    /// re-deriving an encoding it never owned.
    pub credential: String,
    pub discord_user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

/// The batch answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletOwners {
    pub resolved: Vec<WalletOwner>,
    /// Asked about and not linked.
    ///
    /// Echoed so "nobody has linked this wallet" is distinguishable from "the
    /// request never carried it" — a truncated request and a cold holder set
    /// look identical otherwise, and the first is a bug while the second is a
    /// fact.
    #[serde(default)]
    pub unresolved: Vec<String>,
}

pub mod link;

impl LinkedWallet {
    /// The address when this is a Cardano link, else `None`.
    ///
    /// Most consumers of this crate resolve *Cardano* ownership — a bundle's
    /// `?stake=` filter, a stake-keyed allowlist, a roster keyed on stake
    /// addresses — so they want the Cardano subset and nothing else. Filtering
    /// through this method rather than reading `address` at each call site
    /// means one definition of "a Cardano link": the CAIP-2 namespace half of
    /// `chain`, which is the same test [`chains::ChainRef`] parses by.
    ///
    /// An EVM account handed to a stake-keyed query finds nothing and fails
    /// *silently*, which is why this is a method rather than a field every
    /// consumer decides about for itself.
    pub fn cardano_address(&self) -> Option<&str> {
        (self.namespace() == "cardano").then_some(self.address.as_str())
    }

    /// The account when this is an EVM link, else `None` — the EVM sibling of
    /// [`Self::cardano_address`], for a consumer resolving EVM ownership.
    ///
    /// Valid on any EVM chain the consumer reads; see [`Self::chain`].
    pub fn evm_address(&self) -> Option<&str> {
        (self.namespace() == "eip155").then_some(self.address.as_str())
    }

    /// The unshielded address when this is a Midnight link, else `None` — the
    /// Midnight sibling of [`Self::cardano_address`].
    ///
    /// Chain-specific, unlike [`Self::evm_address`]: the link names
    /// `midnight:mainnet`, and a bech32m address for another Midnight network
    /// would decode to the same 32 bytes, so a consumer that accepted the
    /// address without reading the chain could confuse the two.
    pub fn midnight_address(&self) -> Option<&str> {
        (self.namespace() == "midnight").then_some(self.address.as_str())
    }

    /// The CAIP namespace half of `chain` — the whole string for a family-wide
    /// link, the part before the colon for a chain-specific one. Read by hand
    /// rather than through `chains`, so this crate keeps serde as its only
    /// dependency.
    fn namespace(&self) -> &str {
        self.chain
            .split_once(':')
            .map_or(self.chain.as_str(), |(namespace, _)| namespace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cardano(address: &str, handle: Option<&str>) -> LinkedWallet {
        LinkedWallet {
            chain: "cardano:mainnet".to_string(),
            address: address.to_string(),
            handle: handle.map(str::to_string),
        }
    }

    #[test]
    fn a_wallet_without_a_handle_round_trips() {
        let wallet = cardano(
            "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9",
            None,
        );
        let json = serde_json::to_string(&wallet).unwrap();
        assert!(!json.contains("handle"), "absent handle should be omitted");
        assert_eq!(serde_json::from_str::<LinkedWallet>(&json).unwrap(), wallet);
    }

    const ACCOUNT: &str = "0x7980aa64093853cb78c927e05b88fed96e945f81";

    fn evm(chain: &str) -> LinkedWallet {
        LinkedWallet {
            chain: chain.to_string(),
            address: ACCOUNT.to_string(),
            handle: None,
        }
    }

    /// An EVM link names the family, not a chain.
    #[test]
    fn an_evm_account_round_trips_beside_a_stake_address() {
        let wallet = evm("eip155");
        let json = serde_json::to_string(&wallet).unwrap();
        assert!(
            json.contains(r#""chain":"eip155""#),
            "family on the wire: {json}"
        );
        assert_eq!(serde_json::from_str::<LinkedWallet>(&json).unwrap(), wallet);
    }

    /// Family-wide and chain-specific rows are both EVM links; a payload
    /// produced before the migration still reads as one.
    #[test]
    fn evm_address_yields_evm_links_however_they_are_scoped() {
        assert_eq!(evm("eip155").evm_address(), Some(ACCOUNT));
        assert_eq!(evm("eip155:4663").evm_address(), Some(ACCOUNT));
        assert_eq!(evm("eip155").cardano_address(), None);
        assert_eq!(cardano("stake1abc", None).evm_address(), None);
    }

    /// The address from the connector specification's own `makeTransfer`
    /// example — a wallet's output, not something this crate built.
    const UNSHIELDED: &str = "mn_addr1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9s6e0fs3";

    /// A Midnight link is chain-specific, and each of the three accessors names
    /// its own chain and nothing else — a Midnight address handed to a Cardano
    /// lookup must not resolve.
    #[test]
    fn midnight_address_yields_only_midnight_links() {
        let midnight = LinkedWallet {
            chain: "midnight:mainnet".to_string(),
            address: UNSHIELDED.to_string(),
            handle: None,
        };
        assert_eq!(midnight.midnight_address(), Some(UNSHIELDED));
        assert_eq!(midnight.cardano_address(), None);
        assert_eq!(midnight.evm_address(), None);
        assert_eq!(cardano("stake1abc", None).midnight_address(), None);
        assert_eq!(evm("eip155").midnight_address(), None);

        let json = serde_json::to_string(&midnight).unwrap();
        assert!(
            json.contains(r#""chain":"midnight:mainnet""#),
            "chain on the wire: {json}"
        );
        assert_eq!(
            serde_json::from_str::<LinkedWallet>(&json).unwrap(),
            midnight
        );
    }

    /// Producers may add fields; a consumer must not break on them.
    #[test]
    fn unknown_fields_are_ignored() {
        let json =
            r#"{"chain":"cardano:mainnet","address":"stake1abc","handle":"$boef","linked_at":123}"#;
        let wallet: LinkedWallet = serde_json::from_str(json).unwrap();
        assert_eq!(wallet.handle.as_deref(), Some("$boef"));
    }

    /// The chain is required — a producer that forgot it is a hard error, not a
    /// wallet that silently reads as Cardano.
    #[test]
    fn a_wallet_with_no_chain_is_rejected() {
        let json = r#"{"address":"stake1abc"}"#;
        assert!(serde_json::from_str::<LinkedWallet>(json).is_err());
    }

    #[test]
    fn path_includes_the_user_id() {
        assert_eq!(
            wallets_path("179744071361757184"),
            "/_internal/wallets/179744071361757184"
        );
    }

    #[test]
    fn the_batch_owner_answer_round_trips() {
        let answer = WalletOwners {
            resolved: vec![WalletOwner {
                credential: "aa11".into(),
                discord_user_id: "42".into(),
                handle: None,
            }],
            unresolved: vec!["bb22".into()],
        };
        let json = serde_json::to_string(&answer).unwrap();
        assert!(!json.contains("handle"), "absent handle should be omitted");
        assert_eq!(serde_json::from_str::<WalletOwners>(&json).unwrap(), answer);
    }

    /// A resolver that is only ever asked about linked wallets answers with an
    /// empty list, not an error — an empty guild is not a failed request.
    #[test]
    fn an_empty_query_is_a_valid_request() {
        let query = WalletOwnerQuery::default();
        assert_eq!(
            serde_json::from_str::<WalletOwnerQuery>(r#"{"credentials":[]}"#).unwrap(),
            query
        );
    }

    #[test]
    fn cardano_address_yields_only_cardano_links() {
        assert_eq!(
            cardano(
                "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9",
                None
            )
            .cardano_address(),
            Some("stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9")
        );
        // Any Cardano network, not just mainnet: which one is the consumer's
        // question, and a preprod stake simply won't match mainnet ownership.
        assert_eq!(
            cardano("stake_test1abc", None).cardano_address(),
            Some("stake_test1abc")
        );
        assert_eq!(evm("eip155").cardano_address(), None);
    }
}
