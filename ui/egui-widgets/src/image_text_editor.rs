//! Image text overlay editor — place, style, and drag text on top of an image.
//!
//! The editor renders a base image with movable text overlays. Users can add text,
//! select fonts, apply effects (outline, shadow), adjust letter spacing, and drag
//! to position. Corner handles allow interactive resizing. The final composite can
//! be flattened to an `image::RgbaImage` for saving/uploading.
//!
//! Feature-gated behind `image-editor`.

use egui::{
    pos2, Color32, CursorIcon, FontFamily, FontId, Rect, Response, Sense, Stroke, TextureHandle,
    Ui, Vec2,
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

/// Available font choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontChoice {
    /// Default egui proportional font.
    Default,
    /// Bold monospace (system).
    Monospace,
    /// Custom font slot 0 (caller registers via [`ImageTextEditor::set_custom_font`]).
    Custom0,
    /// Custom font slot 1.
    Custom1,
    /// Custom font slot 2.
    Custom2,
}

impl FontChoice {
    pub const ALL: &[Self] = &[
        Self::Default,
        Self::Monospace,
        Self::Custom0,
        Self::Custom1,
        Self::Custom2,
    ];

    pub fn label<'a>(&self, editor: &'a ImageTextEditor) -> &'a str {
        match self {
            Self::Default => "Sans Serif",
            Self::Monospace => "Monospace",
            Self::Custom0 => editor.custom_font_names[0].as_deref().unwrap_or("Custom 1"),
            Self::Custom1 => editor.custom_font_names[1].as_deref().unwrap_or("Custom 2"),
            Self::Custom2 => editor.custom_font_names[2].as_deref().unwrap_or("Custom 3"),
        }
    }

    fn family(&self) -> FontFamily {
        match self {
            Self::Default | Self::Custom0 | Self::Custom1 | Self::Custom2 => {
                FontFamily::Proportional
            }
            Self::Monospace => FontFamily::Monospace,
        }
    }

    /// Custom font slot index (0, 1, 2) or None for built-in fonts.
    pub fn custom_slot(&self) -> Option<usize> {
        match self {
            Self::Custom0 => Some(0),
            Self::Custom1 => Some(1),
            Self::Custom2 => Some(2),
            _ => None,
        }
    }
}

/// Text effect applied to an overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEffect {
    /// No effect — plain text.
    None,
    /// Stroke outline around each character.
    Outline,
    /// Drop shadow behind text.
    Shadow,
    /// Both outline and shadow.
    OutlineAndShadow,
}

impl TextEffect {
    pub const ALL: &[Self] = &[
        Self::None,
        Self::Outline,
        Self::Shadow,
        Self::OutlineAndShadow,
    ];

    pub fn label(&self) -> &str {
        match self {
            Self::None => "None",
            Self::Outline => "Outline",
            Self::Shadow => "Shadow",
            Self::OutlineAndShadow => "Outline + Shadow",
        }
    }

    pub fn has_outline(&self) -> bool {
        matches!(self, Self::Outline | Self::OutlineAndShadow)
    }

    pub fn has_shadow(&self) -> bool {
        matches!(self, Self::Shadow | Self::OutlineAndShadow)
    }
}

/// A single text overlay on the image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextOverlay {
    pub text: String,
    /// Font size relative to image height (0.0..1.0). 0.08 = 8% of image height.
    pub font_scale: f32,
    /// Selected font.
    pub font: FontChoice,
    /// Text color.
    pub color: [u8; 4],
    /// Effect type.
    pub effect: TextEffect,
    /// Outline/stroke color.
    pub outline_color: [u8; 4],
    /// Outline thickness relative to font size (0.0 = no outline, 0.1 = 10% of font size).
    pub outline_scale: f32,
    /// Shadow color.
    pub shadow_color: [u8; 4],
    /// Shadow offset relative to font size.
    pub shadow_offset: [f32; 2],
    /// Extra letter spacing in pixels (relative to image height, like font_scale).
    pub letter_spacing: f32,
    /// Horizontal position (0.0 = left, 0.5 = center, 1.0 = right).
    pub offset_x: f32,
    /// Vertical position (0.0 = top, 1.0 = bottom).
    pub offset_y: f32,
    /// Positioning anchor.
    pub anchor: TextOverlayAnchor,
}

