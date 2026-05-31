//! Cell-grid sprite primitives.
//!
//! A sprite is a fixed-size rectangle of [`Cell`]s, some of which may
//! be transparent (`None`). Painting a sprite into a [`Grid`] writes
//! the non-transparent cells at the chosen top-left corner — the
//! same compositing model as a Game Boy tile, applied to character
//! cells instead of pixels.
//!
//! ## When to use
//!
//! - Multi-cell game entities (Pacman ghosts, Space Invaders aliens,
//!   bigger UFOs / bosses) where the shape is known ahead of time
//!   and the same art repeats across many positions on screen.
//! - HUD widgets / icons rendered into the same `Grid` as the rest
//!   of the scene — health bars, key prompts, mini-map markers.
//! - Boot animations, splash screens, intro cards.
//!
//! For single-cell entities (Snake segments, Tetris blocks), the
//! existing [`Grid::put_at`] is cheaper — no need for the sprite
//! indirection. The sprite layer pays off once you're stamping the
//! same shape repeatedly.
//!
//! ## Authoring sprites
//!
//! [`CellSprite::from_ascii`] reads a multi-line string where each
//! visible character becomes a cell with the supplied `fg` colour,
//! and `.` is transparent. Convenient for hand-rolled art:
//!
//! ```ignore
//! let invader = CellSprite::from_ascii(
//!     "..███..\n.█████.\n██.█.██",
//!     RED,
//! );
//! invader.paint(&mut grid, 2, 10);
//! ```
//!
//! For programmatic sprites (e.g. computed every frame), use
//! [`CellSprite::new_blank`] + [`CellSprite::set`].
//!
//! ## What's deliberately out of scope (for now)
//!
//! - Half-block pixel sprites (2× vertical resolution). Add when a
//!   pixel-art game lands — the existing [`crate::HalfBlockCanvas`]
//!   already covers the rendering primitive.
//! - Animation frames + timing. Add when a game needs cyclic art.
//! - GPU-texture sprites. The viewer's [`crate::ShardedAtlas`]
//!   already covers that use case for image atlases.

use std::collections::HashMap;

use macroquad::prelude::Color;

use crate::grid::{Cell, CellAttrs, Grid};

/// A `width × height` rectangle of optional [`Cell`]s.
///
/// `None` slots are transparent — paint() skips them so the
/// underlying grid content shows through. This is what makes
/// multi-shape sprites composable: an L-shaped piece can live in a
/// `3 × 3` bounding box without forcing the surrounding two cells
/// to be solid.
#[derive(Clone, Debug)]
pub struct CellSprite {
    pub width: usize,
    pub height: usize,
    /// Row-major: index = `y * width + x`. Length always equals
    /// `width * height`.
    pub cells: Vec<Option<Cell>>,
}

