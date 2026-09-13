//! [`AuthStrategy`] for the stake-key wallet session — the one where the
//! wallet *is* the identity.
//!
//! # The shape, and how it differs from the Discord paths
//!
//! Sign-in is an **in-page round trip**: connect a wallet, fetch a challenge,
//! `signData`, exchange the signature for an opaque session. Several seconds,
//! all of it watchable. This is the only strategy that reports
//! [`AuthPhase::Pending`] — the redirect paths go from `Anonymous` to *gone*.
//!
//! Every consumer serves the same two same-origin routes (`POST /auth/challenge`,
//! `POST /auth/verify`) from `services/wallet-auth`'s `Gate`, so unlike
//! [`crate::discord_session`] there is **no URL to configure**: a storage key
//! and a realm label are the whole per-app surface.
//!
//! # ⚠️ The signing wallet MUST be the wallet that signed in
//!
//! This is an authorisation hole that the three admin surfaces all had open.
//!
//! You sign in with wallet A, proving stake address A. The worker issues a
//! session bound to A and its allowlist tier. You then switch account in the
//! extension — or reconnect a different wallet — and the app happily builds and
//! signs a transaction with wallet **B**, while every request still carries A's
//! token. The server believes A authorised it. Nothing in CIP-30 tells the page
//! the account changed, so this is silent.
//!
//! [`WalletBinding`] is the answer, and it is enforced structurally rather than
//! by asking call sites to remember:
//!
//! **[`StakeAuthStrategy::wallet_api`] returns `None` unless the binding is
//! [`WalletBinding::Bound`].** An app that forgets to check cannot obtain the
//! handle to sign with, so the hole cannot be reopened by omission.
//!
//! A [`WalletBinding::Mismatched`] is reported as an auth error, loudly, naming
//! both stake addresses. It deliberately does **not** sign the user out: the
//! session is still valid for reads, and silently dropping it would lose their
//! place and hide the cause. They are told what happened and given the two ways
//! out — reconnect the original wallet, or sign in again as the new one.
//!
//! ## What this catches, and the one thing it does not
//!
//! Caught: reconnecting a different wallet, connecting after a reload as
//! someone else, and any drift observed by [`StakeAuthStrategy::recheck_binding`]
//! (which re-reads the live wallet rather than trusting the value cached at
//! connect time).
//!
//! ⚠️ **Not caught by itself: a silent in-extension account switch** between one
//! recheck and the next. CIP-30 has no account-changed event, so the only
//! defence is to re-read before it matters. **Call `recheck_binding` before
//! building or submitting a transaction**, not merely on a timer — the window
//! that matters is between "user clicks sign" and "wallet signs", and a timer
//! cannot close it.
//!
//! # Why only half of this module is `wasm32`-gated
//!
//! [`classify`] is the security-relevant decision in the whole design. Gating
//! the module would mean `cargo test` never ran it. So the decisions live out
//! here and the browser plumbing is gated — same split as [`crate::auth_worker`].

// Off-wasm the only callers of the logic below are the tests — see above.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use authorizations::SessionClaims;

use crate::auth::AuthPhase;

#[cfg(target_arch = "wasm32")]
mod imp;
#[cfg(target_arch = "wasm32")]
pub use imp::StakeAuthStrategy;

/// The two things that differ between properties on this strategy.
#[derive(Clone, Debug)]
pub struct StakeAuthConfig {
    /// localStorage key for the session — `"abandonware/session"`,
    /// `"actions/session"`, `"script-depot/session"`.
    ///
    /// Per-app because the sessions are per-realm: a token minted for one
    /// worker's allowlist is meaningless to another, and sharing a key would
    /// have one property's sign-out silently log you out of the next.
    pub session_key: String,
    /// What the user is signing in to, as the panel names it.
    pub realm_label: String,
}

impl StakeAuthConfig {
    pub fn new(session_key: impl Into<String>, realm_label: impl Into<String>) -> Self {
        Self {
            session_key: session_key.into(),
            realm_label: realm_label.into(),
        }
    }
}

