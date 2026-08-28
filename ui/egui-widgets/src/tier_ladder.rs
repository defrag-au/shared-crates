//! `TierLadder` — the whole access ladder as a modal: what each rung gives,
//! every way to reach it, and where the reader currently stands.
//!
//! The answer to "why can I only see 30 days?", which a tier chip alone
//! cannot give. A chip is a verdict; this is the working behind it — so it
//! shows the rung you are on, the rungs above and below, and for the next one
//! up, how far short you are on each route to it.
//!
//! **Data-only, and deliberately not tied to any tier crate.** Everything
//! arrives pre-formatted: thresholds as display strings, not integers. A
//! widget that took a `u128` would have to decide on thousands separators and
//! token decimals, and it has no business knowing either — the caller already
//! resolved them.
//!
//! **Every route is listed, not just the cheapest.** A rung reachable by two
//! different assets is the case this exists for: showing only one of them
//! tells a holder who owns the *other* asset that they do not qualify.
//!
//! ```no_run
//! # use egui_widgets::tier_ladder::{TierLadder, TierLadderAction, TierRung, TierRoute, Standing};
//! # fn demo(ui: &mut egui::Ui, open: &mut bool) {
//! let rungs = vec![
//!     TierRung::new("Free", "30 days of history").free(true).standing(Standing::Current),
//!     TierRung::new("90 days", "90 days of history").route(
//!         TierRoute::new("$Aliens", "250,000").have("120,000"),
//!     ),
//! ];
//! if TierLadder::new(&rungs).anonymous(true).show(ui, open) == TierLadderAction::SignIn {
//!     // start the wallet sign-in flow
//! }
//! # }
//! ```

use egui::{Color32, RichText, Ui};

use crate::icons::{PhosphorIcon, install_phosphor_font};
use crate::theme;

/// Where a rung sits relative to the reader.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Standing {
    /// The rung in force right now.
    Current,
    /// Below the current rung — already covered by it.
    Held,
    /// Above the current rung.
    #[default]
    Locked,
}

/// One way to reach a rung, pre-formatted for display.
#[derive(Clone, Debug)]
pub struct TierRoute<'a> {
    /// What to hold, as a holder would name it — "$Aliens", "$PERP".
    pub label: &'a str,
    /// How much, already formatted with separators.
    pub need: String,
    /// How much the reader has, when that is worth saying. `None` on rungs
    /// they already hold — telling someone their balance against a threshold
    /// they cleared is noise.
    pub have: Option<String>,
}

impl<'a> TierRoute<'a> {
    pub fn new(label: &'a str, need: impl Into<String>) -> Self {
        Self {
            label,
            need: need.into(),
            have: None,
        }
    }

    /// Show progress against this route.
    pub fn have(mut self, have: impl Into<String>) -> Self {
        self.have = Some(have.into());
        self
    }
}

/// One rung of the ladder.
#[derive(Clone, Debug)]
pub struct TierRung<'a> {
    pub label: &'a str,
    /// What reaching it buys, as a phrase — "90 days of history".
    pub gives: &'a str,
    pub routes: Vec<TierRoute<'a>>,
    pub standing: Standing,
    /// Reachable without holding anything. Called out explicitly, because
    /// "this is the free tier and you are on it" is the single most useful
    /// thing this modal can tell an anonymous reader.
    pub free: bool,
}

impl<'a> TierRung<'a> {
    pub fn new(label: &'a str, gives: &'a str) -> Self {
        Self {
            label,
            gives,
            routes: Vec::new(),
            standing: Standing::Locked,
            free: false,
        }
    }

    pub fn route(mut self, route: TierRoute<'a>) -> Self {
        self.routes.push(route);
        self
    }

    pub fn standing(mut self, standing: Standing) -> Self {
        self.standing = standing;
        self
    }

    pub fn free(mut self, free: bool) -> Self {
        self.free = free;
        self
    }
}

/// What the reader did in the modal this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TierLadderAction {
    None,
    /// Close without acting.
    Dismiss,
    /// Start the wallet sign-in flow.
    SignIn,
}

/// The ladder modal.
pub struct TierLadder<'a> {
    rungs: &'a [TierRung<'a>],
    title: &'a str,
    anonymous: bool,
    /// Copy under the title — the one-line "how this works".
    intro: Option<&'a str>,
}

