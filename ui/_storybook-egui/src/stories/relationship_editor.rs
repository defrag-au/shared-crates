//! `RelationshipEditor` story — directed `source → target` edges over an option set.

use egui_widgets::relationship_editor::RelationshipEditor;

use crate::{accent, muted};

pub struct RelationshipEditorState {
    pub options: Vec<String>,
    pub edges: Vec<(String, String)>,
}

impl Default for RelationshipEditorState {
    fn default() -> Self {
        Self {
            options: vec![
                "skin".into(),
                "clothes".into(),
                "neck".into(),
                "hand".into(),
                "hair".into(),
            ],
            edges: vec![
                ("skin".into(), "clothes".into()),
                ("skin".into(), "neck".into()),
            ],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut RelationshipEditorState) {
    ui.label(
        egui::RichText::new("Relationship Editor")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Directed edges (source to target) over a known option set — backs \
             variant_flow / dependencies / slot-locks in the config editor (and \
             becomes the wires in the node-graph view).",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset").clicked() {
        *state = RelationshipEditorState::default();
    }
    ui.add_space(8.0);

    let resp = RelationshipEditor::new("story_variant_flow", &state.edges, &state.options)
        .add_label("Add edge")
        .show(ui);
    if let Some(edge) = resp.added {
        if !state.edges.contains(&edge) {
            state.edges.push(edge);
        }
    }
    if let Some(i) = resp.removed {
        if i < state.edges.len() {
            state.edges.remove(i);
        }
    }

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!("{} edge(s)", state.edges.len()))
            .color(crate::secondary(ui))
            .small(),
    );
}
