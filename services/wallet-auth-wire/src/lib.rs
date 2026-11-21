//! Shared types for Cardano wallet authentication (CIP-8 challenge/verify flow).
//!
//! Used by both `services/wallet-auth` (worker-side) and frontend auth flows.

use serde::{Deserialize, Serialize};

// ─── Challenge ───────────────────────────────────────────────────────────────

/// Response from the challenge endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResponse {
    pub nonce: String,
    pub message: String,
    pub expires: u64,
}

// ─── Verify ──────────────────────────────────────────────────────────────────

/// Core fields for the verify request.
///
/// Applications may embed this in a larger struct with app-specific fields
/// (e.g. `community: String`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyRequest {
    /// Bech32 stake address (stake1...).
    pub stake_address: String,
    /// Hex-encoded CIP-8 COSE_Sign1 signature.
    pub signature: String,
    /// Hex-encoded CIP-8 COSE_Key public key.
    pub public_key: String,
}

/// Response from the verify endpoint (success path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub token: String,
    pub stake_address: String,
}

/// Error response from auth endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthError {
    pub error: String,
}

// ─── JWT session claims ──────────────────────────────────────────────────────

/// Standard wallet session claims embedded in a JWT.
///
/// Applications can use this directly or define their own claims struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSession {
    /// Bech32 stake address — the authenticated identity.
    pub stake_address: String,
    /// Application-assigned tier (e.g. "basic", "pro", "ultra").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Whether this wallet has admin privileges.
    #[serde(default)]
    pub is_admin: bool,
    /// What the gate measured when it assigned the tier, in whole units —
    /// the holding, NFT count or locked amount behind the verdict.
    ///
    /// A snapshot taken at sign-in, not a live balance: it is as old as the
    /// token, and the token outlives any trade the holder makes. Show it as
    /// what the session was granted on, never as "your balance".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holding: Option<u64>,
    /// Everything the gate measured, keyed by its condition source.
    ///
    /// [`Self::holding`] is the figure behind the verdict; this is every
    /// figure, so a UI can show progress PER route. Empty on a session issued
    /// before this field existed — which reads as "no progress known", not as
    /// "holds nothing of everything".
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub holdings: std::collections::BTreeMap<String, u64>,
}

// ─── Stake-session sign-in (wallet-auth `Gate`) ──────────────────────────────
//
// The wire shapes of the KV-backed challenge → CIP-30 `signData` → opaque
// session flow that `services/wallet-auth`'s `Gate` serves for any worker
// with a stake allowlist. Frontends consume these without the worker crate.

/// `POST /auth/challenge` response — what the wallet must sign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakeChallenge {
    pub challenge_id: String,
    /// Hex of the canonical challenge bytes, passed to CIP-30 `signData`.
    pub payload_to_sign_hex: String,
    pub expires_at_ms: i64,
    /// The network the worker serves (`cardano:preprod` / `cardano:mainnet`),
    /// so a frontend can refuse a wrong-network wallet before it signs.
    #[serde(default)]
    pub network: String,
}

/// `POST /auth/verify` request — the wallet's CIP-8 signature over the challenge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakeVerifyRequest {
    pub challenge_id: String,
    pub cose_sig_hex: String,
    pub cose_key_hex: String,
}

/// `POST /auth/verify` response — the session to carry as `X-Session-Token`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakeSession {
    pub token: String,
    pub stake_address: String,
    /// `super_admin` or `whitelisted` from an allowlist `Gate`; `holder` from
    /// the staking site, which signs in any wallet.
    pub tier: String,
    pub expires_at_ms: i64,
}
