//! Perspective Text story — demonstrates extracting galley mesh vertices
//! and applying geometric transforms (scale, skew, perspective) to text.
//!
//! This is the technique that makes the FlipCounter's 3D card flip work:
//! render text to a galley, clone the Arc, mutate vertex Y positions via
//! Arc::make_mut, then draw the modified galley as a TextShape.

use std::sync::Arc;

use egui::epaint::{Mesh, TextShape, Vertex};
use egui::{Color32, FontId, Pos2, Rect, Vec2};

use crate::{ACCENT, TEXT_MUTED};

const WHITE_UV: Pos2 = Pos2::new(0.0, 0.0);

pub struct PerspectiveTextState {
    pub flip_angle: f32,
    pub wave_amount: f32,
    pub scale_y: f32,
    pub animating: bool,
    pub phase: f32,
    /// 0.0..1.0 progress for the split-flap demo
    pub flap_progress: f32,
    pub flap_animating: bool,
}

impl Default for PerspectiveTextState {
    fn default() -> Self {
        Self {
            flip_angle: 0.0,
            wave_amount: 0.0,
            scale_y: 1.0,
            animating: false,
            phase: 0.0,
            flap_progress: 0.0,
            flap_animating: false,
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

    ui.add_space(16.0);

    // --- 5. Split-Flap Card (flip counter simulation) ---
    ui.label(
        egui::RichText::new("5. Split-Flap Card")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Two-phase flip animation matching the FlipCounter widget. \
             Phase 1: top flap folds forward toward the viewer (hinge at middle). \
             Phase 2: bottom flap unfolds downward. Text on each flap is \
             bilinearly mapped into the perspective trapezoid.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    // Advance flap animation
    if state.flap_animating {
        state.flap_progress += dt * 1.5; // slower to observe the effect
        if state.flap_progress > 1.0 {
            state.flap_progress -= 1.0; // loop
        }
        ui.ctx().request_repaint();
    }

    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.flap_progress, 0.0..=1.0).text("Progress"));
        if ui
            .button(if state.flap_animating {
                "Pause"
            } else {
                "Animate"
            })
            .clicked()
        {
            state.flap_animating = !state.flap_animating;
        }
        if ui.button("Reset").clicked() {
            state.flap_progress = 0.0;
            state.flap_animating = false;
        }
    });
    ui.add_space(4.0);

    let card_w = 200.0_f32;
    let card_h = 120.0_f32;
    let half_h = card_h / 2.0;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(card_w + 40.0, card_h + 40.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);

    let card_left = rect.center().x - card_w / 2.0;
    let card_top = rect.center().y - card_h / 2.0;
    let hinge_y = rect.center().y;
    let card_bottom = rect.center().y + card_h / 2.0;

    let top_color = Color32::from_rgb(45, 45, 65);
    let bot_color = Color32::from_rgb(38, 38, 56);
    let divider_color = Color32::from_rgb(20, 20, 35);
    let border_color = Color32::from_rgb(60, 60, 80);
    let text_color = Color32::from_rgb(220, 220, 235);
    let old_char = '3';
    let new_char = '4';
    let flap_font_size = card_h * 0.55;
    let corner = 4.0;

    let top_rect = Rect::from_min_max(
        Pos2::new(card_left, card_top),
        Pos2::new(card_left + card_w, hinge_y),
    );
    let bot_rect = Rect::from_min_max(
        Pos2::new(card_left, hinge_y),
        Pos2::new(card_left + card_w, card_bottom),
    );
    let full_rect = Rect::from_min_max(
        Pos2::new(card_left, card_top),
        Pos2::new(card_left + card_w, card_bottom),
    );

    // Phase split at raw progress 0.5. Easing is applied within each phase.
    let p = state.flap_progress;

    // During phase 1, the flap casts a shadow on the bottom card.
    // Quadratic ramp so shadow kicks in later (past ~25% progress).
    let bot_shadow = if p < 0.5 {
        let t = p * 2.0; // 0→1 over phase 1
        (30.0 * t * t) as u8
    } else {
        let t = (1.0 - p) * 2.0; // 1→0 over phase 2
        (30.0 * t * t) as u8
    };
    let shadowed_bot = darken(bot_color, bot_shadow);

    let border_stroke = egui::Stroke::new(1.0, border_color);

    // Draw each half-card with its own rounded border
    painter.rect_filled(bot_rect, corner, shadowed_bot);
    painter.rect_stroke(bot_rect, corner, border_stroke, egui::StrokeKind::Inside);
    painter.rect_filled(top_rect, corner, top_color);
    painter.rect_stroke(top_rect, corner, border_stroke, egui::StrokeKind::Inside);

    // Font texture UV normalizer
    let font_tex_size = ui.ctx().fonts(|f| f.font_image_size());
    let uv_norm = Vec2::new(1.0 / font_tex_size[0] as f32, 1.0 / font_tex_size[1] as f32);

    if p < 0.5 {
        // Phase 1: top flap folds forward toward viewer
        // Base: old digit bottom, new digit top (revealed behind flap)
        draw_clipped_text(
            &painter,
            full_rect,
            bot_rect,
            flap_font_size,
            old_char,
            text_color,
        );
        draw_clipped_text(
            &painter,
            full_rect,
            top_rect,
            flap_font_size,
            new_char,
            text_color,
        );

        // Ease within phase: 0→1 over the first half of progress
        let phase_t = p * 2.0;
        let eased = 1.0 - (1.0 - phase_t) * (1.0 - phase_t);
        let angle = eased * std::f32::consts::FRAC_PI_2;
        let cos_a = angle.cos();
        let flip_h = (half_h * cos_a).round();

        if flip_h >= 1.0 {
            let top_y = hinge_y - flip_h;
            let pinch = card_w * 0.06 * (1.0 - cos_a);
            let shadow = (40.0 * (1.0 - cos_a)) as u8;
            let flap_color = darken(top_color, shadow);

            // Trapezoid: top flap folding toward viewer.
            // Top edge (near, coming toward us) expands outward.
            // Hinge edge (far, pivot point) stays at card width.
            let corners = [
                Pos2::new(card_left - pinch, top_y), // TL (near, expanded)
                Pos2::new(card_left + card_w + pinch, top_y), // TR (near, expanded)
                Pos2::new(card_left + card_w, hinge_y), // BR (far, original)
                Pos2::new(card_left, hinge_y),       // BL (far, original)
            ];

            draw_card_with_text(
                &painter,
                corners,
                flap_color,
                full_rect,
                0.0,
                0.5,
                flap_font_size,
                old_char,
                text_color,
                &uv_norm,
            );
            stroke_trapezoid(&painter, &corners, border_stroke);
        }
    } else {
        // Phase 2: bottom flap unfolds downward from hinge
        // Base: old digit bottom, new digit top
        draw_clipped_text(
            &painter,
            full_rect,
            bot_rect,
            flap_font_size,
            old_char,
            text_color,
        );
        draw_clipped_text(
            &painter,
            full_rect,
            top_rect,
            flap_font_size,
            new_char,
            text_color,
        );

        // Ease within phase: 0→1 over the second half of progress
        let phase_t = (p - 0.5) * 2.0;
        let eased = 1.0 - (1.0 - phase_t) * (1.0 - phase_t);
        let angle = eased * std::f32::consts::FRAC_PI_2;
        let cos_a = angle.sin(); // 0→1
        let flip_h = (half_h * cos_a).round();

        if flip_h >= 1.0 {
            let bot_y = hinge_y + flip_h;
            let pinch = card_w * 0.06 * (1.0 - cos_a);
            let shadow = (40.0 * (1.0 - cos_a)) as u8;
            let flap_color = darken(bot_color, shadow);

            // Trapezoid: bottom flap unfolding toward viewer.
            // Hinge edge (far, pivot point) stays at card width.
            // Bottom edge (near, coming toward us) expands outward.
            let corners = [
                Pos2::new(card_left, hinge_y),                // TL (far, original)
                Pos2::new(card_left + card_w, hinge_y),       // TR (far, original)
                Pos2::new(card_left + card_w + pinch, bot_y), // BR (near, expanded)
                Pos2::new(card_left - pinch, bot_y),          // BL (near, expanded)
            ];

            draw_card_with_text(
                &painter,
                corners,
                flap_color,
                full_rect,
                0.5,
                1.0,
                flap_font_size,
                new_char,
                text_color,
                &uv_norm,
            );
            stroke_trapezoid(&painter, &corners, border_stroke);
        }
    }

    // Divider line at hinge
    painter.line_segment(
        [
            Pos2::new(card_left, hinge_y),
            Pos2::new(card_left + card_w, hinge_y),
        ],
        egui::Stroke::new(1.5, divider_color),
    );

    // Phase indicator
    let phase_text = if p < 0.5 {
        let phase_t = p * 2.0;
        let eased = 1.0 - (1.0 - phase_t) * (1.0 - phase_t);
        format!("Phase 1: top flap folding ({:.0}°)", eased * 90.0)
    } else {
        let phase_t = (p - 0.5) * 2.0;
        let eased = 1.0 - (1.0 - phase_t) * (1.0 - phase_t);
        format!("Phase 2: bottom flap unfolding ({:.0}°)", eased * 90.0)
    };
    ui.label(egui::RichText::new(phase_text).color(TEXT_MUTED).small());

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

