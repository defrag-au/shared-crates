//! KV-backed challenge/verify + session storage for wallet-stake sign-in.
//!
//! One shared KV namespace per environment (`WALLET_AUTH`), with keys scoped
//! by [`Realm`] so every wallet-authed worker can bind the same namespace and
//! still only see its own records — see `realm.rs` for the layout and why.
//!
//! - `auth:{realm}:challenge:<challenge_id>` (TTL 5min) — short-lived nonce
//!   records, created by `POST /auth/challenge`, consumed by `POST /auth/verify`.
//!   Single-use: deleted on successful verify.
//! - `auth:{realm}:session:<token>` (TTL 8h) — opaque-token session records,
//!   issued on successful verify, looked up on each authenticated request via
//!   `X-Session-Token`. Deleted on `POST /auth/logout`.
//!
//! The CIP-8 signature verification reuses [`super::cip8::verify_signature`]
//! — the cryptographic primitive is the same as the per-request path. Only
//! the payload format differs:
//!
//! ```text
//! v1-challenge\n
//! {domain}\n
//! {challenge_id}\n
//! {nonce_hex}\n
//! {expires_at_ms}
//! ```
//!
//! The `domain` line pins the signature to a specific deployment — a
//! signature obtained against `platform-dev.hodlcroft.com` won't verify on
//! `platform.hodlcroft.com` because the canonical payload differs.
//!
//! [`WalletAuthStore`] is the API. The free functions at the bottom are the
//! original unscoped surface, kept for the client portal; they are
//! [`Realm::LEGACY`] on the same store.
//!
//! See `docs/design/PLATFORM_ADMIN_AUTH.md` for the full model.

use pallas_addresses::Network;
use serde::{Deserialize, Serialize};
use worker_stack::worker::kv::KvStore;

use crate::cip8;
use crate::realm::Realm;

const CHALLENGE_TTL_SECS: u64 = 300;
pub const DEFAULT_SESSION_TTL_SECS: u64 = 8 * 3600;
const NONCE_BYTES: usize = 32;
const TOKEN_BYTES: usize = 32;

// ─────────────────────────────────────────────────────────────────────
// Wire types
// ─────────────────────────────────────────────────────────────────────

/// Returned to the client by `POST /auth/challenge`. The wallet signs the
/// `payload_to_sign_hex` bytes; the client then POSTs the COSE blobs back
/// alongside `challenge_id`.
///
/// Named `ChallengeIssuance` (not `ChallengeResponse`) to avoid the legacy
/// `wallet_auth_wire::ChallengeResponse` (DO-SQLite flow, different
/// schema). Both modules coexist behind the `worker-do` feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeIssuance {
    pub challenge_id: String,
    /// Hex-encoded bytes the wallet should sign via CIP-30 `signData()`.
    /// Same bytes the server will reconstruct + use as the Sig_Structure
    /// payload when verifying.
    pub payload_to_sign_hex: String,
    /// Convenience for clients — same window the server enforces. Unix ms.
    pub expires_at_ms: i64,
}

/// Returned to the client by `POST /auth/verify`. The opaque `token` goes
/// into the `X-Session-Token` header for subsequent authenticated requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub token: String,
    pub stake_address: String,
    pub tier: String,
    pub expires_at_ms: i64,
}

/// What a verified challenge yielded. The caller (worker) takes this, applies
/// the allowlist, then issues a session.
#[derive(Debug, Clone)]
pub struct VerifiedChallenge {
    pub stake_address: String,
}

// ─────────────────────────────────────────────────────────────────────
// KV records (stored as JSON under the prefix keys)
// ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChallengeRecord {
    challenge_id: String,
    nonce_hex: String,
    domain: String,
    expires_at_ms: i64,
}

/// In-KV session record. Looked up by the auth gate. Public for callers that
/// want to surface session details (e.g. `/api/whoami`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub stake_address: String,
    /// Mirrors the consumer's `StakeTier`. String for KV-format portability.
    pub tier: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
    /// Which realm issued this session. Absent on records written before
    /// realms existed (legacy namespace only); a scoped store refuses a
    /// record whose realm is not its own even if the key matched.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub realm: String,
}

// ─────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────

/// Errors from the session-store API. All client-facing variants should map
/// to 401/400; internal variants to 500.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("challenge not found or already consumed")]
    ChallengeNotFound,
    #[error("challenge expired")]
    ChallengeExpired,
    #[error("CIP-8 verify: {0}")]
    CipVerify(String),
    #[error("could not derive stake address: {0}")]
    AddressDerive(String),
    #[error("KV: {0}")]
    Kv(String),
    #[error("serde: {0}")]
    Serde(String),
    #[error("rand: {0}")]
    Rand(String),
}

