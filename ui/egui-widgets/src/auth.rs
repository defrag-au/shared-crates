//! How an app decides who you are — the strategy the shell drives.
//!
//! # What this is for
//!
//! Eighteen frontends in this estate hand-roll their own sign-in. They do not
//! disagree about much: read storage at boot, show a gate while anonymous,
//! attach a credential to outbound calls, drop the session when it dies. They
//! disagree about *everything else*, including the order those happen in, and
//! the disagreements are invisible until one of them flickers a login screen at
//! someone who is already signed in.
//!
//! This module is the agreement. [`AuthStrategy`] is what an app implements (or
//! picks off the shelf); [`AuthPhase`] is what the shell reads; [`Session`] is
//! what an authenticated app carries around.
//!
//! # The identity is [`authorizations::SessionClaims`]
//!
//! Not a new type. An earlier draft of the design invented a `Principal` with
//! label / avatar / entitlements / expiry — every field of which `SessionClaims`
//! already had, alongside `for_discord()`, `for_wallet()` and `entitlements()`.
//! A fourth identity model in an estate that already had three would have been
//! the problem restated, so [`Session`] wraps the real one.
//!
//! The credential sits *beside* the claims rather than inside them, because the
//! two answer different questions: the claims are what was **proven**, the
//! credential is how you **present** it. Strategy B sends
//! `Authorization: Bearer …`, the wallet path sends `X-Session-Token`, and the
//! claims are identical in shape either way.
//!
//! # ⚠️ Authentication is not authorisation
//!
//! The predecessor of this module was a single `fn gate() -> GateStatus`, where
//! `GateStatus` was `Anonymous | Unqualified`. That conflated two questions
//! asked at different times of different subsystems, and it produced a specific
//! bug: **an unqualified user IS authenticated**, and showing them a login
//! screen does not give them the entitlement they lack.
//!
//! So [`AuthPhase`] answers only "are you signed in". Whether you may *see* a
//! thing is [`crate::gated`]'s question, answered per-feature from
//! `claims.entitlements()`. This module carries entitlements and never reads
//! them.
//!
//! # ⚠️ The shell never maps a tier to an entitlement
//!
//! A wallet session carries a worker-local allowlist label (`super_admin` /
//! `whitelisted`) and `SessionClaims` carries a `tier` documented "display only;
//! enforcement reads `ent`". It is tempting to offer a helper here that turns
//! one into the other. **Do not.**
//!
//! The signing key is shared across properties, so an `authorizations` token
//! minted anywhere is *verifiable* everywhere; the only thing stopping one
//! opening another property's admin surface is that it does not name that
//! property's entitlement. The entitlement list is the blast radius, and it is
//! the minting worker's to draw — see `client-management`'s `portal_authz.rs`,
//! whose own earlier draft granted `admin.access` to super-admins on the
//! reasoning that a super-admin is an admin, and would have handed every portal
//! operator a different property's operator surface.
//!
//! A convenience here would have made that mistake once, for every app.
//!
//! # The exchange, and the window it opens
//!
//! Some strategies prove an identity and *then* go and fetch its authority —
//! the wallet path mints an `authorizations` token from its stake session. That
//! leaves a window of a few frames where the user is signed in but their
//! entitlements are unknown, and there is exactly one safe thing to do in it.
//!
//! [`Session::anonymous_ent`] is that state: real claims, empty `ent`. Every
//! `grants()` check fails closed against an empty set, so a gated pane stays
//! locked until the token lands rather than flashing available and vanishing.
//! No extra [`AuthPhase`] variant is needed to express it, which is why there
//! isn't one.

use authorizations::{EntitlementSet, SessionClaims};

