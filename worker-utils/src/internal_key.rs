//! Shared-secret gating for service-to-service endpoints.
//!
//! Three workers had their own copy of this — and they had **diverged**. Two
//! compared in constant time; the third did not, and it was the one publicly
//! routed on its own hostname gating six endpoints. That is the failure mode
//! of copied security code: it is not that a copy is wrong on the day it is
//! written, it is that the copies stop agreeing and nobody is looking at all
//! three at once.
//!
//! It lives beside [`crate::secrets`], which every copy already used to read
//! the value.

use worker_stack::worker::{Env, Request, Response};

/// Env var / secret name. One spelling, so a typo can't silently open a door.
pub const INTERNAL_API_KEY: &str = "INTERNAL_API_KEY";

/// Gate a request on the internal shared secret.
///
/// Returns `None` when the caller is authorised, or `Some(response)` to return
/// immediately — the shape callers already use:
///
/// ```ignore
/// if let Some(denied) = worker_utils::internal_key::require(&req, &env).await {
///     return Ok(denied);
/// }
/// ```
///
/// # Refuses when the secret is unset
///
/// A missing secret means *everything* would otherwise be let through, since
/// an empty expected value trivially matches. Failing closed with 503 says
/// "this worker is misconfigured", which is both true and actionable — as
/// against 401, which blames the caller for our deployment.
pub async fn require(req: &Request, env: &Env) -> Option<Response> {
    let expected = crate::secrets::get_secret(env, INTERNAL_API_KEY)
        .await
        .unwrap_or_default();

    if expected.is_empty() {
        tracing::error!("{INTERNAL_API_KEY} is not configured — refusing internal traffic");
        return Response::error("service unavailable", 503).ok();
    }

    let presented = req.headers().get("X-Internal-Key").ok().flatten();
    if presented
        .as_deref()
        .is_some_and(|key| constant_time_eq(key.as_bytes(), expected.as_bytes()))
    {
        return None;
    }

    tracing::warn!("rejected internal request without a valid key");
    Response::error("unauthorized", 401).ok()
}

/// Compare without leaking where the first mismatch is.
///
/// A byte-wise `==` returns as soon as it finds a difference, so response time
/// reveals how many leading bytes were right — enough to recover a secret one
/// byte at a time. Length is folded into the accumulator rather than
/// short-circuited for the same reason.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn matches_only_on_exact_equality() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secrez"));
        assert!(!constant_time_eq(b"secret", b"secre"));
        assert!(!constant_time_eq(b"secret", b"secrett"));
        assert!(!constant_time_eq(b"", b"secret"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn a_shared_prefix_does_not_match() {
        // The case the timing attack exploits: a long correct prefix must be
        // no more "equal" than a wrong first byte.
        assert!(!constant_time_eq(b"secretkey", b"secretkez"));
        assert!(!constant_time_eq(b"aaaaaaaaa", b"aaaaaaaab"));
    }
}
