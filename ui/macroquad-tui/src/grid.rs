//! Fixed character grid — the terminal's backing buffer.
//!
//! Every visible character lives in a `Cell`. The renderer walks the cells
//! once per frame and draws each at its cell coordinates. Cursor position
//! and a `dirty` flag for the cursor blink live alongside the cell array.

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

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<Cell>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    default_fg: Color,
}

impl Grid {
    pub fn new(cols: usize, rows: usize, default_fg: Color) -> Self {
        Self {
            cols,
            rows,
            cells: vec![Cell::blank(default_fg); cols * rows],
            cursor_row: 0,
            cursor_col: 0,
            default_fg,
        }
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::blank(self.default_fg);
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    pub fn put(&mut self, row: usize, col: usize, ch: char) {
        if row < self.rows && col < self.cols {
            let idx = row * self.cols + col;
            self.cells[idx] = Cell { ch, fg: self.default_fg };
        }
    }

    /// Write `ch` at the cursor and advance. Wraps to the next line at the
    /// right edge; scrolls up by one row if the next line is past the bottom.
    pub fn write_char(&mut self, ch: char) {
        if ch == '\n' {
            self.newline();
            return;
        }
        self.put(self.cursor_row, self.cursor_col, ch);
        self.cursor_col += 1;
        if self.cursor_col >= self.cols {
            self.newline();
        }
    }

    pub fn write_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.write_char(ch);
        }
    }

    pub fn newline(&mut self) {
        self.cursor_col = 0;
        self.cursor_row += 1;
        if self.cursor_row >= self.rows {
            self.scroll_up();
            self.cursor_row = self.rows - 1;
        }
    }

    fn scroll_up(&mut self) {
        for row in 0..self.rows - 1 {
            for col in 0..self.cols {
                let src = (row + 1) * self.cols + col;
                let dst = row * self.cols + col;
                self.cells[dst] = self.cells[src];
            }
        }
        let last = self.rows - 1;
        for col in 0..self.cols {
            self.cells[last * self.cols + col] = Cell::blank(self.default_fg);
        }
    }

    pub fn cells(&self) -> impl Iterator<Item = (usize, usize, &Cell)> {
        let cols = self.cols;
        self.cells
            .iter()
            .enumerate()
            .map(move |(i, c)| (i / cols, i % cols, c))
    }
}
