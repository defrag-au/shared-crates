//! `EventWiring` story — one event node wired to action cards, with palette-driven add.

use egui_widgets::command_palette::{CommandPalette, PaletteAction, PaletteState};
use egui_widgets::event_wiring::{ActionCardVm, EventNodeVm, EventWiring};
use egui_widgets::theme;
use egui_widgets::typeahead_search::TypeaheadOption;
use egui_widgets::PhosphorIcon;

#[derive(Clone)]
pub enum StoryAction {
    Render { policy: String },
    React { emoji: String },
}

pub struct EventWiringState {
    pub patterns: Vec<String>,
    pub actions: Vec<StoryAction>,
    pub enabled: bool,
    pub cooldown_seconds: Option<u32>,
    pub palette: PaletteState,
    pub last: String,
}

impl Default for EventWiringState {
    fn default() -> Self {
        Self {
            patterns: vec!["ahoy".into(), "yarr".into()],
            actions: vec![
                StoryAction::React {
                    emoji: "gm:1291038099278790738".into(),
                },
                StoryAction::Render {
                    policy: "guild default".into(),
                },
            ],
            enabled: true,
            cooldown_seconds: Some(60),
            palette: PaletteState::default(),
            last: String::new(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut EventWiringState) {
    ui.label(
        egui::RichText::new("Event Wiring")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "One event source wired to its dispatched actions — IFTTT made \
             visible. Pattern chips fire the node; the '+ action' port opens \
             the command palette. Small-scale node-link on purpose (see \
             flow_matrix's argument for anything bigger).",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset").clicked() {
        *state = EventWiringState::default();
    }
    ui.add_space(8.0);

    let event = EventNodeVm {
        kind_label: "ON_MESSAGE".into(),
        icon: PhosphorIcon::Lightning,
        patterns: state.patterns.clone(),
        enabled: state.enabled,
        cooldown_seconds: state.cooldown_seconds,
    };
    let cards: Vec<ActionCardVm> = state
        .actions
        .iter()
        .enumerate()
        .map(|(i, action)| match action {
            StoryAction::Render { policy } => ActionCardVm {
                id: format!("a{i}"),
                icon: PhosphorIcon::Package,
                title: "Random owned asset".into(),
                subtitle: Some(format!("policy: {policy}")),
            },
            StoryAction::React { emoji } => ActionCardVm {
                id: format!("a{i}"),
                icon: PhosphorIcon::Heart,
                title: "React to message".into(),
                subtitle: Some(emoji.clone()),
            },
        })
        .collect();

    let resp = EventWiring::new("story_event_wiring", &event, &cards).show(ui);
    if let Some(pattern) = resp.pattern_added {
        state.last = format!("pattern added: {pattern}");
        state.patterns.push(pattern);
    }
    if let Some(index) = resp.pattern_removed {
        if index < state.patterns.len() {
            state.last = format!("pattern removed: {}", state.patterns[index]);
            state.patterns.remove(index);
        }
    }
    if let Some(index) = resp.action_removed {
        if index < state.actions.len() {
            state.last = format!("action {index} removed");
            state.actions.remove(index);
        }
    }
    if let Some(index) = resp.action_clicked {
        state.last = format!("action {index} clicked (would open its config)");
    }
    if resp.enabled_toggled {
        state.enabled = !state.enabled;
        state.last = format!("enabled → {}", state.enabled);
    }
    if resp.remove_clicked {
        state.last = "binding remove clicked (story keeps it)".into();
    }
    if let Some(secs) = resp.cooldown_set {
        state.cooldown_seconds = (secs > 0).then_some(secs);
        state.last = format!("cooldown → {secs}s");
    }
    if resp.add_action_clicked {
        state.palette.open();
    }

    // The add-action palette — the composition the gateway admin uses.
    let add_commands = vec![
        TypeaheadOption::new("render", "Add action: Random owned asset")
            .subtitle("render + inline reply"),
        TypeaheadOption::new("react", "Add action: React to message")
            .subtitle("instant emoji acknowledgment"),
    ];
    match CommandPalette::new("story_wiring_palette", &add_commands)
        .placeholder("Add an action…")
        .keybinding(false)
        .show(ui, &mut state.palette)
    {
        PaletteAction::Invoke(id) => {
            match id.as_str() {
                "render" => state.actions.push(StoryAction::Render {
                    policy: "guild default".into(),
                }),
                "react" => state.actions.push(StoryAction::React {
                    emoji: "⚓".into()
                }),
                _ => {}
            }
            state.last = format!("added via palette: {id}");
        }
        PaletteAction::None => {}
    }

    ui.add_space(8.0);
    if !state.last.is_empty() {
        ui.label(
            egui::RichText::new(&state.last)
                .color(theme::TEXT_SECONDARY)
                .small(),
        );
    }
}
