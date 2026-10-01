//! `RegionEditor` story — named slots drawn over the canvas, moved and resized.

use egui_widgets::region_editor::{NormRect, Region, RegionEditor, RegionEditorView};

use crate::{accent, muted};

pub struct RegionEditorState {
    pub editor: RegionEditor,
    pub regions: Vec<Region>,
}

impl Default for RegionEditorState {
    fn default() -> Self {
        let region = |name: &str, x: f32, y: f32, w: f32, h: f32| Region {
            name: name.into(),
            rect: NormRect::new(x, y, w, h),
        };
        let mut editor = RegionEditor::default();
        editor.select(Some(1));
        Self {
            editor,
            regions: vec![
                region("background", 0.0, 0.0, 1.0, 1.0),
                region("body", 0.30, 0.20, 0.40, 0.60),
                region("headwear", 0.36, 0.06, 0.28, 0.20),
            ],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut RegionEditorState) {
    ui.label(
        egui::RichText::new("Region Editor")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Named slots over the canvas as normalised rects. Drag a slot to move it, pull \
             a corner to resize, click to select. This is where a template's placement is \
             drawn — no image is supplied in the story, so the field stands in for one.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset").clicked() {
        *state = RegionEditorState::default();
    }
    ui.add_space(8.0);

    let RegionEditorState { editor, regions } = state;
    editor.show(
        ui,
        RegionEditorView {
            regions: regions.as_mut_slice(),
            texture: None,
            aspect: 1.0,
            width: None,
        },
    );
}
