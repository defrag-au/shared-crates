//! What auth-service says about a user's linked wallets.
//!
//! auth-service owns the Discord↔stake mapping. This crate exists so the
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
/// Order is meaningful — the list arrives in the order the user arranged it,
/// so the first entry is their primary. Consumers that only need addresses
/// should still preserve it rather than collecting into a set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedWallet {
    /// Bech32 stake address (`stake1…`).
    pub stake_address: String,

    /// ADA handle, when the wallet has one. Display only — never an identifier,
    /// since handles are transferable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

/// Build the lookup path for a Discord user.
pub fn wallets_path(discord_user_id: &str) -> String {
    format!("{INTERNAL_WALLETS_PATH}/{discord_user_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wallet_without_a_handle_round_trips() {
        let wallet = LinkedWallet {
            stake_address: "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9".to_string(),
            handle: None,
        };
        let json = serde_json::to_string(&wallet).unwrap();
        assert!(!json.contains("handle"), "absent handle should be omitted");
        assert_eq!(serde_json::from_str::<LinkedWallet>(&json).unwrap(), wallet);
    }

    /// Producers may add fields; a consumer must not break on them.
    #[test]
    fn unknown_fields_are_ignored() {
        let json = r#"{"stake_address":"stake1abc","handle":"$boef","linked_at":123}"#;
        let wallet: LinkedWallet = serde_json::from_str(json).unwrap();
        assert_eq!(wallet.handle.as_deref(), Some("$boef"));
    }

    #[test]
    fn path_includes_the_user_id() {
        assert_eq!(
            wallets_path("179744071361757184"),
            "/_internal/wallets/179744071361757184"
        );
    }
}
