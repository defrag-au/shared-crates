//! Cell + grid painting — the "Tier 3" renderer that actually uses
//! [`Cell`]'s `bg` and `attrs` fields.
//!
//! Earlier versions of the consumer's render loops only read `cell.fg`
//! and `cell.ch`, leaving the background and attribute bitset
//! invisible to the user. This module supplies a one-stop painter
//! that respects every cell field:
//!
//! - **bg**: drawn as a `draw_rectangle` underneath the glyph when not
//!   fully transparent
//! - **INVERSE**: swaps `fg`/`bg` before applying everything else
//! - **BLINK**: time-based on/off at ~1.5 Hz; while off, neither glyph
//!   nor bg is drawn (the cleared backdrop shows through)
//! - **DIM**: multiplies foreground channels by 0.6
//! - **BOLD**: multiplies foreground channels by 1.2 (clamped to 1.0)
//!
//! Use [`paint_cell`] when you need fine control over which cells get
//! painted (e.g. the terminal skips the row the editor prompt occupies
//! to avoid overdraw). Otherwise [`paint_grid`] is the convenience
//! that loops once over the grid's visible viewport.

use macroquad::prelude::*;

use crate::grid::{Cell, CellAttrs, Grid};

const BLINK_HZ: f32 = 1.5;
const DIM_FACTOR: f32 = 0.6;
const BOLD_FACTOR: f32 = 1.2;

/// Cell-and-line measurements derived from the chosen font. The
/// renderer needs these to map `(row, col)` → pixel coordinates.
/// `cell_h` is the *line* height (≈ `font_size × 1.35`), not the glyph
/// height — gives breathing room above + below the text.
pub struct GridMetrics {
    pub cell_w: f32,
    pub cell_h: f32,
    pub baseline_offset: f32,
}

impl GridMetrics {
    pub fn measure(font: Option<&Font>, font_size: u16) -> Self {
        // Monospaced fonts measure all glyphs to the same width;
        // 'M' is a stable reference.
        let m = measure_text("M", font, font_size, 1.0);
        Self {
            cell_w: m.width,
            cell_h: (font_size as f32) * 1.35,
            baseline_offset: m.offset_y,
        }
    }
}

/// Paint one cell at the given screen position. Lower-level than
/// [`paint_grid`] — used when consumers need to filter cells (e.g.
/// the terminal skips its editor row, games that want to apply
/// custom transforms).
///
/// `x` / `y_top` are the *top-left* of the cell in screen pixels.
/// `time` is used to drive BLINK; pass the same value that
/// `SceneCtx::time` carries.
pub fn paint_cell(
    cell: &Cell,
    x: f32,
    y_top: f32,
    metrics: &GridMetrics,
    font: Option<&Font>,
    font_size: u16,
    time: f32,
) {
    let (mut fg, bg) = if cell.attrs.contains(CellAttrs::INVERSE) {
        (cell.bg, cell.fg)
    } else {
        (cell.fg, cell.bg)
    };

    // BLINK gates everything — when off, draw nothing so the cleared
    // background shows through.
    if cell.attrs.contains(CellAttrs::BLINK) {
        let on = ((time * BLINK_HZ) % 1.0) < 0.5;
        if !on {
            return;
        }
    }

    // Background fill if not transparent. The alpha check uses a
    // small threshold to skip near-zero alphas — many of our `bg`
    // values come from `Color::new(0,0,0,0)` literals.
    if bg.a > 0.05 {
        draw_rectangle(x, y_top, metrics.cell_w, metrics.cell_h, bg);
    }

    if cell.ch == ' ' {
        return;
    }

    if cell.attrs.contains(CellAttrs::DIM) {
        fg = scale_rgb(fg, DIM_FACTOR);
    }
    if cell.attrs.contains(CellAttrs::BOLD) {
        fg = scale_rgb(fg, BOLD_FACTOR);
    }

    let params = TextParams {
        font,
        font_size,
        color: fg,
        ..Default::default()
    };
    let y_baseline = y_top + metrics.baseline_offset;
    let mut buf = [0u8; 4];
    let s = cell.ch.encode_utf8(&mut buf);
    draw_text_ex(s, x, y_baseline, params);
}

/// Paint every visible cell in `grid`. Layout maps cells to pixel
/// space anchored at `(padding, padding)`. Pass `skip_row = Some(r)`
/// to leave row `r` unpainted (e.g. when the consumer is going to
/// overlay an editor prompt there and doesn't want overdraw).
pub fn paint_grid(
    grid: &Grid,
    font: Option<&Font>,
    font_size: u16,
    metrics: &GridMetrics,
    padding: f32,
    time: f32,
    skip_row: Option<usize>,
) {
    for (row, col, cell) in grid.cells() {
        if Some(row) == skip_row {
            continue;
        }
        let x = padding + col as f32 * metrics.cell_w;
        let y_top = padding + row as f32 * metrics.cell_h;
        paint_cell(cell, x, y_top, metrics, font, font_size, time);
    }
}

fn scale_rgb(c: Color, factor: f32) -> Color {
    Color::new(
        (c.r * factor).min(1.0),
        (c.g * factor).min(1.0),
        (c.b * factor).min(1.0),
        c.a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_rgb_clamps_at_full_brightness() {
        let c = Color::new(0.9, 0.5, 0.1, 1.0);
        let bright = scale_rgb(c, 2.0);
        assert!((bright.r - 1.0).abs() < 1e-6);
        assert!((bright.g - 1.0).abs() < 1e-6);
        assert!((bright.b - 0.2).abs() < 1e-6);
    }

    #[test]
    fn scale_rgb_dim_halves_brightness() {
        let c = Color::new(1.0, 0.6, 0.4, 1.0);
        let dim = scale_rgb(c, 0.5);
        assert!((dim.r - 0.5).abs() < 1e-6);
        assert!((dim.g - 0.3).abs() < 1e-6);
        assert!((dim.b - 0.2).abs() < 1e-6);
    }
}
