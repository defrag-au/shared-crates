//! Reusable Cardano wallet authentication for Cloudflare Workers.
//!
//! Two layered tracks over a shared CIP-8 verification primitive:
//!
//! - **[`per_request`]** — stateless per-request signing for the platform
//!   admin auth path (CLI track from `docs/design/PLATFORM_ADMIN_AUTH.md`).
//!   Each call carries `X-Stake-*` headers; the worker re-derives the
//!   canonical payload and verifies the signature in one shot. No challenge,
//!   no session.
//!
//! - **[`ChallengeManager`] + [`AccessGate`]** — challenge/verify flow with
//!   DO-SQLite-backed nonce storage and one-time use. Used by app-specific
//!   sign-in flows (CrowdLock, NFT-gated apps). This is the model the future
//!   admin portal frontend uses (Slice C).
//!
//! - **[`evm`]** — the EVM sibling of the CIP-8 primitive: EIP-191
//!   `personal_sign` recovery, so an EVM address is proven the way a stake
//!   address is (challenge → sign → recover → compare). One primitive, no
//!   layered tracks; not re-exported at the crate root because its
//!   `verify_signature` is a different operation from the CIP-8 one.
//!
//! The low-level CIP-8 primitives ([`verify_signature`], [`derive_stake_address`],
//! [`derive_payment_pkh`]) sit underneath both — one verifier, two integration
//! shapes.
//!
//! # The two bindings
//!
//! A CIP-8 signature is only as good as what it's bound to, and there are two
//! separate bindings that both have to hold:
//!
//! 1. **Message binding** — the signature must attest to *our* challenge bytes,
//!    not to some other message. [`verify_signature`] enforces this; without
//!    it, a signature harvested from any other dapp satisfies our flow.
//! 2. **Identity binding** — the signing key must own the address the client
//!    *claims*. [`verify_signature`] deliberately can't do this (it never sees
//!    an address), so callers derive with [`derive_stake_address`] and compare.
//!    All three tracks now do; treat the derived address as ground truth and
//!    the claim as a sanity check, never the reverse.
//!
//! # Usage — per-request (Slice B)
//!
//! ```rust,ignore
//! use wallet_auth::{verify_per_request, VerifiedRequest};
//! use pallas_addresses::Network;
//!
//! let verified = verify_per_request(
//!     "POST", "/api/clients", body, timestamp, now_unix,
//!     cose_sig_hex, cose_key_hex, claimed_stake, Network::Testnet,
//! )?;
//! // verified.stake_address → hand to the allowlist
//! ```
//!
//! # Usage — challenge/verify (Slice C / existing apps)
//!
//! ```rust,ignore
//! use wallet_auth::{ChallengeManager, verify_signature, AccessGate, GateOutcome};
//!
//! let challenge_mgr = ChallengeManager::new("Sign in to My App");
//! let resp = challenge_mgr.create_challenge(storage, &stake_address)?;
//! // ... wallet signs ...
//! let result = challenge_mgr.verify_and_gate(storage, &verify_req, &my_gate).await?;
//! ```

// Keep the `js` feature unified for `rand_core` 0.6 / `getrandom` 0.2 in a
// consumer's wasm graph. The dependency is deliberate even though nothing here
// calls it: `ed25519-dalek` and `pallas-crypto` both reach that major.
use getrandom_02 as _;

mod cip8;
pub mod evm;
pub mod midnight;
pub mod per_request;
pub mod realm;

pub use cip8::{derive_payment_pkh, derive_stake_address, verify_signature};
pub use per_request::{
    PerRequestError, SignError, SignedRequest, TIMESTAMP_WINDOW_SECS, VerifiedRequest,
    canonical_payload, sign_payload, sign_per_request, verify_per_request,
};
pub use realm::Realm;

// ── `worker-do` feature: DO-backed + KV-backed challenge/session machinery ──
// Gated so native binaries (e.g. `tools/platformadm`) can consume the
// per-request + sign primitives without dragging the `worker` runtime in.
#[cfg(feature = "worker-do")]
mod challenge;
#[cfg(feature = "worker-do")]
pub mod gate;
#[cfg(feature = "worker-do")]
pub mod session;
/// The whole stake-allowlist admin surface for a worker — bind the shared
/// `WALLET_AUTH` KV, name a realm, wire three routes.
#[cfg(feature = "worker-do")]
pub mod worker_gate;

#[cfg(feature = "worker-do")]
pub use challenge::ChallengeManager;
#[cfg(feature = "worker-do")]
pub use gate::{AccessGate, GateOutcome};
#[cfg(feature = "worker-do")]
pub use session::{
    ChallengeIssuance, DEFAULT_SESSION_TTL_SECS, SessionError, SessionRecord, SessionResponse,
    VerifiedChallenge, WalletAuthStore, canonical_challenge_payload, consume_challenge,
    create_challenge, issue_session, lookup_session, revoke_session,
};
#[cfg(feature = "worker-do")]
pub use worker_gate::{AuthFailure, AuthPrincipal, Gate, GateConfig, StakeTier};

// The wire types the challenge/session flows speak. Re-exported so a consumer
// can name them through this crate as well as `wallet-auth-wire`.
pub use wallet_auth_wire::*;
