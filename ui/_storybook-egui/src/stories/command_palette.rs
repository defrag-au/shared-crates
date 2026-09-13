//! `CommandPalette` story — modal ⌘K launcher over a caller-supplied command set.

use egui_widgets::command_palette::{CommandPalette, PaletteAction, PaletteState};

use crate::{accent, muted};
use egui_widgets::typeahead_search::TypeaheadOption;

pub struct CommandPaletteState {
    pub palette: PaletteState,
    pub invoked: Vec<String>,
    /// Stand-ins for widgets coming and going as the reader navigates.
    pub show_roster: bool,
    pub show_exporter: bool,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self {
            palette: PaletteState::default(),
            invoked: Vec::new(),
            show_roster: true,
            show_exporter: true,
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
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Modal keyboard-first launcher: ⌘K/Ctrl-K (or the button) opens an \
             autofocused fuzzy search over what the app can do right now; enter \
             dispatches, escape dismisses. Rendering rides TypeaheadSearch.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Open palette (or press ⌘K / Ctrl-K)").clicked() {
        state.palette.open();
    }

    ui.add_space(10.0);
    crate::heading(ui, "Fed by the widgets on screen");
    crate::caption(
        ui,
        "The two below `offer` commands from where they live. Nothing here \
         listed them — this story asks `commands::offered_options(ctx)` and \
         hands the result to the palette alongside its own static set. Untick \
         one and watch its entry leave the palette, because an offer that stops \
         happening expires.",
    );
    ui.add_space(6.0);
    ui.checkbox(&mut state.show_roster, "a roster widget is on screen");
    ui.checkbox(&mut state.show_exporter, "an exporter widget is on screen");
    ui.add_space(4.0);

    if state.show_roster
        && egui_widgets::commands::Command::new("story.roster.add", "Add wallet")
            .hint("stake address or $handle")
            .group("Wallets")
            .offer(ui)
    {
        state
            .invoked
            .push("story.roster.add (claimed by the roster)".into());
    }
    if state.show_exporter
        && egui_widgets::commands::Command::new("story.export.csv", "Export as CSV")
            .hint("the current selection")
            .group("Export")
            .offer(ui)
    {
        state
            .invoked
            .push("story.export.csv (claimed by the exporter)".into());
    }

    // The app's own static commands, plus whatever is currently on screen.
    // Merging rather than replacing: an app has commands no widget owns.
    let mut cmds = commands();
    cmds.extend(egui_widgets::commands::offered_options(ui.ctx()));

    match CommandPalette::new("story_palette", &cmds)
        .placeholder("Type a command…")
        .show(ui, &mut state.palette)
    {
        // The app does not act on widget commands — it just passes the id back
        // to the registry, and whichever widget offered it claims it.
        PaletteAction::Invoke(id) => {
            egui_widgets::commands::invoke(ui.ctx(), id.clone());
            state.invoked.push(id);
        }
        PaletteAction::None => {}
    }

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new("Invocation log")
            .color(crate::secondary(ui))
            .strong(),
    );
    if state.invoked.is_empty() {
        ui.label(
            egui::RichText::new("nothing invoked yet")
                .color(muted(ui))
                .small(),
        );
    }
    for id in state.invoked.iter().rev().take(6) {
        ui.label(
            egui::RichText::new(format!("invoked: {id}"))
                .color(crate::ink(ui))
                .small(),
        );
    }
}
