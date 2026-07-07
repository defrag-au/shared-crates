//! Entitlement model, feature registry, and session-claim verification.
//!
//! One shared vocabulary for "who may use what", consumed from three sides:
//!
//! - **Token mint** (augie / bot-jwt): maps Discord roles → entitlement ids
//!   and embeds them in an HS256 session JWT (`ent` claim, RFC 8693-style
//!   space-delimited scope string — entitlements, NOT tiers, so the
//!   role→entitlement mapping can evolve at the mint site without
//!   redeploying consumers).
//! - **Backend enforcement** (workers-rs): a route handler calls
//!   [`SessionClaims::require`] with the [`Feature`] const it protects and
//!   receives a [`Grant`] proof or a typed denial. Gated inner functions
//!   take `Grant` as a parameter — the "parse, don't validate" shape: one
//!   runtime check at the boundary, a type-system proof everywhere past it.
//! - **Frontend display** (egui): widgets query [`EntitlementSet::grants`]
//!   each frame and render locked affordances from the feature's metadata
//!   (see `egui-widgets`' `gated` helpers).
//!
//! Design notes (2026-07 research pass): the declare-next-to-the-code +
//! pluggable-extractor split follows the protect-endpoints pattern; no
//! existing crate compiles for wasm32/workers-rs, hence this local
//! implementation. Pure compile-time capability tokens can't express
//! JWT-carried per-user entitlements (no dependent types), so the boundary
//! check is necessarily runtime; `Grant` gives the compile-time
//! propagation half. Feature declaration is `macro_rules`-based — no
//! proc-macro crate — because workers-rs routes are closures, which an
//! attribute macro can't decorate anyway.

use serde::{Deserialize, Serialize};

pub mod features;

// ============================================================================
// Feature — a gated capability, declared next to the code that implements it
// ============================================================================

/// A gated feature. Declare with [`features!`]; reference the const from
/// both the enforcing route and the rendering widget so the entitlement id,
/// display name, and locked-state copy live in exactly one place.
#[derive(Debug)]
pub struct Feature {
    /// Entitlement id — an RFC 8693-style scope token (`[a-z0-9._-]+`,
    /// no spaces; spaces delimit the `ent` claim). Namespace by surface,
    /// e.g. `tools.visual-search`.
    pub id: &'static str,
    /// Human-readable name for UI ("Visual Search").
    pub name: &'static str,
    /// Locked-state copy shown to unauthorized users — what the feature is
    /// and how to gain access (e.g. "Run /collector in a partner Discord").
    pub locked_hint: &'static str,
}

/// Declare features and collect them into a registry slice.
///
/// ```
/// authorizations::features! {
///     pub const VISUAL_SEARCH = {
///         id: "tools.visual-search",
///         name: "Visual Search",
///         locked_hint: "Collector-gated: run /collector in Discord",
///     };
/// }
/// // Each const is a `Feature`; `ALL_FEATURES` lists every declaration.
/// assert_eq!(VISUAL_SEARCH.id, "tools.visual-search");
/// assert_eq!(ALL_FEATURES.len(), 1);
/// ```
#[macro_export]
macro_rules! features {
    ( $( $(#[$meta:meta])* pub const $name:ident = {
            id: $id:expr, name: $display:expr, locked_hint: $hint:expr $(,)?
        }; )+ ) => {
        $(
            $(#[$meta])*
            pub const $name: $crate::Feature = $crate::Feature {
                id: $id,
                name: $display,
                locked_hint: $hint,
            };
        )+
        /// Every feature declared in this registry block.
        pub const ALL_FEATURES: &[&$crate::Feature] = &[ $( &$name ),+ ];
    };
}

// ============================================================================
// EntitlementSet — the parsed `ent` claim
// ============================================================================

/// A set of entitlement ids, parsed from the JWT's space-delimited `ent`
/// claim. The wildcard entitlement `*` grants every feature (operator /
/// super-tier tokens).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntitlementSet {
    ids: Vec<String>,
}

impl EntitlementSet {
    /// Parse the RFC 8693-style space-delimited scope string.
    pub fn from_scope_string(s: &str) -> Self {
        Self {
            ids: s.split_whitespace().map(str::to_owned).collect(),
        }
    }

    /// Render back to the compact claim encoding.
    pub fn to_scope_string(&self) -> String {
        self.ids.join(" ")
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Whether this set grants `feature`.
    pub fn grants(&self, feature: &Feature) -> bool {
        self.ids.iter().any(|id| id == feature.id || id == "*")
    }
}

// ============================================================================
// SessionClaims — the JWT payload augie mints
// ============================================================================

/// Custom claims carried by the session JWT (standard `exp`/`iat` are
/// handled by the `jwt-compact` envelope).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionClaims {
    /// Discord user id.
    pub sub: String,
    /// Guild the session was granted through (provenance, not authority —
    /// the entitlements are the authority).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild: Option<String>,
    /// Informational tier label ("collector", "partner:xyz"). Display only;
    /// enforcement reads `ent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Space-delimited entitlement ids (RFC 8693 scope-string style).
    #[serde(default)]
    pub ent: String,
}

impl SessionClaims {
    pub fn entitlements(&self) -> EntitlementSet {
        EntitlementSet::from_scope_string(&self.ent)
    }

