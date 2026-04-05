//! Image text editor story — demonstrates text overlay placement on images.

use crate::ACCENT;
use egui_widgets::image_text_editor::TextEffect;
use egui_widgets::{ImageTextEditor, TextOverlay};

pub struct ImageTextEditorState {
    editor: ImageTextEditor,
    texture: Option<egui::TextureHandle>,
}

impl Default for ImageTextEditorState {
    fn default() -> Self {
        let mut editor = ImageTextEditor::new();
        // Classic meme layout with different effects
        let mut top = TextOverlay::top("TOP TEXT");
        top.effect = TextEffect::OutlineAndShadow;
        top.font_scale = 0.10;
        editor.overlays.push(top);

        let mut bottom = TextOverlay::bottom("BOTTOM TEXT");
        bottom.effect = TextEffect::Outline;
        bottom.font_scale = 0.10;
        editor.overlays.push(bottom);

        editor.selected = Some(0);
        Self {
            editor,
            texture: None,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut ImageTextEditorState) {
    ui.label("Drag text to reposition. Drag the bottom-right handle to resize. Click to select, then edit properties in the right panel.");
    ui.add_space(8.0);

    // Create a sample texture on first frame
    let texture = state.texture.get_or_insert_with(|| {
        let (w, h) = (512, 512);
        let mut pixels = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                pixels[i] = (x * 255 / w) as u8;
                pixels[i + 1] = (y * 255 / h) as u8;
                pixels[i + 2] = 128;
                pixels[i + 3] = 255;
            }
        }
        let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels);
        ui.ctx()
            .load_texture("sample_image", color_image, egui::TextureOptions::LINEAR)
    });

    ui.columns(2, |cols| {
        // Left: image editor
        cols[0].label(egui::RichText::new("Image Editor").color(ACCENT).strong());
        cols[0].add_space(4.0);
        let available = cols[0].available_size();
        let editor_size = egui::vec2(available.x.min(500.0), available.x.min(500.0));
        state.editor.show(&mut cols[0], texture, editor_size);

        // Right: properties panel
        cols[1].label(
            egui::RichText::new("Text Properties")
                .color(ACCENT)
                .strong(),
        );
        cols[1].add_space(4.0);
        state.editor.show_properties(&mut cols[1]);

        cols[1].add_space(16.0);
        cols[1].separator();
        cols[1].add_space(8.0);

        // Overlay list
        cols[1].label(
            egui::RichText::new(format!("Overlays ({})", state.editor.overlays.len()))
                .color(ACCENT)
                .strong(),
        );
        cols[1].add_space(4.0);

        for (i, overlay) in state.editor.overlays.iter().enumerate() {
            let selected = state.editor.selected == Some(i);
            let label = if overlay.text.is_empty() {
                format!("[{i}] (empty)")
            } else {
                format!(
                    "[{i}] \"{}\" — {} / {}",
                    overlay.text,
                    overlay.font.label(&state.editor),
                    overlay.effect.label(),
                )
            };
            let text = if selected {
                egui::RichText::new(label).strong().color(ACCENT)
            } else {
                egui::RichText::new(label)
            };
            if cols[1].selectable_label(selected, text).clicked() {
                state.editor.selected = Some(i);
            }
        }
    });
}