/// Stroke the outline of a trapezoid defined by 4 corners [TL, TR, BR, BL].
fn stroke_trapezoid(painter: &egui::Painter, corners: &[Pos2; 4], stroke: egui::Stroke) {
    for i in 0..4 {
        painter.line_segment([corners[i], corners[(i + 1) % 4]], stroke);
    }
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

/// Darken a colour by subtracting `amount` from each RGB channel.
fn darken(c: Color32, amount: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        c.r().saturating_sub(amount),
        c.g().saturating_sub(amount),
        c.b().saturating_sub(amount),
        c.a(),
    )
}

/// Draw a character centered in `full_rect`, clipped to `clip_rect`.
fn draw_clipped_text(
    painter: &egui::Painter,
    full_rect: Rect,
    clip_rect: Rect,
    font_size: f32,
    ch: char,
    color: Color32,
) {
    let galley = painter.layout_no_wrap(
        ch.to_string(),
        FontId::new(font_size, egui::FontFamily::Monospace),
        color,
    );
    let text_pos = Pos2::new(
        full_rect.center().x - galley.size().x / 2.0,
        full_rect.center().y - galley.size().y / 2.0,
    );
    let clipped = painter.clip_rect().intersect(clip_rect);
    if clipped.is_positive() {
        let sub = painter.with_clip_rect(clipped);
        sub.galley(text_pos, galley, Color32::TRANSPARENT);
    }
}