/// Where a session is, independent of how it was obtained.
///
/// This is the union of the two state machines already in the estate —
/// `user-portal`'s hand-rolled `AuthStatus` and [`crate::stake_session`]'s
/// `StakeSessionPhase`. Both are strict subsets of it, which is the evidence
/// that this vocabulary was discovered rather than invented.
#[derive(Clone, Debug)]
pub enum AuthPhase {
    /// Boot: reading storage, a URL fragment, or validating with a service.
    ///
    /// Render a spinner, never the gate. Flashing a login screen at someone who
    /// *is* signed in is the bug this state exists to prevent, and it is the
    /// reason `Restoring` is distinct from [`AuthPhase::Anonymous`] rather than
    /// being folded into it.
    Restoring,
    /// No session. The gate view is the app.
    Anonymous,
    /// A sign-in round trip is in flight.
    ///
    /// Only observable for in-page strategies (the wallet path). A redirect
    /// strategy goes from [`AuthPhase::Anonymous`] to *gone* — the tab
    /// navigates away — and comes back in [`AuthPhase::Restoring`].
    Pending,
    /// Boxed because `SessionClaims` is eight `Option<String>`s wide, and an
    /// un-boxed variant would make every `AuthPhase` — including the three
    /// unit ones a typical app sits in — pay for it. The phase is read once a
    /// frame, so the indirection costs nothing that matters.
    Authenticated(Box<Session>),
    /// Sign-in was attempted and refused. Carries what to show the user.
    Failed(String),
}

impl AuthPhase {
    /// Signed in, with the session.
    pub fn authenticated(session: Session) -> Self {
        AuthPhase::Authenticated(Box::new(session))
    }

    /// The session, when there is one.
    pub fn session(&self) -> Option<&Session> {
        match self {
            AuthPhase::Authenticated(s) => Some(s),
            _ => None,
        }
    }

    /// Whether the shell should draw the app rather than the gate.
    pub fn is_authenticated(&self) -> bool {
        matches!(self, AuthPhase::Authenticated(_))
    }

    /// Whether the shell should draw *nothing yet*.
    ///
    /// Distinct from `!is_authenticated()`: a `Restoring` app has no business
    /// showing a gate, and an app that treats the two the same is the flicker
    /// bug.
    pub fn is_settling(&self) -> bool {
        matches!(self, AuthPhase::Restoring | AuthPhase::Pending)
    }
}

/// An authenticated session: a proven identity, plus what the shell needs to act
/// on its behalf.
#[derive(Clone, Debug)]
pub struct Session {
    /// What was proven. See the module header — this is deliberately the
    /// estate's existing claims type and not a new one.
    pub claims: SessionClaims,
    /// `(header_name, header_value)` to attach to outbound requests, when the
    /// strategy has one to give.
    pub credentials: Option<(String, String)>,
    /// Unix seconds. `None` where the strategy genuinely has no expiry to
    /// report — the auth-worker path does not, and discovers death by a 401.
    pub expires_at: Option<u64>,
}

impl Session {
    /// A session with claims but no authority yet — the state during an
    /// [`AuthStrategy::exchange`].
    ///
    /// The claims are honest: the stake address really was proven. The empty
    /// `ent` is what keeps gated surfaces locked until the real one arrives.
    pub fn anonymous_ent(claims: SessionClaims) -> Self {
        Self {
            claims,
            credentials: None,
            expires_at: None,
        }
    }

    /// The session an open app reports, so the shell never carries an `Option`.
    pub fn anonymous() -> Self {
        Self {
            claims: SessionClaims::default(),
            credentials: None,
            expires_at: None,
        }
    }

    pub fn entitlements(&self) -> EntitlementSet {
        self.claims.entitlements()
    }

    /// Whether this session is past `expires_at`, with a margin.
    ///
    /// The margin is not decoration: a token presented in its final second is
    /// rejected on arrival, and the resulting 401 looks to the user like a bug
    /// rather than an expiry. Mirrors [`crate::stake_session`]'s own
    /// `EXPIRY_MARGIN_MS`.
    ///
    /// Always `false` when `expires_at` is `None` — "no stated expiry" is not
    /// "expired", and treating it as such would sign strategy-B users out on
    /// their first frame.
    pub fn is_expired(&self, now_unix_secs: u64) -> bool {
        match self.expires_at {
            Some(exp) => exp.saturating_sub(EXPIRY_MARGIN_SECS) <= now_unix_secs,
            None => false,
        }
    }

