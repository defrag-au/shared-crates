//! The shared binding editor over `gateway-wiring`'s vocabulary (feature
//! `gateway`).
//!
//! Two renderers exist — the standalone gateway admin in augminted-bots and
//! the client portal's gateway pane in cnft.dev-workers — and the accepted
//! risk of that split is drift (`ADMIN_SURFACE_CONSOLIDATION_DESIGN.md` §11).
//! The mitigation is layering: `gateway-wiring` owns the rules,
//! [`event_wiring`](crate::event_wiring) owns how a node group looks, and
//! THIS module owns the glue between them — how a [`WiredAction`] becomes an
//! action card, what its config panel edits, how a frame's interactions apply
//! back to the model. A renderer owns only layout, navigation, and which
//! affordance offers the add-action catalogue.
//!
//! If a surface needs something this editor cannot express, extend it here —
//! a second mapping in a renderer is the failure this module exists to
//! prevent, and it would show up as customers seeing different affordances
//! from operators for the same config.

use egui::Ui;
use gateway_wiring::{
    AgentEntitlement, AgentMode, EventBinding, EventSource, GuildRole, RenderStyle, WiredAction,
    text_to_variants, variants_to_text,
};

use crate::event_wiring::{ActionCardVm, EventNodeVm, EventWiring};
use crate::{PhosphorIcon, theme};

/// Cross-frame editor state. The caller holds one per editor and resets it
/// when the draft it refers to is reloaded (indices go stale with the draft).
#[derive(Default)]
pub struct WiringEditorState {
    /// Open config panel: (binding id, action index).
    pub config_open: Option<(String, usize)>,
    /// Draft text for the trait-filter editor — parsed on change, kept as a
    /// buffer so reformatting doesn't fight typing. (Re)seeded whenever a
    /// `RandomOwnedAsset` config opens.
    pub variants_draft: String,
}

impl WiringEditorState {
    /// Forget everything tied to the current draft. Call on reload/select.
    pub fn reset(&mut self) {
        self.config_open = None;
        self.variants_draft.clear();
    }
}

/// What a frame of editing asks of the caller. `dirty` is the save-state
/// signal; the rest are affordance requests the editor deliberately does not
/// answer itself — each surface offers the catalogue its own way.
#[derive(Default)]
pub struct WiringEditorResponse {
    /// Something in the draft changed — advance the save lifecycle to dirty.
    pub dirty: bool,
    /// The "+ action" port on this binding was clicked.
    pub add_action_for: Option<String>,
    /// The "+ add ON_MESSAGE binding" button was clicked. The caller pushes
    /// [`EventBinding::new_message`] with an id of its own minting — id
    /// generation needs a clock, and this crate does not take one.
    pub add_message_binding: bool,
    /// The "+ add ON_MENTION binding" button was clicked
    /// ([`EventBinding::new_mention`] — comes pre-wired and paced).
    pub add_mention_binding: bool,
}

