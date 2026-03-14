//! Perspective Text story — demonstrates extracting galley mesh vertices
//! and applying geometric transforms (scale, skew, perspective) to text.
//!
//! This is the technique that makes the FlipCounter's 3D card flip work:
//! render text to a galley, clone the Arc, mutate vertex Y positions via
//! Arc::make_mut, then draw the modified galley as a TextShape.

use std::sync::Arc;

use egui::epaint::{Mesh, TextShape, Vertex};
use egui::{Color32, FontId, Pos2, Vec2};

use crate::{ACCENT, TEXT_MUTED};

const WHITE_UV: Pos2 = Pos2::new(0.0, 0.0);

pub struct PerspectiveTextState {
    pub flip_angle: f32,
    pub wave_amount: f32,
    pub scale_y: f32,
    pub animating: bool,
    pub phase: f32,
}

impl Default for PerspectiveTextState {
    fn default() -> Self {
        Self {
            flip_angle: 0.0,
            wave_amount: 0.0,
            scale_y: 1.0,
            animating: false,
            phase: 0.0,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut PerspectiveTextState) {
    let dt = ui.input(|i| i.stable_dt).min(0.1);

    if state.animating {
        state.phase += dt * 2.0;
        state.flip_angle = (state.phase.sin() * 0.5 + 0.5) * 90.0; // 0° → 90° oscillation
        state.wave_amount = (state.phase * 0.7).sin().abs() * 15.0;
        ui.ctx().request_repaint();
    }

    // --- 1. Y-Scale (vertical compression) ---
    ui.label(
        egui::RichText::new("1. Vertical Scale")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Galley mesh vertices scaled on Y axis toward center. \
             Simulates viewing text at an angle.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    ui.add(egui::Slider::new(&mut state.scale_y, 0.05..=1.0).text("Y Scale"));
    ui.add_space(4.0);

    let text = "HELLO WORLD";
    let font_size = 32.0;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(400.0, 50.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let galley = painter.layout_no_wrap(
        text.to_string(),
        FontId::new(font_size, egui::FontFamily::Monospace),
        Color32::from_rgb(220, 220, 235),
    );

    // Clone and transform vertices
    let mut modified = galley.clone();
    let galley_ref = Arc::make_mut(&mut modified);
    let galley_h = galley_ref.rect.height();
    let center_y = galley_h / 2.0;

    for placed_row in &mut galley_ref.rows {
        let row = Arc::make_mut(&mut placed_row.row);
        for vertex in &mut row.visuals.mesh.vertices {
            // Scale Y toward vertical center
            vertex.pos.y = center_y + (vertex.pos.y - center_y) * state.scale_y;
        }
    }

    let text_x = rect.left() + 10.0;
    let text_y = rect.center().y - (galley_h * state.scale_y) / 2.0;
    painter.add(egui::Shape::Text(TextShape::new(
        Pos2::new(text_x, text_y),
        modified,
        Color32::TRANSPARENT,
    )));

    ui.add_space(16.0);

    // --- 2. Wave distortion ---
    ui.label(
        egui::RichText::new("2. Wave Distortion")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Each vertex Y offset by sin(x). Demonstrates per-vertex manipulation \
             on actual glyph geometry.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    ui.add(egui::Slider::new(&mut state.wave_amount, 0.0..=20.0).text("Wave"));
    ui.add_space(4.0);

    let (rect, _) = ui.allocate_exact_size(Vec2::new(400.0, 60.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let galley = painter.layout_no_wrap(
        text.to_string(),
        FontId::new(font_size, egui::FontFamily::Monospace),
        Color32::from_rgb(68, 255, 180),
    );

    let mut modified = galley.clone();
    let galley_ref = Arc::make_mut(&mut modified);
    let galley_w = galley_ref.rect.width();

    for placed_row in &mut galley_ref.rows {
        let row = Arc::make_mut(&mut placed_row.row);
        for vertex in &mut row.visuals.mesh.vertices {
            let t = vertex.pos.x / galley_w;
            vertex.pos.y +=
                (t * std::f32::consts::TAU * 2.0 + state.phase).sin() * state.wave_amount;
        }
    }

    painter.add(egui::Shape::Text(TextShape::new(
        Pos2::new(rect.left() + 10.0, rect.top() + 15.0),
        modified,
        Color32::TRANSPARENT,
    )));

    ui.add_space(16.0);

    // --- 3. Perspective flip — unified card + text surface ---
    ui.label(
        egui::RichText::new("3. Perspective Flip (Unified Surface)")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Card and text rendered as a single surface. Galley mesh vertices \
             are extracted and bilinearly mapped into the card's trapezoid \
             coordinate space via raw Shape::mesh(). No TextShape — card and \
             glyphs share the same perspective transform.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.flip_angle, 0.0..=89.0).text("Angle"));
        if ui
            .button(if state.animating { "Pause" } else { "Animate" })
            .clicked()
        {
            state.animating = !state.animating;
        }
    });
    ui.add_space(4.0);

    let card_w = 350.0_f32;
    let card_h = 80.0_f32;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(card_w + 20.0, card_h + 20.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);

    let angle_rad = state.flip_angle.to_radians();
    let cos_a = angle_rad.cos();
    let pinch_frac = 0.12 * (1.0 - cos_a); // up to 12% pinch at far edge
    let shadow = (60.0 * (1.0 - cos_a)) as u8;

    // Card trapezoid corners — top edge pinched (tilted away), bottom edge full width (near)
    let card_left = rect.left() + 10.0;
    let card_top = rect.center().y - (card_h * cos_a) / 2.0;
    let card_bottom = rect.center().y + (card_h * cos_a) / 2.0;
    let pinch_px = card_w * pinch_frac;

    // The four corners of our trapezoid (TL, TR, BR, BL)
    let tl = Pos2::new(card_left + pinch_px, card_top);
    let tr = Pos2::new(card_left + card_w - pinch_px, card_top);
    let br = Pos2::new(card_left + card_w, card_bottom);
    let bl = Pos2::new(card_left, card_bottom);

    let card_color = Color32::from_rgb(
        45_u8.saturating_sub(shadow),
        45_u8.saturating_sub(shadow),
        65_u8.saturating_sub(shadow),
    );

    // Draw card background trapezoid (solid colour, uses WHITE_UV)
    let mut card_mesh = Mesh::default();
    card_mesh.vertices.extend_from_slice(&[
        Vertex {
            pos: tl,
            uv: WHITE_UV,
            color: card_color,
        },
        Vertex {
            pos: tr,
            uv: WHITE_UV,
            color: card_color,
        },
        Vertex {
            pos: br,
            uv: WHITE_UV,
            color: card_color,
        },
        Vertex {
            pos: bl,
            uv: WHITE_UV,
            color: card_color,
        },
    ]);
    card_mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(card_mesh));

    // Layout text to get glyph mesh with font atlas UVs
    let galley = painter.layout_no_wrap(
        "FLIPPING".to_string(),
        FontId::new(36.0, egui::FontFamily::Monospace),
        Color32::from_rgb(220, 220, 235),
    );

    // Extract galley mesh vertices and map into the card's trapezoid space.
    // Each vertex pos is galley-local; we normalise to (u, v) ∈ [0,1]×[0,1]
    // within the galley rect, then bilinearly interpolate into the trapezoid.
    //
    // IMPORTANT: row.visuals.mesh stores UVs as texel coordinates (not normalised).
    // The tessellator normally divides by font texture size — we must do the same.
    let g_rect = galley.rect;
    let font_tex_size = ui.ctx().fonts(|f| f.font_image_size());
    let uv_norm = Vec2::new(1.0 / font_tex_size[0] as f32, 1.0 / font_tex_size[1] as f32);

    // Text centred within the card with some padding
    let pad_x = 0.08; // 8% horizontal padding inside card
    let pad_y = 0.15; // 15% vertical padding inside card

    let mut text_mesh = Mesh::with_texture(egui::TextureId::default());

    for placed_row in &galley.rows {
        let row_offset = placed_row.pos;
        let row_mesh = &placed_row.row.visuals.mesh;

        let idx_offset = text_mesh.vertices.len() as u32;

        for vertex in &row_mesh.vertices {
            // Galley-local position (row vertices are relative to row origin)
            let galley_pos = Pos2::new(vertex.pos.x + row_offset.x, vertex.pos.y + row_offset.y);

            // Normalise position to [0,1] within galley bounding rect
            let u = if g_rect.width() > 0.0 {
                (galley_pos.x - g_rect.left()) / g_rect.width()
            } else {
                0.5
            };
            let v = if g_rect.height() > 0.0 {
                (galley_pos.y - g_rect.top()) / g_rect.height()
            } else {
                0.5
            };

            // Map normalised coords into card space with padding
            let card_u = pad_x + u * (1.0 - 2.0 * pad_x);
            let card_v = pad_y + v * (1.0 - 2.0 * pad_y);

            // Bilinear interpolation into trapezoid corners
            let screen_pos = bilinear(tl, tr, br, bl, card_u, card_v);

            // Normalise texel UVs to [0,1] range for the font atlas
            let normalised_uv = Pos2::new(vertex.uv.x * uv_norm.x, vertex.uv.y * uv_norm.y);

            text_mesh.vertices.push(Vertex {
                pos: screen_pos,
                uv: normalised_uv,
                color: vertex.color,
            });
        }

        for &idx in &row_mesh.indices {
            text_mesh.indices.push(idx + idx_offset);
        }
    }

    painter.add(egui::Shape::mesh(text_mesh));

    ui.add_space(16.0);

    // --- 4. Vertex colour tinting ---
    ui.label(
        egui::RichText::new("4. Vertex Colour Tinting")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Modifying vertex.color on text glyphs for gradient text effects. \
             GPU interpolates colour across each glyph's triangles.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    let (rect, _) = ui.allocate_exact_size(Vec2::new(400.0, 50.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let galley = painter.layout_no_wrap(
        "RAINBOW TEXT".to_string(),
        FontId::new(32.0, egui::FontFamily::Monospace),
        Color32::WHITE, // Base colour, will be overridden per vertex
    );

    let mut modified = galley.clone();
    let galley_ref = Arc::make_mut(&mut modified);
    let total_w = galley_ref.rect.width();

    for placed_row in &mut galley_ref.rows {
        let row = Arc::make_mut(&mut placed_row.row);
        for vertex in &mut row.visuals.mesh.vertices {
            let t = (vertex.pos.x / total_w).clamp(0.0, 1.0);
            let hue = ((t * 360.0 + state.phase * 60.0) % 360.0) as u16;
            vertex.color = hue_to_rgb(hue);
        }
    }

    painter.add(egui::Shape::Text(TextShape::new(
        Pos2::new(rect.left() + 10.0, rect.top() + 8.0),
        modified,
        Color32::TRANSPARENT,
    )));

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Key patterns:").color(ACCENT).strong());
    ui.label("- Galley mesh: galley.rows[i].row.visuals.mesh.vertices");
    ui.label("- Mutable access: Arc::make_mut(&mut galley).rows → Arc::make_mut(&mut row)");
    ui.label("- TextShape: modify vertices in-place, draw at a screen position");
    ui.label("- Raw mesh: extract vertices, map into arbitrary quad via bilinear()");
    ui.label("  Use Mesh::with_texture(TextureId::default()) to preserve font atlas UVs");
    ui.label("- Unified surface: card background (WHITE_UV) + text (font UVs) as separate meshes");
    ui.label("- Vertex colour can be modified for gradient/tint effects");
}

/// Bilinear interpolation within a quad defined by four corners.
///
/// `u` goes left→right (0 = left edge, 1 = right edge).
/// `v` goes top→bottom (0 = top edge, 1 = bottom edge).
/// Corners: `tl` (top-left), `tr` (top-right), `br` (bottom-right), `bl` (bottom-left).
fn bilinear(tl: Pos2, tr: Pos2, br: Pos2, bl: Pos2, u: f32, v: f32) -> Pos2 {
    // Top edge interpolation
    let top = Pos2::new(tl.x + (tr.x - tl.x) * u, tl.y + (tr.y - tl.y) * u);
    // Bottom edge interpolation
    let bot = Pos2::new(bl.x + (br.x - bl.x) * u, bl.y + (br.y - bl.y) * u);
    // Vertical interpolation
    Pos2::new(top.x + (bot.x - top.x) * v, top.y + (bot.y - top.y) * v)
}

/// Simple HSV→RGB (saturation=1, value=1).
fn hue_to_rgb(hue: u16) -> Color32 {
    let h = (hue % 360) as f32 / 60.0;
    let i = h.floor() as u8;
    let f = h - h.floor();
    let q = (255.0 * (1.0 - f)) as u8;
    let t = (255.0 * f) as u8;
    match i {
        0 => Color32::from_rgb(255, t, 0),
        1 => Color32::from_rgb(q, 255, 0),
        2 => Color32::from_rgb(0, 255, t),
        3 => Color32::from_rgb(0, q, 255),
        4 => Color32::from_rgb(t, 0, 255),
        _ => Color32::from_rgb(255, 0, q),
    }
}
