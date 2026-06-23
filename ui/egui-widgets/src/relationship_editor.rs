//! relationship_editor — edit a list of directed `source → target` edges over a
//! known option set. Backs variant_flow, dependencies, and slot-lock style
//! relationships in the config editor (and the wires in the node-graph view).
//!
//! Existing edges render as removable rows; an "add" row picks a source + target
//! from the options. The host owns the edge list; the widget reports add/remove.
//! The add-row's pending selection is kept in egui temp memory, so the host
//! doesn't have to manage it.

use egui::Ui;

#[derive(Default, Debug, Clone)]
pub struct RelationshipEditorResponse {
    /// A complete `(source, target)` edge chosen on the add row this frame.
    pub added: Option<(String, String)>,
    /// Index of the edge removed this frame.
    pub removed: Option<usize>,
}

pub struct RelationshipEditor<'a> {
    id_salt: &'a str,
    edges: &'a [(String, String)],
    options: &'a [String],
    add_label: &'a str,
}

impl<'a> RelationshipEditor<'a> {
    pub fn new(id_salt: &'a str, edges: &'a [(String, String)], options: &'a [String]) -> Self {
        Self {
            id_salt,
            edges,
            options,
            add_label: "Add",
        }
    }

    pub fn add_label(mut self, label: &'a str) -> Self {
        self.add_label = label;
        self
    }

    pub fn show(self, ui: &mut Ui) -> RelationshipEditorResponse {
        // The → arrow is a Phosphor icon (the default font lacks U+2192). Self-
        // install for safety; hosts should also install_phosphor_font at startup.
        crate::install_phosphor_font(ui.ctx());
        let arrow_color = crate::theme::TEXT_MUTED;
        let mut resp = RelationshipEditorResponse::default();

        for (i, (src, tgt)) in self.edges.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(src.as_str());
                crate::PhosphorIcon::ArrowRight.show(ui, 12.0, arrow_color);
                ui.label(tgt.as_str());
                if ui
                    .button("×")
                    .on_hover_text("Remove relationship")
                    .clicked()
                {
                    resp.removed = Some(i);
                }
            });
        }

        // Add row — pending source/target kept in temp memory by id.
        let id = egui::Id::new(("relationship_editor", self.id_salt));
        let mut pending: (String, String) = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        ui.horizontal(|ui| {
            combo(ui, id.with("src"), "source", &mut pending.0, self.options);
            crate::PhosphorIcon::ArrowRight.show(ui, 12.0, arrow_color);
            combo(ui, id.with("tgt"), "target", &mut pending.1, self.options);
            let can_add = !pending.0.is_empty() && !pending.1.is_empty();
            if ui
                .add_enabled(can_add, egui::Button::new(self.add_label))
                .clicked()
            {
                resp.added = Some((pending.0.clone(), pending.1.clone()));
                pending = (String::new(), String::new());
            }
        });
        ui.data_mut(|d| d.insert_temp(id, pending));

        resp
    }
}

fn combo(ui: &mut Ui, id: egui::Id, placeholder: &str, current: &mut String, options: &[String]) {
    let text = if current.is_empty() {
        placeholder
    } else {
        current.as_str()
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .show_ui(ui, |ui| {
            for opt in options {
                ui.selectable_value(current, opt.clone(), opt.as_str());
            }
        });
}
