//! Image text overlay editor — place, style, and drag text on top of an image.
//!
//! The editor renders a base image with movable text overlays. Users can add text,
//! change font size, color, outline, and drag to position. The final composite can
//! be flattened to an `image::RgbaImage` for saving/uploading.
//!
//! Feature-gated behind `image-editor`.

use egui::{
    pos2, Color32, FontFamily, FontId, Rect, Response, Sense, Stroke, TextureHandle, Ui, Vec2,
};
use serde::{Deserialize, Serialize};

// ─── Types ──────────────────────────────────────────────────────────────────

/// Vertical anchor for text positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextOverlayAnchor {
    Top,
    Center,
    Bottom,
    /// Free position — `offset_y` is absolute (0.0 = top, 1.0 = bottom).
    Free,
}

/// A single text overlay on the image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextOverlay {
    pub text: String,
    /// Font size relative to image height (0.0..1.0). 0.08 = 8% of image height.
    pub font_scale: f32,
    /// Text color.
    pub color: [u8; 4],
    /// Outline/stroke color (typically black for meme text).
    pub outline_color: [u8; 4],
    /// Outline thickness relative to font size (0.0 = no outline, 0.1 = 10% of font size).
    pub outline_scale: f32,
    /// Horizontal position (0.0 = left, 0.5 = center, 1.0 = right).
    pub offset_x: f32,
    /// Vertical position (0.0 = top, 1.0 = bottom). Used when anchor is Free.
    pub offset_y: f32,
    /// Positioning anchor.
    pub anchor: TextOverlayAnchor,
}

impl Default for TextOverlay {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_scale: 0.08,
            color: [255, 255, 255, 255],
            outline_color: [0, 0, 0, 255],
            outline_scale: 0.08,
            offset_x: 0.5,
            offset_y: 0.5,
            anchor: TextOverlayAnchor::Free,
        }
    }
}

impl TextOverlay {
    /// Create a top-anchored text overlay (classic meme top text).
    pub fn top(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            offset_y: 0.05,
            anchor: TextOverlayAnchor::Top,
            ..Default::default()
        }
    }

    /// Create a bottom-anchored text overlay (classic meme bottom text).
    pub fn bottom(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            offset_y: 0.92,
            anchor: TextOverlayAnchor::Bottom,
            ..Default::default()
        }
    }
}

/// State for the image text editor.
pub struct ImageTextEditor {
    /// Text overlays on the image.
    pub overlays: Vec<TextOverlay>,
    /// Index of the currently selected overlay (for editing).
    pub selected: Option<usize>,
    /// Which overlay is being dragged (and the drag start offset).
    dragging: Option<(usize, Vec2)>,
}

impl Default for ImageTextEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageTextEditor {
    pub fn new() -> Self {
        Self {
            overlays: Vec::new(),
            selected: None,
            dragging: None,
        }
    }

