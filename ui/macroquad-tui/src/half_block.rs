//! Half-block canvas — 2× vertical "pixel" resolution by splitting
//! each character cell into a top + bottom half via the Unicode
//! block-drawing characters.
//!
//! Each `Cell` in a [`crate::Grid`] is normally one foreground colour.
//! By choosing the glyph carefully we can independently colour the
//! top and bottom halves of a single character cell:
//!
//! | Top half | Bottom half | Glyph | `fg` | `bg` |
//! |---|---|---|---|---|
//! | transparent | transparent | ` ` | — | — |
//! | colour A | transparent | `▀` | A | — |
//! | transparent | colour B | `▄` | B | — |
//! | colour A | colour B (A ≠ B) | `▀` | A | B |
//! | colour A | colour A | `█` | A | — |
//!
//! `▀ ▄ █` are CP437 charset members — they render correctly in
//! JetBrains Mono, DOS bitmap fonts, every standard terminal font.
//! The fg/bg pair is what our Tier-3 renderer (`paint_cell`) already
//! draws, so the composite "looks right" without any rendering
//! changes.
//!
//! ## Use case
//!
//! Build a [`HalfBlockCanvas`] at some pixel-ish size (e.g. 40 cols
//! × 30 char-rows = effective 40×60 subpixels), set individual
//! "pixels" by colour, then `write_to_grid` to composite the result
//! into a [`crate::Grid`] at a chosen offset. The grid then renders
//! normally via [`crate::paint_grid`].
//!
//! Suitable for:
//! - Smooth-edged game sprites (Snake bodies, Tetris ghost piece)
//! - Persona portraits from existing AI sketch images
//! - Geographic maps with half-cell precision
//! - Animated logo / boot sequences
//!
//! ## Coordinate convention
//!
//! `x` is char columns (so the canvas's pixel-x is char-x).
//! `y` is *subrows* — twice the resolution of grid rows. Subrow 0 is
//! the top half of char-row 0; subrow 1 is the bottom half; subrow 2
//! is the top half of char-row 1; etc. A canvas with `height = 30`
//! char-rows is `60` subrows tall.

use macroquad::prelude::Color;

use crate::grid::{Cell, CellAttrs, Grid};

const TRANSPARENT: Color = Color::new(0.0, 0.0, 0.0, 0.0);
const ALPHA_THRESHOLD: f32 = 0.05;

/// 2× vertical resolution pixel canvas. Backed by a row-major
/// `Vec<Color>` of `width × (height * 2)` entries.
pub struct HalfBlockCanvas {
    width: usize,
    /// Char-row count. The pixel grid is `width × (height * 2)`.
    height_chars: usize,
    pixels: Vec<Color>,
}

