//! Small shapes the fonts have no glyph for: the ADA mark, a change arrow, a
//! soft glow, and a pill label. Infrastructure for the trading views, not a
//! widget.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::painter::{Painter, draw_rounded_rect};
use crate::theme::with_alpha;

/// The ADA mark: an A with a double bar, drawn, since the fonts have no ₳.
pub fn ada_mark(c: Vec2, size: f32, color: Color) {
    let h = size * 0.5;
    let w = size * 0.42;
    let t = (size * 0.13).max(1.0);
    draw_line(c.x - w, c.y + h, c.x, c.y - h, t, color);
    draw_line(c.x + w, c.y + h, c.x, c.y - h, t, color);
    draw_line(
        c.x - w * 0.9,
        c.y + h * 0.05,
        c.x + w * 0.9,
        c.y + h * 0.05,
        t * 0.8,
        color,
    );
    draw_line(
        c.x - w,
        c.y + h * 0.4,
        c.x + w,
        c.y + h * 0.4,
        t * 0.8,
        color,
    );
}

/// A filled triangle pointing up (`up`) or down, centred on `c`.
pub fn arrow(c: Vec2, size: f32, up: bool, color: Color) {
    let h = size * 0.5;
    let (tip, base) = if up { (-h, h * 0.8) } else { (h, -h * 0.8) };
    draw_triangle(
        vec2(c.x, c.y + tip),
        vec2(c.x - h, c.y + base),
        vec2(c.x + h, c.y + base),
        color,
    );
}

/// A soft glow: rings of falling alpha.
pub fn glow(c: Vec2, r: f32, color: Color, strength: f32) {
    for i in 0..6 {
        let t = i as f32 / 6.0;
        draw_circle(
            c.x,
            c.y,
            r * (1.0 + t * 1.2),
            with_alpha(color, strength * (1.0 - t) * 0.16),
        );
    }
}

/// A label in a dark pill, centred on `x` and kept inside `bounds`, with its
/// pill's top at `top`: what a finger on the chart reads.
pub fn pill_label(p: &Painter, text: &str, x: f32, top: f32, bounds: Rect, color: Color) {
    let s = p.size(TextSize::Xs);
    let w = p.measure(text, s).width + 8.0;
    let left = (x - w * 0.5).clamp(bounds.x, bounds.right() - w);
    let pill = Rect::new(left, top, w, 14.0);
    draw_rounded_rect(
        pill.x,
        pill.y,
        pill.w,
        pill.h,
        4.0,
        p.theme.color.bg_primary,
    );
    p.text_in(text, pill, s, color, crate::Align::Center);
}