impl Default for TextOverlay {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_scale: 0.08,
            font: FontChoice::Default,
            color: [255, 255, 255, 255],
            effect: TextEffect::Outline,
            outline_color: [0, 0, 0, 255],
            outline_scale: 0.08,
            shadow_color: [0, 0, 0, 180],
            shadow_offset: [0.04, 0.04],
            letter_spacing: 0.0,
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

// ─── Drag mode ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum DragMode {
    /// Dragging the body — moves position.
    Move(usize),
    /// Dragging a corner handle — resizes font.
    Resize(usize),
}

// ─── Editor ─────────────────────────────────────────────────────────────────

/// State for the image text editor.
pub struct ImageTextEditor {
    /// Text overlays on the image.
    pub overlays: Vec<TextOverlay>,
    /// Index of the currently selected overlay (for editing).
    pub selected: Option<usize>,
    /// Current drag interaction.
    drag_mode: Option<DragMode>,
    /// Custom font names for display in the font selector.
    custom_font_names: [Option<String>; 3],
    /// Custom font data for flatten — caller provides via [`set_custom_font`].
    custom_font_data: [Option<Vec<u8>>; 3],
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
            drag_mode: None,
            custom_font_names: [None, None, None],
            custom_font_data: [None, None, None],
        }
    }

    /// Register a custom font for use in the editor.
    ///
    /// `slot` is 0, 1, or 2. `name` is the display label. `data` is the TTF/OTF bytes.
    /// Also registers the font with egui for preview rendering.
    pub fn set_custom_font(
        &mut self,
        ctx: &egui::Context,
        slot: usize,
        name: impl Into<String>,
        data: Vec<u8>,
    ) {
        assert!(slot < 3, "Custom font slot must be 0, 1, or 2");
        let name = name.into();

        // Register with egui for live preview
        let family_name = format!("custom_{slot}");
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            family_name.clone(),
            egui::FontData::from_owned(data.clone()).into(),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push(family_name);
        ctx.set_fonts(fonts);

        self.custom_font_names[slot] = Some(name);
        self.custom_font_data[slot] = Some(data);
    }

    /// Show the editor canvas. `texture` is the base image, `available_size` limits display.
    pub fn show(&mut self, ui: &mut Ui, texture: &TextureHandle, available_size: Vec2) -> Response {
        let tex_size = texture.size_vec2();
        let scale = (available_size.x / tex_size.x).min(available_size.y / tex_size.y);
        let display_size = tex_size * scale;

        let (response, painter) = ui.allocate_painter(display_size, Sense::click_and_drag());
        let image_rect = response.rect;

        // Draw the base image
        painter.image(
            texture.id(),
            image_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        // Click on background deselects
        if response.clicked() {
            self.selected = None;
        }

        let id_base = ui.id().with("text_overlay");
        let handle_radius = 5.0_f32;

        for (i, overlay) in self.overlays.iter_mut().enumerate() {
            if overlay.text.is_empty() {
                continue;
            }

            let is_selected = self.selected == Some(i);
            let font_size = overlay.font_scale * image_rect.height();
            let font_id = FontId::new(font_size, overlay.font.family());
            let extra_spacing = overlay.letter_spacing * image_rect.height();

            // Measure text width with letter spacing
            let text_width = measure_text_width(ui, &overlay.text, &font_id, extra_spacing);
            let text_pos = pos2(
                image_rect.left() + overlay.offset_x * image_rect.width(),
                image_rect.top() + overlay.offset_y * image_rect.height(),
            );
            let text_rect = Rect::from_min_size(
                pos2(text_pos.x - text_width * 0.5, text_pos.y),
                egui::vec2(text_width, font_size * 1.2),
            );

            let text_color = Color32::from_rgba_unmultiplied(
                overlay.color[0],
                overlay.color[1],
                overlay.color[2],
                overlay.color[3],
            );

            // ── Shadow ──
            if overlay.effect.has_shadow() {
                let shadow_dx = overlay.shadow_offset[0] * font_size;
                let shadow_dy = overlay.shadow_offset[1] * font_size;
                let shadow_color = Color32::from_rgba_unmultiplied(
                    overlay.shadow_color[0],
                    overlay.shadow_color[1],
                    overlay.shadow_color[2],
                    overlay.shadow_color[3],
                );
                draw_spaced_text(
                    &painter,
                    &overlay.text,
                    &font_id,
                    extra_spacing,
                    pos2(text_rect.left() + shadow_dx, text_rect.top() + shadow_dy),
                    shadow_color,
                );
            }

            // ── Outline ──
            if overlay.effect.has_outline() && overlay.outline_scale > 0.0 {
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
                        draw_spaced_text(
                            &painter,
                            &overlay.text,
                            &font_id,
                            extra_spacing,
                            pos2(text_rect.left() + dx, text_rect.top() + dy),
                            outline_color,
                        );
                    }
                }
            }

            // ── Main text ──
            draw_spaced_text(
                &painter,
                &overlay.text,
                &font_id,
                extra_spacing,
                text_rect.min,
                text_color,
            );

            // ── Selection UI ──
            if is_selected {
                painter.rect_stroke(
                    text_rect.expand(2.0),
                    2.0,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(100, 180, 255, 180)),
                    egui::StrokeKind::Outside,
                );

                // Corner resize handles — all four are interactive
                let hit_size = 28.0; // generous hit target
                let corners = [
                    text_rect.left_top(),
                    text_rect.right_top(),
                    text_rect.left_bottom(),
                    text_rect.right_bottom(),
                ];
                for (ci, corner) in corners.iter().enumerate() {
                    painter.circle_filled(*corner, handle_radius, Color32::from_rgb(100, 180, 255));
                    painter.circle_stroke(*corner, handle_radius, Stroke::new(1.0, Color32::WHITE));

                    let handle_rect =
                        Rect::from_center_size(*corner, egui::vec2(hit_size, hit_size));
                    let handle_id = id_base.with(("resize", i, ci));
                    let handle_resp = ui.interact(handle_rect, handle_id, Sense::drag());

                    if handle_resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
                    }

                    if handle_resp.drag_started() {
                        self.drag_mode = Some(DragMode::Resize(i));
                    }

                    if handle_resp.dragged()
                        && matches!(self.drag_mode, Some(DragMode::Resize(idx)) if idx == i)
                    {
                        // Use vertical drag distance for resize; bottom corners
                        // drag down = bigger, top corners drag up = bigger.
                        let sign = if ci < 2 { -1.0 } else { 1.0 };
                        let delta_y = handle_resp.drag_delta().y * sign;
                        overlay.font_scale += delta_y / image_rect.height();
                        overlay.font_scale = overlay.font_scale.clamp(0.02, 0.4);
                    }

                    if handle_resp.drag_stopped()
                        && matches!(self.drag_mode, Some(DragMode::Resize(idx)) if idx == i)
                    {
                        self.drag_mode = None;
                    }
                }
            }

            // ── Body drag (move) ──
            let drag_id = id_base.with(("move", i));
            let drag_response =
                ui.interact(text_rect.expand(4.0), drag_id, Sense::click_and_drag());

            if drag_response.clicked() {
                self.selected = Some(i);
            }

            if drag_response.hovered() && !matches!(self.drag_mode, Some(DragMode::Resize(_))) {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            }

            if drag_response.drag_started() {
                self.drag_mode = Some(DragMode::Move(i));
                self.selected = Some(i);
            }

            if drag_response.dragged()
                && matches!(self.drag_mode, Some(DragMode::Move(idx)) if idx == i)
            {
                let delta = drag_response.drag_delta();
                overlay.offset_x += delta.x / image_rect.width();
                overlay.offset_y += delta.y / image_rect.height();
                overlay.offset_x = overlay.offset_x.clamp(0.0, 1.0);
                overlay.offset_y = overlay.offset_y.clamp(0.0, 1.0);
                overlay.anchor = TextOverlayAnchor::Free;
            }

            if drag_response.drag_stopped()
                && matches!(self.drag_mode, Some(DragMode::Move(idx)) if idx == i)
            {
                self.drag_mode = None;
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
                self.overlays.push(TextOverlay {
                    text: "TEXT".into(),
                    ..Default::default()
                });
                self.selected = Some(self.overlays.len() - 1);
                changed = true;
            }
        });

        let Some(idx) = self.selected else {
            return changed;
        };
        if idx >= self.overlays.len() {
            return changed;
        }

        ui.separator();

        // Need to snapshot custom_font_names for the closure
        let font_labels: Vec<(FontChoice, String)> = FontChoice::ALL
            .iter()
            .filter(|f| {
                // Only show custom fonts that have been registered
                f.custom_slot()
                    .map(|s| self.custom_font_names[s].is_some())
                    .unwrap_or(true)
            })
            .map(|f| (*f, f.label(self).to_owned()))
            .collect();

        let overlay = &mut self.overlays[idx];

        // ── Text input ──
        ui.label("Text:");
        changed |= ui.text_edit_singleline(&mut overlay.text).changed();

        // ── Font selector ──
        ui.add_space(4.0);
        ui.label("Font:");
        egui::ComboBox::from_id_salt(ui.id().with("font_select"))
            .selected_text(
                font_labels
                    .iter()
                    .find(|(f, _)| *f == overlay.font)
                    .map(|(_, l)| l.as_str())
                    .unwrap_or("Unknown"),
            )
            .show_ui(ui, |ui| {
                for (font_choice, label) in &font_labels {
                    if ui
                        .selectable_value(&mut overlay.font, *font_choice, label)
                        .changed()
                    {
                        changed = true;
                    }
                }
            });

        // ── Font size ──
        ui.add_space(4.0);
        ui.label("Font Size:");
        changed |= ui
            .add(egui::Slider::new(&mut overlay.font_scale, 0.02..=0.4).text("scale"))
            .changed();

        // ── Letter spacing ──
        ui.add_space(4.0);
        ui.label("Letter Spacing:");
        changed |= ui
            .add(
                egui::Slider::new(&mut overlay.letter_spacing, -0.02..=0.1)
                    .text("spacing")
                    .fixed_decimals(3),
            )
            .changed();

        // ── Effect selector ──
        ui.add_space(4.0);
        ui.label("Effect:");
        egui::ComboBox::from_id_salt(ui.id().with("effect_select"))
            .selected_text(overlay.effect.label())
            .show_ui(ui, |ui| {
                for effect in TextEffect::ALL {
                    if ui
                        .selectable_value(&mut overlay.effect, *effect, effect.label())
                        .changed()
                    {
                        changed = true;
                    }
                }
            });

        // ── Outline controls (if effect uses outline) ──
        if overlay.effect.has_outline() {
            ui.add_space(4.0);
            ui.label("Outline Thickness:");
            changed |= ui
                .add(egui::Slider::new(&mut overlay.outline_scale, 0.01..=0.2).text("px"))
                .changed();
        }

        // ── Shadow controls (if effect uses shadow) ──
        if overlay.effect.has_shadow() {
            ui.add_space(4.0);
            ui.label("Shadow Offset:");
            ui.horizontal(|ui| {
                ui.label("X:");
                changed |= ui
                    .add(egui::DragValue::new(&mut overlay.shadow_offset[0]).speed(0.005))
                    .changed();
                ui.label("Y:");
                changed |= ui
                    .add(egui::DragValue::new(&mut overlay.shadow_offset[1]).speed(0.005))
                    .changed();
            });
        }

        // ── Colors ──
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Color:");
            let mut c = color_to_rgb(overlay.color);
            if ui.color_edit_button_rgb(&mut c).changed() {
                overlay.color = rgb_to_color(c, overlay.color[3]);
                changed = true;
            }

            if overlay.effect.has_outline() {
                ui.label("Outline:");
                let mut o = color_to_rgb(overlay.outline_color);
                if ui.color_edit_button_rgb(&mut o).changed() {
                    overlay.outline_color = rgb_to_color(o, overlay.outline_color[3]);
                    changed = true;
                }
            }

            if overlay.effect.has_shadow() {
                ui.label("Shadow:");
                let mut s = color_to_rgb(overlay.shadow_color);
                if ui.color_edit_button_rgb(&mut s).changed() {
                    overlay.shadow_color = rgb_to_color(s, overlay.shadow_color[3]);
                    changed = true;
                }
            }
        });

        // ── Delete ──
        ui.add_space(8.0);
        if ui
            .button("Delete")
            .on_hover_text("Remove this text overlay")
            .clicked()
        {
            self.overlays.remove(idx);
            self.selected = None;
            changed = true;
        }

        changed
    }

    /// Flatten the overlays onto the source image using the provided font.
    ///
    /// `source` is the original image. `font_data` is the raw TTF/OTF bytes for the
    /// default font. Custom fonts use their registered data.
    pub fn flatten(&self, source: &image::RgbaImage, font_data: &[u8]) -> image::RgbaImage {
        use ab_glyph::{FontArc, PxScale};

        let mut canvas = source.clone();
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);

        let default_font =
            FontArc::try_from_vec(font_data.to_vec()).expect("Failed to parse default font");

        for overlay in &self.overlays {
            if overlay.text.is_empty() {
                continue;
            }

            // Select font
            let font = if let Some(slot) = overlay.font.custom_slot() {
                self.custom_font_data[slot]
                    .as_ref()
                    .map(|d| FontArc::try_from_vec(d.clone()).expect("Failed to parse custom font"))
                    .unwrap_or_else(|| default_font.clone())
            } else {
                default_font.clone()
            };

            let font_size = overlay.font_scale * h;
            let scale = PxScale::from(font_size);
            let extra_spacing = overlay.letter_spacing * h;

            let text_width = measure_text_width_ab(&font, scale, &overlay.text, extra_spacing);
            let x_start = (overlay.offset_x * w - text_width * 0.5).round();
            let y_start = (overlay.offset_y * h).round();

            // Shadow
            if overlay.effect.has_shadow() {
                let sx = x_start + overlay.shadow_offset[0] * font_size;
                let sy = y_start + overlay.shadow_offset[1] * font_size;
                draw_text_onto(
                    &mut canvas,
                    &font,
                    scale,
                    &overlay.text,
                    sx,
                    sy,
                    overlay.shadow_color,
                    extra_spacing,
                );
            }

            // Outline
            if overlay.effect.has_outline() {
                let outline_px = (overlay.outline_scale * font_size).max(0.0) as i32;
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
                                extra_spacing,
                            );
                        }
                    }
                }
            }

            // Main text
            draw_text_onto(
                &mut canvas,
                &font,
                scale,
                &overlay.text,
                x_start,
                y_start,
                overlay.color,
                extra_spacing,
            );
        }

        canvas
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn color_to_rgb(c: [u8; 4]) -> [f32; 3] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    ]
}