    /// The boundary check: exchange claims for a [`Grant`] proof, or a
    /// typed denial carrying the feature (so the error response can say
    /// what was missing and how to get it).
    pub fn require<'f>(&self, feature: &'f Feature) -> Result<Grant<'f>, Denied<'f>> {
        if self.entitlements().grants(feature) {
            Ok(Grant {
                feature,
                _proof: (),
            })
        } else {
            Err(Denied { feature })
        }
    }
}

/// Proof that the current session was checked against a feature. Only
/// mintable via [`SessionClaims::require`] (or [`Grant::for_test`] in
/// tests) — gated functions take it as a parameter so "forgot to check"
/// is a compile error, not a code-review hope.
#[derive(Debug)]
pub struct Grant<'f> {
    pub feature: &'f Feature,
    _proof: (),
}

impl<'f> Grant<'f> {
    /// Test-only constructor so gated internals stay unit-testable.
    pub fn for_test(feature: &'f Feature) -> Self {
        Self {
            feature,
            _proof: (),
        }
    }
}

/// A refused boundary check.
#[derive(Debug)]
pub struct Denied<'f> {
    pub feature: &'f Feature,
}

// ============================================================================
// Token verification (HS256, aligned with augminted-bots' bot-jwt)
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("invalid token: {0}")]
    Invalid(String),
    #[error("token expired")]
    Expired,
}

/// Verify an HS256 session token and return its claims. `secret` is the
/// raw shared key (same one the mint side uses).
pub fn verify_token(token: &str, secret: &[u8]) -> Result<SessionClaims, TokenError> {
    use jwt_compact::{alg::Hs256, alg::Hs256Key, prelude::*, AlgorithmExt};

    let key = Hs256Key::new(secret);
    let token: Token<SessionClaims> = Hs256
        .validator(&key)
        .validate(&UntrustedToken::new(token).map_err(|e| TokenError::Invalid(e.to_string()))?)
        .map_err(|e| match e {
            jwt_compact::ValidationError::Expired => TokenError::Expired,
            other => TokenError::Invalid(other.to_string()),
        })?;
    token
        .claims()
        .validate_expiration(&TimeOptions::default())
        .map_err(|e| match e {
            jwt_compact::ValidationError::Expired => TokenError::Expired,
            other => TokenError::Invalid(other.to_string()),
        })?;
    Ok(token.claims().custom.clone())
}

/// Mint a session token (used by the bot side and by tests; consumers that
/// only verify never call this).
pub fn mint_token(
    claims: SessionClaims,
    secret: &[u8],
    ttl: chrono::Duration,
) -> Result<String, TokenError> {
    use jwt_compact::{alg::Hs256, alg::Hs256Key, prelude::*, AlgorithmExt};

    let key = Hs256Key::new(secret);
    let time_options = TimeOptions::default();
    let claims = Claims::new(claims)
        .set_duration_and_issuance(&time_options, ttl)
        .set_not_before(chrono::Utc::now());
    Hs256
        .token(&Header::empty(), &claims, &key)
        .map_err(|e| TokenError::Invalid(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::features! {
        pub const TEST_FEATURE = {
            id: "test.feature",
            name: "Test Feature",
            locked_hint: "hold the test badge",
        };
        pub const OTHER_FEATURE = {
            id: "test.other",
            name: "Other",
            locked_hint: "nope",
        };
    }

    fn claims(ent: &str) -> SessionClaims {
        SessionClaims {
            sub: "user123".into(),
            guild: Some("guild1".into()),
            tier: Some("collector".into()),
            ent: ent.into(),
        }
    }

    #[test]
    fn registry_collects_features() {
        assert_eq!(ALL_FEATURES.len(), 2);
        assert_eq!(TEST_FEATURE.id, "test.feature");
    }

    #[test]
    fn scope_string_round_trips() {
        let set = EntitlementSet::from_scope_string("a.b c.d  e.f");
        assert_eq!(set.to_scope_string(), "a.b c.d e.f");
        assert!(!set.is_empty());
        assert!(EntitlementSet::from_scope_string("").is_empty());
    }

    #[test]
    fn grants_exact_and_wildcard() {
        assert!(claims("test.feature").require(&TEST_FEATURE).is_ok());
        assert!(claims("test.feature").require(&OTHER_FEATURE).is_err());
        assert!(claims("*").require(&OTHER_FEATURE).is_ok());
        assert!(claims("").require(&TEST_FEATURE).is_err());
        // Prefixes must not match — scope tokens are exact.
        assert!(claims("test.featurex").require(&TEST_FEATURE).is_err());
    }

    #[test]
    fn denial_carries_feature_metadata() {
        let err = claims("").require(&TEST_FEATURE).unwrap_err();
        assert_eq!(err.feature.id, "test.feature");
        assert_eq!(err.feature.locked_hint, "hold the test badge");
    }

    #[test]
    fn token_mint_verify_round_trip() {
        let secret = b"super-secret-key-for-tests";
        let token = mint_token(
            claims("tools.visual-search *"),
            secret,
            chrono::Duration::hours(12),
        )
        .unwrap();
        let parsed = verify_token(&token, secret).unwrap();
        assert_eq!(parsed.sub, "user123");
        assert!(parsed.entitlements().grants(&TEST_FEATURE)); // via wildcard
        assert!(verify_token(&token, b"wrong-secret").is_err());
    }
}
