//! The link protocol a client speaks to auth-service: who the bearer is, the
//! wallets linked to them, and the challenge → signature → verify round that
//! links one more.
//!
//! Every route here takes `Authorization: Bearer <jwt>`, where the token's
//! subject is a Discord user id: a `/link-wallet` widget token, a Discord
//! Activity's `widget_token`, or a `discord-auth` session minted with the
//! secret auth-service also accepts. The server's own copies of these shapes
//! are in cnft.dev-workers `workers/auth/src/social_link.rs`; the field names
//! here match them one for one, and the tests below are written from that
//! file's JSON.
//!
//! The message a wallet signs is built by the server and only by the server:
//! a client signs [`LinkChallenge::message`] exactly as it arrives. Cardano
//! signs the UTF-8 bytes as bare hex through CIP-30 `signData`, over the
//! **reward** address; EVM signs `0x`-prefixed hex through EIP-191
//! `personal_sign` and sends the signature back without the `0x`; Midnight
//! hands the wallet the plain UTF-8 string through the DApp connector's
//! `signData`, which prepends its own `midnight_signed_message:` header before
//! signing — so the client must **not** pre-hash or pre-hex it.
//!
//! One challenge is pending per Discord user, shared by every chain, for five
//! minutes, and it is burned before the signature is checked: a failed verify
//! needs a new challenge.

use serde::{Deserialize, Serialize};

/// `GET` — who the bearer is. Answers [`Identity`].
pub const ME_PATH: &str = "/auth/me";
/// `GET` — the bearer's linked wallets, in the order they were linked.
/// Answers `Vec<LinkedWallet>`.
pub const WALLETS_PATH: &str = "/auth/wallets";
/// `POST` [`UnlinkRequest`] — remove one. Answers the wallets that remain.
pub const UNLINK_PATH: &str = "/auth/wallets/unlink";
/// `POST`, no body — a challenge for a Cardano link. Answers [`LinkChallenge`].
pub const CARDANO_CHALLENGE_PATH: &str = "/auth/cardano/challenge";
/// `POST` [`CardanoLinkRequest`]. Answers [`LinkVerified`].
pub const CARDANO_VERIFY_PATH: &str = "/auth/cardano/verify";
/// `POST`, no body — a challenge for an EVM link. Answers [`LinkChallenge`].
pub const EVM_CHALLENGE_PATH: &str = "/auth/evm/challenge";
/// `POST` [`EvmLinkRequest`]. Answers [`LinkVerified`] with `chain: "eip155"`.
pub const EVM_VERIFY_PATH: &str = "/auth/evm/verify";
/// `POST`, no body — a challenge for a Midnight link. Answers [`LinkChallenge`].
pub const MIDNIGHT_CHALLENGE_PATH: &str = "/auth/midnight/challenge";
/// `POST` [`MidnightLinkRequest`]. Answers [`LinkVerified`] with
/// `chain: "midnight:mainnet"`.
pub const MIDNIGHT_VERIFY_PATH: &str = "/auth/midnight/verify";

/// The auth-service host every client uses today. There is no dev
/// deployment: dev and prod share it, and share the links it holds.
pub const DEFAULT_AUTH_BASE: &str = "https://auth.hodlcroft.com";

/// `GET /auth/me`: the Discord user the bearer token names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub discord_user_id: String,
    /// What a widget token was minted for (`link_wallet`), when it says.
    #[serde(default)]
    pub intent: Option<String>,
}

/// A challenge to sign. Shared by every chain's route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkChallenge {
    pub nonce: String,
    /// Exactly the bytes the wallet must sign.
    pub message: String,
    /// Unix seconds.
    pub expires: u64,
}

/// `POST /auth/cardano/verify`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardanoLinkRequest {
    /// The bech32 **reward** address being claimed (`stake1…`).
    pub stake_address: String,
    /// CIP-8 COSE_Sign1, hex.
    pub signature: String,
    /// CIP-8 COSE_Key, hex.
    pub public_key: String,
}

/// `POST /auth/evm/verify`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmLinkRequest {
    /// The `0x…` account being claimed. Any case; compared lower-cased.
    pub address: String,
    /// 65-byte `r || s || v`, hex, without `0x`.
    pub signature: String,
}

