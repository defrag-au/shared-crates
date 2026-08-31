//! `agent_config` story — the bring-your-own-key setup a server owner meets.
//!
//! Three widgets that have to read as one screen: pick a provider, give it a
//! key, decide who may spend it. The states worth looking at are the ones
//! that are awkward to reach live — a key that was set but has never
//! answered, a key the provider just rejected, a roster that hasn't loaded so
//! tiers show snowflakes instead of names.
//!
//! The credential is **write-only** everywhere, and this story is where that
//! is easiest to check: there is no fixture anywhere below holding a real
//! key, because the widget has no way to render one.

use egui_widgets::agent_config::{
    agent_config_section, AgentConfigResponse, CredentialDraft, ProviderDraft,
};
use egui_widgets::theme;
use gateway_wiring::{AgentEntitlement, AgentTier, GuildRole, ProviderStatus};

const NOW_MS: f64 = 1_780_000_000_000.0;

/// Which credential state to render.
#[derive(Clone, Copy, PartialEq)]
pub enum KeyState {
    /// Day one: nothing configured, the form is the whole screen.
    Unset,
    /// Set and working.
    Healthy,
    /// Set, but the provider rejected the last attempt — the admin sees the
    /// real error, members get a canned line.
    Failing,
    /// Set and never used. Ordinary for a fresh key; a symptom for an old
    /// one, and the widget cannot tell which, so it just says so.
    NeverUsed,
}

#[derive(Clone, Copy, PartialEq)]
pub enum HostedIn {
    Bare,
    PortalShell,
}

pub struct AgentConfigStory {
    pub key_state: KeyState,
    pub hosted_in: HostedIn,
    pub roles_loaded: bool,
    pub provider: ProviderDraft,
    pub credential: CredentialDraft,
    pub agent: Option<AgentEntitlement>,
    pub last: String,
}

impl Default for AgentConfigStory {
    fn default() -> Self {
        Self {
            key_state: KeyState::Healthy,
            hosted_in: HostedIn::Bare,
            roles_loaded: true,
            provider: ProviderDraft {
                base_url: "https://api.x.ai/v1".into(),
                model: "grok-4.3".into(),
            },
            credential: CredentialDraft::default(),
            agent: Some(AgentEntitlement {
                tiers: vec![
                    AgentTier {
                        role: "1291038099278790738".into(),
                        daily_tokens: 500_000,
                    },
                    AgentTier {
                        role: "1291038099278790999".into(),
                        daily_tokens: 100_000,
                    },
                ],
                default_daily_tokens: 0,
            }),
            last: String::new(),
        }
    }
}

fn roles() -> Vec<GuildRole> {
    vec![
        GuildRole {
            id: "1291038099278790738".into(),
            name: "crew".into(),
            color: 0x5865f2,
        },
        GuildRole {
            id: "1291038099278790999".into(),
            name: "deckhand".into(),
            color: 0x57f287,
        },
    ]
}

/// Note what is NOT here: a key. `masked_key` is what the server sends, and
/// it is all a renderer ever receives.
fn status(state: KeyState, provider: &ProviderDraft) -> Option<ProviderStatus> {
    let base = ProviderStatus {
        base_url: provider.base_url.clone(),
        model: provider.model.clone(),
        masked_key: "sk-…4f2c".into(),
        set_at_ms: NOW_MS - 3.0 * 86_400_000.0,
        last_used_ms: Some(NOW_MS - 2.0 * 3_600_000.0),
        last_error: None,
    };
    match state {
        KeyState::Unset => None,
        KeyState::Healthy => Some(base),
        KeyState::NeverUsed => Some(ProviderStatus {
            last_used_ms: None,
            set_at_ms: NOW_MS - 40.0 * 86_400_000.0,
            ..base
        }),
        KeyState::Failing => Some(ProviderStatus {
            last_error: Some("401 Unauthorized — incorrect API key provided".into()),
            ..base
        }),
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut AgentConfigStory) {
    ui.label(
        egui::RichText::new("Agent Config (BYO key)")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Provider, credential and per-role daily budgets. The key is write-only — \
             there is no fixture here holding one, because the widget cannot render one.",
        )
        .small()
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(8.0);

    ui.horizontal_wrapped(|ui| {
        ui.label("key:");
        for (label, value) in [
            ("unset", KeyState::Unset),
            ("healthy", KeyState::Healthy),
            ("failing", KeyState::Failing),
            ("never used", KeyState::NeverUsed),
        ] {
            if ui
                .selectable_label(state.key_state == value, label)
                .clicked()
            {
                state.key_state = value;
                state.credential.clear();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("hosted in:");
        ui.selectable_value(&mut state.hosted_in, HostedIn::Bare, "bare Ui");
        ui.selectable_value(
            &mut state.hosted_in,
            HostedIn::PortalShell,
            "portal shell (ScrollArea)",
        );
        ui.separator();
        ui.checkbox(&mut state.roles_loaded, "roles fetched");
        if ui.button("reset").clicked() {
            *state = AgentConfigStory::default();
        }
    });
    if !state.last.is_empty() {
        ui.colored_label(theme::TEXT_MUTED, &state.last);
    }
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    match state.hosted_in {
        HostedIn::Bare => body(ui, state),
        HostedIn::PortalShell => {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(560.0)
                .show(ui, |ui| {
                    let inner_width = 900.0_f32.min(ui.available_width());
                    let pad = ((ui.available_width() - inner_width).max(0.0)) / 2.0;
                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        ui.vertical(|ui| {
                            ui.set_max_width(inner_width);
                            body(ui, state);
                        });
                    });
                });
        }
    }
}

fn body(ui: &mut egui::Ui, state: &mut AgentConfigStory) {
    let status = status(state.key_state, &state.provider);
    let all_roles = roles();
    let roles_slice = state.roles_loaded.then_some(all_roles.as_slice());

    let AgentConfigResponse {
        provider_changed,
        submit_credential,
        test_clicked,
        clear_credential,
        budget_changed,
        refresh_roles,
    } = agent_config_section(
        ui,
        status.as_ref(),
        &mut state.provider,
        &mut state.credential,
        &mut state.agent,
        roles_slice,
        "story",
        NOW_MS,
    );

    if provider_changed {
        state.last = "provider edited".into();
    }
    if budget_changed {
        state.last = "budgets edited".into();
    }
    if refresh_roles {
        state.last = "RefreshRoles sent".into();
    }
    if test_clicked {
        state.last = "test completion requested".into();
    }
    if clear_credential {
        state.key_state = KeyState::Unset;
        state.last = "credential cleared".into();
    }
    if submit_credential {
        // What the real caller does: send it write-only, then forget it. The
        // story reports only the LENGTH — printing the secret is precisely
        // the habit this whole design exists to prevent.
        let len = state.credential.secret.trim().len();
        state.credential.clear();
        state.key_state = KeyState::Healthy;
        state.last = format!("credential submitted ({len} chars) and cleared from the draft");
    }
}