/// The full bindings editor: one [`EventWiring`] node group per binding, with
/// config expanding in the open card, and the add-binding buttons after.
pub fn bindings_editor(
    ui: &mut Ui,
    bindings: &mut Vec<EventBinding>,
    state: &mut WiringEditorState,
) -> WiringEditorResponse {
    let mut response = WiringEditorResponse::default();
    let mut remove_binding: Option<usize> = None;

    // One `&mut` per binding for the whole iteration: the reads that build the
    // VMs are reborrows of it, and the expanded card's config takes its own
    // mutable reborrow inside the closure. The index-and-reindex shape this
    // replaced predated that being possible and only looked necessary.
    for (index, binding) in bindings.iter_mut().enumerate() {
        let binding_id = binding.id.clone();
        // A mention trigger has no patterns to show — the whole message is
        // the question. Rendered as an empty list rather than a special case
        // downstream, so the node reads the same either way.
        let (kind_label, patterns) = match &binding.event {
            EventSource::OnMessage { patterns } => ("ON_MESSAGE".to_string(), patterns.clone()),
            EventSource::OnMention {} => ("ON_MENTION".to_string(), Vec::new()),
        };
        let event_vm = EventNodeVm {
            kind_label,
            icon: PhosphorIcon::Lightning,
            patterns,
            enabled: binding.enabled,
            cooldown_seconds: binding.cooldown_seconds,
        };
        let cards: Vec<ActionCardVm> = binding
            .actions
            .iter()
            .enumerate()
            .map(|(i, action)| action_card_vm(&binding_id, i, action))
            .collect();

        // Config renders IN the expanded card (the flow stays the view); the
        // closure borrows the open action mutably for the duration of
        // show(), so dirty comes out via a local.
        let expanded_index = match &state.config_open {
            Some((open_binding, i)) if *open_binding == binding_id => Some(*i),
            _ => None,
        };
        let mut config_dirty = false;
        let resp = {
            let wiring = EventWiring::new(&binding_id, &event_vm, &cards);
            let variants_draft = &mut state.variants_draft;
            match expanded_index.and_then(|i| binding.actions.get_mut(i).map(|a| (i, a))) {
                Some((i, action)) => wiring
                    .expanded(i, |ui| {
                        config_dirty = render_action_config(ui, action, variants_draft);
                    })
                    .show(ui),
                None => wiring.show(ui),
            }
        };
        response.dirty |= config_dirty;

        // Pattern edits only apply to a pattern trigger. A mention binding
        // renders no pattern controls, so these are unreachable there rather
        // than merely ignored.
        if let EventSource::OnMessage { patterns } = &mut binding.event {
            if let Some(pattern) = resp.pattern_added {
                patterns.push(pattern);
                response.dirty = true;
            }
            if let Some(i) = resp.pattern_removed
                && i < patterns.len() {
                    patterns.remove(i);
                    response.dirty = true;
                }
        }
        if let Some(i) = resp.action_removed
            && i < binding.actions.len() {
                binding.actions.remove(i);
                response.dirty = true;
                state.config_open = None;
            }
        if let Some(i) = resp.action_clicked {
            // Toggle: the caret on an expanded card collapses it.
            state.config_open = if expanded_index == Some(i) {
                None
            } else {
                if let Some(WiredAction::RandomOwnedAsset { variants, .. }) = binding.actions.get(i)
                {
                    state.variants_draft = variants_to_text(variants);
                }
                Some((binding_id.clone(), i))
            };
        }
        if resp.enabled_toggled {
            binding.enabled = !binding.enabled;
            response.dirty = true;
        }
        if let Some(secs) = resp.cooldown_set {
            binding.cooldown_seconds = (secs > 0).then_some(secs);
            response.dirty = true;
        }
        if resp.remove_clicked {
            remove_binding = Some(index);
        }
        if resp.add_action_clicked {
            response.add_action_for = Some(binding_id.clone());
        }

        ui.add_space(16.0);
    }

    if let Some(index) = remove_binding {
        bindings.remove(index);
        response.dirty = true;
        state.config_open = None;
    }

    ui.horizontal(|ui| {
        if ui.button("+ add ON_MESSAGE binding").clicked() {
            response.add_message_binding = true;
        }
        if ui.button("+ add ON_MENTION binding").clicked() {
            response.add_mention_binding = true;
        }
    });

    response
}