impl<'a> TierLadder<'a> {
    pub fn new(rungs: &'a [TierRung<'a>]) -> Self {
        Self {
            rungs,
            title: "Access tiers",
            anonymous: false,
            intro: None,
        }
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    /// No wallet connected. Adds a sign-in call to action, since for an
    /// anonymous reader every rung above the floor is unreachable until they
    /// connect one — and saying "hold 250,000 $Aliens" without that step is
    /// an instruction they cannot follow.
    pub fn anonymous(mut self, anonymous: bool) -> Self {
        self.anonymous = anonymous;
        self
    }

    pub fn intro(mut self, intro: &'a str) -> Self {
        self.intro = Some(intro);
        self
    }

    /// Render while `open` is true. Sets `open` to false when dismissed, so
    /// the caller stores one bool and nothing else.
    pub fn show(self, ui: &mut Ui, open: &mut bool) -> TierLadderAction {
        if !*open {
            return TierLadderAction::None;
        }
        install_phosphor_font(ui.ctx());

        let mut action = TierLadderAction::None;
        let response = egui::Modal::new(egui::Id::new("tier_ladder_modal")).show(ui.ctx(), |ui| {
            ui.set_min_width(420.0);
            ui.set_max_width(520.0);

            ui.label(RichText::new(self.title).size(16.0).strong());
            if let Some(intro) = self.intro {
                ui.label(RichText::new(intro).small().color(theme::TEXT_MUTED));
            }
            ui.add_space(10.0);

            for (i, rung) in self.rungs.iter().enumerate() {
                if i > 0 {
                    ui.add_space(6.0);
                }
                rung_row(ui, rung);
            }

            ui.add_space(12.0);
            ui.separator();
            ui.horizontal(|ui| {
                if self.anonymous && ui.button("Connect wallet").clicked() {
                    action = TierLadderAction::SignIn;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        action = TierLadderAction::Dismiss;
                    }
                });
            });
        });

        // A click outside the modal is a dismissal too — the standard escape
        // hatch, and egui reports it separately from anything we drew.
        if response.should_close() && action == TierLadderAction::None {
            action = TierLadderAction::Dismiss;
        }
        if action != TierLadderAction::None {
            *open = false;
        }
        action
    }
}

/// One rung: a marker, its name, what it gives, and how to reach it.
fn rung_row(ui: &mut Ui, rung: &TierRung<'_>) {
    let current = rung.standing == Standing::Current;
    let (marker, marker_color) = match rung.standing {
        Standing::Current => (PhosphorIcon::CheckCircle, theme::ACCENT),
        Standing::Held => (PhosphorIcon::Check, theme::SUCCESS),
        Standing::Locked => (PhosphorIcon::Lock, theme::TEXT_MUTED),
    };

    // The current rung is filled rather than merely coloured. Colour alone
    // carries the "you are here" for readers who can distinguish it; a fill
    // and a word carry it for everyone else.
    let frame = egui::Frame::default()
        .fill(if current {
            theme::BG_HIGHLIGHT
        } else {
            Color32::TRANSPARENT
        })
        .inner_margin(egui::Margin::symmetric(8, 6))
        .corner_radius(6.0);

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(marker.rich_text(14.0, marker_color));
            ui.add_space(2.0);

            let name = RichText::new(rung.label).strong().color(if current {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            });
            ui.label(name);

            if rung.free {
                ui.label(
                    RichText::new("no wallet needed")
                        .small()
                        .color(theme::TEXT_MUTED),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if current {
                    ui.label(RichText::new("you are here").small().color(theme::ACCENT));
                }
            });
        });

        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.label(
                RichText::new(rung.gives)
                    .small()
                    .color(theme::TEXT_SECONDARY),
            );
        });

        for (i, route) in rung.routes.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                // "or" carries the whole meaning here. Stacked without it,
                // two routes read as two requirements — telling a holder who
                // owns either asset that they need both.
                let verb = if i == 0 { "hold" } else { "or hold" };
                ui.label(
                    RichText::new(format!("{verb} {} {}", route.need, route.label))
                        .small()
                        .color(theme::TEXT_MUTED),
                );
                // Progress is the difference between "you don't qualify" and
                // "you are most of the way there", and only the second is
                // worth acting on.
                if let Some(have) = &route.have {
                    ui.label(
                        RichText::new(format!("({have} held)"))
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_has_no_progress_until_asked() {
        let route = TierRoute::new("$Aliens", "250,000");
        assert!(route.have.is_none());
        assert_eq!(route.have("120,000").have.as_deref(), Some("120,000"));
    }

    /// Locked is the default, so a caller that forgets to mark standing shows
    /// a rung as unreachable rather than as held — the fail-closed direction.
    #[test]
    fn a_rung_defaults_to_locked() {
        assert_eq!(
            TierRung::new("6 months", "182 days").standing,
            Standing::Locked
        );
        assert_eq!(Standing::default(), Standing::Locked);
    }

    #[test]
    fn routes_accumulate_in_order() {
        let rung = TierRung::new("12 months", "365 days of history")
            .route(TierRoute::new("$Aliens", "2,000,000"))
            .route(TierRoute::new("$PERP", "500"));
        let labels: Vec<&str> = rung.routes.iter().map(|r| r.label).collect();
        assert_eq!(labels, vec!["$Aliens", "$PERP"], "every route is offered");
    }

    #[test]
    fn a_closed_ladder_renders_nothing_and_reports_nothing() {
        let ctx = egui::Context::default();
        let mut open = false;
        let mut action = TierLadderAction::SignIn;
        let _ = ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                action = TierLadder::new(&[]).show(ui, &mut open);
            });
        });
        assert_eq!(action, TierLadderAction::None);
        assert!(!open);
    }
}
