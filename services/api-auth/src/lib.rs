//! Centralised request authentication for Cloudflare Workers.
//!
//! Supports token-based auth (debug token, internal API key) and optionally
//! JWT-based admin auth behind the `jwt` feature flag.
//!
//! Sibling [`allowlist`] module handles stake-address allowlisting for the
//! platform admin path; the CIP-8 signature check itself lives in `wallet-auth`.

pub mod allowlist;

use worker_stack::worker::{Env, Error, Request, Result};

// ============================================================================
// Auth key types
// ============================================================================

/// Which authentication method(s) to accept.
pub enum AuthKeyType {
    /// `X-Debug-Token` header, checked against `DEBUG_TOKEN` secret.
    DebugToken,
    /// `X-Internal-Key` header, checked against `INTERNAL_API_KEY` secret.
    InternalKey,
    /// Any token-based method — tries InternalKey then DebugToken.
    /// With `jwt` feature enabled, also tries JWT as a last resort.
    Any,
}

/// Which principal a request authenticated as. Returned by [`authenticate`] so
/// callers can apply capability tiering — e.g. allow the operator debug token to
/// read but require the internal key to move funds or reveal seeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    /// Authenticated via `X-Internal-Key` (worker-to-worker, full trust).
    Internal,
    /// Authenticated via `X-Debug-Token` (CLI / MCP operator, read/observability
    /// trust). Must NOT unlock custodial spends or plaintext-seed routes.
    Debug,
    /// Authenticated via JWT admin (feature-gated). Treated as full trust.
    #[cfg(feature = "jwt")]
    JwtAdmin,
}

impl Caller {
    /// True for the higher-trust principals permitted to move funds / reveal
    /// seeds. The debug token is deliberately excluded.
    pub fn is_internal(self) -> bool {
        match self {
            Caller::Internal => true,
            Caller::Debug => false,
            #[cfg(feature = "jwt")]
            Caller::JwtAdmin => true,
        }
    }
}

// ============================================================================
// Core auth function
// ============================================================================

/// Require authentication on a request.
///
/// Checks the appropriate header(s) against secrets via the worker-utils
/// secret store. Returns `Ok(())` on success, or an error suitable for
/// mapping to a 401 response.
pub async fn require(req: &Request, env: &Env, key_type: AuthKeyType) -> Result<()> {
    authenticate(req, env, key_type).await.map(|_| ())
}

/// Like [`require`], but returns WHICH principal authenticated so callers can
/// tier capabilities (read for [`Caller::Debug`], spend/seed for
/// [`Caller::is_internal`]). For `Any`, the internal key is tried first so a
/// caller presenting it is classified as the higher-trust principal.
pub async fn authenticate(req: &Request, env: &Env, key_type: AuthKeyType) -> Result<Caller> {
    match key_type {
        AuthKeyType::DebugToken => check_debug_token(req, env).await.map(|()| Caller::Debug),
        AuthKeyType::InternalKey => check_internal_key(req, env)
            .await
            .map(|()| Caller::Internal),
        AuthKeyType::Any => {
            if check_internal_key(req, env).await.is_ok() {
                return Ok(Caller::Internal);
            }
            if check_debug_token(req, env).await.is_ok() {
                return Ok(Caller::Debug);
            }
            #[cfg(feature = "jwt")]
            if check_jwt_admin(req, env).await.is_ok() {
                return Ok(Caller::JwtAdmin);
            }
            Err(Error::RustError("Unauthorized".into()))
        }
    }
}

/// [`authenticate`], plus `Authorization: Bearer <token>` as an alias for the
/// same secrets.
///
/// For protocols whose clients carry one credential header and it is always
/// this one. MCP is the case that prompted it: a stock client sends
/// `Authorization: Bearer`, and telling every one of them to send
/// `X-Internal-Key` instead is a bet that every client has a way to.
///
/// **Not the default.** A custom header cannot be attached by a browser
/// cross-origin without appearing in the CORS preflight, which makes it a
/// deliberate act; `Authorization` is more likely to be forwarded by a proxy or
/// filled in by a generic client than either of ours. Endpoints that want the
/// narrower surface keep using [`authenticate`].
///
/// Tried in trust order, so a caller presenting the internal key is classified
/// as [`Caller::Internal`] whichever header carried it.
pub async fn authenticate_with_bearer(
    req: &Request,
    env: &Env,
    key_type: AuthKeyType,
) -> Result<Caller> {
    if let Ok(caller) = authenticate(req, env, key_type).await {
        return Ok(caller);
    }

    let Some(token) = bearer_token(req) else {
        return Err(Error::RustError("Unauthorized".into()));
    };

    for (secret, caller) in [
        ("INTERNAL_API_KEY", Caller::Internal),
        ("DEBUG_TOKEN", Caller::Debug),
    ] {
        if let Ok(expected) = worker_utils::secrets::get_secret(env, secret).await {
            // The same constant-time compare the header path uses. A second
            // implementation of this is a second chance to get it wrong.
            if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
                return Ok(caller);
            }
        }
    }

    Err(Error::RustError("Unauthorized".into()))
}

/// The token from `Authorization: Bearer <token>`, if there is one.
fn bearer_token(req: &Request) -> Option<String> {
    let header = req.headers().get("Authorization").ok().flatten()?;
    parse_bearer(&header)
}

