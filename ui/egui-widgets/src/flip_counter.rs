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
        let corner = 3.0;

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

            // Draw the card background as a single solid rect so there are
            // no anti-aliased rounded-corner seams at the hinge line.
            painter.rect_filled(full_rect, corner, self.card_color_bottom);
            painter.rect_filled(top_rect, 0.0, self.card_color);

            if digit.is_animating() {
                let p = digit.progress;
                let ep = 1.0 - (1.0 - p) * (1.0 - p); // ease-out quadratic

                // The bottom half always shows the OLD digit as base text.
                self.draw_clipped_char(&painter, full_rect, bot_rect, font_size, digit.previous);

                if ep < 0.5 {
                    // Phase 1: old top flap rotates forward around the hinge.
                    // Behind it, the new digit's top half is revealed.
                    self.draw_clipped_char(&painter, full_rect, top_rect, font_size, digit.current);

                    // Flap angle: 0° (flat) → 90° (edge-on).
                    // Visible height foreshortens as cos(angle).
                    let angle = ep * 2.0 * std::f32::consts::FRAC_PI_2;
                    let cos_a = angle.cos(); // 1.0 → 0.0
                    let flip_h = (half_h * cos_a).round();
                    if flip_h >= 1.0 {
                        // Flap anchored at hinge. Top edge drops toward hinge.
                        let top_y = hinge_y - flip_h;
                        let pinch = slot_w * 0.06 * (1.0 - cos_a);
                        let shadow = (40.0 * (1.0 - cos_a)) as u8;
                        let flap_color = darken(self.card_color, shadow);

                        Self::draw_trapezoid(
                            &painter,
                            card_x + pinch,
                            top_y,
                            card_x + slot_w - pinch,
                            top_y,
                            card_x + slot_w,
                            hinge_y,
                            card_x,
                            hinge_y,
                            flap_color,
                        );

                        let clip = Rect::from_min_max(
                            Pos2::new(card_x, top_y),
                            Pos2::new(card_x + slot_w, hinge_y),
                        );
                        self.draw_clipped_char(
                            &painter,
                            full_rect,
                            clip,
                            font_size,
                            digit.previous,
                        );
                    }
                } else {
                    // Phase 2: flap continues past vertical, now showing its
                    // back face (new digit bottom half) growing downward.
                    self.draw_clipped_char(&painter, full_rect, top_rect, font_size, digit.current);

                    // Angle: 90° → 180° (flat again).
                    let angle = (ep - 0.5) * 2.0 * std::f32::consts::FRAC_PI_2;
                    let cos_a = angle.sin(); // 0.0 → 1.0
                    let flip_h = (half_h * cos_a).round();
                    if flip_h >= 1.0 {
                        // Flap anchored at hinge. Bottom edge grows downward.
                        let bot_y = hinge_y + flip_h;
                        let pinch = slot_w * 0.06 * (1.0 - cos_a);
                        let shadow = (40.0 * (1.0 - cos_a)) as u8;
                        let flap_color = darken(self.card_color_bottom, shadow);

                        Self::draw_trapezoid(
                            &painter,
                            card_x,
                            hinge_y,
                            card_x + slot_w,
                            hinge_y,
                            card_x + slot_w - pinch,
                            bot_y,
                            card_x + pinch,
                            bot_y,
                            flap_color,
                        );

                        let clip = Rect::from_min_max(
                            Pos2::new(card_x, hinge_y),
                            Pos2::new(card_x + slot_w, bot_y),
                        );
                        self.draw_clipped_char(&painter, full_rect, clip, font_size, digit.current);
                    }
                }
            } else {
                // Static: both halves show current digit
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
    /// Vertices: top-left, top-right, bottom-right, bottom-left (clockwise).
    fn draw_trapezoid(
        painter: &egui::Painter,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
        color: Color32,
    ) {
        let white_uv = Pos2::new(0.0, 0.0);
        let mut mesh = Mesh::default();
        mesh.vertices.extend_from_slice(&[
            Vertex {
                pos: Pos2::new(x0, y0),
                uv: white_uv,
                color,
            },
            Vertex {
                pos: Pos2::new(x1, y1),
                uv: white_uv,
                color,
            },
            Vertex {
                pos: Pos2::new(x2, y2),
                uv: white_uv,
                color,
            },
            Vertex {
                pos: Pos2::new(x3, y3),
                uv: white_uv,
                color,
            },
        ]);
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
}
