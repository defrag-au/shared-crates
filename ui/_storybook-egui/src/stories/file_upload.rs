//! File upload widget story

use crate::ACCENT;

pub struct FileUploadState {
    image_uploader: egui_widgets::FileUploadButton,
    any_uploader: egui_widgets::FileUploadButton,
    uploaded_files: Vec<UploadedEntry>,
}

struct UploadedEntry {
    name: String,
    mime_type: String,
    size: usize,
    /// Retained egui texture for image preview.
    texture: Option<egui::TextureHandle>,
}

impl Default for FileUploadState {
    fn default() -> Self {
        Self {
            image_uploader: egui_widgets::FileUploadButton::new("storybook_upload_img"),
            any_uploader: egui_widgets::FileUploadButton::new("storybook_upload_any"),
            uploaded_files: Vec::new(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut FileUploadState) {
    ui.label("Click the button to open the browser file picker. Selected files are read into memory and displayed below.");
    ui.add_space(12.0);

    // ── Upload button (images) ──────────────────────────────────────────
    ui.label(
        egui::RichText::new("Image Upload")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);

    if let Some(file) = state.image_uploader.show(ui, "Upload Image", "image/*") {
        let texture = try_load_texture(ui.ctx(), &file);
        state.uploaded_files.push(UploadedEntry {
            name: file.name,
            mime_type: file.mime_type,
            size: file.data.len(),
            texture,
        });
    }

    ui.add_space(12.0);

    // ── Any file ────────────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Any File Upload")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);

    if let Some(file) = state.any_uploader.show(ui, "Upload Any File", "*/*") {
        state.uploaded_files.push(UploadedEntry {
            name: file.name,
            mime_type: file.mime_type,
            size: file.data.len(),
            texture: None,
        });
    }

    ui.add_space(16.0);

    // ── Results ─────────────────────────────────────────────────────────
    if state.uploaded_files.is_empty() {
        ui.label("No files uploaded yet.");
        return;
    }

    ui.label(
        egui::RichText::new(format!("Uploaded Files ({})", state.uploaded_files.len()))
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);

    let mut remove_idx = None;
    for (i, entry) in state.uploaded_files.iter().enumerate() {
        ui.horizontal(|ui| {
            if let Some(tex) = &entry.texture {
                ui.add(
                    egui::Image::new(tex)
                        .max_size(egui::vec2(64.0, 64.0))
                        .corner_radius(4.0),
                );
            } else {
                ui.label("📄");
            }
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&entry.name).strong());
                ui.label(format!("{} — {} bytes", entry.mime_type, entry.size));
            });
            if ui.small_button("✕").clicked() {
                remove_idx = Some(i);
            }
        });
        ui.add_space(4.0);
    }

    if let Some(idx) = remove_idx {
        state.uploaded_files.remove(idx);
    }
}

/// Try to decode image bytes into an egui texture for preview.
fn try_load_texture(
    ctx: &egui::Context,
    file: &egui_widgets::UploadedFile,
) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(&file.data).ok()?;
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture(&file.name, color_image, egui::TextureOptions::LINEAR))
}
