//! `wiring_editor` story — the composed binding editor two surfaces mount.
//!
//! `event_wiring` (its own story) is the primitive: one node, its cards.
//! THIS is what the gateway admin and the portal's gateway pane actually
//! render — the VM mapping, the config panels, the add/remove/toggle drain —
//! and it is the piece with two consumers and, until this story, no way to
//! look at it outside a deployment.
//!
//! ## Why this exists (it is not decoration)
//!
//! Reaching these states live costs a Discord server, a claimed guild, a
//! running listener and the right wallet. So the states nobody could see were
//! the states that shipped broken:
//!
//! - **A roster with more than zero entries.** The portal pane laid its guild
//!   list out with `SidePanel::show_inside` inside the shell's vertical
//!   `ScrollArea` — the trap `detail_split`'s module docs describe — and it
//!   rendered NOTHING. Invisible for as long as every client had no guilds,
//!   which was the whole development period.
//! - **An editor with bindings in it.** Empty-list rendering proves nothing
//!   about a list.
//!
//! The [`HostedIn`] control is the direct answer to the first one: it renders
//! the editor inside the portal shell's actual hosting context, so "does this
//! survive being a pane" is a click rather than a deploy.

use egui_widgets::theme;
use egui_widgets::wiring_editor::{self, WiringEditorState};
use gateway_wiring::{
    action_choices, AgentEntitlement, AgentTier, EventBinding, EventSource, GuildRole, RenderStyle,
    WiredAction,
};

/// Which hosting context to render the editor in.
///
/// Not a cosmetic toggle. A widget that works at the eframe root and breaks
/// inside a scrolled, width-constrained `Ui` is a widget that works in the
/// storybook and breaks in the portal — which is exactly what happened.
#[derive(Clone, Copy, PartialEq)]
pub enum HostedIn {
    /// Plain top-down `Ui`, the storybook's own context.
    Bare,
    /// What the client portal actually does: a vertical `ScrollArea` with a
    /// max-width column inside it. Panels and nested scroll areas misbehave
    /// here; this is the context every `portal_pane::Pane` really gets.
    PortalShell,
}

pub struct WiringEditorStory {
    pub bindings: Vec<EventBinding>,
    pub editor: WiringEditorState,
    pub hosted_in: HostedIn,
    /// Roster for the read-only entitlement view — `None` models "never
    /// fetched", which the view renders differently from "fetched, role gone".
    pub roles_loaded: bool,
    pub entitlement: Option<AgentEntitlement>,
    pub dirty: bool,
    pub last: String,
    /// The add-action catalogue is open for this binding. Held across frames:
    /// `add_action_for` on the response is "clicked THIS frame", so a row
    /// rendered straight from it would vanish before it could be used.
    pub add_action_for: Option<String>,
}

impl Default for WiringEditorStory {
    fn default() -> Self {
        Self {
            bindings: fixture_bindings(),
            editor: WiringEditorState::default(),
            hosted_in: HostedIn::Bare,
            roles_loaded: true,
            entitlement: Some(AgentEntitlement {
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
                default_daily_tokens: 25_000,
            }),
            dirty: false,
            last: String::new(),
            add_action_for: None,
        }
    }
}

/// Several bindings, deliberately not all alike: a pattern trigger with
/// multiple actions, a mention trigger (no patterns, pre-wired to the agent),
/// and a disabled one — disabling is not deleting, and it should look it.
fn fixture_bindings() -> Vec<EventBinding> {
    let mut ahoy = EventBinding::new_message("b-ahoy".into());
    ahoy.event = EventSource::OnMessage {
        patterns: vec!["ahoy".into(), "yarr".into()],
    };
    ahoy.insert_action(WiredAction::RandomOwnedAsset {
        policy_id: None,
        style: RenderStyle::Greeting {
            greeting: "GM".into(),
        },
        variants: Vec::new(),
    });
    ahoy.insert_action(WiredAction::React {
        emoji: "⚓".into()
    });

    let mention = EventBinding::new_mention("b-mention".into());

    let mut quiet = EventBinding::new_message("b-quiet".into());
    quiet.event = EventSource::OnMessage {
        patterns: vec!["gm".into()],
    };
    quiet.enabled = false;
    quiet.cooldown_seconds = Some(300);
    quiet.insert_action(WiredAction::React {
        emoji: "gm:1291038099278790738".into(),
    });

    vec![ahoy, mention, quiet]
}

fn fixture_roles() -> Vec<GuildRole> {
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

pub fn show(ui: &mut egui::Ui, state: &mut WiringEditorStory) {
    ui.label(
        egui::RichText::new("Wiring Editor")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The composed editor the gateway admin and the portal pane both mount. \
             Switch the host to check it survives the portal's scrolled, \
             width-constrained column — a SidePanel here renders nothing.",
        )
        .small()
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(8.0);

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
            *state = WiringEditorStory::default();
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.colored_label(
            if state.dirty {
                theme::ACCENT_YELLOW
            } else {
                theme::TEXT_MUTED
            },
            if state.dirty {
                "unsaved edits"
            } else {
                "clean"
            },
        );
        if !state.last.is_empty() {
            ui.colored_label(theme::TEXT_MUTED, &state.last);
        }
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    match state.hosted_in {
        HostedIn::Bare => body(ui, state),
        HostedIn::PortalShell => {
            // Mirrors `client-management/frontend/src/app.rs`: a vertical
            // ScrollArea wrapping a width-constrained column. If the editor
            // renders here, it renders in the portal.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(520.0)
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

fn body(ui: &mut egui::Ui, state: &mut WiringEditorStory) {
    let roles = fixture_roles();
    wiring_editor::agent_entitlement_readonly(
        ui,
        state.entitlement.as_ref(),
        state.roles_loaded.then_some(roles.as_slice()),
    );
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    let resp = wiring_editor::bindings_editor(ui, &mut state.bindings, &mut state.editor);

    if resp.dirty {
        state.dirty = true;
        state.last = "edited".to_string();
    }
    if resp.add_message_binding {
        let id = format!("b-new-{}", state.bindings.len());
        state.bindings.push(EventBinding::new_message(id));
        state.dirty = true;
        state.last = "added ON_MESSAGE binding".to_string();
    }
    if resp.add_mention_binding {
        let id = format!("b-new-{}", state.bindings.len());
        state.bindings.push(EventBinding::new_mention(id));
        state.dirty = true;
        state.last = "added ON_MENTION binding".to_string();
    }
    if let Some(binding_id) = resp.add_action_for {
        state.add_action_for = Some(binding_id);
    }

    // The add-action affordance is the renderer's own (a palette in the
    // standalone admin, a button row in the pane) — the CATALOGUE is shared,
    // so the story offers it the simplest way there is.
    if let Some(binding_id) = state.add_action_for.clone() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("add action:");
            for choice in action_choices() {
                if ui
                    .button(choice.title)
                    .on_hover_text(choice.subtitle)
                    .clicked()
                {
                    if let Some(binding) = state.bindings.iter_mut().find(|b| b.id == binding_id) {
                        let at = binding.insert_action(choice.action.clone());
                        state.editor.config_open = Some((binding_id.clone(), at));
                        state.editor.variants_draft.clear();
                        state.dirty = true;
                        state.last = format!("added {}", choice.title);
                    }
                    state.add_action_for = None;
                }
            }
            if ui.button("cancel").clicked() {
                state.add_action_for = None;
            }
        });
    }
}
