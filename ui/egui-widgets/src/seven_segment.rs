//! SevenSegmentDisplay — retro LED-style numeric display.
//!
//! Renders numbers using painter-drawn 7-segment digit glyphs, like a
//! mission control counter or retro scoreboard. Best suited for integer
//! counters and time displays. Supports digits, colons, minus signs, and spaces.
//!
//! Segment geometry derived from the classic approach: each segment is a
//! 6-point hexagon. Horizontal segment tips extend into the vertical columns
//! by half-thickness, and vertical segments are shortened to leave gaps.
//! Reference: https://dmitrybrant.com/2009/07/01/seven-segment-display-for-dot-net

use egui::{Color32, Pos2, Ui};

use crate::theme;

// bit 6=a, 5=b, 4=c, 3=d, 2=e, 1=f, 0=g
const SEGMENTS: [u8; 10] = [
    0b1111110, // 0
    0b0110000, // 1
    0b1101101, // 2
    0b1111001, // 3
    0b0110011, // 4
    0b1011011, // 5
    0b1011111, // 6
    0b1110000, // 7
    0b1111111, // 8
    0b1111011, // 9
];

pub struct SevenSegmentDisplay<'a> {
    text: &'a str,
    on_color: Color32,
    off_color: Color32,
    digit_height: f32,
}

impl<'a> SevenSegmentDisplay<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            on_color: theme::ACCENT_GREEN,
            off_color: Color32::from_rgba_premultiplied(40, 40, 50, 80),
            digit_height: 40.0,
        }
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.on_color = color;
        self
    }

    pub fn off_color(mut self, color: Color32) -> Self {
        self.off_color = color;
        self
    }

    pub fn digit_height(mut self, height: f32) -> Self {
        self.digit_height = height;
        self
    }

    pub fn show(self, ui: &mut Ui) {
        let h = self.digit_height;
        // Reference grid: 48w x 80h, element_width=10
        // We scale everything from that grid to the requested digit_height.
        let scale = h / 80.0;
        let w = 48.0 * scale;
        let char_gap = 8.0 * scale;

        let total_width: f32 = self
            .text
            .chars()
            .map(|ch| match ch {
                ':' => 12.0 * scale + char_gap,
                ' ' => w * 0.5 + char_gap,
                _ => w + char_gap,
            })
            .sum::<f32>()
            - char_gap;

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(total_width.max(0.0), h), egui::Sense::hover());

        let painter = ui.painter_at(rect);
        let mut cx = rect.left();
        let cy = rect.top();

        for ch in self.text.chars() {
            match ch {
                '0'..='9' => {
                    let digit = (ch as u8 - b'0') as usize;
                    self.draw_digit(&painter, cx, cy, scale, SEGMENTS[digit]);
                    cx += w + char_gap;
                }
                '-' => {
                    self.draw_digit(&painter, cx, cy, scale, 0b0000001);
                    cx += w + char_gap;
                }
                ':' => {
                    let dot = 6.0 * scale;
                    let dx = cx + 3.0 * scale;
                    let dy1 = cy + 25.0 * scale;
                    let dy2 = cy + 55.0 * scale;
                    painter.rect_filled(
                        egui::Rect::from_min_size(Pos2::new(dx, dy1), egui::vec2(dot, dot)),
                        0.0,
                        self.on_color,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_size(Pos2::new(dx, dy2), egui::vec2(dot, dot)),
                        0.0,
                        self.on_color,
                    );
                    cx += 12.0 * scale + char_gap;
                }
                ' ' => {
                    cx += w * 0.5 + char_gap;
                }
                _ => {
                    cx += w + char_gap;
                }
            }
        }
    }

    /// Draw one digit at (ox, oy) using the reference grid scaled by `s`.
    ///
    /// Reference grid (48 x 80, element_width=10):
    ///
    /// Seg 0 (a, top H):    (11,0)  (37,0)  (43,5)  (37,10) (11,10) (6,5)
    /// Seg 1 (f, upper-L V): (0,11)  (5,6)   (10,11) (10,29) (4,39)  (0,39)
    /// Seg 2 (b, upper-R V): (38,11) (43,6)  (48,11) (48,39) (44,39) (38,29)
    /// Seg 3 (g, mid H):    (11,35) (37,35) (43,40) (37,45) (11,45) (5,40)
    /// Seg 4 (e, lower-L V): (0,41)  (4,41)  (10,46) (10,69) (6,74)  (0,69)
    /// Seg 5 (c, lower-R V): (38,46) (44,41) (48,41) (48,69) (43,74) (38,70)
    /// Seg 6 (d, bot H):    (11,70) (37,70) (43,75) (37,80) (11,80) (6,75)
    fn draw_digit(&self, painter: &egui::Painter, ox: f32, oy: f32, s: f32, segments: u8) {
        // a: top horizontal (bit 6)
        self.draw_seg(
            painter,
            ox,
            oy,
            s,
            &[(11, 0), (37, 0), (43, 5), (37, 10), (11, 10), (6, 5)],
            segments & 0b1000000 != 0,
        );
        // f: upper-left vertical (bit 1)
        self.draw_seg(
            painter,
            ox,
            oy,
            s,
            &[(0, 11), (5, 6), (10, 11), (10, 29), (4, 39), (0, 39)],
            segments & 0b0000010 != 0,
        );
        // b: upper-right vertical (bit 5)
        self.draw_seg(
            painter,
            ox,
            oy,
            s,
            &[(38, 11), (43, 6), (48, 11), (48, 39), (44, 39), (38, 29)],
            segments & 0b0100000 != 0,
        );
        // g: middle horizontal (bit 0)
        self.draw_seg(
            painter,
            ox,
            oy,
            s,
            &[(11, 35), (37, 35), (43, 40), (37, 45), (11, 45), (5, 40)],
            segments & 0b0000001 != 0,
        );
        // e: lower-left vertical (bit 2)
        self.draw_seg(
            painter,
            ox,
            oy,
            s,
            &[(0, 41), (4, 41), (10, 46), (10, 69), (6, 74), (0, 69)],
            segments & 0b0000100 != 0,
        );
        // c: lower-right vertical (bit 4)
        self.draw_seg(
            painter,
            ox,
            oy,
            s,
            &[(38, 46), (44, 41), (48, 41), (48, 69), (43, 74), (38, 70)],
            segments & 0b0010000 != 0,
        );
        // d: bottom horizontal (bit 3)
        self.draw_seg(
            painter,
            ox,
            oy,
            s,
            &[(11, 70), (37, 70), (43, 75), (37, 80), (11, 80), (6, 75)],
            segments & 0b0001000 != 0,
        );
    }

    fn draw_seg(
        &self,
        painter: &egui::Painter,
        ox: f32,
        oy: f32,
        s: f32,
        pts: &[(i32, i32)],
        on: bool,
    ) {
        let color = if on { self.on_color } else { self.off_color };
        let points: Vec<Pos2> = pts
            .iter()
            .map(|&(px, py)| Pos2::new(ox + px as f32 * s, oy + py as f32 * s))
            .collect();
        painter.add(egui::Shape::convex_polygon(
            points,
            color,
            egui::Stroke::NONE,
        ));
    }
}