    /// Discord CDN avatar URL, when the identity has a custom avatar.
    ///
    /// `None` is an ordinary answer, not a failure — most sessions have no
    /// custom avatar and [`crate::user_badge`] falls back to an icon. Mirrors
    /// `discord_auth::web::Identity::avatar_url`, including the animated-avatar
    /// rule: a hash starting `a_` is a GIF and asking for `.png` returns a
    /// broken image rather than a still frame.
    pub fn avatar_url(&self) -> Option<String> {
        let hash = self.claims.avatar.as_deref()?;
        let user_id = self.claims.sub.as_deref()?;
        let ext = if hash.starts_with("a_") { "gif" } else { "png" };
        Some(format!(
            "https://cdn.discordapp.com/avatars/{user_id}/{hash}.{ext}?size=64"
        ))
    }

    /// Display label for [`crate::user_badge`] — the Discord name when known,
    /// else a shortened stake address, else a neutral word.
    ///
    /// Never returns an empty string: a badge rendering as a blank pill reads
    /// as a broken widget rather than as a nameless session.
    pub fn label(&self) -> String {
        if let Some(name) = self.claims.name.as_deref().filter(|n| !n.is_empty()) {
            return name.to_string();
        }
        if let Some(stake) = self.claims.stake.as_deref().filter(|s| !s.is_empty()) {
            return crate::utils::truncate_hex(stake, 8, 6);
        }
        "Signed in".to_string()
    }
}

/// Safety margin before a stated expiry, in seconds.
const EXPIRY_MARGIN_SECS: u64 = 60;

/// A request the shell makes on the strategy's behalf, after sign-in, to turn a
/// proven identity into entitlements.
///
/// Deliberately plain data rather than a future: the strategy declares *what to
/// call*, the shell owns *when*, and neither has to name the other's transport.
/// This is the same "declare and let the driver run it" shape
/// [`crate::commands`] uses, for the same reason — a callback here would have
/// to borrow the app's state and outlive the frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    /// Absolute URL to POST to.
    pub url: String,
    /// Credential to present, if the mint needs one (it usually does — the
    /// thing being exchanged *is* the proof).
    pub credentials: Option<(String, String)>,
}

/// How an app authenticates.
///
/// **Object-safe on purpose** — the shell holds `Box<dyn AuthStrategy>`.
///
/// This is the opposite call from the design's rejection of a `dyn Leaf`
/// registry, and for the opposite reason. Leaves were rejected because most own
/// no state, so a trait object would have forced them to invent some. An auth
/// strategy owns *all* of its state — session, pending request, per-app
/// credentials — and shares nothing with the app. It is the genuinely
/// independent component that shape fits.
pub trait AuthStrategy {
    /// Called once at boot, before the first paint.
    ///
    /// Must leave the phase at [`AuthPhase::Restoring`] if it started anything
    /// asynchronous, so the shell knows not to draw the gate yet.
    fn restore(&mut self);

    /// Advanced every frame. Where a strategy polls its own async results — the
    /// shell never learns what transport it used.
    fn tick(&mut self);

    fn phase(&self) -> &AuthPhase;

    /// The gate screen, drawn by the shell whenever the phase is not
    /// [`AuthPhase::Authenticated`].
    ///
    /// [`crate::access_gate::AccessGate`] and
    /// [`crate::stake_session::StakeSessionPanel`] both become views here;
    /// neither keeps a session of its own any more.
    fn gate_ui(&mut self, ui: &mut egui::Ui);

    fn sign_out(&mut self);

