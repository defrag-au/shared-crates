//! Provider-agnostic transaction submission — a [`SubmitProvider`] trait plus an
//! ordered [`submit_with_fallback`] so a caller can PREFER a self-hosted submitter
//! (e.g. mitos/dolos, which holds the node connection and diffuses via Ouroboros)
//! and fall through to a third party (e.g. Maestro) ONLY on genuine downtime.
//!
//! The load-bearing decision lives in [`SubmitError`]: distinguishing
//! **provider unavailable** (try the next provider) from **chain rejection**
//! (stop — the tx is invalid everywhere). Getting that wrong is how a naive
//! fallback either gives up on a healthy tx (treating downtime as rejection) or
//! pointlessly hammers every provider with an invalid one (treating rejection as
//! downtime). The classification reuses the platform-agnostic ledger-rejection
//! patterns in [`crate::error::SubmissionResult`].
//!
//! Falling through on `Unavailable` is SAFE because a signed tx is deterministic
//! (same body ⇒ same tx hash): if the first provider actually landed it before
//! appearing down, the next provider (or a later retry) sees a duplicate, which is
//! [`SubmitOk::Duplicate`] — idempotent success, not a double-spend.

use async_trait::async_trait;

use crate::error::SubmissionResult;

/// A successful — or idempotently-successful — submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOk {
    /// The provider accepted the tx into its mempool.
    Accepted,
    /// The tx was already known to the provider (duplicate / already in mempool /
    /// already on chain). Idempotent success: the deterministic tx hash means a
    /// prior attempt — or a fallback sibling — already delivered this exact tx.
    Duplicate,
}

/// Why a submit attempt didn't land — the classification that makes an ordered
/// fallback safe.
#[derive(Debug, Clone)]
pub enum SubmitError {
    /// The provider couldn't be reached or is degraded (connect failure, timeout,
    /// 5xx). The tx's fate AT THIS PROVIDER is unknown, so the fallback may try the
    /// next one. Maps to the caller's "transient → retry later" path.
    Unavailable(String),
    /// The chain / mempool REJECTED the tx (phase-1/2 invalid, bad inputs, fee too
    /// small, …). The tx is invalid on every provider → the fallback STOPS and the
    /// caller treats it as a permanent failure (no amount of retrying helps).
    Rejected(String),
}

impl std::fmt::Display for SubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubmitError::Unavailable(m) => write!(f, "provider unavailable: {m}"),
            SubmitError::Rejected(m) => write!(f, "tx rejected: {m}"),
        }
    }
}

impl std::error::Error for SubmitError {}

impl SubmitError {
    /// True if the fallback should try the NEXT provider (downtime), false if it
    /// should stop (the chain rejected the tx).
    pub fn is_unavailable(&self) -> bool {
        matches!(self, SubmitError::Unavailable(_))
    }
}

/// Classify a provider's NON-success HTTP response into the fallback decision,
/// reusing the shared ledger-rejection heuristics ([`SubmissionResult`]): an
/// HTTP 400 or a known ledger-validation pattern in the body ⇒ [`SubmitError::Rejected`];
/// anything else (5xx, gateway/timeout text, connect failure surfaced as a
/// synthetic status) ⇒ [`SubmitError::Unavailable`]. Providers detect their own
/// success (2xx) and duplicate (e.g. dolos `409 DuplicateTx`) responses BEFORE
/// calling this — "already known" wording is provider-specific.
pub fn classify_failure(status_code: u16, message: impl Into<String>) -> SubmitError {
    let message = message.into();
    let res = SubmissionResult {
        status_code,
        message: message.clone(),
    };
    if res.is_permanent_rejection() {
        SubmitError::Rejected(message)
    } else {
        SubmitError::Unavailable(message)
    }
}

/// A transaction submitter — one route from signed CBOR to the chain's mempool.
///
/// Implementors own their transport (HTTP to mitos/dolos, HTTP to Maestro, …) and
/// MUST map their outcome onto [`SubmitOk`] / [`SubmitError`] honestly — in
/// particular, classify "I couldn't reach the chain" as [`SubmitError::Unavailable`]
/// and "the chain said no" as [`SubmitError::Rejected`] (use [`classify_failure`]).
/// Object-safe (`dyn`) so [`submit_with_fallback`] can hold a heterogeneous list.
#[async_trait(?Send)]
pub trait SubmitProvider {
    /// Stable identifier for logs / metrics (e.g. `"mitos"`, `"maestro"`).
    fn name(&self) -> &str;

    /// Submit raw signed transaction CBOR. The caller already knows the tx hash
    /// (deterministic from the signed body), so the result reports only acceptance
    /// vs. the reason it didn't land.
    async fn submit(&self, tx_cbor: &[u8]) -> Result<SubmitOk, SubmitError>;
}