/// How one [`WiredAction`] presents as a card — icon, title, and a subtitle
/// summarising its config at a glance.
fn action_card_vm(binding_id: &str, i: usize, action: &WiredAction) -> ActionCardVm {
    match action {
        WiredAction::RandomOwnedAsset {
            policy_id,
            style,
            variants,
        } => {
            let style_label = match style {
                RenderStyle::Image => "image".to_string(),
                RenderStyle::Greeting { greeting } => format!("greeting: {greeting}"),
                RenderStyle::OverlayGreeting { overlay, .. } => format!("overlay: {overlay}"),
            };
            let policy_label = policy_id
                .as_deref()
                .map(|p| format!("{}…", &p[..p.len().min(8)]))
                .unwrap_or_else(|| "guild default".to_string());
            let filter_label = if variants.is_empty() {
                String::new()
            } else {
                format!(" · {} filter(s)", variants.len())
            };
            ActionCardVm {
                id: format!("{binding_id}:{i}"),
                icon: PhosphorIcon::Package,
                title: "Random owned asset".to_string(),
                subtitle: Some(format!("{policy_label} · {style_label}{filter_label}")),
            }
        }
        WiredAction::React { emoji } => ActionCardVm {
            id: format!("{binding_id}:{i}"),
            icon: PhosphorIcon::Heart,
            title: "React to message".to_string(),
            subtitle: Some(emoji.clone()),
        },
        WiredAction::Ask {} => ActionCardVm {
            id: format!("{binding_id}:{i}"),
            icon: PhosphorIcon::Lightning,
            title: "Answer with the agent".to_string(),
            // Nothing to configure, so the subtitle says where the capability
            // actually comes from — otherwise "no options" reads as
            // unfinished.
            subtitle: Some("tools come from this server's plugins".to_string()),
        },
    }
}

/// Render one action's config fields (inside its expanded card — the
/// `EventWiring::expanded` closure). Returns true when anything changed.
/// Fits the widened card by design; if an action's config ever outgrows it,
/// push that action to a centred `egui::Modal` instead of growing the card.
fn render_action_config(
    ui: &mut Ui,
    action: &mut WiredAction,
    variants_draft: &mut String,
) -> bool {
    let mut dirty = false;
    match action {
        WiredAction::Ask {} => {
            // Nothing to edit — and saying so beats an empty panel that reads
            // as a rendering bug. What it can answer is decided by the
            // guild's plugins, which is config that lives elsewhere.
            ui.label("Answers the mention using this server's plugin tools.");
            ui.label(
                egui::RichText::new(
                    "No options here: which questions it can answer comes from \
                     the tools each opted-in plugin advertises, and refreshes \
                     with the bot config.",
                )
                .small()
                .color(theme::TEXT_MUTED),
            );
        }
        WiredAction::React { emoji } => {
            ui.label("emoji — unicode, or a custom emote");
            ui.label(
                egui::RichText::new(
                    "paste any form: <:gm:id>, :gm:id, or gm:id \
                             (normalized on save)",
                )
                .small()
                .color(theme::TEXT_MUTED),
            );
            if ui.text_edit_singleline(emoji).changed() {
                dirty = true;
            }
        }
        WiredAction::RandomOwnedAsset {
            policy_id,
            style,
            variants,
        } => {
            ui.label("policy id (56-hex; empty = guild default)");
            let mut buffer = policy_id.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut buffer).changed() {
                let trimmed = buffer.trim();
                *policy_id = (!trimmed.is_empty()).then(|| trimmed.to_string());
                dirty = true;
            }
            ui.add_space(6.0);

            ui.label("style");
            let style_name = match style {
                RenderStyle::Image => "image",
                RenderStyle::Greeting { .. } => "greeting",
                RenderStyle::OverlayGreeting { .. } => "overlay greeting",
            };
            egui::ComboBox::from_id_salt("cfg_style")
                .selected_text(style_name)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            matches!(style, RenderStyle::Image),
                            "image — the plain asset",
                        )
                        .clicked()
                    {
                        *style = RenderStyle::Image;
                        dirty = true;
                    }
                    if ui
                        .selectable_label(
                            matches!(style, RenderStyle::Greeting { .. }),
                            "greeting — \"{greeting} from {asset}!\"",
                        )
                        .clicked()
                    {
                        *style = RenderStyle::Greeting {
                            greeting: "GM".to_string(),
                        };
                        dirty = true;
                    }
                    if ui
                        .selectable_label(
                            matches!(style, RenderStyle::OverlayGreeting { .. }),
                            "overlay greeting — composited template",
                        )
                        .clicked()
                    {
                        *style = RenderStyle::OverlayGreeting {
                            overlay: "blackflag/greetings".to_string(),
                            animated: true,
                            greeting: None,
                        };
                        dirty = true;
                    }
                });

            match style {
                RenderStyle::Image => {}
                RenderStyle::Greeting { greeting } => {
                    ui.label("greeting");
                    if ui.text_edit_singleline(greeting).changed() {
                        dirty = true;
                    }
                }
                RenderStyle::OverlayGreeting {
                    overlay,
                    animated,
                    greeting,
                } => {
                    ui.label("overlay template (e.g. blackflag/greetings)");
                    if ui.text_edit_singleline(overlay).changed() {
                        dirty = true;
                    }
                    if ui.checkbox(animated, "animated").changed() {
                        dirty = true;
                    }
                    ui.label("greeting (optional caption)");
                    let mut buffer = greeting.clone().unwrap_or_default();
                    if ui.text_edit_singleline(&mut buffer).changed() {
                        let trimmed = buffer.trim();
                        *greeting = (!trimmed.is_empty()).then(|| trimmed.to_string());
                        dirty = true;
                    }
                }
            }

            ui.add_space(6.0);
            ui.label("trait filters (optional; empty = any owned asset)");
            ui.label(
                egui::RichText::new(
                    "one search per line: Category = value | value ; Category2 = value \
                     — lines are unioned, categories on a line must all match",
                )
                .small()
                .color(theme::TEXT_MUTED),
            );
            if ui
                .add(
                    egui::TextEdit::multiline(variants_draft)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                *variants = text_to_variants(variants_draft);
                dirty = true;
            }
        }
    }
    dirty
}

