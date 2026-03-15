//! FlipCounter — split-flap style animated digit counter.
//!
//! Each digit is rendered as two cards (top half, bottom half) with the same
//! text drawn in both, clipped to show only the relevant half. When a digit
//! changes, a flap rotates forward around the hinge line: the old top half
//! folds down (phase 1), then the new bottom half unfolds (phase 2).
//!
//! The flap is drawn as a trapezoid mesh to simulate perspective foreshortening,
//! with a subtle shadow that darkens as the flap rotates away from the viewer.

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Rect, Ui, Vec2};

use crate::theme;

/// Darken a colour by subtracting `amount` from each RGB channel.
fn darken(c: Color32, amount: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        c.r().saturating_sub(amount),
        c.g().saturating_sub(amount),
        c.b().saturating_sub(amount),
        c.a(),
    )
}

/// State for a single flipping digit.
#[derive(Clone)]
struct DigitFlip {
    current: char,
    previous: char,
    /// 0.0 = flip just started, 1.0 = flip complete.
    progress: f32,
}

impl DigitFlip {
    fn new(ch: char) -> Self {
        Self {
            current: ch,
            previous: ch,
            progress: 1.0,
        }
    }

    fn set(&mut self, ch: char) {
        if ch != self.current {
            self.previous = self.current;
            self.current = ch;
            self.progress = 0.0;
        }
    }

    fn is_animating(&self) -> bool {
        self.progress < 1.0
    }
}

/// A split-flap counter display.
pub struct FlipCounter {
    digits: Vec<DigitFlip>,
    num_slots: usize,
    text_color: Color32,
    card_color: Color32,
    card_color_bottom: Color32,
    card_height: f32,
    card_width: f32,
    card_gap: f32,
    flip_speed: f32,
    divider_color: Color32,
    border_color: Color32,
}

impl FlipCounter {
    pub fn new(num_slots: usize) -> Self {
        Self {
            digits: vec![DigitFlip::new(' '); num_slots],
            num_slots,
            text_color: theme::TEXT_PRIMARY,
            card_color: Color32::from_rgb(45, 45, 65),
            card_color_bottom: Color32::from_rgb(38, 38, 56),
            card_height: 60.0,
            card_width: 40.0,
            card_gap: 4.0,
            flip_speed: 3.0,
            divider_color: Color32::from_rgb(20, 20, 35),
            border_color: Color32::from_rgb(60, 60, 80),
        }
    }

    pub fn text_color(mut self, color: Color32) -> Self {
        self.text_color = color;
        self
    }

    pub fn card_height(mut self, height: f32) -> Self {
        self.card_height = height;
        self.card_width = height * 0.667;
        self
    }

    pub fn flip_speed(mut self, speed: f32) -> Self {
        self.flip_speed = speed;
        self
    }

    /// Update the displayed value. Right-aligns within slots, pads with spaces.
    pub fn set_value(&mut self, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        let pad = self.num_slots.saturating_sub(chars.len());

        for (i, digit) in self.digits.iter_mut().enumerate() {
            if i < pad {
                digit.set(' ');
            } else {
                digit.set(chars.get(i - pad).copied().unwrap_or(' '));
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        let dt = ui.input(|i| i.stable_dt).min(0.1); // clamp to avoid jumps
        let mut needs_repaint = false;

        for digit in &mut self.digits {
            if digit.is_animating() {
                digit.progress = (digit.progress + self.flip_speed * dt).min(1.0);
                needs_repaint = true;
            }
        }

        // Calculate total width accounting for colons being narrower
        let colon_width = self.card_width * 0.4;
        let total_width: f32 = self
            .digits
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let w = if d.current == ':' {
                    colon_width
                } else {
                    self.card_width
                };
                if i > 0 {
                    w + self.card_gap
                } else {
                    w
                }
            })
            .sum();

        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(total_width, self.card_height),
            egui::Sense::hover(),
        );

