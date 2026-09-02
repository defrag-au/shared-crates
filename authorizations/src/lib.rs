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
//!   [`SessionClaims::require`] with the [`Feature`] variant it protects and
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
//! propagation half.
//!
//! Features were originally macro-declared consts, on the reasoning that a
//! consuming crate should be able to declare its own next to the code it
//! gates. Nothing ever did — see [`Feature`] — so they are now one closed
//! enum, which an exhaustive `match` can be checked against.

use serde::{Deserialize, Serialize};

// ============================================================================
// Feature — the closed set of gated capabilities
// ============================================================================

/// A gated capability. Reference the same variant from the enforcing route
/// and the rendering widget so the entitlement id, display name and
/// locked-state copy can never drift apart.
///
/// # Why an enum and not a registry of consts
///
/// This was six `pub const Feature` values produced by a `features!` macro,
/// justified by wanting features declared next to the code implementing them
/// — an open set any crate could extend. In practice every one of them lived
/// in the single registry module and the only out-of-tree declaration was a
/// test fixture, so the extensibility was being paid for and not used.
///
/// Closed buys back what it costs: [`Self::ALL`] is the variants rather than
/// a slice a declaration can forget to join, `from_id` round-trips the wire
/// encoding, and a `match` over features is exhaustive — so adding one is a
/// compile error at every site that has to answer for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// Base entitlement — access to the tool at all. Gates the whole app;
    /// every qualifying partner role grants it.
    AppAccess,
    /// Perceptual-hash reverse image search over indexed collections.
    VisualSearch,
    /// Market scenario modelling — re-price a collection's listing book
    /// against a hypothetical floor to surface what becomes under-priced if
    /// the floor moves.
    ///
    /// **Deliberately granted to nobody yet.** The capability is built and
    /// enforced, but which holding earns it is an open decision, so today only
    /// wildcard (`*`) operator tokens see it. Naming the entitlement now means
    /// turning it on later is a gate-config change, not a code change — and
    /// nothing ships to general users in the meantime.
    MarketScenarios,
    /// Operator control surface — add/edit/delete tracked collections,
    /// trigger syncs, and the visual-analysis tooling. Granted to specific
    /// Discord accounts via the gate config; the operator `X-Debug-Token`
    /// bypasses it for shell/CLI ops.
    Admin,
    /// Augminted gateway-listener admin surface — live editing of chat
    /// trigger wiring (which utterances invoke which bot commands, with
    /// which reactions) per guild. Granted via a Gateway Admin role in
    /// HODLCroft; enforced by the augminted-bots gateway worker.
    GatewayAdmin,
    /// Platform-operator authority over the gateway: setting a guild's agent
    /// **entitlement** — who may ask, and how much.
    ///
    /// Deliberately separate from [`Self::GatewayAdmin`], which is now the
    /// *client* tier: their server, their trigger wiring. The entitlement is
    /// what we sold them, so a paying guild's own admins must not be able to
    /// grant themselves more of it.
    ///
    /// This split exists because the gate used to be the SCREEN — the whole
    /// admin app was operator-only, so hiding controls was sufficient. Once
    /// the surface is a pane in a client-facing portal that stops being true,
    /// and "only operators can see this section" is a much weaker guarantee
    /// than "the DO refuses it". See `ADMIN_SURFACE_CONSOLIDATION_DESIGN.md`
    /// §6.
    GatewayOperator,
    /// Authoring the shared meme-template library — uploading templates and
    /// placing the text boxes on them. Write access only: rendering a meme
    /// from an existing template is not gated by this.
    MemesAdmin,
}