/// Submit through `providers` in PREFERENCE ORDER, returning on the first
/// acceptance (or idempotent duplicate). Falls through to the next provider only
/// on [`SubmitError::Unavailable`]; a [`SubmitError::Rejected`] stops immediately
/// (the tx is invalid everywhere). If every provider is unavailable, returns the
/// LAST `Unavailable` so the caller treats the whole attempt as transient and
/// retries later. An empty list is itself `Unavailable` (misconfiguration).
///
/// Safe under at-least-once / double-submit: a deterministic tx hash means a tx
/// that landed at an earlier provider surfaces as `Duplicate` at a later one.
pub async fn submit_with_fallback(
    providers: &[&dyn SubmitProvider],
    tx_cbor: &[u8],
) -> Result<SubmitOk, SubmitError> {
    let mut last: Option<SubmitError> = None;
    for provider in providers {
        match provider.submit(tx_cbor).await {
            Ok(ok) => {
                tracing::info!(provider = provider.name(), ?ok, "tx submitted");
                return Ok(ok);
            }
            Err(SubmitError::Rejected(m)) => {
                // Invalid everywhere — stop; trying others only repeats it.
                tracing::warn!(provider = provider.name(), reason = %m, "tx rejected — not retrying other providers");
                return Err(SubmitError::Rejected(m));
            }
            Err(e @ SubmitError::Unavailable(_)) => {
                tracing::warn!(provider = provider.name(), error = %e, "provider unavailable — trying next");
                last = Some(e);
            }
        }
    }
    Err(last.unwrap_or_else(|| SubmitError::Unavailable("no submit providers configured".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A provider scripted with a fixed outcome that records whether it was hit.
    struct Mock {
        id: &'static str,
        outcome: Cell<Option<Result<SubmitOk, SubmitError>>>,
        called: Cell<bool>,
    }
    impl Mock {
        fn new(id: &'static str, outcome: Result<SubmitOk, SubmitError>) -> Self {
            Self {
                id,
                outcome: Cell::new(Some(outcome)),
                called: Cell::new(false),
            }
        }
    }
    #[async_trait(?Send)]
    impl SubmitProvider for Mock {
        fn name(&self) -> &str {
            self.id
        }
        async fn submit(&self, _tx: &[u8]) -> Result<SubmitOk, SubmitError> {
            self.called.set(true);
            self.outcome.take().expect("mock called twice")
        }
    }

    #[tokio::test]
    async fn first_acceptance_short_circuits() {
        let a = Mock::new("a", Ok(SubmitOk::Accepted));
        let b = Mock::new("b", Err(SubmitError::Unavailable("down".into())));
        let r = submit_with_fallback(&[&a, &b], b"tx").await;
        assert_eq!(r.unwrap(), SubmitOk::Accepted);
        assert!(a.called.get());
        assert!(
            !b.called.get(),
            "second provider must not be tried after accept"
        );
    }

    #[tokio::test]
    async fn unavailable_falls_through() {
        let a = Mock::new("a", Err(SubmitError::Unavailable("5xx".into())));
        let b = Mock::new("b", Ok(SubmitOk::Accepted));
        let r = submit_with_fallback(&[&a, &b], b"tx").await;
        assert_eq!(r.unwrap(), SubmitOk::Accepted);
        assert!(a.called.get() && b.called.get());
    }

    #[tokio::test]
    async fn rejected_stops_immediately() {
        let a = Mock::new("a", Err(SubmitError::Rejected("BadInputsUTxO".into())));
        let b = Mock::new("b", Ok(SubmitOk::Accepted));
        let r = submit_with_fallback(&[&a, &b], b"tx").await;
        assert!(matches!(r, Err(SubmitError::Rejected(_))));
        assert!(a.called.get());
        assert!(
            !b.called.get(),
            "a rejection must NOT fall through to other providers"
        );
    }

    #[tokio::test]
    async fn all_unavailable_returns_last() {
        let a = Mock::new("a", Err(SubmitError::Unavailable("a-down".into())));
        let b = Mock::new("b", Err(SubmitError::Unavailable("b-down".into())));
        let r = submit_with_fallback(&[&a, &b], b"tx").await;
        match r {
            Err(SubmitError::Unavailable(m)) => assert_eq!(m, "b-down"),
            other => panic!("expected last Unavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn duplicate_is_success() {
        let a = Mock::new("a", Ok(SubmitOk::Duplicate));
        let r = submit_with_fallback(&[&a], b"tx").await;
        assert_eq!(r.unwrap(), SubmitOk::Duplicate);
    }

    #[tokio::test]
    async fn empty_list_is_unavailable() {
        let r = submit_with_fallback(&[], b"tx").await;
        assert!(matches!(r, Err(SubmitError::Unavailable(_))));
    }

    #[test]
    fn classify_400_is_rejected() {
        assert!(matches!(
            classify_failure(400, "bad"),
            SubmitError::Rejected(_)
        ));
    }

    #[test]
    fn classify_ledger_pattern_is_rejected() {
        assert!(matches!(
            classify_failure(500, "ValueNotConservedUTxO ..."),
            SubmitError::Rejected(_)
        ));
    }

    #[test]
    fn classify_5xx_is_unavailable() {
        assert!(matches!(
            classify_failure(503, "gateway timeout"),
            SubmitError::Unavailable(_)
        ));
    }
}
