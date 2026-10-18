//! `macroquad-text` — text for every macroquad surface, drawn one way.
//!
//! macroquad draws a run of text in one style and leaves the rest to the
//! caller, and each thing it leaves is a silent trap: a fractional size drawn
//! at a rounded one, glyphs rasterised at twice the size they land at, a font
//! atlas re-uploaded whole for every new glyph. This crate holds the answers
//! once, below both `macroquad-widgets` and `macroquad-gamekit`:
//!
//! - [`TextStyle`] and [`draw_run`]: one run, at exactly its size, rasterised
//!   at the device pixels it lands on. A widget's label goes through this.
//! - [`RichText`] and [`TextLayout`]: wrapped text in several styles with
//!   inline icon boxes, measured through [`Measure`] so wrapping is
//!   unit-tested without a window.
//! - The **glyph gate** ([`enable_glyph_gate`]): text waits a frame for glyphs
//!   it has not cached, and [`flush_glyphs`] caches them in one batch after the
//!   frame's last draw, so a font atlas uploads at most once a frame.
//! - [`report`]: every line drawn through here is recorded for a capture's
//!   text report, so a reviewer reading it sees widget text and game text
//!   alike.
//!
//! Sizes are points from the caller, resolved through the theme's ramp; this
//! crate never writes one down. Time is never read: nothing here animates.

mod gate;
mod layout;
mod measure;
mod raster;
pub mod report;
mod run;
mod style;

pub use gate::{
    GlyphFlush, enable_glyph_gate, flush_glyphs, glyphs_pending, load_font, register_font, ungated,
};
pub use layout::{Align, Placed, RichText, StyleId, TextLayout};
pub use measure::{MacroquadMeasure, Measure, MeasureAt, measure_run, warm_font_cache};
pub use run::draw_run;
pub use style::{Outline, Shadow, TextStyle};