impl HalfBlockCanvas {
    /// Construct a canvas of the given dimensions, initially fully
    /// transparent. `height_chars` is the number of character rows
    /// the canvas occupies — there are `height_chars * 2` subrows.
    pub fn new(width: usize, height_chars: usize) -> Self {
        Self {
            width,
            height_chars,
            pixels: vec![TRANSPARENT; width * height_chars * 2],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    /// Number of *subrows* — twice the number of character rows.
    pub fn height_subrows(&self) -> usize {
        self.height_chars * 2
    }

    pub fn height_chars(&self) -> usize {
        self.height_chars
    }

    /// Fill every pixel with `color`. `Color::new(0, 0, 0, 0)` clears
    /// to transparent (the default state after `new`).
    pub fn clear(&mut self, color: Color) {
        for p in &mut self.pixels {
            *p = color;
        }
    }

    /// Set one subpixel. Out-of-bounds writes are silently dropped.
    pub fn set(&mut self, x: usize, sub_y: usize, color: Color) {
        if x >= self.width || sub_y >= self.height_chars * 2 {
            return;
        }
        self.pixels[sub_y * self.width + x] = color;
    }

    /// Read one subpixel. Out-of-bounds reads return transparent.
    pub fn get(&self, x: usize, sub_y: usize) -> Color {
        if x >= self.width || sub_y >= self.height_chars * 2 {
            return TRANSPARENT;
        }
        self.pixels[sub_y * self.width + x]
    }

    /// Fill a rectangle of subpixels. `w` and `h` are pixel widths /
    /// heights (h in subrows). Clipped to canvas bounds.
    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: Color) {
        let max_x = (x + w).min(self.width);
        let max_y = (y + h).min(self.height_chars * 2);
        for sub_y in y..max_y {
            for px in x..max_x {
                self.pixels[sub_y * self.width + px] = color;
            }
        }
    }

    /// Composite into a [`Grid`] starting at `(top_row, left_col)`.
    /// Each pair of subrows becomes one character cell. Cells where
    /// both halves are transparent are left untouched (so multiple
    /// canvases can be layered).
    pub fn write_to_grid(&self, grid: &mut Grid, top_row: usize, left_col: usize) {
        for char_row in 0..self.height_chars {
            for x in 0..self.width {
                let top = self.pixels[(char_row * 2) * self.width + x];
                let bot = self.pixels[(char_row * 2 + 1) * self.width + x];
                if let Some((ch, fg, bg)) = encode_pair(top, bot) {
                    grid.put_at(
                        top_row + char_row,
                        left_col + x,
                        Cell {
                            ch,
                            fg,
                            bg,
                            attrs: CellAttrs::PLAIN,
                        },
                    );
                }
            }
        }
    }
}

/// Decide which glyph + (fg, bg) renders a `(top, bottom)` subpixel
/// pair. Returns `None` when both halves are transparent — callers
/// can use that to skip writes and preserve underlying content.
pub fn encode_pair(top: Color, bot: Color) -> Option<(char, Color, Color)> {
    let top_visible = top.a > ALPHA_THRESHOLD;
    let bot_visible = bot.a > ALPHA_THRESHOLD;

    match (top_visible, bot_visible) {
        (false, false) => None,
        (true, true) => {
            if colors_match(top, bot) {
                Some(('█', top, TRANSPARENT))
            } else {
                Some(('▀', top, bot))
            }
        }
        (true, false) => Some(('▀', top, TRANSPARENT)),
        (false, true) => Some(('▄', bot, TRANSPARENT)),
    }
}

fn colors_match(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 1.0 / 255.0
        && (a.g - b.g).abs() < 1.0 / 255.0
        && (a.b - b.b).abs() < 1.0 / 255.0
        && (a.a - b.a).abs() < 1.0 / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color = Color::new(1.0, 0.0, 0.0, 1.0);
    const BLUE: Color = Color::new(0.0, 0.0, 1.0, 1.0);
    const GREEN: Color = Color::new(0.0, 1.0, 0.0, 1.0);

    #[test]
    fn both_transparent_skips() {
        assert!(encode_pair(TRANSPARENT, TRANSPARENT).is_none());
    }

    #[test]
    fn top_only_uses_upper_half_block() {
        let (ch, fg, bg) = encode_pair(RED, TRANSPARENT).unwrap();
        assert_eq!(ch, '▀');
        assert_eq!(fg.r, RED.r);
        assert_eq!(bg.a, 0.0);
    }

    #[test]
    fn bottom_only_uses_lower_half_block() {
        let (ch, fg, bg) = encode_pair(TRANSPARENT, BLUE).unwrap();
        assert_eq!(ch, '▄');
        assert_eq!(fg.b, BLUE.b);
        assert_eq!(bg.a, 0.0);
    }

    #[test]
    fn matching_colors_use_full_block() {
        let (ch, fg, _bg) = encode_pair(GREEN, GREEN).unwrap();
        assert_eq!(ch, '█');
        assert_eq!(fg.g, GREEN.g);
    }

    #[test]
    fn different_colors_split_via_upper_half() {
        let (ch, fg, bg) = encode_pair(RED, BLUE).unwrap();
        assert_eq!(ch, '▀');
        assert_eq!(fg.r, RED.r);
        assert_eq!(bg.b, BLUE.b);
    }

    #[test]
    fn set_get_round_trip() {
        let mut c = HalfBlockCanvas::new(10, 5);
        c.set(3, 7, RED);
        assert_eq!(c.get(3, 7).r, 1.0);
        assert_eq!(c.get(0, 0).a, 0.0);
    }

    #[test]
    fn out_of_bounds_set_does_nothing() {
        let mut c = HalfBlockCanvas::new(10, 5);
        c.set(99, 99, RED); // should not panic
        c.set(0, 99, RED);
        c.set(99, 0, RED);
    }

    #[test]
    fn fill_rect_clips_to_bounds() {
        let mut c = HalfBlockCanvas::new(10, 5);
        c.fill_rect(8, 8, 100, 100, RED); // clips
        assert_eq!(c.get(8, 8).r, 1.0);
        assert_eq!(c.get(9, 9).r, 1.0);
    }

    #[test]
    fn write_to_grid_renders_full_block_for_same_color_pair() {
        let mut canvas = HalfBlockCanvas::new(3, 2);
        canvas.fill_rect(0, 0, 3, 2, RED);
        let mut grid = Grid::new(10, 5, RED);
        canvas.write_to_grid(&mut grid, 0, 0);
        // Top char-row's pair is (RED, RED) → '█'.
        let cells: Vec<_> = grid.cells().collect();
        let row0_first_cell = cells.iter().find(|(r, c, _)| *r == 0 && *c == 0).unwrap();
        assert_eq!(row0_first_cell.2.ch, '█');
    }

    #[test]
    fn write_to_grid_renders_upper_half_for_different_colors() {
        let mut canvas = HalfBlockCanvas::new(1, 1);
        canvas.set(0, 0, RED);
        canvas.set(0, 1, BLUE);
        let mut grid = Grid::new(5, 5, RED);
        canvas.write_to_grid(&mut grid, 0, 0);
        let cells: Vec<_> = grid.cells().collect();
        let cell = cells.iter().find(|(r, c, _)| *r == 0 && *c == 0).unwrap();
        assert_eq!(cell.2.ch, '▀');
        assert_eq!(cell.2.fg.r, RED.r);
        assert_eq!(cell.2.bg.b, BLUE.b);
    }

    #[test]
    fn write_to_grid_skips_transparent_pair() {
        let canvas = HalfBlockCanvas::new(2, 2); // all transparent
        let mut grid = Grid::new(5, 5, RED);
        // Pre-populate the grid with a sentinel so we can verify it's
        // left untouched where the canvas is fully transparent.
        grid.put_at(0, 0, Cell { ch: 'X', fg: RED, bg: TRANSPARENT, attrs: CellAttrs::PLAIN });
        canvas.write_to_grid(&mut grid, 0, 0);
        let cells: Vec<_> = grid.cells().collect();
        let cell = cells.iter().find(|(r, c, _)| *r == 0 && *c == 0).unwrap();
        assert_eq!(cell.2.ch, 'X', "transparent pair should preserve underlying cell");
    }

    #[test]
    fn dimensions_match_construction() {
        let c = HalfBlockCanvas::new(40, 30);
        assert_eq!(c.width(), 40);
        assert_eq!(c.height_chars(), 30);
        assert_eq!(c.height_subrows(), 60);
    }
}
