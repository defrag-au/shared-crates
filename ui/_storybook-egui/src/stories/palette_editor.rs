//! `PaletteEditor` story — colorization palettes (name + base color + variants).

use egui_widgets::palette_editor::{Palette, PaletteEditor, PaletteVariant};
use egui_widgets::theme;

pub struct PaletteEditorState {
    pub palettes: Vec<Palette>,
}

impl Default for PaletteEditorState {
    fn default() -> Self {
        Self {
            palettes: vec![Palette {
                name: "warm".into(),
                base_color: [212, 165, 116],
                variants: vec![
                    PaletteVariant {
                        name: "golden".into(),
                        color: [230, 184, 92],
                        weight: 1.0,
                    },
                    PaletteVariant {
                        name: "bronze".into(),
                        color: [200, 149, 109],
                        weight: 1.0,
                    },
                ],
            }],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut PaletteEditorState) {
    ui.label(
        egui::RichText::new("Palette Editor")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Colorization palettes — a base color (the source pixels to recolor) \
             plus weighted variant colors. Backs the colorization config.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset").clicked() {
        *state = PaletteEditorState::default();
    }
    ui.add_space(8.0);

    PaletteEditor::new(&mut state.palettes).show(ui);

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!("{} palette(s)", state.palettes.len()))
            .color(theme::TEXT_SECONDARY)
            .small(),
    );
}
