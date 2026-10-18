//! Widths and vertical metrics: the [`Measure`] trait layout goes through, the
//! two macroquad-backed measures, and a single run's box.

use macroquad::prelude::*;

use crate::gate::{admits, printable_ascii};
use crate::raster::{MEASURE_SCALE, atlas_size, draw_raster, raster};
use crate::style::TextStyle;

/// Text widths and vertical metrics for a style.
pub trait Measure {
    fn width(&self, text: &str, style: &TextStyle) -> f32;
    /// Distance from the baseline up to the top of tall glyphs.
    fn ascent(&self, style: &TextStyle) -> f32;
    /// Distance from the baseline down to the bottom of descenders.
    fn descent(&self, style: &TextStyle) -> f32;
}

/// Measures with macroquad's own font rasteriser at a fixed fine raster,
/// independent of the screen. Prefer [`MeasureAt`] when the pixel scale is
/// known: this one caches a second, much larger, copy of every glyph.
pub struct MacroquadMeasure;

impl Measure for MacroquadMeasure {
    fn width(&self, text: &str, style: &TextStyle) -> f32 {
        fine(text, style).width
    }

    fn ascent(&self, style: &TextStyle) -> f32 {
        fine("Mg", style).offset_y
    }

    fn descent(&self, style: &TextStyle) -> f32 {
        let d = fine("Mg", style);
        (d.height - d.offset_y).max(0.0)
    }
}

/// Measured at a fine raster, scaled to the exact size.
fn fine(text: &str, style: &TextStyle) -> TextDimensions {
    let (px, scale) = raster(style.size, MEASURE_SCALE);
    gated(text, style, px, scale)
}

/// Measures at the size text will be drawn at, `pixel_scale` device pixels per
/// point: the same glyphs [`draw_run`](crate::draw_run) and
/// [`TextLayout::draw`](crate::TextLayout::draw) rasterise, so layout and
/// drawing share one set of atlas entries and agree to the pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureAt(pub f32);

impl Measure for MeasureAt {
    fn width(&self, text: &str, style: &TextStyle) -> f32 {
        measure_run(text, style, self.0).width
    }

    fn ascent(&self, style: &TextStyle) -> f32 {
        measure_run("Mg", style, self.0).offset_y
    }

    fn descent(&self, style: &TextStyle) -> f32 {
        let d = measure_run("Mg", style, self.0);
        (d.height - d.offset_y).max(0.0)
    }
}

/// The box of `text` in `style` drawn at `pixel_scale` device pixels per
/// point: what [`draw_run`](crate::draw_run) will draw. While the glyph gate
/// holds the glyphs back it is an estimate, for the one frame in which the text
/// is not drawn.
pub fn measure_run(text: &str, style: &TextStyle, pixel_scale: f32) -> TextDimensions {
    let (px, scale) = draw_raster(style.size, pixel_scale, screen_dpi_scale());
    gated(text, style, px, scale)
}

/// Measure through macroquad, unless the glyph gate is holding these glyphs
/// back: then an estimate, so measuring never caches mid-frame.
fn gated(text: &str, style: &TextStyle, font_size: u16, scale: f32) -> TextDimensions {
    let size = atlas_size(font_size, screen_dpi_scale());
    if admits(text, style.font.as_ref(), size) {
        measure_text(text, style.font.as_ref(), font_size, scale)
    } else {
        estimate(text, style)
    }
}

/// A plausible box for text whose glyphs are not cached yet.
fn estimate(text: &str, style: &TextStyle) -> TextDimensions {
    TextDimensions {
        width: text.chars().count() as f32 * style.size * 0.55,
        height: style.size,
        offset_y: style.size * 0.8,
    }
}

/// Cache every printable ASCII glyph `style` will draw at `pixel_scale`.
///
/// See [`enable_glyph_gate`](crate::enable_glyph_gate) for why caching ahead
/// of drawing matters, and for a way to do it without listing the styles.
pub fn warm_font_cache(style: &TextStyle, pixel_scale: f32) {
    let Some(font) = style.font.as_ref() else {
        return;
    };
    let dpi = screen_dpi_scale();
    let (px, _) = draw_raster(style.size, pixel_scale, dpi);
    let characters: Vec<char> = printable_ascii().collect();
    font.populate_font_cache(&characters, atlas_size(px, dpi));
}