        let painter = ui.painter_at(rect);
        let half_h = self.card_height / 2.0;
        let font_size = self.card_height * 0.55;
        let corner = 4.0;
        let border_stroke = egui::Stroke::new(1.0, self.border_color);

        // Pre-compute font texture UV normalizer once per frame
        let font_tex_size = ui.ctx().fonts(|f| f.font_image_size());
        let uv_norm = Vec2::new(1.0 / font_tex_size[0] as f32, 1.0 / font_tex_size[1] as f32);

        let mut card_x = rect.left();

        for (i, digit) in self.digits.iter().enumerate() {
            if i > 0 {
                card_x += self.card_gap;
            }

            // Colons are rendered as separator dots, not flip cards
            if digit.current == ':' {
                let col_rect = Rect::from_min_size(
                    Pos2::new(card_x, rect.top()),
                    Vec2::new(colon_width, self.card_height),
                );
                self.draw_colon(&painter, col_rect);
                card_x += colon_width;
                continue;
            }

            let slot_w = self.card_width;
            let full_rect = Rect::from_min_size(
                Pos2::new(card_x, rect.top()),
                Vec2::new(slot_w, self.card_height),
            );
            let top_rect = Rect::from_min_max(
                full_rect.left_top(),
                Pos2::new(full_rect.right(), full_rect.top() + half_h),
            );
            let bot_rect = Rect::from_min_max(
                Pos2::new(full_rect.left(), full_rect.top() + half_h),
                full_rect.right_bottom(),
            );
            let hinge_y = full_rect.top() + half_h;

            if digit.is_animating() {
                // Phase split at raw progress 0.5. Easing applied within each phase.
                let p = digit.progress;

                // Shadow on bottom card from flap — quadratic ramp delays onset
                let bot_shadow = if p < 0.5 {
                    let t = p * 2.0;
                    (30.0 * t * t) as u8
                } else {
                    let t = (1.0 - p) * 2.0;
                    (30.0 * t * t) as u8
                };
                let shadowed_bot = darken(self.card_color_bottom, bot_shadow);

                // Draw each half-card with its own border
                painter.rect_filled(bot_rect, corner, shadowed_bot);
                painter.rect_stroke(bot_rect, corner, border_stroke, egui::StrokeKind::Inside);
                painter.rect_filled(top_rect, corner, self.card_color);
                painter.rect_stroke(top_rect, corner, border_stroke, egui::StrokeKind::Inside);

                // Base text on both halves
                self.draw_clipped_char(&painter, full_rect, bot_rect, font_size, digit.previous);

                if p < 0.5 {
                    // Phase 1: old top flap rotates forward toward the viewer.
                    // Behind it, the new digit's top half is revealed.
                    self.draw_clipped_char(&painter, full_rect, top_rect, font_size, digit.current);

                    // Ease within phase: 0→1 over the first half of progress
                    let phase_t = p * 2.0;
                    let eased = 1.0 - (1.0 - phase_t) * (1.0 - phase_t);
                    let angle = eased * std::f32::consts::FRAC_PI_2;
                    let cos_a = angle.cos(); // 1.0 → 0.0
                    let flip_h = (half_h * cos_a).round();
                    if flip_h >= 1.0 {
                        let top_y = hinge_y - flip_h;
                        let pinch = slot_w * 0.06 * (1.0 - cos_a);
                        let shadow = (40.0 * (1.0 - cos_a)) as u8;
                        let flap_color = darken(self.card_color, shadow);

                        // Near edge (falling toward viewer) expands outward
                        let corners = [
                            Pos2::new(card_x - pinch, top_y),
                            Pos2::new(card_x + slot_w + pinch, top_y),
                            Pos2::new(card_x + slot_w, hinge_y),
                            Pos2::new(card_x, hinge_y),
                        ];

                        self.draw_flap_with_text(
                            &painter,
                            full_rect,
                            corners,
                            flap_color,
                            0.0,
                            0.5,
                            font_size,
                            digit.previous,
                            &uv_norm,
                        );
                        stroke_trapezoid(&painter, &corners, border_stroke);
                    }
                } else {
                    // Phase 2: flap continues past vertical, showing its
                    // back face (new digit bottom half) growing downward.
                    self.draw_clipped_char(&painter, full_rect, top_rect, font_size, digit.current);

                    // Ease within phase: 0→1 over the second half of progress
                    let phase_t = (p - 0.5) * 2.0;
                    let eased = 1.0 - (1.0 - phase_t) * (1.0 - phase_t);
                    let angle = eased * std::f32::consts::FRAC_PI_2;
                    let cos_a = angle.sin(); // 0.0 → 1.0
                    let flip_h = (half_h * cos_a).round();
                    if flip_h >= 1.0 {
                        let bot_y = hinge_y + flip_h;
                        let pinch = slot_w * 0.06 * (1.0 - cos_a);
                        let shadow = (40.0 * (1.0 - cos_a)) as u8;
                        let flap_color = darken(self.card_color_bottom, shadow);

                        // Near edge (unfolding toward viewer) expands outward
                        let corners = [
                            Pos2::new(card_x, hinge_y),
                            Pos2::new(card_x + slot_w, hinge_y),
                            Pos2::new(card_x + slot_w + pinch, bot_y),
                            Pos2::new(card_x - pinch, bot_y),
                        ];

                        self.draw_flap_with_text(
                            &painter,
                            full_rect,
                            corners,
                            flap_color,
                            0.5,
                            1.0,
                            font_size,
                            digit.current,
                            &uv_norm,
                        );
                        stroke_trapezoid(&painter, &corners, border_stroke);
                    }
                }
            } else {
                // Static: draw each half-card with border, both halves show current digit
                painter.rect_filled(bot_rect, corner, self.card_color_bottom);
                painter.rect_stroke(bot_rect, corner, border_stroke, egui::StrokeKind::Inside);
                painter.rect_filled(top_rect, corner, self.card_color);
                painter.rect_stroke(top_rect, corner, border_stroke, egui::StrokeKind::Inside);

                self.draw_clipped_char(&painter, full_rect, top_rect, font_size, digit.current);
                self.draw_clipped_char(&painter, full_rect, bot_rect, font_size, digit.current);
            }

            // Divider line at hinge (always on top)
            painter.line_segment(
                [
                    Pos2::new(card_x, hinge_y),
                    Pos2::new(card_x + slot_w, hinge_y),
                ],
                egui::Stroke::new(1.5, self.divider_color),
            );

            card_x += slot_w;
        }