impl CellSprite {
    /// Construct an all-transparent sprite of the given size. Use
    /// [`Self::set`] to fill cells in.
    pub fn new_blank(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![None; width * height],
        }
    }

    /// Parse a multi-line ASCII art string into a sprite. Each line
    /// becomes one row; the longest line sets `width`, shorter rows
    /// are right-padded with transparent cells.
    ///
    /// - `'.'` → transparent
    /// - `' '` (space) → transparent (lets you align art naturally)
    /// - anything else → opaque [`Cell`] with the supplied `fg`
    ///   colour, default bg, no attrs
    ///
    /// Lines may be separated by `\n` or `\r\n`. Empty input yields
    /// a 0×0 sprite.
    pub fn from_ascii(art: &str, fg: Color) -> Self {
        let rows: Vec<&str> = art.lines().collect();
        let height = rows.len();
        let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
        let transparent = Color::new(0.0, 0.0, 0.0, 0.0);
        let mut cells = vec![None; width * height];
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if ch == '.' || ch == ' ' {
                    continue;
                }
                cells[y * width + x] = Some(Cell {
                    ch,
                    fg,
                    bg: transparent,
                    attrs: CellAttrs::PLAIN,
                });
            }
        }
        Self { width, height, cells }
    }

    /// Set or clear a single cell. Out-of-bounds writes silently
    /// drop — cheaper than panicking, and consistent with `Grid`.
    pub fn set(&mut self, x: usize, y: usize, cell: Option<Cell>) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = cell;
        }
    }

    /// Read a single cell. Returns `None` for out-of-bounds OR for
    /// transparent positions; callers that need to distinguish the
    /// two should compare against `self.width`/`self.height` first.
    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x].as_ref()
        } else {
            None
        }
    }

    /// Stamp this sprite into a [`Grid`], with its top-left corner at
    /// `(top_row, left_col)`. Transparent cells preserve whatever the
    /// grid had there; opaque cells overwrite. Cells outside the
    /// grid's rows/cols are clipped silently — sprites near the edge
    /// just lose their off-screen portion.
    pub fn paint(&self, grid: &mut Grid, top_row: usize, left_col: usize) {
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(cell) = &self.cells[y * self.width + x] {
                    grid.put_at(top_row + y, left_col + x, *cell);
                }
            }
        }
    }
}

/// A named collection of sprites. Game scenes typically build one
/// of these once at construction (e.g. all enemy types + the player
/// + bullets + UI icons) and then look up by name in the per-frame
/// render loop.
///
/// The sheet itself is just a `HashMap<String, CellSprite>`; the
/// purpose of the type is documentation + a single import path
/// instead of leaking the map shape into every consumer.
#[derive(Default, Debug)]
pub struct SpriteSheet {
    sprites: HashMap<String, CellSprite>,
}

