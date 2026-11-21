//! Centralised request authentication for Cloudflare Workers.
//!
//! Supports token-based auth (debug token, internal API key) and optionally
//! JWT-based admin auth behind the `jwt` feature flag.
//!
//! Failures are typed: [`AuthError::Unauthorized`] is a rejected credential,
//! while [`AuthError::Misconfigured`] means the worker's own binding is missing
//! or empty and no credential could be checked. [`AuthError::into_response`]
//! turns those into 401 and 503 respectively — the rule
//! `worker_utils::internal_key` learned the hard way, kept here so a deployment
//! fault is never reported as the caller's fault.
//!
//! Sibling [`allowlist`] module handles stake-address allowlisting for the
//! platform admin path; the CIP-8 signature check itself lives in `wallet-auth`.

pub mod allowlist;

use std::fmt;

use worker_stack::worker::{Env, Request, Response, Result as WorkerResult};

// ============================================================================
// Failures
// ============================================================================

/// Why an authentication attempt failed.
///
/// The split matters at the HTTP boundary. [`Self::Unauthorized`] is the
/// caller's problem and maps to 401; [`Self::Misconfigured`] is ours and maps
/// to 503, because a missing binding must not read as a rejected credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// The request did not present a credential that authenticates.
    Unauthorized,
    /// The worker cannot check the credential because its own binding is
    /// missing or empty. Names the secret.
    Misconfigured(&'static str),
}

impl AuthError {
    /// The response a handler should return for this failure.
    ///
    /// 401 for a rejected credential, 503 for a worker that cannot
    /// authenticate anyone until the named binding is set.
    pub fn into_response(self) -> WorkerResult<Response> {
        match self {
            AuthError::Unauthorized => Response::error("unauthorized", 401),
            AuthError::Misconfigured(name) => {
                tracing::error!("{name} is not configured — refusing authenticated traffic");
                Response::error("service unavailable", 503)
            }
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Unauthorized => f.write_str("unauthorized"),
            AuthError::Misconfigured(name) => write!(f, "{name} is not configured"),
        }
    }
}

impl std::error::Error for AuthError {}

// ============================================================================
// Auth key types
// ============================================================================

