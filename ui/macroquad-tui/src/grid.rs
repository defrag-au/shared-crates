//! Character grid with scrollback buffer.
//!
//! The buffer is a `VecDeque<Vec<Cell>>` of full-width rows. The cursor
//! always lives at the *end* of the buffer (the newest row); incoming
//! characters land there. When the row fills it wraps to a new one;
//! when the buffer exceeds [`scrollback_max`] the oldest row is
//! discarded.
//!
//! Rendering is "windowed" — [`Grid::cells`] yields only the slice of
//! rows currently visible, picked by [`viewport_offset`]. An offset of
//! `0` means the viewport is anchored at the bottom (newest content
//! visible, cursor in frame); positive offsets scroll the window
//! up the buffer.
//!
//! Any `write_char` / `newline` resets `viewport_offset` to `0` — new
//! content always snaps the view back so the user sees the result of
//! their command immediately. Explicit scroll APIs ([`scroll_up`],
//! [`scroll_down`], [`scroll_to_top`], [`scroll_to_bottom`]) update
//! the offset without disturbing the buffer.

use std::collections::VecDeque;

use macroquad::prelude::*;

#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
}

impl Cell {
    pub const fn blank(fg: Color) -> Self {
        Self { ch: ' ', fg }
    }
}

/// Default scrollback capacity — number of rows retained above the
/// visible viewport. 1000 is a comfortable default for terminal-style
/// apps; the corpus-dump and tour outputs in `uap-terminal` rarely
/// exceed a couple hundred rows. Adjust via
/// [`Grid::new_with_scrollback`].
pub const DEFAULT_SCROLLBACK_MAX: usize = 1000;

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    /// All rows ever written, capped at `scrollback_max`. The cursor
    /// always sits at the end (`lines.back()`).
    lines: VecDeque<Vec<Cell>>,
    scrollback_max: usize,
    /// How many rows from the bottom the viewport is offset.
    /// `0` = at bottom (cursor visible); positive = scrolled up.
    viewport_offset: usize,
    /// Column within the cursor's line (always the last in `lines`).
    cursor_col: usize,
    default_fg: Color,
}

impl Grid {
    pub fn new(cols: usize, rows: usize, default_fg: Color) -> Self {
        Self::new_with_scrollback(cols, rows, default_fg, DEFAULT_SCROLLBACK_MAX)
    }

    pub fn new_with_scrollback(
        cols: usize,
        rows: usize,
        default_fg: Color,
        scrollback_max: usize,
    ) -> Self {
        let mut lines = VecDeque::with_capacity(rows.min(scrollback_max));
        // Seed one empty row — the cursor's line.
        lines.push_back(vec![Cell::blank(default_fg); cols]);
        Self {
            cols,
            rows,
            lines,
            scrollback_max: scrollback_max.max(rows),
            viewport_offset: 0,
            cursor_col: 0,
            default_fg,
        }
    }