impl SpriteSheet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert / replace a sprite by name. Returns the old sprite if
    /// the name was already taken.
    pub fn insert(&mut self, name: impl Into<String>, sprite: CellSprite) -> Option<CellSprite> {
        self.sprites.insert(name.into(), sprite)
    }

    pub fn get(&self, name: &str) -> Option<&CellSprite> {
        self.sprites.get(name)
    }

    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }

    /// Iterator over all `(name, sprite)` pairs. Useful for atlas
    /// previews or hot-reload diffs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &CellSprite)> {
        self.sprites.iter().map(|(k, v)| (k.as_str(), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::Color;

    const RED: Color = Color::new(1.0, 0.0, 0.0, 1.0);
    const PHOSPHOR: Color = Color::new(0.267, 1.0, 0.267, 1.0);

    #[test]
    fn blank_sprite_is_fully_transparent() {
        let s = CellSprite::new_blank(3, 2);
        assert_eq!(s.width, 3);
        assert_eq!(s.height, 2);
        assert_eq!(s.cells.len(), 6);
        assert!(s.cells.iter().all(Option::is_none));
    }

    #[test]
    fn from_ascii_parses_rectangle() {
        let s = CellSprite::from_ascii("AB.\n.CD", RED);
        assert_eq!(s.width, 3);
        assert_eq!(s.height, 2);
        // (0,0)=A, (1,0)=B, (2,0)=transparent
        assert_eq!(s.get(0, 0).unwrap().ch, 'A');
        assert_eq!(s.get(1, 0).unwrap().ch, 'B');
        assert!(s.get(2, 0).is_none());
        // (0,1)=transparent, (1,1)=C, (2,1)=D
        assert!(s.get(0, 1).is_none());
        assert_eq!(s.get(1, 1).unwrap().ch, 'C');
        assert_eq!(s.get(2, 1).unwrap().ch, 'D');
    }

    #[test]
    fn from_ascii_pads_short_rows() {
        // Second row is shorter; should be padded with transparent
        // cells to the longest line's width.
        let s = CellSprite::from_ascii("ABCD\nXY", RED);
        assert_eq!(s.width, 4);
        assert_eq!(s.height, 2);
        assert!(s.get(2, 1).is_none());
        assert!(s.get(3, 1).is_none());
    }

    #[test]
    fn from_ascii_treats_space_and_dot_as_transparent() {
        let s = CellSprite::from_ascii("A B.C", RED);
        assert_eq!(s.get(0, 0).unwrap().ch, 'A');
        assert!(s.get(1, 0).is_none()); // space
        assert_eq!(s.get(2, 0).unwrap().ch, 'B');
        assert!(s.get(3, 0).is_none()); // dot
        assert_eq!(s.get(4, 0).unwrap().ch, 'C');
    }

    #[test]
    fn paint_writes_into_grid_at_offset() {
        let mut grid = Grid::new(10, 5, PHOSPHOR);
        let sprite = CellSprite::from_ascii("XX\nXX", RED);
        sprite.paint(&mut grid, 1, 3);
        // Grab the cells out via the public iterator — confirm we
        // wrote 4 cells starting at (row 1, col 3).
        let painted: Vec<(usize, usize, char)> = grid
            .cells()
            .filter_map(|(r, c, cell)| {
                if cell.ch == 'X' {
                    Some((r, c, cell.ch))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(painted.len(), 4);
        // Order isn't guaranteed but the positions should be exactly
        // these four; sort to compare.
        let mut got: Vec<(usize, usize)> = painted.iter().map(|(r, c, _)| (*r, *c)).collect();
        got.sort();
        assert_eq!(got, vec![(1, 3), (1, 4), (2, 3), (2, 4)]);
    }

    #[test]
    fn paint_skips_transparent_cells() {
        let mut grid = Grid::new(5, 3, PHOSPHOR);
        // Pre-write a 'Z' at (0, 1) so we can check whether paint
        // overwrote it.
        grid.put_at(
            0,
            1,
            Cell {
                ch: 'Z',
                fg: PHOSPHOR,
                bg: Color::new(0.0, 0.0, 0.0, 0.0),
                attrs: CellAttrs::PLAIN,
            },
        );
        // Sprite has a transparent middle column, so (0, 1) should
        // be preserved.
        let sprite = CellSprite::from_ascii("X.X", RED);
        sprite.paint(&mut grid, 0, 0);
        let z = grid
            .cells()
            .find(|(_, _, cell)| cell.ch == 'Z')
            .map(|(_, _, c)| c.ch);
        assert_eq!(z, Some('Z'));
    }

    #[test]
    fn paint_clips_out_of_bounds() {
        let mut grid = Grid::new(4, 3, PHOSPHOR);
        let sprite = CellSprite::from_ascii("XXX\nXXX", RED);
        // Paint with a left offset that pushes the right column past
        // the grid edge — shouldn't panic.
        sprite.paint(&mut grid, 0, 3);
        // Only the (col 3) column lands inside; cols 4 and 5 clip.
        let xs: Vec<(usize, usize)> = grid
            .cells()
            .filter_map(|(r, c, cell)| {
                if cell.ch == 'X' {
                    Some((r, c))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(xs.len(), 2);
    }

    #[test]
    fn sheet_round_trips() {
        let mut sheet = SpriteSheet::new();
        sheet.insert("invader", CellSprite::from_ascii(".█.", RED));
        sheet.insert("player", CellSprite::from_ascii("█▀█", RED));
        assert_eq!(sheet.len(), 2);
        assert!(sheet.get("invader").is_some());
        assert!(sheet.get("player").is_some());
        assert!(sheet.get("missing").is_none());
    }

    #[test]
    fn sheet_iter_yields_inserted_pairs() {
        let mut sheet = SpriteSheet::new();
        sheet.insert("a", CellSprite::from_ascii("X", RED));
        sheet.insert("b", CellSprite::from_ascii("Y", RED));
        let mut names: Vec<&str> = sheet.iter().map(|(k, _)| k).collect();
        names.sort();
        assert_eq!(names, vec!["a", "b"]);
    }
}