/// Whether the wallet in the browser is the wallet the session was issued to.
///
/// A named enum rather than a `bool`, because "not bound" has three genuinely
/// different causes and only one of them is an error the user must act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletBinding {
    /// No session, so nothing to bind to. Not an error — the ordinary state
    /// before sign-in.
    Unauthenticated,
    /// Signed in, wallet connected, same stake key. The only state in which
    /// [`StakeAuthStrategy::wallet_api`] yields a handle.
    Bound,
    /// Signed in, but no wallet connected — a reload, or the user disconnected.
    /// Recoverable by reconnecting; not an authorisation failure.
    Disconnected,
    /// ⚠️ **Signed in as one stake key while holding another.**
    ///
    /// An authorisation failure: the session's authority belongs to
    /// `signed_in`, and anything `active` signs would be attributed to
    /// `signed_in` by the server.
    Mismatched {
        /// The stake address the session was issued to.
        signed_in: String,
        /// The stake address the connected wallet actually holds.
        active: String,
    },
}

impl WalletBinding {
    /// Whether a transaction may be signed in this state.
    pub fn may_sign(&self) -> bool {
        matches!(self, WalletBinding::Bound)
    }

    /// Whether this is an authorisation failure, as opposed to an ordinary
    /// not-ready state. Drives whether the app shouts or just disables.
    pub fn is_auth_error(&self) -> bool {
        matches!(self, WalletBinding::Mismatched { .. })
    }

    /// The message to show the user, when there is something to say.
    ///
    /// `None` for the two ordinary states. `Some` for both problems, because a
    /// user staring at a disabled button deserves to know which one they are in.
    pub fn problem(&self) -> Option<String> {
        match self {
            WalletBinding::Unauthenticated | WalletBinding::Bound => None,
            WalletBinding::Disconnected => {
                Some("Your wallet is disconnected — reconnect it to sign.".to_string())
            }
            WalletBinding::Mismatched { signed_in, active } => Some(format!(
                "Wrong wallet. You signed in as {}, but {} is connected. \
                 Reconnect the original wallet, or sign in again as this one.",
                short_stake(signed_in),
                short_stake(active),
            )),
        }
    }
}

/// `stake1u9abc…wxyz` — enough to tell two wallets apart without a wall of hex.
fn short_stake(addr: &str) -> String {
    crate::utils::truncate_hex(addr, 12, 6)
}

/// Compare the session's stake address against the connected wallet's.
///
/// Split out from the strategy so the decision is testable off-target — it is
/// the security-relevant line in this module, and it should not be reachable
/// only through a browser.
pub(crate) fn classify(signed_in: Option<&str>, active: Option<&str>) -> WalletBinding {
    let Some(signed_in) = signed_in else {
        return WalletBinding::Unauthenticated;
    };
    let Some(active) = active else {
        return WalletBinding::Disconnected;
    };
    // Bech32 is case-insensitive in principle and always lowercase in practice
    // from CIP-30; compare case-insensitively anyway, so a wallet that shouts
    // at us is not reported as an impostor.
    if signed_in.eq_ignore_ascii_case(active) {
        WalletBinding::Bound
    } else {
        WalletBinding::Mismatched {
            signed_in: signed_in.to_string(),
            active: active.to_string(),
        }
    }
}

/// Claims for a stake session.
///
/// `tier` is carried for display only — the authority is `ent`, which this
/// strategy leaves empty. A worker that wants entitlements from a stake tier
/// mints them server-side (see `client-management`'s `portal_authz.rs`); the
/// shell never maps one to the other.
pub(crate) fn claims_for(stake_address: &str, tier: &str) -> SessionClaims {
    let mut claims = SessionClaims::for_wallet(stake_address, "");
    claims.tier = Some(tier.to_string());
    claims
}