    /// Wipe the buffer back to a single empty line and snap the
    /// viewport to bottom. Used by the `clear` command.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.lines
            .push_back(vec![Cell::blank(self.default_fg); self.cols]);
        self.cursor_col = 0;
        self.viewport_offset = 0;
    }

    /// Column the cursor sits at in its current line.
    pub fn cursor_col(&self) -> usize {
        self.cursor_col
    }

    /// Cursor's row index *within the visible viewport* assuming the
    /// viewport is at the bottom (offset 0). Used by renderers that
    /// position the editor prompt under the most-recent content.
    ///
    /// While the buffer is still smaller than `rows`, this returns
    /// `lines.len() - 1` so the prompt sits flush with the top.
    /// Once the buffer has overflowed past `rows`, returns `rows - 1`
    /// (the bottom row of the visible window).
    pub fn cursor_row(&self) -> usize {
        let total = self.lines.len();
        if total <= self.rows {
            total - 1
        } else {
            self.rows - 1
        }
    }

    /// `true` when the viewport is anchored above the bottom — i.e.
    /// the user has scrolled up and the cursor is below the visible
    /// window. Renderers use this to swap the prompt for a status
    /// hint or to pin the prompt at the bottom visible row.
    pub fn is_scrolled(&self) -> bool {
        self.viewport_offset > 0
    }

    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    /// How many rows still exist above the current viewport — i.e.
    /// the remaining room to scroll up. `0` means the top of the
    /// buffer is already visible.
    pub fn scrollback_above(&self) -> usize {
        let total = self.lines.len();
        total.saturating_sub(self.rows + self.viewport_offset)
    }

    /// How many rows exist below the current viewport — i.e. the
    /// remaining room to scroll back down. `0` means the bottom row
    /// (cursor row) is visible.
    pub fn scrollback_below(&self) -> usize {
        self.viewport_offset
    }

    pub fn scroll_up(&mut self, n: usize) {
        let max_offset = self.lines.len().saturating_sub(self.rows);
        self.viewport_offset = (self.viewport_offset + n).min(max_offset);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.viewport_offset = self.viewport_offset.saturating_sub(n);
    }

    pub fn scroll_to_top(&mut self) {
        self.viewport_offset = self.lines.len().saturating_sub(self.rows);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.viewport_offset = 0;
    }

    /// Write one character at the cursor and advance. Wraps to the
    /// next line at the right edge. Any new content snaps the
    /// viewport back to the bottom.
    pub fn write_char(&mut self, ch: char) {
        if ch == '\n' {
            self.newline();
            return;
        }
        let cols = self.cols;
        if self.cursor_col < cols {
            let fg = self.default_fg;
            let col = self.cursor_col;
            self.lines.back_mut().unwrap()[col] = Cell { ch, fg };
        }
        self.cursor_col += 1;
        if self.cursor_col >= cols {
            self.newline();
        }
        self.viewport_offset = 0;
    }

    pub fn write_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.write_char(ch);
        }
    }

    /// Move the cursor onto a fresh row. The previous row stays in
    /// the buffer (potentially exiting the visible viewport).
    pub fn newline(&mut self) {
        self.cursor_col = 0;
        self.lines
            .push_back(vec![Cell::blank(self.default_fg); self.cols]);
        // Cap the scrollback.
        while self.lines.len() > self.scrollback_max {
            self.lines.pop_front();
        }
        self.viewport_offset = 0;
    }

    /// Iterate visible cells as `(visual_row, col, &Cell)`. `visual_row`
    /// is the row within the viewport (0 = top); the renderer maps it
    /// to a pixel y-coordinate. When the buffer hasn't filled, the
    /// returned rows occupy the top of the viewport — matching the
    /// non-scrollback Grid's behaviour of growing downward.
    pub fn cells(&self) -> impl Iterator<Item = (usize, usize, &Cell)> {
        let total = self.lines.len();
        let visible = total.min(self.rows);
        // The newest visible row is `total - 1 - viewport_offset`.
        // The oldest visible row is `total - viewport_offset - visible`.
        let start = total.saturating_sub(self.viewport_offset + visible);
        self.lines
            .iter()
            .skip(start)
            .take(visible)
            .enumerate()
            .flat_map(move |(visual_row, line)| {
                line.iter()
                    .enumerate()
                    .map(move |(col, cell)| (visual_row, col, cell))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHOSPHOR: Color = Color::new(0.0, 1.0, 0.0, 1.0);

    fn fresh(cols: usize, rows: usize) -> Grid {
        Grid::new(cols, rows, PHOSPHOR)
    }

    fn collect_chars(g: &Grid) -> Vec<String> {
        let mut rows: Vec<Vec<char>> = Vec::new();
        for (r, c, cell) in g.cells() {
            while rows.len() <= r {
                rows.push(Vec::new());
            }
            while rows[r].len() <= c {
                rows[r].push(' ');
            }
            rows[r][c] = cell.ch;
        }
        rows.into_iter()
            .map(|r| r.iter().collect::<String>().trim_end().to_string())
            .collect()
    }

    #[test]
    fn fresh_grid_is_empty_with_cursor_at_top() {
        let g = fresh(80, 30);
        assert_eq!(g.cursor_row(), 0);
        assert_eq!(g.cursor_col(), 0);
        assert!(!g.is_scrolled());
        assert_eq!(g.scrollback_above(), 0);
    }

    #[test]
    fn writes_within_rows_grow_downward_from_top() {
        let mut g = fresh(10, 5);
        g.write_str("abc");
        g.newline();
        g.write_str("def");
        let r = collect_chars(&g);
        assert_eq!(r[0], "abc");
        assert_eq!(r[1], "def");
        assert_eq!(g.cursor_row(), 1);
    }

    #[test]
    fn writes_past_rows_keep_buffer_growing_but_cursor_pins_to_bottom() {
        let mut g = fresh(10, 5);
        // Push 10 lines so buffer has 10 rows but viewport only shows 5.
        for i in 0..10 {
            g.write_str(&format!("L{i}"));
            g.newline();
        }
        let r = collect_chars(&g);
        // Newest visible should be the empty post-newline row "" after L9.
        // The 5-row viewport shows the last 5: L6, L7, L8, L9, "".
        assert_eq!(r, vec!["L6", "L7", "L8", "L9", ""]);
        assert_eq!(g.cursor_row(), 4);
        assert!(!g.is_scrolled());
    }

    #[test]
    fn scroll_up_reveals_older_lines() {
        let mut g = fresh(10, 5);
        for i in 0..10 {
            g.write_str(&format!("L{i}"));
            g.newline();
        }
        g.scroll_up(3);
        assert!(g.is_scrolled());
        assert_eq!(g.viewport_offset(), 3);
        let r = collect_chars(&g);
        // Visible window slides back: L3..L7
        assert_eq!(r, vec!["L3", "L4", "L5", "L6", "L7"]);
    }

    #[test]
    fn scroll_up_clamps_at_top_of_buffer() {
        let mut g = fresh(10, 5);
        for i in 0..10 {
            g.write_str(&format!("L{i}"));
            g.newline();
        }
        // Buffer has 11 rows (10 writes + the empty post-newline). Max
        // offset is 11 - 5 = 6.
        g.scroll_up(99);
        assert_eq!(g.viewport_offset(), 6);
        let r = collect_chars(&g);
        assert_eq!(r, vec!["L0", "L1", "L2", "L3", "L4"]);
        assert_eq!(g.scrollback_above(), 0);
    }

    #[test]
    fn scroll_down_clamps_at_bottom() {
        let mut g = fresh(10, 5);
        for i in 0..10 {
            g.write_str(&format!("L{i}"));
            g.newline();
        }
        g.scroll_up(3);
        g.scroll_down(99);
        assert_eq!(g.viewport_offset(), 0);
        assert!(!g.is_scrolled());
    }

    #[test]
    fn new_write_snaps_viewport_to_bottom() {
        let mut g = fresh(10, 5);
        for i in 0..10 {
            g.write_str(&format!("L{i}"));
            g.newline();
        }
        g.scroll_up(3);
        assert_eq!(g.viewport_offset(), 3);
        g.write_str("new");
        // Any character write snaps back.
        assert_eq!(g.viewport_offset(), 0);
    }

    #[test]
    fn scrollback_max_caps_the_buffer() {
        let mut g = Grid::new_with_scrollback(10, 5, PHOSPHOR, 8);
        for i in 0..20 {
            g.write_str(&format!("L{i}"));
            g.newline();
        }
        // Buffer should hold at most 8 lines.
        assert!(g.lines.len() <= 8);
        // scrollback_max is enforced — older lines dropped.
    }

    #[test]
    fn clear_resets_buffer_cursor_and_viewport() {
        let mut g = fresh(10, 5);
        for i in 0..10 {
            g.write_str(&format!("L{i}"));
            g.newline();
        }
        g.scroll_up(3);
        g.clear();
        assert_eq!(g.cursor_row(), 0);
        assert_eq!(g.cursor_col(), 0);
        assert_eq!(g.viewport_offset(), 0);
        let r = collect_chars(&g);
        // After clear the only line is an empty one.
        assert!(r.iter().all(|line| line.is_empty()));
    }

    #[test]
    fn cells_returns_correct_window_at_arbitrary_offset() {
        let mut g = fresh(10, 4);
        for i in 0..8 {
            g.write_str(&format!("L{i}"));
            g.newline();
        }
        g.scroll_up(2);
        let r = collect_chars(&g);
        // Buffer has 9 lines (L0..L7 + empty post-newline). Offset 2,
        // viewport 4 → visible window is indices [3..7] → L3..L6.
        assert_eq!(r, vec!["L3", "L4", "L5", "L6"]);
    }
}