    /// Show the editor. Returns true if any overlay changed.
    ///
    /// `texture` is the base image texture. The editor draws overlays on top.
    /// `available_size` controls the display area — the image is scaled to fit.
    pub fn show(&mut self, ui: &mut Ui, texture: &TextureHandle, available_size: Vec2) -> Response {
        let tex_size = texture.size_vec2();
        let scale = (available_size.x / tex_size.x).min(available_size.y / tex_size.y);
        let display_size = tex_size * scale;

        // Allocate the image area
        let (response, painter) = ui.allocate_painter(display_size, Sense::click_and_drag());
        let image_rect = response.rect;

        // Draw the base image
        painter.image(
            texture.id(),
            image_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        // Handle click to deselect
        if response.clicked() && !response.dragged() {
            self.selected = None;
        }

        // Draw each overlay
        let id_base = ui.id().with("text_overlay");
        for (i, overlay) in self.overlays.iter_mut().enumerate() {
            let is_selected = self.selected == Some(i);

            // Compute position in image rect
            let text_pos = pos2(
                image_rect.left() + overlay.offset_x * image_rect.width(),
                image_rect.top() + overlay.offset_y * image_rect.height(),
            );

            let font_size = overlay.font_scale * image_rect.height();
            let font_id = FontId::new(font_size, FontFamily::Proportional);

            if overlay.text.is_empty() {
                continue;
            }

            let galley = painter.layout_no_wrap(
                overlay.text.clone(),
                font_id.clone(),
                Color32::from_rgba_unmultiplied(
                    overlay.color[0],
                    overlay.color[1],
                    overlay.color[2],
                    overlay.color[3],
                ),
            );

            let text_size = galley.size();
            // Center horizontally around offset_x
            let text_rect =
                Rect::from_min_size(pos2(text_pos.x - text_size.x * 0.5, text_pos.y), text_size);

            // Draw outline by rendering text offset in 8 directions
            if overlay.outline_scale > 0.0 {
                let outline_px = (overlay.outline_scale * font_size).max(1.0);
                let outline_color = Color32::from_rgba_unmultiplied(
                    overlay.outline_color[0],
                    overlay.outline_color[1],
                    overlay.outline_color[2],
                    overlay.outline_color[3],
                );
                for dx in [-outline_px, 0.0, outline_px] {
                    for dy in [-outline_px, 0.0, outline_px] {
                        if dx == 0.0 && dy == 0.0 {
                            continue;
                        }
                        let outline_galley = painter.layout_no_wrap(
                            overlay.text.clone(),
                            font_id.clone(),
                            outline_color,
                        );
                        painter.galley(
                            pos2(text_rect.left() + dx, text_rect.top() + dy),
                            outline_galley,
                            Color32::TRANSPARENT,
                        );
                    }
                }
            }

            // Draw the main text
            painter.galley(text_rect.min, galley, Color32::TRANSPARENT);

            // Selection highlight
            if is_selected {
                painter.rect_stroke(
                    text_rect.expand(2.0),
                    2.0,
                    Stroke::new(1.5, Color32::from_rgb(100, 180, 255)),
                    egui::StrokeKind::Outside,
                );
            }

            // Drag interaction
            let drag_id = id_base.with(i);
            let drag_response =
                ui.interact(text_rect.expand(4.0), drag_id, Sense::click_and_drag());

            if drag_response.clicked() {
                self.selected = Some(i);
            }

            if drag_response.drag_started() {
                self.dragging = Some((i, Vec2::ZERO));
                self.selected = Some(i);
            }

            if drag_response.dragged() && self.dragging.map(|(idx, _)| idx) == Some(i) {
                let delta = drag_response.drag_delta();
                overlay.offset_x += delta.x / image_rect.width();
                overlay.offset_y += delta.y / image_rect.height();
                overlay.offset_x = overlay.offset_x.clamp(0.0, 1.0);
                overlay.offset_y = overlay.offset_y.clamp(0.0, 1.0);
                overlay.anchor = TextOverlayAnchor::Free;
            }

            if drag_response.drag_stopped() {
                self.dragging = None;
            }
        }

        response
    }

    /// Show the overlay property editor panel for the selected overlay.
    /// Returns true if anything changed.
    pub fn show_properties(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            if ui.button("+ Top Text").clicked() {
                self.overlays.push(TextOverlay::top("TOP TEXT"));
                self.selected = Some(self.overlays.len() - 1);
                changed = true;
            }
            if ui.button("+ Bottom Text").clicked() {
                self.overlays.push(TextOverlay::bottom("BOTTOM TEXT"));
                self.selected = Some(self.overlays.len() - 1);
                changed = true;
            }
            if ui.button("+ Free Text").clicked() {
                self.overlays.push(TextOverlay::default());
                self.selected = Some(self.overlays.len() - 1);
                changed = true;
            }
        });

        if let Some(idx) = self.selected {
            if idx < self.overlays.len() {
                ui.separator();
                let overlay = &mut self.overlays[idx];

                ui.label("Text:");
                changed |= ui.text_edit_singleline(&mut overlay.text).changed();

                ui.add_space(4.0);
                ui.label("Font Size:");
                changed |= ui
                    .add(egui::Slider::new(&mut overlay.font_scale, 0.02..=0.25).text("scale"))
                    .changed();

                ui.add_space(4.0);
                ui.label("Outline:");
                changed |= ui
                    .add(egui::Slider::new(&mut overlay.outline_scale, 0.0..=0.2).text("thickness"))
                    .changed();

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    let mut color3 = [
                        overlay.color[0] as f32 / 255.0,
                        overlay.color[1] as f32 / 255.0,
                        overlay.color[2] as f32 / 255.0,
                    ];
                    if ui.color_edit_button_rgb(&mut color3).changed() {
                        overlay.color[0] = (color3[0] * 255.0) as u8;
                        overlay.color[1] = (color3[1] * 255.0) as u8;
                        overlay.color[2] = (color3[2] * 255.0) as u8;
                        changed = true;
                    }

                    ui.label("Outline:");
                    let mut outline3 = [
                        overlay.outline_color[0] as f32 / 255.0,
                        overlay.outline_color[1] as f32 / 255.0,
                        overlay.outline_color[2] as f32 / 255.0,
                    ];
                    if ui.color_edit_button_rgb(&mut outline3).changed() {
                        overlay.outline_color[0] = (outline3[0] * 255.0) as u8;
                        overlay.outline_color[1] = (outline3[1] * 255.0) as u8;
                        overlay.outline_color[2] = (outline3[2] * 255.0) as u8;
                        changed = true;
                    }
                });

