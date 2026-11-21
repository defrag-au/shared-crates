//! Challenge generation and verification backed by DO SQLite.

use crate::cip8;
use crate::gate::{AccessGate, GateOutcome};
use pallas_addresses::Network;
use serde::Deserialize;
use wallet_auth_wire::{ChallengeResponse, VerifyRequest};
use worker_stack::worker::*;

/// Manages challenge nonces for the CIP-8 auth flow.
///
/// Stores nonces in DO SQLite and verifies CIP-8 signatures before
/// delegating to an [`AccessGate`] for application-specific authorization.
pub struct ChallengeManager {
    /// Prefix for the sign-in message (e.g. "Sign in to Meme Builder").
    message_prefix: String,
    /// Challenge TTL in seconds (default: 300 = 5 minutes).
    ttl_secs: u64,
    /// Network the claimed stake address must belong to. Verification derives
    /// the address from the signing key using this byte, so a mainnet
    /// signature can't satisfy a testnet claim or vice versa.
    network: Network,
}

impl ChallengeManager {
    /// Create a new challenge manager on mainnet.
    ///
    /// `message_prefix` appears in the wallet signing prompt:
    /// `"{message_prefix}\n\nNonce: {nonce}"`
    pub fn new(message_prefix: &str) -> Self {
        Self {
            message_prefix: message_prefix.to_string(),
            ttl_secs: 300,
            network: Network::Mainnet,
        }
    }

    /// Override the challenge TTL (default: 300 seconds).
    pub fn with_ttl_secs(mut self, secs: u64) -> Self {
        self.ttl_secs = secs;
        self
    }

    /// Override the network (default: mainnet).
    pub fn with_network(mut self, network: Network) -> Self {
        self.network = network;
        self
    }

    /// Format the sign-in message for a given nonce.
    pub fn sign_in_message(&self, nonce: &str) -> String {
        format!("{}\n\nNonce: {nonce}", self.message_prefix)
    }

    // ─── Challenge creation ──────────────────────────────────────────────────

    /// Generate a challenge nonce and store it in DO SQLite.
    ///
    /// Call this from your `GET /api/auth/challenge?stake_address=...` handler.
    pub fn create_challenge(
        &self,
        storage: &Storage,
        stake_address: &str,
    ) -> Result<ChallengeResponse> {
        self.ensure_schema(storage)?;

        let nonce = ulid::Ulid::new().to_string().to_lowercase();
        let now_secs = js_now_secs();
        let expires = now_secs + self.ttl_secs;
        let message = self.sign_in_message(&nonce);

        storage.sql().exec(
            "INSERT OR REPLACE INTO challenges (stake_address, nonce, expires)
             VALUES (?1, ?2, ?3)",
            vec![
                SqlStorageValue::String(stake_address.to_string()),
                SqlStorageValue::String(nonce.clone()),
                SqlStorageValue::Integer(expires as i64),
            ],
        )?;

        Ok(ChallengeResponse {
            nonce,
            message,
            expires,
        })
    }

    // ─── Verify + gate ───────────────────────────────────────────────────────

    /// Verify a CIP-8 signature and run the access gate.
    ///
    /// 1. Looks up the stored nonce for this stake address
    /// 2. Checks nonce expiry
    /// 3. Verifies the CIP-8 COSE_Sign1 signature against the nonce message
    /// 4. **Binds the signing key to the claimed stake address**
    /// 5. Consumes the nonce (one-time use)
    /// 6. Derives the payment PKH from the public key
    /// 7. Calls `gate.evaluate()` for application-specific authorization
    ///
    /// Returns `(stake_address, GateOutcome)` on success — the **derived**
    /// address, not the caller's claim, so downstream code can't accidentally
    /// propagate an unverified value.
    pub async fn verify_and_gate<G: AccessGate>(
        &self,
        storage: &Storage,
        req: &VerifyRequest,
        gate: &G,
    ) -> std::result::Result<(String, GateOutcome), String> {
        self.ensure_schema(storage)
            .map_err(|e| format!("Schema error: {e}"))?;

        // 1. Retrieve nonce
        let stored = self
            .get_nonce(storage, &req.stake_address)
            .map_err(|e| format!("Challenge lookup failed: {e}"))?
            .ok_or("No pending challenge for this stake address")?;

        // 2. Check expiry
        let now = js_now_secs();
        if stored.expires < now {
            return Err("Challenge has expired".into());
        }

        // 3. Verify CIP-8 signature
        let message = self.sign_in_message(&stored.nonce);
        let valid = cip8::verify_signature(&req.signature, &req.public_key, &message)
            .map_err(|e| format!("Signature verification failed: {e}"))?;

        if !valid {
            return Err("Invalid signature".into());
        }

        // 4. Bind the signing key to the claimed address.
        //
        //    A valid signature proves only that *some* key signed our nonce —
        //    the nonce was looked up by a stake address the caller supplied, so
        //    without this check an attacker signs our challenge with their own
        //    wallet while claiming someone else's address, and the gate below
        //    is handed a victim's identity next to an attacker's credential.
        //    The derived address is ground truth; the claim is a sanity check.
        let derived_address = cip8::derive_stake_address(&req.public_key, self.network)
            .map_err(|e| format!("Failed to derive stake address: {e}"))?;

        if derived_address != req.stake_address {
            return Err(format!(
                "Signing key does not own the claimed stake address \
                 (derived {derived_address}, claimed {})",
                req.stake_address
            ));
        }

        // 5. Consume nonce (one-time use)
        let _ = storage.sql().exec(
            "DELETE FROM challenges WHERE stake_address = ?1",
            vec![SqlStorageValue::String(req.stake_address.clone())],
        );

        // 6. Derive payment PKH for on-chain lookups
        let payment_pkh = cip8::derive_payment_pkh(&req.public_key)
            .map_err(|e| format!("Failed to derive PKH: {e}"))?;

        // 7. Application-specific access gate
        let outcome = gate.evaluate(&derived_address, &payment_pkh).await?;

        Ok((derived_address, outcome))
    }

    // ─── Internal ────────────────────────────────────────────────────────────

    fn ensure_schema(&self, storage: &Storage) -> Result<()> {
        storage.sql().exec(
            "CREATE TABLE IF NOT EXISTS challenges (
                stake_address TEXT PRIMARY KEY,
                nonce TEXT NOT NULL,
                expires INTEGER NOT NULL
            )",
            None::<Vec<SqlStorageValue>>,
        )?;
        Ok(())
    }

    fn get_nonce(&self, storage: &Storage, stake_address: &str) -> Result<Option<StoredChallenge>> {
        let rows: Vec<StoredChallenge> = storage
            .sql()
            .exec(
                "SELECT nonce, expires FROM challenges WHERE stake_address = ?1",
                vec![SqlStorageValue::String(stake_address.to_string())],
            )?
            .to_array()?;

        Ok(rows.into_iter().next())
    }
}

#[derive(Deserialize)]
struct StoredChallenge {
    nonce: String,
    expires: u64,
}

fn js_now_secs() -> u64 {
    js_sys::Date::now() as u64 / 1000
}