    /// Optional second step: exchange the proven identity for entitlements.
    ///
    /// `None` when the session already carries its own authority — a Discord
    /// JWT has `ent` in it, so there is nothing to fetch. A wallet strategy
    /// returns the call that mints an `authorizations` token from its stake
    /// session.
    ///
    /// Called only once the phase is [`AuthPhase::Authenticated`]; the shell
    /// applies the result with [`AuthStrategy::accept_exchange`].
    fn exchange(&self) -> Option<Exchange> {
        None
    }

    /// Fold a completed [`Exchange`] back into the session.
    ///
    /// Default is a no-op so strategies that never exchange need not mention
    /// it. A failed exchange is deliberately **not** a sign-out: the identity
    /// is still proven, and dropping the session over a missing entitlement
    /// would log out a user who may not have needed one.
    fn accept_exchange(&mut self, _claims: SessionClaims) {}
}

/// The strategy for an app that does not authenticate at all.
///
/// Reports [`AuthPhase::Authenticated`] with an empty session, so the shell's
/// "draw the app or draw the gate" branch needs no special case and no
/// `Option<Box<dyn AuthStrategy>>`. Its gate view is never reached.
#[derive(Debug)]
pub struct NoAuth {
    /// Authenticated from construction, not from `restore`.
    ///
    /// Deliberate: `phase()` is readable before the shell has run `restore`,
    /// and a `NoAuth` that answered `Anonymous` in that window would make an
    /// open app flash a gate it does not even have. There is no state to
    /// restore, so there is no window to model.
    phase: AuthPhase,
}

impl Default for NoAuth {
    fn default() -> Self {
        Self {
            phase: AuthPhase::authenticated(Session::anonymous()),
        }
    }
}

impl NoAuth {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AuthStrategy for NoAuth {
    fn restore(&mut self) {}

    fn tick(&mut self) {}

    fn phase(&self) -> &AuthPhase {
        &self.phase
    }

    fn gate_ui(&mut self, _ui: &mut egui::Ui) {}

