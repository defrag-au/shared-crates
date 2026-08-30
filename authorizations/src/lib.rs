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
///
/// **Two ways in, one token.** Discord OAuth (`discord-auth`) and CIP-8
/// wallet auth (the client portal) both mint this, and they know different
/// things about the holder: one has a Discord user id and no wallet, the
/// other a stake address and a client id and possibly no Discord link at
/// all. Every identity field is therefore optional, with the invariant that
/// **at least one is present** — enforced at the mint sites by the
/// [`SessionClaims::for_discord`] / [`SessionClaims::for_wallet`]
/// constructors, and checkable with [`SessionClaims::has_identity`].
///
/// Making `sub` optional rather than overloading it (`discord:123` vs
/// `stake1…`) is deliberate: a prefix scheme silently changes the meaning of
/// a field every existing consumer already reads, whereas an `Option` makes
/// each of them a compile error that has to be answered.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionClaims {
    /// Discord user id, **when known**. Absent for a wallet-authed session
    /// whose holder has never linked Discord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    /// Client id, for a session minted by the client portal. Absent for
    /// Discord-authed sessions, which have no client context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// The bech32 stake address this session authenticated as (`stake1…` /
    /// `stake_test1…`). Absent for Discord-authed sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stake: Option<String>,
    /// Guild the session was granted through (provenance, not authority —
    /// the entitlements are the authority).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild: Option<String>,
    /// Informational tier label ("collector", "partner:xyz"). Display only;
    /// enforcement reads `ent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Display name (Discord global name or username). Display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Discord avatar hash (build a CDN URL with `sub` + this). Display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// Space-delimited entitlement ids (RFC 8693 scope-string style).
    #[serde(default)]
    pub ent: String,
}

impl SessionClaims {
    /// A Discord-authed session (the `discord-auth` OAuth callback).
    pub fn for_discord(discord_user_id: impl Into<String>, ent: impl Into<String>) -> Self {
        Self {
            sub: Some(discord_user_id.into()),
            ent: ent.into(),
            ..Self::default()
        }
    }

    /// A wallet-authed session (the client portal, after CIP-8 verify).
    ///
    /// The stake address is the identity that was actually proven; `client`
    /// and `sub` are resolutions of it — the client it owns, and the Discord
    /// user who linked it, when either is known. Attach them with
    /// [`with_client`](Self::with_client) / [`with_discord`](Self::with_discord).
    pub fn for_wallet(stake_address: impl Into<String>, ent: impl Into<String>) -> Self {
        Self {
            stake: Some(stake_address.into()),
            ent: ent.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_client(mut self, client_id: impl Into<String>) -> Self {
        self.client = Some(client_id.into());
        self
    }

    /// Attach the Discord identity resolved from the stake address
    /// (`social:stake:{addr}` → `StakeOwner`). Never self-asserted.
    #[must_use]
    pub fn with_discord(mut self, discord_user_id: impl Into<String>) -> Self {
        self.sub = Some(discord_user_id.into());
        self
    }

    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The invariant: a token naming nobody authenticates nobody. Verifiers
    /// reject a token that fails this — see [`verify_token`].
    pub fn has_identity(&self) -> bool {
        self.sub.is_some() || self.client.is_some() || self.stake.is_some()
    }

    /// Who to write into an audit trail, best available.
    ///
    /// Degrades through the identities rather than inventing a name: display
    /// name + Discord id, then the bare Discord id, then the client id, then
    /// the stake address. The last case is the honest one for a wallet-authed
    /// operator who has never linked Discord — "who did this" is answerable,
    /// just not by a name.
    pub fn actor(&self) -> String {
        match (self.name.as_deref(), self.sub.as_deref()) {
            (Some(name), Some(id)) => format!("{name} ({id})"),
            (Some(name), None) => name.to_string(),
            (None, Some(id)) => id.to_string(),
            (None, None) => match (self.client.as_deref(), self.stake.as_deref()) {
                (Some(client), _) => format!("client {client}"),
                (None, Some(stake)) => stake.to_string(),
                // Unreachable for a verified token — `verify_token` rejects
                // an identity-less one — but this is an audit field, and a
                // panic here would be a worse answer than a legible marker.
                (None, None) => "unidentified".to_string(),
            },
        }
    }

    pub fn entitlements(&self) -> EntitlementSet {
        EntitlementSet::from_scope_string(&self.ent)
    }

    /// The boundary check: exchange claims for a [`Grant`] proof, or a
    /// typed denial carrying the feature (so the error response can say
    /// what was missing and how to get it).
    pub fn require<'f>(&self, feature: &'f Feature) -> Result<Grant<'f>, Denied<'f>> {
        if self.entitlements().grants(feature) {
            Ok(Grant { feature })
        } else {
            Err(Denied { feature })
        }
    }
}

/// Proof that the current session was checked against a feature. Only
/// mintable via [`SessionClaims::require`] (or [`Grant::for_test`] in
/// tests) — gated functions take it as a parameter so "forgot to check"
/// is a compile error, not a code-review hope.
///
/// `#[non_exhaustive]` is what makes it unforgeable: outside this crate a
/// `Grant` can't be built with a struct literal, so the only way to hold one
/// is to have gone through [`SessionClaims::require`] (or [`Grant::for_test`]).
#[derive(Debug)]
#[non_exhaustive]
pub struct Grant<'f> {
    pub feature: &'f Feature,
}

