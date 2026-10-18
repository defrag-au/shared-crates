//! One run of text in one style: the primitive every line on every surface is
//! drawn by.

use macroquad::prelude::*;

use crate::gate::admits;
use crate::measure::{Measure, MeasureAt};
use crate::raster::{atlas_size, draw_raster};
use crate::report;
use crate::style::TextStyle;

/// Draw `text` in `style` with the left end of its baseline at `at`.
///
/// `pixel_scale` is how many device pixels a point covers where this lands
/// (the screen's DPI scale, times any camera zoom or bake scale): glyphs
/// rasterise at that size, so text stays sharp rather than being stretched,
/// and the run is drawn at exactly `style.size` however fractional. `alpha`
/// fades the run, outline and shadow included.
///
/// With the [glyph gate](crate::enable_glyph_gate) on, a run whose glyphs are
/// not cached draws nothing this frame and returns false; it draws the next.
/// Every run drawn is recorded for the capture [`report`].
pub fn draw_run(text: &str, style: &TextStyle, at: Vec2, pixel_scale: f32, alpha: f32) -> bool {
    let dpi = screen_dpi_scale();
    let (px, _) = draw_raster(style.size, pixel_scale, dpi);
    if !admits(text, style.font.as_ref(), atlas_size(px, dpi)) {
        return false;
    }
    paint(text, style, at, pixel_scale, 1.0, alpha);
    true
}

/// Draw a run already admitted by the gate, scaled by `scale` about `at`.
pub(crate) fn paint(
    text: &str,
    style: &TextStyle,
    at: Vec2,
    pixel_scale: f32,
    scale: f32,
    alpha: f32,
) {
    let k = pixel_scale.max(0.01);
    let (font_size, font_scale) = draw_raster(style.size, k, screen_dpi_scale());
    let fade = |c: Color| Color::new(c.r, c.g, c.b, c.a * alpha);
    let params = |color: Color| TextParams {
        font: style.font.as_ref(),
        font_size,
        font_scale: font_scale * scale,
        color: fade(color),
        ..Default::default()
    };
    if let Some(sh) = style.shadow {
        let off = sh.offset * scale;
        draw_text_ex(text, at.x + off.x, at.y + off.y, params(sh.color));
    }
    if let Some(o) = style.outline {
        for (dx, dy) in OUTLINE_DIRS {
            draw_text_ex(
                text,
                at.x + dx * o.width * scale,
                at.y + dy * o.width * scale,
                params(o.color),
            );
        }
    }
    draw_text_ex(text, at.x, at.y, params(style.color));
    if report::on() {
        // At the draw raster: the box of what was drawn, and no second set of
        // glyphs cached mid-frame to get it.
        let m = MeasureAt(k);
        let ascent = m.ascent(style) * scale;
        let rect = Rect::new(
            at.x,
            at.y - ascent,
            m.width(text, style) * scale,
            ascent + m.descent(style) * scale,
        );
        report::line(text, rect, style.size * scale, alpha);
    }
}

const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// Eight directions for an outline drawn by offset copies.
const OUTLINE_DIRS: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (D, D),
    (-D, D),
    (D, -D),
    (-D, -D),
];