/// Draw a trapezoid flap with text properly clipped to its half of the card.
///
/// Text is laid out centered in `full_rect` (the whole card). Glyph quads that
/// cross the hinge (`v_cut`) are split into two sub-quads with interpolated UVs.
/// Only the portion within `v_start..v_end` is kept, then bilinearly mapped into
/// the trapezoid `corners`.
///
/// - Top half: v_start=0.0, v_end=0.5 (v_cut=0.5)
/// - Bottom half: v_start=0.5, v_end=1.0 (v_cut=0.5)
#[allow(clippy::too_many_arguments)]
fn draw_card_with_text(
    painter: &egui::Painter,
    corners: [Pos2; 4],
    card_color: Color32,
    full_rect: Rect,
    v_start: f32,
    v_end: f32,
    font_size: f32,
    ch: char,
    text_color: Color32,
    uv_norm: &Vec2,
) {
    // Draw card background
    let mut card_mesh = Mesh::default();
    for &pos in &corners {
        card_mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color: card_color,
        });
    }
    card_mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(card_mesh));

    // Layout text centered in the full card
    let galley = painter.layout_no_wrap(
        ch.to_string(),
        FontId::new(font_size, egui::FontFamily::Monospace),
        text_color,
    );

    let text_origin = Pos2::new(
        full_rect.center().x - galley.size().x / 2.0,
        full_rect.center().y - galley.size().y / 2.0,
    );

    let full_w = full_rect.width();
    let full_h = full_rect.height();
    let v_range = v_end - v_start;
    let [tl, tr, br, bl] = corners;

    let mut text_mesh = Mesh::with_texture(egui::TextureId::default());

    // Process each glyph quad (4 vertices, 2 triangles) individually.
    // Glyphs are axis-aligned quads: TL, TR, BL, BR.
    for placed_row in &galley.rows {
        let row_offset = placed_row.pos;
        let row_mesh = &placed_row.row.visuals.mesh;

        // Each glyph is 4 vertices + 6 indices (2 triangles).
        // Process in chunks of 4 vertices / 6 indices.
        let num_quads = row_mesh.vertices.len() / 4;
        for q in 0..num_quads {
            let vi = q * 4;
            let verts: Vec<_> = (0..4)
                .map(|i| {
                    let v = &row_mesh.vertices[vi + i];
                    let abs_x = text_origin.x + row_offset.x + v.pos.x;
                    let abs_y = text_origin.y + row_offset.y + v.pos.y;
                    let full_u = if full_w > 0.0 {
                        (abs_x - full_rect.left()) / full_w
                    } else {
                        0.5
                    };
                    let full_v = if full_h > 0.0 {
                        (abs_y - full_rect.top()) / full_h
                    } else {
                        0.5
                    };
                    let norm_uv = Pos2::new(v.uv.x * uv_norm.x, v.uv.y * uv_norm.y);
                    (full_u, full_v, norm_uv, v.color)
                })
                .collect();

            // Glyph quad vertices are: 0=TL, 1=TR, 2=BL, 3=BR
            let v_top = verts[0].1; // top edge full_v
            let v_bot = verts[2].1; // bottom edge full_v

            // Skip quads entirely outside our half
            if v_bot <= v_start || v_top >= v_end {
                continue;
            }

            // Determine the effective top/bottom within our half
            let eff_top = v_top.max(v_start);
            let eff_bot = v_bot.min(v_end);

            // Interpolation factor for clipping within the quad
            let quad_v_range = v_bot - v_top;
            let t_top = if quad_v_range > 0.0 {
                (eff_top - v_top) / quad_v_range
            } else {
                0.0
            };
            let t_bot = if quad_v_range > 0.0 {
                (eff_bot - v_top) / quad_v_range
            } else {
                1.0
            };

            // Interpolate the 4 corners of the clipped quad
            // Original: TL(0), TR(1), BL(2), BR(3)
            // Clipped TL = lerp(TL, BL, t_top), clipped TR = lerp(TR, BR, t_top)
            // Clipped BL = lerp(TL, BL, t_bot), clipped BR = lerp(TR, BR, t_bot)
            let lerp_vert =
                |top: &(f32, f32, Pos2, Color32), bot: &(f32, f32, Pos2, Color32), t: f32| {
                    let fu = top.0 + (bot.0 - top.0) * t;
                    let fv = top.1 + (bot.1 - top.1) * t;
                    let uv = Pos2::new(
                        top.2.x + (bot.2.x - top.2.x) * t,
                        top.2.y + (bot.2.y - top.2.y) * t,
                    );
                    (fu, fv, uv, top.3) // keep top colour
                };

            let ctl = lerp_vert(&verts[0], &verts[2], t_top);
            let ctr = lerp_vert(&verts[1], &verts[3], t_top);
            let cbl = lerp_vert(&verts[0], &verts[2], t_bot);
            let cbr = lerp_vert(&verts[1], &verts[3], t_bot);

            // Map each clipped vertex into the flap's trapezoid
            let idx = text_mesh.vertices.len() as u32;
            for cv in [&ctl, &ctr, &cbl, &cbr] {
                let flap_u = cv.0.clamp(0.0, 1.0);
                let flap_v = ((cv.1 - v_start) / v_range).clamp(0.0, 1.0);
                let screen_pos = bilinear(tl, tr, br, bl, flap_u, flap_v);
                text_mesh.vertices.push(Vertex {
                    pos: screen_pos,
                    uv: cv.2,
                    color: cv.3,
                });
            }
            text_mesh.indices.extend_from_slice(&[
                idx,
                idx + 1,
                idx + 2,
                idx + 2,
                idx + 1,
                idx + 3,
            ]);
        }
    }

    painter.add(egui::Shape::mesh(text_mesh));
}