// ─────────────────────────────────────────────────────────────────────
// Canonical payload (pure — shared between server and client)
// ─────────────────────────────────────────────────────────────────────

/// Construct the canonical bytes the wallet signs for a challenge.
///
/// Public so the **browser portal** can independently reconstruct + display
/// the human-readable representation of what they're about to sign, and so
/// any future CLI can sign+POST manually.
pub fn canonical_challenge_payload(
    domain: &str,
    challenge_id: &str,
    nonce_hex: &str,
    expires_at_ms: i64,
) -> Vec<u8> {
    format!("v1-challenge\n{domain}\n{challenge_id}\n{nonce_hex}\n{expires_at_ms}").into_bytes()
}

// ─────────────────────────────────────────────────────────────────────
// The store
// ─────────────────────────────────────────────────────────────────────

/// A realm's view of the shared auth namespace.
pub struct WalletAuthStore<'a> {
    kv: &'a KvStore,
    realm: Realm,
}

impl<'a> WalletAuthStore<'a> {
    pub fn new(kv: &'a KvStore, realm: Realm) -> Self {
        Self { kv, realm }
    }

    pub fn realm(&self) -> Realm {
        self.realm
    }

    /// Generate a fresh challenge, persist it with a 5-minute TTL, and return
    /// the `challenge_id` + canonical payload for the wallet to sign.
    ///
    /// `domain` should be the host the worker was reached on — it is part of
    /// the signed payload so a signature can't be replayed against a
    /// different deployment.
    pub async fn create_challenge(&self, domain: &str) -> Result<ChallengeIssuance, SessionError> {
        let challenge_id = random_hex(16); // 128-bit id
        let nonce_hex = random_hex(NONCE_BYTES);
        let now_ms = (js_sys::Date::now()) as i64;
        let expires_at_ms = now_ms + (CHALLENGE_TTL_SECS as i64) * 1000;

        let record = ChallengeRecord {
            challenge_id: challenge_id.clone(),
            nonce_hex: nonce_hex.clone(),
            domain: domain.to_string(),
            expires_at_ms,
        };
        let body =
            serde_json::to_string(&record).map_err(|e| SessionError::Serde(e.to_string()))?;
        self.kv
            .put(&self.realm.challenge_key(&challenge_id), body)
            .map_err(|e| SessionError::Kv(format!("put builder: {e}")))?
            .expiration_ttl(CHALLENGE_TTL_SECS)
            .execute()
            .await
            .map_err(|e| SessionError::Kv(format!("put exec: {e}")))?;

        let payload = canonical_challenge_payload(domain, &challenge_id, &nonce_hex, expires_at_ms);
        Ok(ChallengeIssuance {
            challenge_id,
            payload_to_sign_hex: hex::encode(payload),
            expires_at_ms,
        })
    }

    /// Verify a wallet-signed challenge response. On success, deletes the
    /// challenge record (single-use) and returns the verified stake address.
    ///
    /// The caller then applies its allowlist and, if accepted, issues a
    /// session via [`Self::issue_session`].
    pub async fn consume_challenge(
        &self,
        challenge_id: &str,
        cose_sig_hex: &str,
        cose_key_hex: &str,
        network: Network,
    ) -> Result<VerifiedChallenge, SessionError> {
        let key = self.realm.challenge_key(challenge_id);
        let stored = self
            .kv
            .get(&key)
            .text()
            .await
            .map_err(|e| SessionError::Kv(format!("get: {e}")))?
            .ok_or(SessionError::ChallengeNotFound)?;
        let record: ChallengeRecord =
            serde_json::from_str(&stored).map_err(|e| SessionError::Serde(e.to_string()))?;

        let now_ms = (js_sys::Date::now()) as i64;
        if record.expires_at_ms < now_ms {
            // Best-effort cleanup; the KV TTL will sweep it anyway.
            let _ = self.kv.delete(&key).await;
            return Err(SessionError::ChallengeExpired);
        }

        // Reconstruct the exact bytes the wallet should have signed. The CIP-8
        // verifier uses the envelope's inlined payload to rebuild Sig_Structure;
        // we additionally compare the envelope's payload to our canonical bytes
        // (same defence-in-depth as per-request) so a wallet that signed a
        // *different* message can't pass verification just because the embedded
        // signature is mathematically valid.
        let expected_payload = canonical_challenge_payload(
            &record.domain,
            &record.challenge_id,
            &record.nonce_hex,
            record.expires_at_ms,
        );
        let envelope_payload =
            extract_cose_sign1_payload(cose_sig_hex).map_err(SessionError::CipVerify)?;
        if envelope_payload != expected_payload {
            return Err(SessionError::CipVerify(
                "envelope payload does not match expected challenge bytes".into(),
            ));
        }

        let expected_str = std::str::from_utf8(&expected_payload)
            .expect("canonical_challenge_payload produces UTF-8");
        let ok = cip8::verify_signature(cose_sig_hex, cose_key_hex, expected_str)
            .map_err(SessionError::CipVerify)?;
        if !ok {
            return Err(SessionError::CipVerify("signature did not verify".into()));
        }

        // Derive stake address from the public key + worker's network byte.
        let stake_address = derive_stake_address(cose_key_hex, network)?;

        // Consume challenge — best-effort delete; if the KV is slow the TTL
        // sweeps it anyway, but we want explicit cleanup on the happy path.
        let _ = self.kv.delete(&key).await;

        Ok(VerifiedChallenge { stake_address })
    }

