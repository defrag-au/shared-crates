//! FlipCounter — split-flap style animated digit counter.
//!
//! Each digit is rendered as two cards (top half, bottom half) with the same
//! text drawn in both, clipped to show only the relevant half. When a digit
//! changes, the old top half flips down and the new bottom half flips in.
//!
//! Technique based on the classic CSS flip-clock approach:
//! two 50%-height containers with overflow:hidden, same full-height text in each.
//! https://github.com/objectivehtml/FlipClock

use egui::{Color32, Pos2, Rect, Ui, Vec2};

use crate::theme;

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
            flip_speed: 6.0,
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

            if digit.is_animating() {
                let p = digit.progress;
                let ep = 1.0 - (1.0 - p) * (1.0 - p); // ease-out quadratic

                if ep < 0.5 {
                    // Phase 1: old top flips away, revealing new digit behind

                    // Background: new digit (static)
                    painter.rect_filled(top_rect, corner, self.card_color);
                    painter.rect_filled(bot_rect, corner, self.card_color_bottom);
                    self.draw_clipped_char(&painter, full_rect, top_rect, font_size, digit.current);
                    self.draw_clipped_char(&painter, full_rect, bot_rect, font_size, digit.current);

                    // Overlay: old top half shrinking from bottom edge upward
                    let flip_frac = 1.0 - ep * 2.0; // 1.0 → 0.0
                    if flip_frac > 0.02 {
                        let flip_h = half_h * flip_frac;
                        let flip_rect =
                            Rect::from_min_size(full_rect.left_top(), Vec2::new(slot_w, flip_h));
                        painter.rect_filled(flip_rect, corner, self.card_color);
                        self.draw_clipped_char(
                            &painter,
                            full_rect,
                            flip_rect,
                            font_size,
                            digit.previous,
                        );
                    }
                } else {
                    // Phase 2: new bottom half grows from hinge line downward

                    // Background: new top + old bottom
                    painter.rect_filled(top_rect, corner, self.card_color);
                    self.draw_clipped_char(&painter, full_rect, top_rect, font_size, digit.current);

                    painter.rect_filled(bot_rect, corner, self.card_color_bottom);
                    self.draw_clipped_char(
                        &painter,
                        full_rect,
                        bot_rect,
                        font_size,
                        digit.previous,
                    );

                    // Overlay: new bottom growing down
                    let flip_frac = (ep - 0.5) * 2.0; // 0.0 → 1.0
                    let flip_h = half_h * flip_frac;
                    if flip_h > 1.0 {
                        let flip_rect = Rect::from_min_size(
                            Pos2::new(card_x, rect.top() + half_h),
                            Vec2::new(slot_w, flip_h),
                        );
                        painter.rect_filled(flip_rect, corner, self.card_color_bottom);
                        self.draw_clipped_char(
                            &painter,
                            full_rect,
                            flip_rect,
                            font_size,
                            digit.current,
                        );
                    }
                }
            } else {
                // Static: both halves show current digit
                painter.rect_filled(top_rect, corner, self.card_color);
                painter.rect_filled(bot_rect, corner, self.card_color_bottom);
                self.draw_clipped_char(&painter, full_rect, top_rect, font_size, digit.current);
                self.draw_clipped_char(&painter, full_rect, bot_rect, font_size, digit.current);
            }

            // Divider line at hinge (always on top)
            let div_y = rect.top() + half_h;
            painter.line_segment(
                [Pos2::new(card_x, div_y), Pos2::new(card_x + slot_w, div_y)],
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