fn rgb_to_color(rgb: [f32; 3], alpha: u8) -> [u8; 4] {
    [
        (rgb[0] * 255.0) as u8,
        (rgb[1] * 255.0) as u8,
        (rgb[2] * 255.0) as u8,
        alpha,
    ]
}

/// Measure text width in egui with extra letter spacing.
fn measure_text_width(ui: &Ui, text: &str, font_id: &FontId, extra_spacing: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }

    if extra_spacing.abs() < 0.01 {
        // Fast path: measure whole string at once
        let galley = ui
            .painter()
            .layout_no_wrap(text.to_string(), font_id.clone(), Color32::WHITE);
        return galley.size().x;
    }

    // Measure each character individually for spacing
    let mut total = 0.0_f32;
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let galley = ui
            .painter()
            .layout_no_wrap(c.to_string(), font_id.clone(), Color32::WHITE);
        total += galley.size().x;
        if i < chars.len() - 1 {
            total += extra_spacing;
        }
    }
    total
}

/// Draw text with extra letter spacing using egui painter.
fn draw_spaced_text(
    painter: &egui::Painter,
    text: &str,
    font_id: &FontId,
    extra_spacing: f32,
    start: egui::Pos2,
    color: Color32,
) {
    if extra_spacing.abs() < 0.01 {
        // Fast path: no spacing adjustment
        let galley = painter.layout_no_wrap(text.to_string(), font_id.clone(), color);
        painter.galley(start, galley, Color32::TRANSPARENT);
        return;
    }

    let mut cursor_x = start.x;
    for c in text.chars() {
        let galley = painter.layout_no_wrap(c.to_string(), font_id.clone(), color);
        let char_width = galley.size().x;
        painter.galley(pos2(cursor_x, start.y), galley, Color32::TRANSPARENT);
        cursor_x += char_width + extra_spacing;
    }
}

/// Measure text width using ab_glyph (for flatten).
fn measure_text_width_ab(
    font: &ab_glyph::FontArc,
    scale: ab_glyph::PxScale,
    text: &str,
    extra_spacing: f32,
) -> f32 {
    use ab_glyph::{Font, ScaleFont};
    let scaled = font.as_scaled(scale);
    let chars: Vec<char> = text.chars().collect();
    let advance_sum: f32 = chars
        .iter()
        .map(|c| scaled.h_advance(font.glyph_id(*c)))
        .sum();
    advance_sum + extra_spacing * chars.len().saturating_sub(1) as f32
}

/// Rasterize text onto an RGBA image with letter spacing.
#[allow(clippy::too_many_arguments)]
fn draw_text_onto(
    canvas: &mut image::RgbaImage,
    font: &ab_glyph::FontArc,
    scale: ab_glyph::PxScale,
    text: &str,
    x: f32,
    y: f32,
    color: [u8; 4],
    extra_spacing: f32,
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

        cursor_x += scaled.h_advance(glyph_id) + extra_spacing;
    }
}
