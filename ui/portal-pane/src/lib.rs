//! The contract a capability pane implements to be hosted by the client
//! portal shell.
//!
//! See `augminted-bots/docs/PORTAL_PANE_CONTRACT_DESIGN.md`. The short
//! version, because the shape below is otherwise surprising:
//!
//! ## Panes are statically linked, not wasm artifacts
//!
//! egui renders through `&mut Ui` — a mutable borrow of deeply pointer-ful
//! state — and Rust has no stable ABI across wasm module boundaries, so a
//! `Ui` cannot cross one. Serialising a render tree between modules instead
//! would reinvent the browser inside wasm and throw away the immediate-mode
//! cheapness that makes egui worth using.
//!
//! So "plugin" here means **a contract, a permission and a flag**, not a
//! separately-shipped binary. What that buys is independent *backend*
//! deploy, which is most of what is wanted; what it costs is that adding a
//! pane rebuilds the shell. For a handful of panes that are all ours, that
//! is a fair trade.
//!
//! ## Why this crate lives in shared-crates
//!
//! The shell is in cnft.dev-workers and the backends are at home (the
//! gateway pane's `GatewayDO` stays in augminted-bots). Neither app repo
//! depends on the other; they meet here. The wire and the widget are
//! shared, the service stays home — the same topology `augie-plugin`
//! already uses.
//!
//! **Mechanism public, policy private.** This repo is public. A trait saying
//! "a pane declares the feature it needs" is fine here; tier values, real
//! role ids, stake addresses, guild snowflakes and endpoint hostnames belong
//! in config and D1 — never here, including in tests.

use authorizations::{Feature, SessionClaims};
use egui::Ui;

/// One capability surface in the portal shell.
///
/// **The protocol triple does not appear in this trait.** Each pane owns a
/// `State`/`Delta`/`Action` ui-flow contract with its backend, but associated
/// types would make `Box<dyn Pane>` impossible and the shell must hold a
/// heterogeneous list. So a pane keeps its protocol private and the shell
/// only ever asks it to render.
pub trait Pane {
    /// The entitlement this pane needs — the **same** [`Feature`] variant its
    /// backend enforces with, so what a reader is shown and what they are
    /// allowed cannot drift.
    fn feature(&self) -> Feature;

    /// Label and icon for the shell's nav.
    fn nav(&self) -> PaneNav;

    /// Render one frame. Only called while this pane is selected.
    fn ui(&mut self, ui: &mut Ui, ctx: &PaneContext<'_>);

    /// Called every frame regardless of selection, so a pane can service its
    /// connection while the reader is looking at another one.
    ///
    /// **The pane owns its connection, not the shell.** Each backend is a
    /// different worker or DO with its own lifetime, reconnect behaviour and
    /// snapshot cadence; a shell-owned pool would have to model all of them.
    /// Without this hook, switching panes would mean resubscribing and
    /// re-snapshotting every time.
    ///
    /// Default: nothing — correct for a pane that only reads on demand.
    fn tick(&mut self, _ctx: &PaneContext<'_>) {}
}

/// How a pane presents itself in the shell's nav.
#[derive(Debug, Clone)]
pub struct PaneNav {
    /// Stable identifier — also the deep-link segment, so `/claim-guild`
    /// handoffs can arrive pointing at a specific destination.
    pub id: &'static str,
    /// What the nav entry reads.
    pub label: &'static str,
    /// Optional leading glyph. A `PhosphorIcon::…​.as_str()` — never a bare
    /// literal, since most decorative codepoints aren't in the font stack and
    /// render as tofu with no error anywhere.
    pub icon: Option<&'static str>,
}

/// What the shell hands a pane on every call.
///
/// **The shell owns the session; a pane never authenticates.** One sign-in,
/// many panes — which is the whole reason the portal is becoming a shell
/// rather than each admin surface keeping its own login.
pub struct PaneContext<'a> {
    /// The credential a pane presents to ITS OWN backend — an
    /// `authorizations` JWT minted by the shell after wallet auth.
    ///
    /// A bearer string rather than a parsed session because that is what
    /// crosses the wire: a pane puts it in a header, or (for a WebSocket,
    /// where a browser cannot set headers) in the query string.
    ///
    /// Short-lived, and **re-read every frame rather than cached** by a pane:
    /// the shell re-mints in the background, and a pane holding a copy from
    /// connect time would reconnect with an expired one.
    pub token: &'a str,

    /// The claims inside that token, already parsed.
    ///
    /// For rendering decisions only — `gated`, and showing who is signed in.
    /// The verification that counts happens at the pane's backend, which has
    /// the key. Nothing a pane decides from these is a control.
    pub claims: &'a SessionClaims,

    /// Which deployment this is, so a pane resolves its own backend URL
    /// rather than being handed one. The pane knows its service; the shell
    /// knows the environment.
    pub env: PaneEnvironment,
}

impl PaneContext<'_> {
    /// Entitlement state for `egui_widgets::gated` — built from the same
    /// claims, so a locked pane and a locked control inside it agree.
    pub fn gate(&self) -> egui_widgets::gated::GateState {
        egui_widgets::gated::GateState::Session(self.claims.entitlements())
    }
}

