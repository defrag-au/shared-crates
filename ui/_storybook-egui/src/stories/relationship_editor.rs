//! `RelationshipEditor` story — directed `source → target` edges over an option set.

use egui_widgets::relationship_editor::RelationshipEditor;

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
    crate::heading(ui, "Relationship Editor");
    crate::caption(
        ui,
        "Directed edges (source to target) over a known option set — backs \
         variant_flow / dependencies / slot-locks in the config editor (and \
         becomes the wires in the node-graph view).",
    );
    crate::caption(
        ui,
        "Every row sits on the same four columns — source, arrow, target, \
         remove — so the list and the add row below it read as one grid rather \
         than two stacked widgets. Add an edge with a long name to watch the \
         column truncate instead of shunting the arrow sideways.",
    );
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui.button("Reset").clicked() {
            *state = RelationshipEditorState::default();
        }
        // Reaching the empty state is the only way to see `empty_text`.
        if ui.button("Clear all").clicked() {
            state.edges.clear();
        }
    });
    ui.add_space(8.0);

    let resp = RelationshipEditor::new("story_variant_flow", &state.edges, &state.options)
        .add_label("Add edge")
        .show(ui);
    if let Some(edge) = resp.added
        && !state.edges.contains(&edge)
    {
        state.edges.push(edge);
    }
    if let Some(i) = resp.removed
        && i < state.edges.len()
    {
        state.edges.remove(i);
    }

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!("{} edge(s)", state.edges.len()))
            .color(crate::secondary(ui))
            .small(),
    );
}
