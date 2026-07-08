//! Entitlement-gated rendering — the frontend half of the
//! `authorizations` framework.
//!
//! Immediate mode means there is no component tree to hide: every frame
//! re-decides. These helpers make that decision in ONE place per feature,
//! driven by the same [`authorizations::Feature`] const the backend route
//! enforces with, so the entitlement id, display name, and locked-state
//! copy can never drift between enforcement and display.
//!
//! ```no_run
//! # use egui_widgets::gated::{gated, GateState, LockedStyle};
//! # use authorizations::features::VISUAL_SEARCH;
//! # fn demo(ui: &mut egui::Ui, gate: &GateState) {
//! gated(ui, gate, &VISUAL_SEARCH, LockedStyle::Card, |ui| {
//!     ui.label("secret collector tooling");
//! });
//! # }
//! ```

use authorizations::{EntitlementSet, Feature};
use egui::{Color32, RichText, Ui};

use crate::icons::{install_phosphor_font, PhosphorIcon};

/// The session's entitlement state as the frontend knows it. Kept as its
/// own type (rather than a bare `Option<EntitlementSet>`) so apps can store
/// it in their state struct and widgets can distinguish "anonymous" from
/// "authenticated but not entitled" for better locked-state copy.
#[derive(Clone, Debug, Default)]
pub enum GateState {
    /// No session at all (never logged in / token expired).
    #[default]
    Anonymous,
    /// A verified session with its entitlements.
    Session(EntitlementSet),
}

impl GateState {
    /// Parse from the JWT's `ent` claim string (frontend decodes the JWT
    /// payload it received from the bot link — verification happens
    /// server-side; the frontend only needs the display decision).
    pub fn from_scope_string(ent: &str) -> Self {
        Self::Session(EntitlementSet::from_scope_string(ent))
    }

    pub fn grants(&self, feature: &Feature) -> bool {
        match self {
            Self::Anonymous => false,
            Self::Session(set) => set.grants(feature),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Session(_))
    }
}

/// How an unauthorized feature presents itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockedStyle {
    /// Render nothing at all (feature invisible to non-holders).
    Hidden,
    /// Render a compact locked card: lock icon, feature name, hint copy.
    /// This is the default posture — the visible lock IS the pitch for
    /// qualifying.
    Card,
    /// Render just a small inline lock chip (for toolbars/menu rows).
    Chip,
}

/// Render `content` when the gate grants `feature`; otherwise render the
/// locked affordance per `style`. Returns `true` when content rendered.
pub fn gated(
    ui: &mut Ui,
    gate: &GateState,
    feature: &Feature,
    style: LockedStyle,
    content: impl FnOnce(&mut Ui),
) -> bool {
    if gate.grants(feature) {
        content(ui);
        return true;
    }
    match style {
        LockedStyle::Hidden => {}
        LockedStyle::Card => locked_card(ui, gate, feature),
        LockedStyle::Chip => {
            locked_chip(ui, feature);
        }
    }
    false
}

/// The locked-card affordance: lock icon + feature name + how-to-unlock
/// copy from the feature registry (plus a "session expired?" nudge when
/// the user is authenticated but lacks the entitlement).
pub fn locked_card(ui: &mut Ui, gate: &GateState, feature: &Feature) {
    install_phosphor_font(ui.ctx());
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(PhosphorIcon::Lock.rich_text(18.0, Color32::from_rgb(224, 175, 104)));
                ui.vertical(|ui| {
                    ui.label(RichText::new(feature.name).strong());
                    ui.label(
                        RichText::new(feature.locked_hint)
                            .size(11.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    if gate.is_authenticated() {
                        ui.label(
                            RichText::new("Your current session doesn't include this entitlement.")
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                });
            });
        });
}

/// Small inline lock chip for menu rows / toolbars. Returns the response
/// so callers can attach tooltips or clicks (e.g. open an "how to unlock"
/// modal).
pub fn locked_chip(ui: &mut Ui, feature: &Feature) -> egui::Response {
    install_phosphor_font(ui.ctx());
    // A button label is single-font, so the Phosphor glyph and the latin
    // name can't share one string — compose a chip-shaped frame instead.
    let weak = ui.visuals().weak_text_color();
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(PhosphorIcon::Lock.rich_text(12.0, weak));
                ui.label(RichText::new(feature.name).size(12.0).color(weak));
            });
        })
        .response
        .on_hover_text(feature.locked_hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    authorizations::features! {
        pub const GATED_TEST = {
            id: "test.gated",
            name: "Gated Test",
            locked_hint: "hold the badge",
        };
    }

    #[test]
    fn gate_state_decisions() {
        assert!(!GateState::Anonymous.grants(&GATED_TEST));
        assert!(!GateState::Anonymous.is_authenticated());
        let session = GateState::from_scope_string("test.gated other.thing");
        assert!(session.grants(&GATED_TEST));
        assert!(session.is_authenticated());
        let wrong = GateState::from_scope_string("other.thing");
        assert!(!wrong.grants(&GATED_TEST));
        assert!(wrong.is_authenticated());
    }
}