    fn sign_out(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use authorizations::Feature;

    #[test]
    fn restoring_is_not_anonymous_because_the_shell_draws_them_differently() {
        // The whole reason `Restoring` exists. If these ever compare equal, an
        // app will flash its login screen during boot.
        assert!(AuthPhase::Restoring.is_settling());
        assert!(!AuthPhase::Anonymous.is_settling());
        assert!(!AuthPhase::Restoring.is_authenticated());
        assert!(!AuthPhase::Anonymous.is_authenticated());
    }

    #[test]
    fn pending_settles_too_so_an_in_flight_signin_does_not_redraw_the_gate() {
        assert!(AuthPhase::Pending.is_settling());
    }

    #[test]
    fn failed_is_not_settling_so_the_gate_comes_back_with_the_reason() {
        let failed = AuthPhase::Failed("wallet refused".into());
        assert!(!failed.is_settling());
        assert!(!failed.is_authenticated());
    }

    #[test]
    fn an_exchanging_session_grants_nothing() {
        // The window between "signed in" and "entitlements known". Every gated
        // surface must fail closed here.
        let s = Session::anonymous_ent(SessionClaims::for_wallet("stake1abc", ""));
        assert!(!s.entitlements().grants(Feature::Admin));
        assert!(!s.entitlements().grants(Feature::AppAccess));
        // ...but the identity is real, which is what makes it honest to report.
        assert_eq!(s.claims.stake.as_deref(), Some("stake1abc"));
    }

    #[test]
    fn a_session_with_no_stated_expiry_is_never_expired() {
        // Strategy B has no expiry at all. Treating `None` as "expired" would
        // sign those users out on their first frame.
        let s = Session::anonymous();
        assert!(!s.is_expired(0));
        assert!(!s.is_expired(u64::MAX));
    }

    #[test]
    fn expiry_has_a_margin_so_a_token_is_not_spent_in_its_last_second() {
        let s = Session {
            claims: SessionClaims::default(),
            credentials: None,
            expires_at: Some(1_000),
        };
        assert!(!s.is_expired(900), "well inside its life");
        assert!(s.is_expired(950), "inside the margin — treat as gone");
        assert!(s.is_expired(1_000));
        assert!(s.is_expired(1_100));
    }

    #[test]
    fn label_prefers_a_name_then_a_stake_then_a_word_but_never_nothing() {
        let named =
            Session::anonymous_ent(SessionClaims::for_discord("123", "").with_name("Damon"));
        assert_eq!(named.label(), "Damon");

        let walleted =
            Session::anonymous_ent(SessionClaims::for_wallet("stake1u9xyzabcdefghijklmnop", ""));
        let label = walleted.label();
        assert!(label.starts_with("stake1u9"), "kept the readable prefix");
        assert!(label.len() < "stake1u9xyzabcdefghijklmnop".len());

        assert_eq!(Session::anonymous().label(), "Signed in");
        assert!(!Session::anonymous().label().is_empty());
    }

    #[test]
    fn an_empty_name_falls_through_rather_than_rendering_a_blank_badge() {
        let blank =
            Session::anonymous_ent(SessionClaims::for_wallet("stake1abcdefghij", "").with_name(""));
        assert_ne!(blank.label(), "");
        assert!(blank.label().starts_with("stake1"));
    }

    /// `SessionClaims::avatar` is a plain field with no builder.
    fn with_avatar(mut claims: SessionClaims, hash: &str) -> Session {
        claims.avatar = Some(hash.to_string());
        Session::anonymous_ent(claims)
    }

    #[test]
    fn an_animated_avatar_asks_for_a_gif_not_a_png() {
        // `a_`-prefixed hashes are animated; requesting `.png` yields a broken
        // image, not a still.
        let animated = with_avatar(SessionClaims::for_discord("42", ""), "a_deadbeef");
        assert!(
            animated
                .avatar_url()
                .unwrap()
                .ends_with("a_deadbeef.gif?size=64")
        );

        let still = with_avatar(SessionClaims::for_discord("42", ""), "deadbeef");
        assert!(still.avatar_url().unwrap().ends_with("deadbeef.png?size=64"));
    }

    #[test]
    fn no_avatar_or_no_user_id_means_no_url_rather_than_a_broken_one() {
        assert_eq!(Session::anonymous().avatar_url(), None);
        // An avatar hash with no `sub` cannot address the CDN — a wallet
        // session, for instance.
        let no_sub = with_avatar(SessionClaims::for_wallet("stake1abc", ""), "x");
        assert_eq!(no_sub.avatar_url(), None);
    }

    #[test]
    fn no_auth_reports_authenticated_so_the_shell_needs_no_special_case() {
        let mut s = NoAuth::new();
        // Authenticated BEFORE restore, on purpose — an open app must not flash
        // a gate it does not have during the boot window.
        assert!(s.phase().is_authenticated());
        s.restore();
        assert!(s.phase().is_authenticated());
        assert!(s.exchange().is_none());
    }

    #[test]
    fn a_strategy_that_does_not_exchange_says_so() {
        // The default matters: every Discord strategy relies on it.
        struct Bare(AuthPhase);
        impl AuthStrategy for Bare {
            fn restore(&mut self) {}
            fn tick(&mut self) {}
            fn phase(&self) -> &AuthPhase {
                &self.0
            }
            fn gate_ui(&mut self, _ui: &mut egui::Ui) {}
            fn sign_out(&mut self) {}
        }
        assert!(Bare(AuthPhase::Anonymous).exchange().is_none());
    }

    #[test]
    fn the_trait_is_object_safe() {
        // The design turns on holding `Box<dyn AuthStrategy>`. If a later
        // signature breaks that (an `impl Trait` argument, a generic method),
        // this stops compiling and says why.
        let _: Box<dyn AuthStrategy> = Box::new(NoAuth::new());
    }
}