/// The token half of an `Authorization` header value.
///
/// Split out from the request so the parse is testable without one — the empty
/// and malformed cases are what stop a blank credential reaching a compare.
fn parse_bearer(header: &str) -> Option<String> {
    header
        .strip_prefix("Bearer ")
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

/// Convenience macro for token-only auth checks.
///
/// Usage:
/// ```ignore
/// api_auth::require_auth!(req, ctx.env, DebugToken)?;
/// ```
#[macro_export]
macro_rules! require_auth {
    ($req:expr, $env:expr, $key_type:ident) => {
        $crate::require(&$req, &$env, $crate::AuthKeyType::$key_type).await
    };
}

// ============================================================================
// Token checks
// ============================================================================

/// Check `X-Debug-Token` header against `DEBUG_TOKEN` secret.
async fn check_debug_token(req: &Request, env: &Env) -> Result<()> {
    check_header(req, env, "X-Debug-Token", "DEBUG_TOKEN").await
}

/// Check `X-Internal-Key` header against `INTERNAL_API_KEY` secret.
async fn check_internal_key(req: &Request, env: &Env) -> Result<()> {
    check_header(req, env, "X-Internal-Key", "INTERNAL_API_KEY").await
}

/// Generic header-vs-secret check.
async fn check_header(req: &Request, env: &Env, header: &str, secret_name: &str) -> Result<()> {
    let provided = req
        .headers()
        .get(header)
        .map_err(|e| Error::RustError(format!("{e}")))?
        .ok_or_else(|| Error::RustError(format!("Missing {header} header")))?;

    let expected = worker_utils::secrets::get_secret(env, secret_name)
        .await
        .map_err(|e| Error::RustError(format!("{secret_name} not configured: {e:?}")))?;

    if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        return Err(Error::RustError(format!("Invalid {header}")));
    }

    Ok(())
}

/// Constant-time byte-string equality for secret comparison, so the token check
/// can't be turned into a byte-by-byte oracle via response timing. A length
/// mismatch short-circuits (token length is not secret), but equal-length inputs
/// are compared in time independent of where they first differ.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn constant_time_eq_matches_and_rejects() {
        assert!(constant_time_eq(b"s3cret-token", b"s3cret-token"));
        assert!(!constant_time_eq(b"s3cret-token", b"s3cret-toker"));
        assert!(!constant_time_eq(b"short", b"longer-value"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }

    /// The empty-token case is the one that matters: `Authorization: Bearer `
    /// with nothing after it must not become a comparison against a secret that
    /// might itself be empty if the binding is missing. `None` short-circuits
    /// before any compare happens.
    #[test]
    fn a_bearer_header_yields_only_a_real_token() {
        for (header, expected) in [
            ("Bearer s3cret", Some("s3cret")),
            // Trimmed, because a client that pads it still means the token.
            ("Bearer  s3cret ", Some("s3cret")),
            ("Bearer ", None),
            ("Bearer", None),
            // A different scheme is not ours to interpret.
            ("Basic s3cret", None),
            // Case-sensitive by the HTTP spec's own example, and by every
            // client that sends it.
            ("bearer s3cret", None),
            ("", None),
        ] {
            assert_eq!(
                super::parse_bearer(header),
                expected.map(str::to_string),
                "for {header:?}"
            );
        }
    }
}

// ============================================================================
// JWT admin auth (behind feature flag)
// ============================================================================

#[cfg(feature = "jwt")]
mod jwt_auth {
    use super::*;
    use jwt_compact::{
        alg::{Hs256, Hs256Key},
        prelude::*,
    };
    use serde::{Deserialize, Serialize};

    /// Hardcoded admin Discord user IDs.
    const ADMIN_UIDS: &[&str] = &[
        "179744071361757184", // djo
    ];

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Claims {
        sub: String,
    }

    /// Validate a JWT Bearer token and check the subject against the admin list.
    /// Returns the user ID on success.
    pub async fn require_admin(req: &Request, env: &Env) -> Result<String> {
        let uid = check_jwt_admin(req, env).await?;
        Ok(uid)
    }

    /// Internal JWT check — returns the user ID on success.
    pub(crate) async fn check_jwt_admin(req: &Request, env: &Env) -> Result<String> {
        let token_str = req
            .headers()
            .get("Authorization")
            .map_err(|e| Error::RustError(format!("{e}")))?
            .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.to_string()))
            .ok_or_else(|| Error::RustError("Missing Bearer token".into()))?;

        let secret = worker_utils::secrets::get_secret(env, "JWT_SECRET")
            .await
            .map_err(|e| Error::RustError(format!("JWT_SECRET not configured: {e:?}")))?;

        let key = Hs256Key::new(secret);
        let token = UntrustedToken::new(&token_str)
            .map_err(|e| Error::RustError(format!("Invalid token format: {e}")))?;
        let token: Token<Claims> = Hs256
            .validator(&key)
            .validate(&token)
            .map_err(|e| Error::RustError(format!("Token validation failed: {e}")))?;

        let uid = token.claims().custom.sub.clone();

        if !ADMIN_UIDS.contains(&uid.as_str()) {
            return Err(Error::RustError(format!("User {uid} is not an admin")));
        }

        tracing::debug!("JWT admin auth: user {uid} authenticated");
        Ok(uid)
    }
}

#[cfg(feature = "jwt")]
pub use jwt_auth::require_admin;

#[cfg(feature = "jwt")]
use jwt_auth::check_jwt_admin;