    /// Issue a session for an authenticated stake. Returns the opaque token;
    /// KV handles the TTL.
    pub async fn issue_session(
        &self,
        stake_address: &str,
        tier: &str,
        ttl_secs: u64,
    ) -> Result<SessionResponse, SessionError> {
        let token = random_hex(TOKEN_BYTES);
        let now_ms = (js_sys::Date::now()) as i64;
        let expires_at_ms = now_ms + (ttl_secs as i64) * 1000;

        let record = SessionRecord {
            stake_address: stake_address.to_string(),
            tier: tier.to_string(),
            issued_at_ms: now_ms,
            expires_at_ms,
            realm: self.realm.name().to_string(),
        };
        let body =
            serde_json::to_string(&record).map_err(|e| SessionError::Serde(e.to_string()))?;
        self.kv
            .put(&self.realm.session_key(&token), body)
            .map_err(|e| SessionError::Kv(format!("put builder: {e}")))?
            .expiration_ttl(ttl_secs)
            .execute()
            .await
            .map_err(|e| SessionError::Kv(format!("put exec: {e}")))?;

        Ok(SessionResponse {
            token,
            stake_address: stake_address.to_string(),
            tier: tier.to_string(),
            expires_at_ms,
        })
    }

    /// Look up a session by token. `None` when the token is unknown, expired,
    /// or was issued by another realm.
    pub async fn lookup_session(&self, token: &str) -> Result<Option<SessionRecord>, SessionError> {
        let key = self.realm.session_key(token);
        let stored = self
            .kv
            .get(&key)
            .text()
            .await
            .map_err(|e| SessionError::Kv(format!("get: {e}")))?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let record: SessionRecord =
            serde_json::from_str(&stored).map_err(|e| SessionError::Serde(e.to_string()))?;
        let now_ms = (js_sys::Date::now()) as i64;
        if record.expires_at_ms < now_ms {
            // Race with TTL — treat as gone, best-effort delete.
            let _ = self.kv.delete(&key).await;
            return Ok(None);
        }
        // Belt and braces over the key scoping: a record is only valid to
        // the realm that wrote it. Legacy records carry no realm and are
        // only honoured by the legacy realm.
        if record.realm != self.realm.name() {
            return Ok(None);
        }
        Ok(Some(record))
    }