impl Feature {
    /// Every feature, for a UI that enumerates them.
    pub const ALL: &'static [Self] = &[
        Self::AppAccess,
        Self::VisualSearch,
        Self::MarketScenarios,
        Self::Admin,
        Self::GatewayAdmin,
        Self::GatewayOperator,
        Self::MemesAdmin,
    ];

    /// Entitlement id — an RFC 8693-style scope token (`[a-z0-9._-]+`, no
    /// spaces; spaces delimit the `ent` claim). Namespaced by surface.
    ///
    /// **This is the wire format.** It appears in minted tokens and in the
    /// checked-in `discord-auth` gate config, so changing one silently
    /// revokes access for everyone already holding it.
    pub fn id(self) -> &'static str {
        match self {
            Self::AppAccess => "app.access",
            Self::VisualSearch => "tools.visual-search",
            Self::MarketScenarios => "tools.market-scenarios",
            Self::Admin => "admin.access",
            Self::GatewayAdmin => "gateway.admin",
            Self::GatewayOperator => "gateway.operator",
            Self::MemesAdmin => "memes.admin",
        }
    }

    /// Human-readable name for UI.
    pub fn name(self) -> &'static str {
        match self {
            Self::AppAccess => "Collection Explorer",
            Self::VisualSearch => "Visual Search",
            Self::MarketScenarios => "Market Scenarios",
            Self::Admin => "Admin",
            Self::GatewayAdmin => "Gateway Admin",
            Self::GatewayOperator => "Gateway Operator",
            Self::MemesAdmin => "Meme Templates",
        }
    }

    /// Locked-state copy shown to unauthorized users — what the feature is
    /// and how to gain access.
    pub fn locked_hint(self) -> &'static str {
        match self {
            Self::AppAccess => {
                "Access is granted through partner communities — hold a qualifying role to unlock"
            }
            Self::VisualSearch => "Hold a qualifying role in a partner Discord — sign in to unlock",
            Self::MarketScenarios => "Advanced market tooling — access tier not yet assigned",
            Self::Admin => "Operator access — granted to specific Discord accounts",
            Self::GatewayAdmin => {
                "Gateway operator access — hold the Gateway Admin role in HODLCroft"
            }
            Self::GatewayOperator => {
                "Platform-operator access — agent entitlements are set by Augminted"
            }
            Self::MemesAdmin => "Meme template authoring — granted to specific Discord accounts",
        }
    }

    /// Resolve a wire id back to a feature.
    ///
    /// `None` for an id this build does not know, which is the ordinary case
    /// for a token minted by a newer deployment — the entitlement is simply
    /// not honoured rather than mistaken for another.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|f| f.id() == id)
    }
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
    pub fn grants(&self, feature: Feature) -> bool {
        self.ids.iter().any(|id| id == feature.id() || id == "*")
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
    pub fn require(&self, feature: Feature) -> Result<Grant, Denied> {
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
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Grant {
    pub feature: Feature,
}

impl Grant {
    /// Test-only constructor so gated internals stay unit-testable.
    pub fn for_test(feature: Feature) -> Self {
        Self { feature }
    }
}

/// A refused boundary check.
#[derive(Debug, Clone, Copy)]
pub struct Denied {
    pub feature: Feature,
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

    fn claims(ent: &str) -> SessionClaims {
        SessionClaims {
            guild: Some("guild1".into()),
            tier: Some("collector".into()),
            ..SessionClaims::for_discord("user123", ent).with_name("tester")
        }
    }

    /// Every variant is reachable from `ALL`, which the const-registry shape
    /// could not guarantee: a declaration could be written and simply left out
    /// of the slice. `from_id` closes the loop the other way.
    #[test]
    fn every_feature_is_listed_and_round_trips_through_its_id() {
        assert_eq!(Feature::ALL.len(), 7);
        for feature in Feature::ALL {
            assert_eq!(Feature::from_id(feature.id()), Some(*feature));
            assert!(!feature.name().is_empty());
            assert!(!feature.locked_hint().is_empty());
        }
    }

    /// Ids are the wire format — they appear in minted tokens and in the
    /// checked-in gate config — so a duplicate would silently grant two
    /// capabilities for the price of one.
    #[test]
    fn feature_ids_are_unique() {
        let mut ids: Vec<&str> = Feature::ALL.iter().map(|f| f.id()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate feature id");
    }

    #[test]
    fn an_unknown_id_resolves_to_nothing_rather_than_a_neighbour() {
        assert_eq!(Feature::from_id("nope.missing"), None);
        assert_eq!(Feature::from_id(""), None);
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
        assert!(claims("tools.visual-search")
            .require(Feature::VisualSearch)
            .is_ok());
        assert!(claims("tools.visual-search")
            .require(Feature::MarketScenarios)
            .is_err());
        assert!(claims("*").require(Feature::MarketScenarios).is_ok());
        assert!(claims("").require(Feature::VisualSearch).is_err());
        // Prefixes must not match — scope tokens are exact.
        assert!(claims("tools.visual-searchx")
            .require(Feature::VisualSearch)
            .is_err());
    }

    #[test]
    fn denial_carries_feature_metadata() {
        let err = claims("").require(Feature::VisualSearch).unwrap_err();
        assert_eq!(err.feature.id(), "tools.visual-search");
        assert!(err.feature.locked_hint().contains("qualifying role"));
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
        assert!(parsed.entitlements().grants(Feature::VisualSearch)); // via wildcard
        assert!(verify_token(&token, b"wrong-secret").is_err());
    }

    /// A wallet-authed session carries no Discord id and must still be a
    /// valid, round-trippable token — this is the whole point of `sub`
    /// becoming optional.
    #[test]
    fn a_wallet_session_needs_no_discord_id() {
        let secret = b"super-secret-key-for-tests";
        let claims =
            SessionClaims::for_wallet("stake_test1abc", "gateway.admin").with_client("client_42");
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