/// Which deployment the shell is running as.
///
/// Deliberately not a URL map: a pane resolves its own service address from
/// this, so adding a pane never means teaching the shell about another
/// hostname.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneEnvironment {
    Local,
    Dev,
    Prod,
}

/// Whether a pane is offered at all, before entitlement is even asked.
///
/// **Separate from the entitlement, and that separation is the point.** Two
/// questions that look alike:
///
/// - *Entitlement* ([`Feature`]) — is this reader allowed? Per-user, enforced
///   by the backend.
/// - *Flag* (this) — is this pane finished? Per-deployment or per-user, and
///   product-facing.
///
/// Custody is the first flag: hidden at launch, on for particular users. The
/// user is permitted; the surface is simply not being offered yet. Collapsing
/// the two would make "not ready" and "not allowed" the same state, and they
/// need different copy and different futures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneFlag {
    /// Offered to everyone this deployment serves.
    Enabled,
    /// Built, but not being offered yet. Absent from the nav entirely —
    /// distinct from [`PaneVisibility::Locked`], which says "you may not".
    Hidden,
}

/// What the shell should do with a pane this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneVisibility {
    /// In the nav, selectable.
    Available,
    /// In the nav, greyed, with the feature's own `locked_hint` as the
    /// explanation. **Never the control** — a pane hidden or locked in the
    /// nav must still be refused by its backend. This decides what a reader
    /// *sees*; the backend decides what they *may do*, and only that counts.
    Locked,
    /// Not in the nav at all.
    Absent,
}

/// Resolve a pane's nav treatment from its flag and the session's claims.
///
/// One function so every pane is treated alike, and so the flag is checked
/// **before** the entitlement: an unfinished pane should not advertise itself
/// to an entitled reader as something they merely cannot reach.
pub fn visibility(flag: PaneFlag, feature: Feature, claims: &SessionClaims) -> PaneVisibility {
    match flag {
        PaneFlag::Hidden => PaneVisibility::Absent,
        PaneFlag::Enabled if claims.require(feature).is_ok() => PaneVisibility::Available,
        PaneFlag::Enabled => PaneVisibility::Locked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Any real feature — the registry is a closed enum, so a test cannot
    /// declare a private one. What is under test is the flag/entitlement
    /// interaction, which is indifferent to which feature it is.
    const DEMO: Feature = Feature::GatewayAdmin;

    fn claims(ent: &str) -> SessionClaims {
        SessionClaims::for_wallet("stake_test1demo", ent)
    }

    /// The flag is asked first. An unfinished pane is absent even for a
    /// reader who would be entitled to it — otherwise "coming soon" and "you
    /// can't have this" become the same message.
    #[test]
    fn a_hidden_pane_is_absent_even_when_entitled() {
        assert_eq!(
            visibility(PaneFlag::Hidden, DEMO, &claims("gateway.admin")),
            PaneVisibility::Absent
        );
    }

    #[test]
    fn an_enabled_pane_locks_rather_than_vanishing() {
        assert_eq!(
            visibility(PaneFlag::Enabled, DEMO, &claims("")),
            PaneVisibility::Locked
        );
        assert_eq!(
            visibility(PaneFlag::Enabled, DEMO, &claims("gateway.admin")),
            PaneVisibility::Available
        );
    }

    /// A wallet-authed session with no Discord link is the ordinary case for
    /// this shell, and must gate exactly like any other.
    #[test]
    fn a_wallet_session_gates_normally() {
        let wallet_only = claims("gateway.admin");
        assert_eq!(wallet_only.sub, None);
        assert_eq!(
            visibility(PaneFlag::Enabled, DEMO, &wallet_only),
            PaneVisibility::Available
        );
    }
}