/// `POST /auth/midnight/verify`.
///
/// Unlike the EVM request this carries the verifying key: Midnight's connector
/// hands it over beside the signature and there is nothing to recover. The
/// server re-derives the unshielded address from that key and requires the
/// claim to name it — so the address here is a claim to be checked, not the
/// source of truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidnightLinkRequest {
    /// The bech32m unshielded address being claimed (`mn_addr…`). Mainnet only:
    /// an address for any other network carries it in the HRP
    /// (`mn_addr_preview…`) and is refused.
    pub address: String,
    /// 64-byte `r ‖ s`, hex.
    pub signature: String,
    /// 32-byte x-only key for `schnorr_bip340`, 33-byte SEC1-compressed for
    /// `ecdsa_secp256k1_sha256`, hex.
    pub verifying_key: String,
    /// `schnorr_bip340`, `ecdsa_secp256k1_sha256`, or absent — which the
    /// connector specification defines as `schnorr_bip340`. Omitted from the
    /// body when absent, because the server's own field is `Option` and defaults
    /// to the same answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
}

/// A verified link: what was linked, and to whom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkVerified {
    pub chain: String,
    pub address: String,
    #[serde(default)]
    pub handle: Option<String>,
    pub discord_user_id: String,
}

impl LinkVerified {
    /// The link as the wallet list carries it.
    pub fn wallet(&self) -> crate::LinkedWallet {
        crate::LinkedWallet {
            chain: self.chain.clone(),
            address: self.address.clone(),
            handle: self.handle.clone(),
        }
    }
}

/// `POST /auth/wallets/unlink`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnlinkRequest {
    pub address: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The challenge as auth-service serialises `LinkChallengeResponse`.
    #[test]
    fn a_challenge_reads_as_the_server_writes_it() {
        let json = r#"{"nonce":"ab12","message":"Link this wallet to your Discord account.\n\nDiscord ID: 179744071361757184\nNonce: ab12","expires":1761900000}"#;
        let c: LinkChallenge = serde_json::from_str(json).unwrap();
        assert_eq!(c.nonce, "ab12");
        assert!(c.message.ends_with("Nonce: ab12"));
        assert_eq!(c.expires, 1_761_900_000);
    }

    /// The verify reply for an EVM link carries `handle: null`.
    #[test]
    fn a_verified_link_becomes_a_wallet_row() {
        let json = r#"{"chain":"eip155","address":"0x7980aa64093853cb78c927e05b88fed96e945f81","handle":null,"discord_user_id":"179744071361757184"}"#;
        let v: LinkVerified = serde_json::from_str(json).unwrap();
        let w = v.wallet();
        assert_eq!(
            w.evm_address(),
            Some("0x7980aa64093853cb78c927e05b88fed96e945f81")
        );
        assert_eq!(w.handle, None);
    }

    /// The request bodies use the server's own field names.
    #[test]
    fn requests_are_written_with_the_servers_field_names() {
        let cardano = CardanoLinkRequest {
            stake_address: "stake1u".into(),
            signature: "84a4".into(),
            public_key: "a401".into(),
        };
        assert_eq!(
            serde_json::to_string(&cardano).unwrap(),
            r#"{"stake_address":"stake1u","signature":"84a4","public_key":"a401"}"#
        );
        let evm = EvmLinkRequest {
            address: "0xab".into(),
            signature: "cd".into(),
        };
        assert_eq!(
            serde_json::to_string(&evm).unwrap(),
            r#"{"address":"0xab","signature":"cd"}"#
        );
        // No `scheme` when the wallet did not name one: the field is optional on
        // the wire and absent means `schnorr_bip340`, so writing
        // `"scheme":null` would be a different body for the same request.
        let midnight = MidnightLinkRequest {
            address: "mn_addr1qq".into(),
            signature: "ef".into(),
            verifying_key: "12".into(),
            scheme: None,
        };
        assert_eq!(
            serde_json::to_string(&midnight).unwrap(),
            r#"{"address":"mn_addr1qq","signature":"ef","verifying_key":"12"}"#
        );
        let ecdsa = MidnightLinkRequest {
            scheme: Some("ecdsa_secp256k1_sha256".into()),
            ..midnight
        };
        assert_eq!(
            serde_json::to_string(&ecdsa).unwrap(),
            r#"{"address":"mn_addr1qq","signature":"ef","verifying_key":"12","scheme":"ecdsa_secp256k1_sha256"}"#
        );
        let unlink = UnlinkRequest {
            address: "0xab".into(),
        };
        assert_eq!(
            serde_json::to_string(&unlink).unwrap(),
            r#"{"address":"0xab"}"#
        );
    }

    /// A token minted for something other than linking has no intent.
    #[test]
    fn an_identity_without_an_intent_reads() {
        let id: Identity =
            serde_json::from_str(r#"{"discord_user_id":"179744071361757184","intent":null}"#)
                .unwrap();
        assert_eq!(id.intent, None);
    }
}
