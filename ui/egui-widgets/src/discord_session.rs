//! [`AuthStrategy`] for the `discord_auth` JWT session — a self-describing
//! token that arrives in a URL fragment.
//!
//! # Which Discord this is
//!
//! The *other* one. See [`crate::auth_worker`] for the opaque-token path; the
//! two are unrelated services and the difference is not cosmetic:
//!
//! | | this (`discord_auth`) | [`crate::auth_worker`] |
//! |---|---|---|
//! | arrives as | `#session=<jwt>` fragment | `?token=<opaque>` query |
//! | restore | **synchronous** — the JWT says who it is | a `/validate` round trip |
//! | authority | `ent` claim, real entitlements | none, identity only |
//! | expiry | JWT `exp` | none; a 401 is the only signal |
//!
//! So this strategy never reports [`AuthPhase::Restoring`] for more than the
//! instant `restore()` takes, and it *does* populate
//! [`Session::expires_at`], which means the shell's expiry check is live here
//! and inert there. Two strategies, one trait, opposite properties — which is
//! the point of having built the second one before trusting the first.
//!
//! # ⚠️ This draws the "sign in" gate and NOT the "you lack access" gate
//!
//! [`crate::access_gate::AccessGate`] has two screens.
//! [`GateStatus::Anonymous`] is a sign-in prompt — *authentication*, so it is
//! this strategy's. [`GateStatus::Unqualified`] is "you're signed in but don't
//! have the entitlement, here are the communities that grant it" —
//! *authorisation*, so it stays the app's, drawn from the app's own
//! requirements data.
//!
//! Folding both in here would have put the shell in the business of deciding
//! who may see what, which is the exact conflation the `gate() -> GateStatus`
//! signature this design replaced was guilty of. An unqualified user is
//! authenticated; [`AuthPhase`] says so, and the app takes it from there.
//!
//! # Login is a worker route, not a client-side OAuth start
//!
//! `discord_auth::web::begin_login` exists and takes a client id, but the apps
//! on this path all delegate to a route on their own worker
//! (`/auth/discord/login`), which owns the client config. That is the "different
//! credentials" axis between two apps on this same strategy: a different
//! [`DiscordSessionConfig::login_url`], not different code.

//! # Why only *half* of this module is `wasm32`-gated
//!
//! `discord_auth::web::Session` is localStorage and URL-fragment machinery, so
//! anything touching it has to be. The claims mapping is not — it is the place
//! a renamed field would silently blank every user badge, and gating the whole
//! module would mean `cargo test` never ran it. So [`claims_from_parts`] takes
//! primitives and lives out here; the adapter that feeds it the session lives
//! behind the cfg. Same split as [`crate::auth_worker`], same reason.

// Off-wasm the only caller of the logic below is the test module — see above.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use authorizations::{Feature, SessionClaims};

#[cfg(target_arch = "wasm32")]
use crate::access_gate::{AccessGate, GateAction, GateStatus};
#[cfg(target_arch = "wasm32")]
use crate::auth::{AuthPhase, AuthStrategy, Session};

/// Per-app configuration — the two things that differ between properties.
#[derive(Clone, Debug)]
pub struct DiscordSessionConfig {
    /// Where to send the browser to start sign-in. A route on the app's own
    /// worker, which holds the OAuth client id and redirect URI.
    pub login_url: String,
    /// The feature this app is gated on, for the gate's heading and tagline.
    pub feature: Feature,
}

impl DiscordSessionConfig {
    pub fn new(login_url: impl Into<String>, feature: Feature) -> Self {
        Self {
            login_url: login_url.into(),
            feature,
        }
    }
}

/// The claims mapping, over primitives so it is testable off-target.
///
/// `ent` is the space-delimited scope string; the rest is the Discord identity
/// when there is one. **A session with entitlements but no identity is normal**
/// — a debug/operator token — and must still report its authority, so the
/// absent identity produces default claims rather than an anonymous session.
fn claims_from_parts(
    ent: &str,
    user_id: Option<&str>,
    name: Option<&str>,
    avatar_hash: Option<&str>,
) -> SessionClaims {
    let Some(user_id) = user_id else {
        return SessionClaims {
            ent: ent.to_string(),
            ..SessionClaims::default()
        };
    };
    let mut claims = SessionClaims::for_discord(user_id, ent);
    if let Some(name) = name {
        claims = claims.with_name(name);
    }
    claims.avatar = avatar_hash.map(str::to_string);
    claims
}

#[cfg(target_arch = "wasm32")]
pub struct DiscordSessionStrategy {
    config: DiscordSessionConfig,
    phase: AuthPhase,
    /// The underlying session, kept so `clear()` can reach its storage keys.
    inner: discord_auth::web::Session,
}

#[cfg(target_arch = "wasm32")]
impl DiscordSessionStrategy {
    pub fn new(config: DiscordSessionConfig) -> Self {
        Self {
            config,
            phase: AuthPhase::Restoring,
            inner: discord_auth::web::Session::default(),
        }
    }

    /// True only when the session arrived from a fragment on *this* page load,
    /// as opposed to being restored from storage.
    ///
    /// Surfaced because apps use it to decide whether to celebrate a login or
    /// silently continue one — `fresh_login` is a fact about the page load, not
    /// about the session, and nothing else can recover it later.
    pub fn fresh_login(&self) -> bool {
        self.inner.fresh_login
    }