/// The phase a freshly-constructed strategy reports.
///
/// [`AuthPhase::Restoring`], never `Anonymous`: an app that painted before
/// `restore()` would flash its sign-in panel at someone with a live session.
pub(crate) fn initial_phase() -> AuthPhase {
    AuthPhase::Restoring
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_session_is_unauthenticated_not_an_error() {
        assert_eq!(classify(None, None), WalletBinding::Unauthenticated);
        // A connected wallet with no session is still just anonymous.
        assert_eq!(
            classify(None, Some("stake1abc")),
            WalletBinding::Unauthenticated
        );
    }

    #[test]
    fn a_session_with_no_wallet_is_disconnected_not_mismatched() {
        // Reload, or the user hit disconnect. Recoverable, and NOT an
        // authorisation failure — reporting it as one would cry wolf on the
        // most ordinary state there is.
        let b = classify(Some("stake1abc"), None);
        assert_eq!(b, WalletBinding::Disconnected);
        assert!(!b.may_sign());
        assert!(!b.is_auth_error());
        assert!(b.problem().is_some());
    }

    #[test]
    fn the_same_wallet_is_bound_and_may_sign() {
        let b = classify(Some("stake1abc"), Some("stake1abc"));
        assert_eq!(b, WalletBinding::Bound);
        assert!(b.may_sign());
        assert!(b.problem().is_none());
    }

    #[test]
    fn a_different_wallet_is_an_auth_error_that_names_both() {
        // THE hole this type exists to close: signed in as one stake key,
        // holding another, server attributing the second's signature to the
        // first.
        let b = classify(Some("stake1aaa000"), Some("stake1bbb111"));
        assert!(!b.may_sign(), "must never sign with the wrong wallet");
        assert!(b.is_auth_error());
        match &b {
            WalletBinding::Mismatched { signed_in, active } => {
                assert_eq!(signed_in, "stake1aaa000");
                assert_eq!(active, "stake1bbb111");
            }
            other => panic!("expected Mismatched, got {other:?}"),
        }
        assert!(b.problem().unwrap().contains("Wrong wallet"));
    }

    #[test]
    fn only_bound_may_sign() {
        // The invariant `wallet_api()` is built on. If a variant is added and
        // defaults to signable, this catches it.
        for b in [
            WalletBinding::Unauthenticated,
            WalletBinding::Disconnected,
            WalletBinding::Mismatched {
                signed_in: "a".into(),
                active: "b".into(),
            },
        ] {
            assert!(!b.may_sign(), "{b:?} must not be signable");
        }
        assert!(WalletBinding::Bound.may_sign());
    }

    #[test]
    fn only_a_mismatch_is_an_auth_error() {
        // Disconnected must not shout — it is the state after every reload.
        assert!(!WalletBinding::Unauthenticated.is_auth_error());
        assert!(!WalletBinding::Bound.is_auth_error());
        assert!(!WalletBinding::Disconnected.is_auth_error());
    }

    #[test]
    fn case_differences_are_the_same_wallet_not_an_impostor() {
        assert_eq!(
            classify(Some("stake1abc"), Some("STAKE1ABC")),
            WalletBinding::Bound
        );
    }

    #[test]
    fn a_stake_session_carries_its_tier_for_display_but_no_authority() {
        let claims = claims_for("stake1abc", "super_admin");
        assert_eq!(claims.stake.as_deref(), Some("stake1abc"));
        assert_eq!(claims.tier.as_deref(), Some("super_admin"));
        // The shell never turns a tier into an entitlement — that is the
        // minting worker's job, per property.
        assert!(claims.entitlements().is_empty());
    }

    #[test]
    fn the_mismatch_message_shortens_both_addresses() {
        let b = WalletBinding::Mismatched {
            signed_in: "stake1u9qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqaaa".into(),
            active: "stake1u9wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwbbb".into(),
        };
        let msg = b.problem().unwrap();
        assert!(msg.contains("stake1u9qqqq"), "kept the signed-in prefix");
        assert!(msg.contains("stake1u9wwww"), "kept the active prefix");
        assert!(msg.contains("aaa") && msg.contains("bbb"), "kept suffixes");
    }

    #[test]
    fn a_fresh_strategy_is_restoring_not_anonymous() {
        assert!(initial_phase().is_settling());
        assert!(!initial_phase().is_authenticated());
    }
}
