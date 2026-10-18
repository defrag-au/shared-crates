//! `RichText` — wrapped text in several styles with inline icon boxes, laid out
//! once and drawn crisp at any pixel scale.
//!
//! The text itself lives in `macroquad-text`, below both this kit and
//! `macroquad-widgets`, so a game's card text and a widget's label draw the
//! same way, share the glyph gate, and appear in the same capture report. This
//! module re-exports it, so a game reaches it through the kit as before.
//!
//! Card text is the motivating case: "**Deal 4** ⟨energy icon⟩ then *Draw 1*",
//! wrapped to a card's width, centred, legible on a phone. Measure layouts with
//! [`MeasureAt`] at the pixel scale they are drawn at, and draw them with
//! [`TextLayout::draw`] (or [`TextLayout::draw_scaled`] for text that pops), so
//! the harness report sees every line.

pub use macroquad_text::{
    Align, GlyphFlush, MacroquadMeasure, Measure, MeasureAt, Outline, Placed, RichText, Shadow,
    StyleId, TextLayout, TextStyle, draw_run, enable_glyph_gate, flush_glyphs, glyphs_pending,
    load_font, measure_run, register_font, warm_font_cache,
};