    /// Revoke a session — `POST /auth/logout`. Idempotent.
    pub async fn revoke_session(&self, token: &str) -> Result<(), SessionError> {
        self.kv
            .delete(&self.realm.session_key(token))
            .await
            .map_err(|e| SessionError::Kv(format!("delete: {e}")))?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────
// Legacy unscoped surface — the client portal's original API. Same store,
// `Realm::LEGACY`. New consumers use `WalletAuthStore` with their own realm.
// ─────────────────────────────────────────────────────────────────────

pub async fn create_challenge(
    kv: &KvStore,
    domain: &str,
) -> Result<ChallengeIssuance, SessionError> {
    WalletAuthStore::new(kv, Realm::LEGACY)
        .create_challenge(domain)
        .await
}

pub async fn consume_challenge(
    kv: &KvStore,
    challenge_id: &str,
    cose_sig_hex: &str,
    cose_key_hex: &str,
    network: Network,
) -> Result<VerifiedChallenge, SessionError> {
    WalletAuthStore::new(kv, Realm::LEGACY)
        .consume_challenge(challenge_id, cose_sig_hex, cose_key_hex, network)
        .await
}

pub async fn issue_session(
    kv: &KvStore,
    stake_address: &str,
    tier: &str,
    ttl_secs: u64,
) -> Result<SessionResponse, SessionError> {
    WalletAuthStore::new(kv, Realm::LEGACY)
        .issue_session(stake_address, tier, ttl_secs)
        .await
}

pub async fn lookup_session(
    kv: &KvStore,
    token: &str,
) -> Result<Option<SessionRecord>, SessionError> {
    WalletAuthStore::new(kv, Realm::LEGACY)
        .lookup_session(token)
        .await
}

pub async fn revoke_session(kv: &KvStore, token: &str) -> Result<(), SessionError> {
    WalletAuthStore::new(kv, Realm::LEGACY)
        .revoke_session(token)
        .await
}

// ─────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────

/// Generate `n_bytes` of CSPRNG entropy, hex-encoded. Used for both challenge
/// IDs and session tokens; both have 128–256 bits of entropy = collision-free
/// at any practical scale.
fn random_hex(n_bytes: usize) -> String {
    let mut buf = vec![0u8; n_bytes];
    // getrandom on wasm32 routes through the `wasm_js` backend (configured
    // at the wallet-auth crate level). On a worker that means the V8 CSPRNG.
    if let Err(e) = getrandom::fill(&mut buf) {
        // Should be unreachable on a Cloudflare worker; surface as a panic
        // rather than silently degrade to a constant-ID token.
        panic!("getrandom failed: {e}");
    }
    hex::encode(buf)
}

/// Pull the embedded payload bytes from a COSE_Sign1 envelope.
fn extract_cose_sign1_payload(cose_sig_hex: &str) -> Result<Vec<u8>, String> {
    let bytes = hex::decode(cose_sig_hex).map_err(|e| format!("hex: {e}"))?;
    let value: ciborium::Value =
        ciborium::from_reader(bytes.as_slice()).map_err(|e| format!("CBOR: {e}"))?;
    let array = match value {
        ciborium::Value::Array(a) => a,
        _ => return Err("COSE_Sign1 must be a CBOR array".into()),
    };
    if array.len() != 4 {
        return Err(format!(
            "COSE_Sign1 expects 4 elements, got {}",
            array.len()
        ));
    }
    match &array[2] {
        ciborium::Value::Bytes(b) => Ok(b.clone()),
        ciborium::Value::Null => Err(
            "COSE_Sign1 payload is null (detached); challenge flow requires inline payload".into(),
        ),
        _ => Err("COSE_Sign1 payload field must be bytes".into()),
    }
}

/// Derive the bech32 stake address from a hex-encoded COSE_Key + a network
/// byte. Thin wrapper over [`cip8::derive_stake_address`] for this module's
/// error type — the derivation itself lives in `cip8` so the challenge,
/// session and per-request tracks cannot drift on what an address means.
fn derive_stake_address(cose_key_hex: &str, network: Network) -> Result<String, SessionError> {
    cip8::derive_stake_address(cose_key_hex, network).map_err(SessionError::AddressDerive)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_challenge_payload_is_stable() {
        let a =
            canonical_challenge_payload("platform.example.com", "abc", "deadbeef", 1717059600000);
        let b =
            canonical_challenge_payload("platform.example.com", "abc", "deadbeef", 1717059600000);
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_challenge_payload_pins_domain() {
        let dev = canonical_challenge_payload("platform-dev.example", "abc", "00", 1);
        let prod = canonical_challenge_payload("platform.example", "abc", "00", 1);
        assert_ne!(dev, prod);
    }

    #[test]
    fn canonical_challenge_payload_format_is_v1_challenge() {
        let p = canonical_challenge_payload("d", "id", "n", 42);
        let s = std::str::from_utf8(&p).unwrap();
        assert!(s.starts_with("v1-challenge\n"));
        assert!(s.contains("\nd\n"));
        assert!(s.contains("\nid\n"));
        assert!(s.contains("\nn\n"));
        assert!(s.ends_with("\n42"));
    }

    /// A record written before realms existed deserialises with an empty
    /// realm — which is exactly what `Realm::LEGACY` reports, so the client
    /// portal's live sessions keep validating.
    #[test]
    fn legacy_session_records_deserialise_with_an_empty_realm() {
        let raw = r#"{"stake_address":"stake1u9x","tier":"super_admin","issued_at_ms":1,"expires_at_ms":2}"#;
        let record: SessionRecord = serde_json::from_str(raw).unwrap();
        assert_eq!(record.realm, "");
        assert_eq!(record.realm, Realm::LEGACY.name());
    }
}
