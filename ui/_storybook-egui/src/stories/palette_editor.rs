//! `PaletteEditor` story — colorization palettes (name + base color + variants).

use egui_widgets::palette_editor::{Palette, PaletteEditor, PaletteVariant};

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
                        // Not 1.0 like the others: equal weights hide whether
                        // the column is actually right-aligned and sized for
                        // the widest value it can hold.
                        weight: 12.5,
                    },
                ],
            }],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut PaletteEditorState) {
    crate::heading(ui, "Palette Editor");
    crate::caption(
        ui,
        "Colorization palettes — a base color (the source pixels to recolor) \
         plus weighted variant colors. Backs the colorization config.",
    );
    crate::caption(
        ui,
        "A palette and its variants are rows of one grid: the variants indent so \
         the hierarchy reads down the left edge, while the weight and the \
         trailing action stay on one axis down the right. Add variants and type \
         long names — the card holds its width and the columns hold their edges.",
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
            .color(crate::secondary(ui))
            .small(),
    );
}
