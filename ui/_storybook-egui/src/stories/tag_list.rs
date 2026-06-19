//! `TagList` story — a wrapping row of removable chips with a clear button.

use egui_widgets::tag_list::TagList;
use egui_widgets::theme;

pub struct TagListState {
    pub tags: Vec<String>,
}

impl Default for TagListState {
    fn default() -> Self {
        Self {
            tags: vec![
                "Background: Purple Glow".into(),
                "Background: Pink Glow".into(),
                "Back: Skateboard".into(),
                "Back: Paint Roller".into(),
                "Necklace: Gold Skull".into(),
                "Hat: Viking Helmet".into(),
            ],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut TagListState) {
    ui.label(egui::RichText::new("Tag List").color(theme::ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "A wrapping row of removable chips with an optional clear-all button — \
             for active filters / selected facets. Resize the window narrow to see \
             it reflow onto multiple lines.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset tags").clicked() {
        *state = TagListState::default();
    }
    ui.add_space(8.0);

    let resp = TagList::new(&state.tags).clearable("Clear").show(ui);
    if let Some(i) = resp.removed {
        state.tags.remove(i);
    }
    if resp.cleared {
        state.tags.clear();
    }

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!("{} tag(s)", state.tags.len()))
            .color(theme::TEXT_SECONDARY)
            .small(),
    );
}
