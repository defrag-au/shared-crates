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

    // --- 3. Perspective flip (like flip counter) ---
    ui.label(
        egui::RichText::new("3. Perspective Flip")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Text on a card that rotates around a horizontal axis. \
             Y vertices compressed by cos(angle), X pinched toward center. \
             This is the FlipCounter technique.",
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
    let pinch_frac = 0.12 * (1.0 - cos_a); // up to 12% pinch
    let shadow = (60.0 * (1.0 - cos_a)) as u8;

    // Draw the card as a trapezoid
    let card_left = rect.left() + 10.0;
    let card_top = rect.center().y - (card_h * cos_a) / 2.0;
    let card_bottom = rect.center().y + (card_h * cos_a) / 2.0;
    let pinch_px = card_w * pinch_frac;

    let card_color = Color32::from_rgb(
        45_u8.saturating_sub(shadow),
        45_u8.saturating_sub(shadow),
        65_u8.saturating_sub(shadow),
    );

    // Trapezoid: top edge pinched, bottom edge full width
    let mut mesh = Mesh::default();
    mesh.vertices.extend_from_slice(&[
        Vertex {
            pos: Pos2::new(card_left + pinch_px, card_top),
            uv: WHITE_UV,
            color: card_color,
        },
        Vertex {
            pos: Pos2::new(card_left + card_w - pinch_px, card_top),
            uv: WHITE_UV,
            color: card_color,
        },
        Vertex {
            pos: Pos2::new(card_left + card_w, card_bottom),
            uv: WHITE_UV,
            color: card_color,
        },
        Vertex {
            pos: Pos2::new(card_left, card_bottom),
            uv: WHITE_UV,
            color: card_color,
        },
    ]);
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));

    // Draw text with matching vertex transforms
    let galley = painter.layout_no_wrap(
        "FLIPPING".to_string(),
        FontId::new(36.0, egui::FontFamily::Monospace),
        Color32::from_rgb(220, 220, 235),
    );

    let mut modified = galley.clone();
    let galley_ref = Arc::make_mut(&mut modified);
    let g_w = galley_ref.rect.width();
    let g_h = galley_ref.rect.height();
    let g_center_y = g_h / 2.0;
    let g_center_x = g_w / 2.0;

    for placed_row in &mut galley_ref.rows {
        let row = Arc::make_mut(&mut placed_row.row);
        for vertex in &mut row.visuals.mesh.vertices {
            // Compress Y toward center
            vertex.pos.y = g_center_y + (vertex.pos.y - g_center_y) * cos_a;
            // Pinch X toward center proportional to distance from center
            let x_offset = vertex.pos.x - g_center_x;
            vertex.pos.x = g_center_x + x_offset * (1.0 - pinch_frac);
        }
    }

    // Position the transformed text centered on the card
    let text_x = card_left + (card_w - g_w * (1.0 - pinch_frac)) / 2.0;
    let text_y = rect.center().y - (g_h * cos_a) / 2.0;
    painter.add(egui::Shape::Text(TextShape::new(
        Pos2::new(text_x, text_y),
        modified,
        Color32::TRANSPARENT,
    )));

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
    ui.label("- Draw via: painter.add(Shape::Text(TextShape::new(pos, galley, fallback)))");
    ui.label("- Y-scale toward center simulates viewing angle (perspective)");
    ui.label("- X-pinch toward center adds convergence (vanishing point)");
    ui.label("- Vertex colour can be modified for gradient/tint effects");
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