/// What a client sees where the entitlement editor would be.
///
/// Shown rather than hidden: their plan is the thing on this screen they are
/// paying for, so "what am I currently entitled to" should be answerable
/// without asking us. What is missing is any control that changes it — and
/// the DO refuses those regardless.
///
/// `roles` is the guild's roster if it has been fetched — role NAMES where it
/// has been, bare ids where it hasn't, the same fallback the picker uses, for
/// the same reason: a bare snowflake tells a reader nothing.
pub fn agent_entitlement_readonly(
    ui: &mut Ui,
    agent: Option<&AgentEntitlement>,
    roles: Option<&[GuildRole]>,
) {
    let mode = AgentMode::of(agent);
    ui.horizontal(|ui| {
        ui.strong("Agent");
        ui.colored_label(theme::TEXT_MUTED, mode.label());
    });

    match agent {
        None => {
            ui.colored_label(
                theme::TEXT_MUTED,
                "This server is not entitled to the agent — talk to Augminted to enable it.",
            );
        }
        Some(agent) if mode == AgentMode::Everyone => {
            ui.colored_label(
                theme::TEXT_MUTED,
                format!("Every member: {} tokens/day", agent.default_daily_tokens),
            );
        }
        Some(agent) => {
            for tier in &agent.tiers {
                let name = roles
                    .and_then(|roles| roles.iter().find(|r| r.id == tier.role))
                    .map(|r| format!("@{}", r.name))
                    .unwrap_or_else(|| tier.role.clone());
                ui.colored_label(
                    theme::TEXT_MUTED,
                    format!("{name}: {} tokens/day", tier.daily_tokens),
                );
            }
            if agent.default_daily_tokens > 0 {
                ui.colored_label(
                    theme::TEXT_MUTED,
                    format!("everyone else: {} tokens/day", agent.default_daily_tokens),
                );
            }
        }
    }
    ui.colored_label(theme::TEXT_MUTED, "Entitlements are set by Augminted.");
}
