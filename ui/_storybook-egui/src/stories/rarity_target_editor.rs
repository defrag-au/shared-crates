//! `RarityTargetEditor` story — labelled 0–100% sliders with a budget cue.

use egui_widgets::rarity_target_editor::{RarityRow, RarityTargetEditor};

use crate::{accent, muted};

pub struct RarityTargetEditorState {
    pub rows: Vec<RarityRow>,
}

impl Default for RarityTargetEditorState {
    fn default() -> Self {
        Self {
            rows: vec![
                RarityRow {
                    label: "Background: Purple".into(),
                    percent: 25.0,
                },
                RarityRow {
                    label: "Background: Pink".into(),
                    percent: 25.0,
                },
                RarityRow {
                    label: "Background:Red".into(),
                    percent: 25.0,
                },
                RarityRow {
                    label: "Background:Blue".into(),
                    percent: 25.0,
                },
            ],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut RarityTargetEditorState) {
    ui.label(
        egui::RichText::new("Rarity Target Editor")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Per-value / per-trait 0–100% target sliders with a running-total vs \
             budget cue (over / under / balanced). Drag the sliders to see the \
             budget colour change.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset").clicked() {
        *state = RarityTargetEditorState::default();
    }
    ui.add_space(8.0);

    // The editor fills what it is given, like every other fill-width widget
    // here, so the CALLER decides how long the faders get. `fit_width` clamps to
    // the CONTAINER and not a bare `set_max_width`, which widens a `Ui` when
    // less space is available.
    use egui_widgets::viewport::LayoutExt as _;
    ui.set_max_width(ui.fit_width(560.0));
    RarityTargetEditor::new(&mut state.rows)
        .budget(100.0)
        .show(ui);
}