    /// Where the session is. Inherent as well as on the trait so the phase is
    /// readable without importing [`AuthStrategy`].
    pub fn phase(&self) -> &AuthPhase {
        &self.phase
    }

    /// Send the browser to the login route. Does not return in practice.
    ///
    /// Public because sign-in is also reachable from the app's *authorisation*
    /// screen — "sign in again to re-check" after joining a community is the
    /// one path where an already-authenticated user starts a fresh login, and
    /// that screen is the app's, not this strategy's.
    pub fn begin_login(&self) {
        goto(&self.config.login_url);
    }

    /// Rebuild the phase from the current inner session.
    fn adopt(&mut self) {
        self.phase = match self.inner.authenticated {
            true => AuthPhase::authenticated(Session {
                claims: claims_from(&self.inner),
                credentials: self.inner.auth_header.clone(),
                expires_at: self.inner.expires_at,
            }),
            false => AuthPhase::Anonymous,
        };
    }
}

/// Adapter: pull the parts out of `discord_auth`'s session and map them.
#[cfg(target_arch = "wasm32")]
fn claims_from(session: &discord_auth::web::Session) -> SessionClaims {
    let ent = session.entitlements.to_scope_string();
    let identity = session.identity.as_ref();
    claims_from_parts(
        &ent,
        identity.map(|i| i.user_id.as_str()),
        identity.and_then(|i| i.name.as_deref()),
        identity.and_then(|i| i.avatar_hash.as_deref()),
    )
}

#[cfg(target_arch = "wasm32")]
impl AuthStrategy for DiscordSessionStrategy {
    fn restore(&mut self) {
        // Synchronous, unlike the auth-worker path: `load()` reads the fragment
        // or localStorage and the JWT describes itself. There is no window in
        // which the answer is unknown, so `Restoring` is left almost
        // immediately — but it is still entered, because an app that painted
        // between construction and `restore()` must not see `Anonymous`.
        self.inner = discord_auth::web::Session::load();
        self.adopt();
    }

    fn tick(&mut self) {}

    fn phase(&self) -> &AuthPhase {
        &self.phase
    }

    fn gate_ui(&mut self, ui: &mut egui::Ui) {
        // `Anonymous` ONLY. The `Unqualified` screen is the app's — see the
        // module header.
        let action = AccessGate::new(self.config.feature, GateStatus::Anonymous).show(ui);
        match action {
            GateAction::Login => goto(&self.config.login_url),
            GateAction::Join(url) => goto(&url),
            GateAction::SignOut => self.sign_out(),
            GateAction::None => {}
        }
    }

    fn sign_out(&mut self) {
        self.inner.clear();
        self.phase = AuthPhase::Anonymous;
    }

    // No `exchange`: the JWT already carries `ent`. This is the strategy the
    // default exists for.
}

#[cfg(target_arch = "wasm32")]
fn goto(url: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_discord_user_maps_to_full_claims() {
        let claims = claims_from_parts("app.access admin.access", Some("42"), Some("Damon"), Some("abc"));
        assert_eq!(claims.sub.as_deref(), Some("42"));
        assert_eq!(claims.name.as_deref(), Some("Damon"));
        assert_eq!(claims.avatar.as_deref(), Some("abc"));
        assert!(claims.entitlements().grants(Feature::AppAccess));
        assert!(claims.entitlements().grants(Feature::Admin));
    }

    #[test]
    fn an_operator_token_with_no_identity_still_reports_its_authority() {
        // A debug/operator session is authenticated and entitled but nameless.
        // Dropping the `ent` here would silently lock an operator out of the
        // surfaces their token exists to reach.
        let claims = claims_from_parts("*", None, None, None);
        assert_eq!(claims.sub, None);
        assert!(claims.entitlements().grants(Feature::Admin));
        assert!(claims.entitlements().grants(Feature::VisualSearch));
    }

    #[test]
    fn a_user_with_no_avatar_or_name_still_has_an_id() {
        let claims = claims_from_parts("app.access", Some("42"), None, None);
        assert_eq!(claims.sub.as_deref(), Some("42"));
        assert_eq!(claims.name, None);
        assert_eq!(claims.avatar, None);
    }

    #[test]
    fn an_empty_scope_string_grants_nothing() {
        let claims = claims_from_parts("", Some("42"), Some("Damon"), None);
        assert!(claims.entitlements().is_empty());
        assert!(!claims.entitlements().grants(Feature::AppAccess));
        // ...but the identity survives, which is what lets the app draw the
        // "signed in, not qualified" screen rather than a login prompt.
        assert_eq!(claims.sub.as_deref(), Some("42"));
    }

    #[test]
    fn the_config_carries_the_two_things_that_differ_between_properties() {
        // The falsification check for "different credentials": two apps on this
        // same strategy differ by a URL and a Feature, not by code.
        let a = DiscordSessionConfig::new("/auth/discord/login", Feature::AppAccess);
        let b = DiscordSessionConfig::new("https://other.example/login", Feature::GatewayAdmin);
        assert_ne!(a.login_url, b.login_url);
        assert_ne!(a.feature.id(), b.feature.id());
    }
}