                ui.add_space(4.0);
                if ui
                    .button("Delete")
                    .on_hover_text("Remove this text overlay")
                    .clicked()
                {
                    self.overlays.remove(idx);
                    self.selected = None;
                    changed = true;
                }
            }
        }

        changed
    }

    /// Flatten the overlays onto the source image using the provided font.
    ///
    /// `source` is the original image. Text is rasterized at the source resolution
    /// using `ab_glyph` for crisp output regardless of display scale.
    ///
    /// `font_data` should be the raw bytes of a TTF/OTF font file.
    pub fn flatten(&self, source: &image::RgbaImage, font_data: &[u8]) -> image::RgbaImage {
        use ab_glyph::{Font, FontArc, PxScale, ScaleFont};

        let mut canvas = source.clone();
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);

        let font = FontArc::try_from_vec(font_data.to_vec()).expect("Failed to parse font data");

        for overlay in &self.overlays {
            if overlay.text.is_empty() {
                continue;
            }

            let font_size = overlay.font_scale * h;
            let scale = PxScale::from(font_size);
            let scaled_font = font.as_scaled(scale);

            let text_width: f32 = overlay
                .text
                .chars()
                .map(|c| {
                    let glyph_id = font.glyph_id(c);
                    scaled_font.h_advance(glyph_id)
                })
                .sum();

            let x_start = (overlay.offset_x * w - text_width * 0.5).round();
            let y_start = (overlay.offset_y * h).round();

            let outline_px = (overlay.outline_scale * font_size).max(0.0) as i32;

            // Draw outline first (8-direction offset)
            if outline_px > 0 {
                for dx in [-outline_px, 0, outline_px] {
                    for dy in [-outline_px, 0, outline_px] {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        draw_text_onto(
                            &mut canvas,
                            &font,
                            scale,
                            &overlay.text,
                            x_start + dx as f32,
                            y_start + dy as f32,
                            overlay.outline_color,
                        );
                    }
                }
            }

            // Draw main text
            draw_text_onto(
                &mut canvas,
                &font,
                scale,
                &overlay.text,
                x_start,
                y_start,
                overlay.color,
            );
        }

        canvas
    }
}

/// Rasterize text onto an RGBA image at the given position.
fn draw_text_onto(
    canvas: &mut image::RgbaImage,
    font: &ab_glyph::FontArc,
    scale: ab_glyph::PxScale,
    text: &str,
    x: f32,
    y: f32,
    color: [u8; 4],
) {
    use ab_glyph::{Font, ScaleFont};

    let scaled = font.as_scaled(scale);
    let mut cursor_x = x;
    let (cw, ch) = (canvas.width() as i32, canvas.height() as i32);

    for c in text.chars() {
        let glyph_id = font.glyph_id(c);
        let glyph =
            glyph_id.with_scale_and_position(scale, ab_glyph::point(cursor_x, y + scaled.ascent()));

        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && px < cw && py < ch {
                    let alpha = (coverage * color[3] as f32) as u8;
                    if alpha > 0 {
                        let pixel = canvas.get_pixel_mut(px as u32, py as u32);
                        // Alpha blend
                        let a = alpha as f32 / 255.0;
                        let inv_a = 1.0 - a;
                        pixel[0] = (color[0] as f32 * a + pixel[0] as f32 * inv_a) as u8;
                        pixel[1] = (color[1] as f32 * a + pixel[1] as f32 * inv_a) as u8;
                        pixel[2] = (color[2] as f32 * a + pixel[2] as f32 * inv_a) as u8;
                        pixel[3] = (alpha as f32 + pixel[3] as f32 * inv_a).min(255.0) as u8;
                    }
                }
            });
        }

        cursor_x += scaled.h_advance(glyph_id);
    }
}
