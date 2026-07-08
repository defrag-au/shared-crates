//! `AccessGate` — the app-level access screen: a "sign in" prompt for
//! anonymous visitors and a "requirements" screen (what to join to gain
//! access) for signed-in-but-unqualified users. Data-only inputs so any
//! egui app can drop it in front of a gated tool; returns an action the
//! app maps to navigation / logout.
//!
//! Pairs with `authorizations::Feature` (title + hint) and a list of
//! [`GateProvider`]s (communities that grant access — usually from a
//! `/api/auth/requirements`-style endpoint).

use authorizations::Feature;
use egui::{Align, Layout, RichText, Ui};

/// A community that grants access — shown on the requirements screen.
#[derive(Clone, Debug, Default)]
pub struct GateProvider {
    pub label: String,
    /// Optional invite URL; renders a "join" button when present.
    pub invite_url: Option<String>,
    /// Human-readable prompts describing what to do to qualify (e.g. "Hold
    /// the Deckhand role — verify an OG NFT in #verify"). Rendered as
    /// indented hint lines under the community when present.
    pub requirements: Vec<String>,
}

/// Which gate screen to render.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    /// No session — show the sign-in prompt.
    Anonymous,
    /// Signed in but lacking the entitlement — show requirements.
    Unqualified,
}

/// What the user did on the gate this frame.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GateAction {
    None,
    /// Start sign-in (also used for "sign in again" to re-check).
    Login,
    /// Sign out.
    SignOut,
    /// Open a community invite URL.
    Join(String),
}

/// The access-gate screen.
pub struct AccessGate<'a> {
    feature: &'a Feature,
    tagline: &'a str,
    status: GateStatus,
    providers: &'a [GateProvider],
    providers_loading: bool,
}

impl<'a> AccessGate<'a> {
    pub fn new(feature: &'a Feature, status: GateStatus) -> Self {
        Self {
            feature,
            tagline: feature.locked_hint,
            status,
            providers: &[],
            providers_loading: false,
        }
    }

    /// Tagline under the title (defaults to the feature's `locked_hint`).
    pub fn tagline(mut self, t: &'a str) -> Self {
        self.tagline = t;
        self
    }

    /// Communities that grant access (for the requirements screen).
    pub fn providers(mut self, providers: &'a [GateProvider]) -> Self {
        self.providers = providers;
        self
    }

    /// Show a spinner instead of "no communities" while the list loads.
    pub fn loading(mut self, loading: bool) -> Self {
        self.providers_loading = loading;
        self
    }

    pub fn show(self, ui: &mut Ui) -> GateAction {
        let accent = ui.visuals().hyperlink_color;
        let weak = ui.visuals().weak_text_color();
        let mut action = GateAction::None;

        ui.add_space(48.0);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.set_max_width(520.0);
            ui.label(RichText::new(self.feature.name).color(accent).size(24.0));
            ui.add_space(6.0);

            match self.status {
                GateStatus::Anonymous => {
                    ui.label(RichText::new(self.tagline).color(weak).size(13.0));
                    ui.add_space(20.0);
                    if ui
                        .add_sized(
                            [220.0, 36.0],
                            egui::Button::new(
                                RichText::new("Sign in with Discord")
                                    .color(accent)
                                    .size(14.0),
                            ),
                        )
                        .clicked()
                    {
                        action = GateAction::Login;
                    }
                }
                GateStatus::Unqualified => {
                    ui.label(
                        RichText::new("You're signed in, but don't have access yet.").size(14.0),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(
                            "Access requires a qualifying role in one of these communities:",
                        )
                        .color(weak)
                        .size(12.0),
                    );
                    ui.add_space(10.0);

                    if self.providers.is_empty() && self.providers_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new("loading requirements…")
                                    .color(weak)
                                    .size(11.0),
                            );
                        });
                    } else if self.providers.is_empty() {
                        ui.label(
                            RichText::new("No communities are currently configured for access.")
                                .color(weak)
                                .size(12.0),
                        );
                    } else {
                        for p in self.providers {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("• {}", p.label)).strong().size(14.0),
                                );
                                if let Some(url) = &p.invite_url {
                                    if ui
                                        .button(RichText::new("join").color(accent).size(11.0))
                                        .clicked()
                                    {
                                        action = GateAction::Join(url.clone());
                                    }
                                }
                            });
                            // Human-readable "what to look for" prompts,
                            // indented under the community.
                            for req in &p.requirements {
                                ui.horizontal(|ui| {
                                    ui.add_space(14.0);
                                    ui.label(RichText::new(req).color(weak).size(12.0));
                                });
                            }
                            ui.add_space(4.0);
                        }
                    }

                    ui.add_space(20.0);
                    ui.label(
                        RichText::new(
                            "Already joined and have the role? Sign in again to refresh access.",
                        )
                        .color(weak)
                        .size(11.0),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("Sign in again").color(accent).size(12.0))
                            .clicked()
                        {
                            action = GateAction::Login;
                        }
                        if ui
                            .button(RichText::new("Sign out").color(weak).size(12.0))
                            .clicked()
                        {
                            action = GateAction::SignOut;
                        }
                    });
                }
            }
        });
        action
    }
}