/// Which authentication method(s) to accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
#[non_exhaustive]
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
    /// seeds. The debug token is deliberately excluded; a JWT admin is
    /// included even though it is not worker-to-worker.
    pub fn is_full_trust(self) -> bool {
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
/// secret store. On failure, [`AuthError::into_response`] maps a rejected
/// credential to 401 and an unconfigured binding to 503.
pub async fn require(req: &Request, env: &Env, key_type: AuthKeyType) -> Result<(), AuthError> {
    authenticate(req, env, key_type).await.map(|_| ())
}

/// Like [`require`], but returns WHICH principal authenticated so callers can
/// tier capabilities (read for [`Caller::Debug`], spend/seed for
/// [`Caller::is_full_trust`]). For `Any`, the internal key is tried first so a
/// caller presenting it is classified as the higher-trust principal.
///
/// A credential whose secret is unset or empty is reported as
/// [`AuthError::Misconfigured`] rather than [`AuthError::Unauthorized`].
pub async fn authenticate(
    req: &Request,
    env: &Env,
    key_type: AuthKeyType,
) -> Result<Caller, AuthError> {
    match key_type {
        AuthKeyType::DebugToken => check_debug_token(req, env).await.map(|()| Caller::Debug),
        AuthKeyType::InternalKey => check_internal_key(req, env)
            .await
            .map(|()| Caller::Internal),
        AuthKeyType::Any => {
            // A presented credential with an unset secret outranks a plain
            // rejection: the deployment is broken, and a 401 would hide it.
            let mut misconfigured = None;

            for (header, secret, caller) in [
                ("X-Internal-Key", "INTERNAL_API_KEY", Caller::Internal),
                ("X-Debug-Token", "DEBUG_TOKEN", Caller::Debug),
            ] {
                match check_header(req, env, header, secret).await {
                    Ok(()) => return Ok(caller),
                    Err(AuthError::Misconfigured(name)) => {
                        if misconfigured.is_none() {
                            misconfigured = Some(name);
                        }
                    }
                    Err(AuthError::Unauthorized) => {}
                }
            }

            #[cfg(feature = "jwt")]
            match check_jwt_admin(req, env).await {
                Ok(_) => return Ok(Caller::JwtAdmin),
                Err(AuthError::Misconfigured(name)) => {
                    if misconfigured.is_none() {
                        misconfigured = Some(name);
                    }
                }
                Err(AuthError::Unauthorized) => {}
            }

            Err(misconfigured.map_or(AuthError::Unauthorized, AuthError::Misconfigured))
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
/// as [`Caller::Internal`] whichever header carried it. A bearer token does not
/// say which secret it is, so both are compared regardless of `key_type`.
///
/// A bearer that matched neither secret is a rejection
/// ([`AuthError::Unauthorized`]); only when neither secret could be read is it
/// [`AuthError::Misconfigured`].
pub async fn authenticate_with_bearer(
    req: &Request,
    env: &Env,
    key_type: AuthKeyType,
) -> Result<Caller, AuthError> {
    let header_error = match authenticate(req, env, key_type).await {
        Ok(caller) => return Ok(caller),
        Err(err) => err,
    };

    let Some(token) = bearer_token(req) else {
        return Err(header_error);
    };

    let mut compared = false;
    let mut unconfigured = None;

    for (secret, caller) in [
        ("INTERNAL_API_KEY", Caller::Internal),
        ("DEBUG_TOKEN", Caller::Debug),
    ] {
        let expected = match worker_utils::secrets::get_secret(env, secret).await {
            Ok(expected) => expected,
            Err(_) => {
                if unconfigured.is_none() {
                    unconfigured = Some(secret);
                }
                continue;
            }
        };
        let Ok(expected) = require_configured(&expected, secret) else {
            if unconfigured.is_none() {
                unconfigured = Some(secret);
            }
            continue;
        };

        compared = true;
        // The same constant-time compare the header path uses. A second
        // implementation of this is a second chance to get it wrong.
        if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
            return Ok(caller);
        }
    }

    if compared {
        return Err(AuthError::Unauthorized);
    }
    Err(unconfigured.map_or(AuthError::Unauthorized, AuthError::Misconfigured))
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

/// Convenience macro for a handler that only needs the token check.
///
/// Expands to an early return of [`AuthError::into_response`], so the enclosing
/// function must return `worker::Result<Response>`:
///
/// ```no_run
/// use worker_stack::worker::{Env, Request, Response, Result};
///
/// async fn handler(req: Request, env: Env) -> Result<Response> {
///     api_auth::require_auth!(req, env, InternalKey);
///     Response::error("ok", 200)
/// }
/// ```
#[macro_export]
macro_rules! require_auth {
    ($req:expr, $env:expr, $key_type:ident) => {
        if let Err(err) = $crate::require(&$req, &$env, $crate::AuthKeyType::$key_type).await {
            return err.into_response();
        }
    };
}

// ============================================================================
// Token checks
// ============================================================================

/// Check `X-Debug-Token` header against `DEBUG_TOKEN` secret.
async fn check_debug_token(req: &Request, env: &Env) -> Result<(), AuthError> {
    check_header(req, env, "X-Debug-Token", "DEBUG_TOKEN").await
}

/// Check `X-Internal-Key` header against `INTERNAL_API_KEY` secret.
async fn check_internal_key(req: &Request, env: &Env) -> Result<(), AuthError> {
    check_header(req, env, "X-Internal-Key", "INTERNAL_API_KEY").await
}

/// Generic header-vs-secret check.
///
/// A missing header is a rejected credential. A header presented for a secret
/// that is unset or empty is a deployment fault, reported as
/// [`AuthError::Misconfigured`].
async fn check_header(
    req: &Request,
    env: &Env,
    header: &'static str,
    secret_name: &'static str,
) -> Result<(), AuthError> {
    let Some(provided) = req.headers().get(header).ok().flatten() else {
        return Err(AuthError::Unauthorized);
    };

    let expected = worker_utils::secrets::get_secret(env, secret_name)
        .await
        .map_err(|_| AuthError::Misconfigured(secret_name))?;
    let expected = require_configured(&expected, secret_name)?;

    if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        return Err(AuthError::Unauthorized);
    }

    Ok(())
}

/// Refuse an empty secret value.
///
/// An empty expected value trivially matches an empty credential, so a binding
/// that exists but is blank would open the door rather than close it. This is
/// the guard `worker_utils::internal_key` makes; the canonical gate keeps it.
fn require_configured<'a>(
    expected: &'a str,
    secret_name: &'static str,
) -> Result<&'a str, AuthError> {
    if expected.is_empty() {
        Err(AuthError::Misconfigured(secret_name))
    } else {
        Ok(expected)
    }
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
    use super::*;

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

    #[test]
    fn an_empty_secret_is_a_deployment_fault() {
        assert_eq!(
            require_configured("", "DEBUG_TOKEN"),
            Err(AuthError::Misconfigured("DEBUG_TOKEN"))
        );
        assert_eq!(require_configured("s3cret", "DEBUG_TOKEN"), Ok("s3cret"));
    }

    /// Compiles the macro where it belongs — inside a handler — so the shape it
    /// expands to cannot rot behind an ignored doctest.
    #[allow(dead_code)]
    async fn handler_using_require_auth(req: Request, env: Env) -> WorkerResult<Response> {
        crate::require_auth!(req, env, InternalKey);
        Response::error("ok", 200)
    }
}

// ============================================================================
// JWT admin auth (behind feature flag)
// ============================================================================

#[cfg(feature = "jwt")]
mod jwt_auth {
    use std::collections::HashSet;

    use jwt_compact::{
        alg::{Hs256, Hs256Key},
        prelude::*,
    };
    use serde::{Deserialize, Serialize};

    use super::*;

    /// Admin Discord user IDs, comma-delimited, from a worker secret.
    const ADMIN_DISCORD_IDS: &str = "ADMIN_DISCORD_IDS";

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Claims {
        sub: String,
    }

    /// Validate a JWT Bearer token and check the subject against the admin list.
    /// Returns the user ID on success.
    ///
    /// Requires `JWT_SECRET` and `ADMIN_DISCORD_IDS`; a token must carry `exp`,
    /// and `nbf` is enforced when present.
    pub async fn require_admin(req: &Request, env: &Env) -> Result<String, AuthError> {
        check_jwt_admin(req, env).await
    }

    /// Internal JWT check — returns the user ID on success.
    pub(crate) async fn check_jwt_admin(req: &Request, env: &Env) -> Result<String, AuthError> {
        let token = bearer_token(req).ok_or(AuthError::Unauthorized)?;

        let secret = worker_utils::secrets::get_secret(env, "JWT_SECRET")
            .await
            .map_err(|_| AuthError::Misconfigured("JWT_SECRET"))?;
        let secret = require_configured(&secret, "JWT_SECRET")?;

        let ids = worker_utils::secrets::get_secret(env, ADMIN_DISCORD_IDS)
            .await
            .map_err(|_| AuthError::Misconfigured(ADMIN_DISCORD_IDS))?;
        let admin_ids = allowlist::parse_csv(&ids);
        if admin_ids.is_empty() {
            return Err(AuthError::Misconfigured(ADMIN_DISCORD_IDS));
        }

        validate_admin_token(&token, secret, &admin_ids)
    }

    /// Verify the signature, then the expiry and subject.
    ///
    /// `jwt-compact`'s signature validation does not check temporal claims, so
    /// an admin token without `exp` would otherwise never expire.
    fn validate_admin_token(
        token_str: &str,
        secret: &str,
        admin_ids: &HashSet<String>,
    ) -> Result<String, AuthError> {
        let token = verified_token(token_str, secret)?;
        let claims = token.claims();

        claims
            .validate_expiration(&TimeOptions::default())
            .map_err(|_| AuthError::Unauthorized)?;
        if claims.not_before.is_some() {
            claims
                .validate_maturity(&TimeOptions::default())
                .map_err(|_| AuthError::Unauthorized)?;
        }

        let uid = claims.custom.sub.clone();
        if !admin_ids.contains(&uid) {
            return Err(AuthError::Unauthorized);
        }

        tracing::debug!("JWT admin auth: user {uid} authenticated");
        Ok(uid)
    }

    /// Signature and algorithm check only, split out so the fixtures below can
    /// prove they are otherwise valid.
    fn verified_token(token_str: &str, secret: &str) -> Result<Token<Claims>, AuthError> {
        let key = Hs256Key::new(secret);
        let untrusted = UntrustedToken::new(token_str).map_err(|_| AuthError::Unauthorized)?;
        Hs256
            .validator(&key)
            .validate(&untrusted)
            .map_err(|_| AuthError::Unauthorized)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const SECRET: &str = "test-jwt-secret";

        // HS256 tokens signed with SECRET; the claims are shown beside each.
        const LIVE_ADMIN: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0LWFkbWluIiwiZXhwIjo0MTAyNDQ0ODAwfQ.yWIz36D3ydGt0uCkIvnlGralsJ2kamr8Ih3EEWfzhZY";
        const EXPIRED_ADMIN: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0LWFkbWluIiwiZXhwIjoxNjAwMDAwMDAwfQ.0rvSsHQLyauLnQBgx2pkCxo4fLYhWqnFd5OKXrsHoeM";
        const ETERNAL_ADMIN: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0LWFkbWluIn0.IDNpWGOkfSby8tATghPZFWGH1yNvR4PkEWiGI3jyauY";
        const LIVE_NON_ADMIN: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJzb21lb25lLWVsc2UiLCJleHAiOjQxMDI0NDQ4MDB9.1e9Umk4AqfK1wpp8mgg6EGIQbAHBEWKHB-13kC12Fto";

        fn admin_ids() -> HashSet<String> {
            HashSet::from(["test-admin".to_string()])
        }

        /// The negative cases below reject for the reason under test only if
        /// each fixture's signature is valid and the token parses.
        #[test]
        fn the_fixtures_are_correctly_signed() {
            for token in [LIVE_ADMIN, EXPIRED_ADMIN, ETERNAL_ADMIN, LIVE_NON_ADMIN] {
                assert!(verified_token(token, SECRET).is_ok());
            }
        }

        #[test]
        fn accepts_a_live_token_for_an_admin() {
            assert_eq!(
                validate_admin_token(LIVE_ADMIN, SECRET, &admin_ids()),
                Ok("test-admin".to_string())
            );
        }

        #[test]
        fn rejects_an_expired_token() {
            assert_eq!(
                validate_admin_token(EXPIRED_ADMIN, SECRET, &admin_ids()),
                Err(AuthError::Unauthorized)
            );
        }

        #[test]
        fn rejects_a_token_without_an_expiry() {
            assert_eq!(
                validate_admin_token(ETERNAL_ADMIN, SECRET, &admin_ids()),
                Err(AuthError::Unauthorized)
            );
        }

        #[test]
        fn rejects_a_live_token_for_a_non_admin() {
            assert_eq!(
                validate_admin_token(LIVE_NON_ADMIN, SECRET, &admin_ids()),
                Err(AuthError::Unauthorized)
            );
        }

        #[test]
        fn rejects_a_token_signed_with_another_secret() {
            assert_eq!(
                validate_admin_token(LIVE_ADMIN, "another-secret", &admin_ids()),
                Err(AuthError::Unauthorized)
            );
        }
    }
}

#[cfg(feature = "jwt")]
pub use jwt_auth::require_admin;

#[cfg(feature = "jwt")]
use jwt_auth::check_jwt_admin;