        if needs_repaint {
            ui.ctx().request_repaint();
        }
    }

    /// Draw a colon separator (two square dots, no card behind).
    fn draw_colon(&self, painter: &egui::Painter, full_rect: Rect) {
        let dot_size = self.card_height * 0.08;
        let cx = full_rect.center().x;
        let quarter = self.card_height * 0.28;

        for y_off in [full_rect.top() + quarter, full_rect.bottom() - quarter] {
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(cx, y_off), Vec2::splat(dot_size)),
                0.0,
                self.text_color,
            );
        }
    }

    /// Draw a solid-colour trapezoid (4-vertex quad) via a custom mesh.
    /// Corners: top-left, top-right, bottom-right, bottom-left (clockwise).
    fn draw_trapezoid(painter: &egui::Painter, corners: [Pos2; 4], color: Color32) {
        let white_uv = Pos2::new(0.0, 0.0);
        let mut mesh = Mesh::default();
        for &pos in &corners {
            mesh.vertices.push(Vertex {
                pos,
                uv: white_uv,
                color,
            });
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        painter.add(egui::Shape::mesh(mesh));
    }

    /// Draw a character centered in `full_rect`, but clipped to only show within `clip`.
    fn draw_clipped_char(
        &self,
        painter: &egui::Painter,
        full_rect: Rect,
        clip: Rect,
        font_size: f32,
        ch: char,
    ) {
        if ch == ' ' {
            return;
        }

        let galley = painter.layout_no_wrap(
            ch.to_string(),
            egui::FontId::new(font_size, egui::FontFamily::Monospace),
            self.text_color,
        );

        // Center the text in the full card rect
        let text_pos = Pos2::new(
            full_rect.center().x - galley.size().x / 2.0,
            full_rect.center().y - galley.size().y / 2.0,
        );

        // Draw with clip rect to only show the relevant portion
        let clipped = painter.clip_rect().intersect(clip);
        if clipped.is_positive() {
            let sub = painter.with_clip_rect(clipped);
            sub.galley(text_pos, galley, Color32::TRANSPARENT);
        }
    }

    /// Draw a trapezoid flap with text properly clipped to its half of the card.
    ///
    /// Text is laid out centered in `full_rect` (the whole card). Glyph quads
    /// that cross the hinge are split into sub-quads with interpolated UVs.
    /// Only the portion within `v_start..v_end` is kept, then bilinearly
    /// mapped into the trapezoid `corners`.
    ///
    /// - Top half: `v_start=0.0, v_end=0.5`
    /// - Bottom half: `v_start=0.5, v_end=1.0`
    #[allow(clippy::too_many_arguments)]
    fn draw_flap_with_text(
        &self,
        painter: &egui::Painter,
        full_rect: Rect,
        corners: [Pos2; 4],
        flap_color: Color32,
        v_start: f32,
        v_end: f32,
        font_size: f32,
        ch: char,
        uv_norm: &Vec2,
    ) {
        // Draw the card surface
        Self::draw_trapezoid(painter, corners, flap_color);

        if ch == ' ' {
            return;
        }

        let galley = painter.layout_no_wrap(
            ch.to_string(),
            egui::FontId::new(font_size, egui::FontFamily::Monospace),
            self.text_color,
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
        // Quads crossing the hinge are split; quads outside the half are skipped.
        for placed_row in &galley.rows {
            let row_offset = placed_row.pos;
            let row_mesh = &placed_row.row.visuals.mesh;

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

                // Glyph quad vertices: 0=TL, 1=TR, 2=BL, 3=BR
                let v_top = verts[0].1;
                let v_bot = verts[2].1;

                // Skip quads entirely outside our half
                if v_bot <= v_start || v_top >= v_end {
                    continue;
                }

                // Effective top/bottom within our half
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
                let lerp_vert =
                    |top: &(f32, f32, Pos2, Color32), bot: &(f32, f32, Pos2, Color32), t: f32| {
                        let fu = top.0 + (bot.0 - top.0) * t;
                        let fv = top.1 + (bot.1 - top.1) * t;
                        let uv = Pos2::new(
                            top.2.x + (bot.2.x - top.2.x) * t,
                            top.2.y + (bot.2.y - top.2.y) * t,
                        );
                        (fu, fv, uv, top.3)
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
                    let screen_pos = bilinear([tl, tr, br, bl], flap_u, flap_v);
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
}

/// Bilinear interpolation within a quad defined by four corners.
/// `u` goes left→right (0..1), `v` goes top→bottom (0..1).
/// Corners order: [TL, TR, BR, BL].
fn bilinear(corners: [Pos2; 4], u: f32, v: f32) -> Pos2 {
    let [tl, tr, br, bl] = corners;
    let top_x = tl.x + (tr.x - tl.x) * u;
    let top_y = tl.y + (tr.y - tl.y) * u;
    let bot_x = bl.x + (br.x - bl.x) * u;
    let bot_y = bl.y + (br.y - bl.y) * u;
    Pos2::new(top_x + (bot_x - top_x) * v, top_y + (bot_y - top_y) * v)
}

/// Stroke the outline of a trapezoid defined by 4 corners [TL, TR, BR, BL].
fn stroke_trapezoid(painter: &egui::Painter, corners: &[Pos2; 4], stroke: egui::Stroke) {
    for i in 0..4 {
        painter.line_segment([corners[i], corners[(i + 1) % 4]], stroke);
    }
}
