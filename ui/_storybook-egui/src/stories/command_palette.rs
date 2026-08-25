//! `CommandPalette` story — modal ⌘K launcher over a caller-supplied command set.

use egui_widgets::command_palette::{CommandPalette, PaletteAction, PaletteState};
use egui_widgets::theme;
use egui_widgets::typeahead_search::TypeaheadOption;

pub struct CommandPaletteState {
    pub palette: PaletteState,
    pub invoked: Vec<String>,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self {
            palette: PaletteState::default(),
            invoked: Vec::new(),
        }
    }
}

fn commands() -> Vec<TypeaheadOption> {
    vec![
        TypeaheadOption::new("add-on-message", "Add ON_MESSAGE event")
            .subtitle("fires on matching chat messages"),
        TypeaheadOption::new("add-render", "Add action: Random owned asset")
            .subtitle("render the user's asset as an inline reply"),
        TypeaheadOption::new("add-react", "Add action: React to message")
            .subtitle("instant emoji acknowledgment"),
        TypeaheadOption::new("goto-pfpcity", "Go to guild: pfpcity"),
        TypeaheadOption::new("goto-blackflag", "Go to guild: BlackFlag"),
        TypeaheadOption::new("gateway-status", "Show gateway status"),
    ]
}

pub fn show(ui: &mut egui::Ui, state: &mut CommandPaletteState) {
    ui.label(
        egui::RichText::new("Command Palette")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Modal keyboard-first launcher: ⌘K/Ctrl-K (or the button) opens an \
             autofocused fuzzy search over what the app can do right now; enter \
             dispatches, escape dismisses. Rendering rides TypeaheadSearch.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Open palette (or press ⌘K / Ctrl-K)").clicked() {
        state.palette.open();
    }

    let cmds = commands();
    match CommandPalette::new("story_palette", &cmds)
        .placeholder("Type a command…")
        .show(ui, &mut state.palette)
    {
        PaletteAction::Invoke(id) => state.invoked.push(id),
        PaletteAction::None => {}
    }

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new("Invocation log")
            .color(theme::TEXT_SECONDARY)
            .strong(),
    );
    if state.invoked.is_empty() {
        ui.label(
            egui::RichText::new("nothing invoked yet")
                .color(theme::TEXT_MUTED)
                .small(),
        );
    }
    for id in state.invoked.iter().rev().take(6) {
        ui.label(
            egui::RichText::new(format!("invoked: {id}"))
                .color(theme::TEXT_PRIMARY)
                .small(),
        );
    }
}