impl<'f> Grant<'f> {
    /// Test-only constructor so gated internals stay unit-testable.
    pub fn for_test(feature: &'f Feature) -> Self {
        Self { feature }
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
    /// The token verified but named nobody — no `sub`, `client` or `stake`.
    /// Checked at both mint and verify: an unsigned-for identity is the one
    /// way a correctly-signed token could still authenticate no one.
    #[error("token carries no identity")]
    NoIdentity,
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
    let claims = token.claims().custom.clone();
    // The identity invariant is enforced HERE, not left to each consumer:
    // every field is optional on the wire, so an all-absent token is
    // syntactically valid and would otherwise sail through to a caller that
    // reads whichever field it happens to care about and finds `None`.
    if !claims.has_identity() {
        return Err(TokenError::NoIdentity);
    }
    Ok(claims)
}

/// Mint a session token (used by the bot side and by tests; consumers that
/// only verify never call this).
pub fn mint_token(
    claims: SessionClaims,
    secret: &[u8],
    ttl: chrono::Duration,
) -> Result<String, TokenError> {
    use jwt_compact::{alg::Hs256, alg::Hs256Key, prelude::*, AlgorithmExt};

    if !claims.has_identity() {
        return Err(TokenError::NoIdentity);
    }
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
            guild: Some("guild1".into()),
            tier: Some("collector".into()),
            ..SessionClaims::for_discord("user123", ent).with_name("tester")
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
        assert_eq!(parsed.sub.as_deref(), Some("user123"));
        assert!(parsed.entitlements().grants(&TEST_FEATURE)); // via wildcard
        assert!(verify_token(&token, b"wrong-secret").is_err());
    }

    /// A wallet-authed session carries no Discord id and must still be a
    /// valid, round-trippable token — this is the whole point of `sub`
    /// becoming optional.
    #[test]
    fn a_wallet_session_needs_no_discord_id() {
        let secret = b"super-secret-key-for-tests";
        let claims = SessionClaims::for_wallet("stake_test1abc", "gateway.admin")
            .with_client("client_42");
        let token = mint_token(claims, secret, chrono::Duration::hours(8)).unwrap();

        let parsed = verify_token(&token, secret).unwrap();
        assert_eq!(parsed.sub, None);
        assert_eq!(parsed.stake.as_deref(), Some("stake_test1abc"));
        assert_eq!(parsed.client.as_deref(), Some("client_42"));
    }

    /// The invariant is enforced at BOTH ends. A correctly-signed token that
    /// names nobody is the one way a valid signature could authenticate no
    /// one, so neither side is allowed to be the only check.
    #[test]
    fn a_token_naming_nobody_is_refused_at_both_ends() {
        let secret = b"super-secret-key-for-tests";
        let anonymous = SessionClaims {
            ent: "*".into(),
            ..SessionClaims::default()
        };
        assert!(matches!(
            mint_token(anonymous.clone(), secret, chrono::Duration::hours(1)),
            Err(TokenError::NoIdentity)
        ));

        // Mint it anyway, the way a future/foreign issuer might, and confirm
        // the verifier refuses it rather than trusting the mint site.
        let forged = {
            use jwt_compact::{alg::Hs256, alg::Hs256Key, prelude::*, AlgorithmExt};
            let key = Hs256Key::new(secret);
            let claims = Claims::new(anonymous)
                .set_duration_and_issuance(&TimeOptions::default(), chrono::Duration::hours(1));
            Hs256.token(&Header::empty(), &claims, &key).unwrap()
        };
        assert!(matches!(
            verify_token(&forged, secret),
            Err(TokenError::NoIdentity)
        ));
    }

    /// The audit actor degrades through the identities rather than inventing
    /// a name — the gateway's change log is the consumer that made this
    /// necessary.
    #[test]
    fn the_actor_degrades_to_whatever_identity_exists() {
        assert_eq!(claims("").actor(), "tester (user123)");
        assert_eq!(SessionClaims::for_discord("user123", "").actor(), "user123");
        assert_eq!(
            SessionClaims::for_wallet("stake_test1abc", "")
                .with_client("client_42")
                .actor(),
            "client client_42"
        );
        assert_eq!(
            SessionClaims::for_wallet("stake_test1abc", "").actor(),
            "stake_test1abc"
        );
    }
}
