//! Typewriter print queue — writes characters into the grid at a
//! configurable rate so output feels like real teletype/CRT output rather
//! than instant text dumps. The user's prompt is gated while the queue
//! drains so they can't type over still-typing output.
//!
//! Speed can change mid-stream: commands like `query` print a slow
//! `▌ SCANNING ARCHIVE...` prelude at the default rate, bump to burst
//! speed for the result table, then drop back to the default for any
//! trailing footer.

use std::collections::VecDeque;

use crate::grid::{Cell, Grid};

#[derive(Clone, Copy)]
enum Op {
    Char(char),
    /// Pre-styled cell — paints at the cursor with its own fg/bg/attrs.
    /// Counts as one "character" of typewriter progress.
    Cell(Cell),
    SetSpeed(f32),
}

pub struct TypeQueue {
    pending: VecDeque<Op>,
    chars_per_second: f32,
    /// The rate to restore to via `set_speed`; the queue itself holds any
    /// transient speed changes as `Op::SetSpeed` markers.
    default_cps: f32,
    accumulator: f32,
}

impl TypeQueue {
    pub fn new(chars_per_second: f32) -> Self {
        Self {
            pending: VecDeque::new(),
            chars_per_second,
            default_cps: chars_per_second,
            accumulator: 0.0,
        }
    }

    pub fn enqueue(&mut self, s: &str) {
        for ch in s.chars() {
            self.pending.push_back(Op::Char(ch));
        }
    }

    pub fn enqueue_line(&mut self, s: &str) {
        self.enqueue(s);
        self.pending.push_back(Op::Char('\n'));
    }

    /// Enqueue a pre-styled [`Cell`]. Use this to emit coloured output
    /// (half-block sprites, palette-mapped sketches) where the source
    /// already knows the exact fg/bg/attrs — `enqueue` would force the
    /// default fg.
    pub fn enqueue_cell(&mut self, cell: Cell) {
        self.pending.push_back(Op::Cell(cell));
    }

    /// Enqueue a row of pre-styled cells followed by a newline.
    pub fn enqueue_cell_line(&mut self, cells: &[Cell]) {
        for &c in cells {
            self.pending.push_back(Op::Cell(c));
        }
        self.pending.push_back(Op::Char('\n'));
    }

    /// Push a speed change into the queue. Takes effect when the typer
    /// reaches this point in the stream — not immediately. Use
    /// `set_speed_now` to change the current rate without queuing.
    pub fn enqueue_speed_change(&mut self, chars_per_second: f32) {
        self.pending.push_back(Op::SetSpeed(chars_per_second));
    }

    pub fn is_idle(&self) -> bool {
        self.pending.is_empty()
    }

    /// Advance the typewriter by `dt` seconds; write as many characters as
    /// the accumulated time budget allows. Returns the count written this
    /// tick so callers can decide whether to schedule a frame redraw.
    pub fn tick(&mut self, grid: &mut Grid, dt: f32) -> usize {
        if self.pending.is_empty() {
            self.accumulator = 0.0;
            return 0;
        }
        self.accumulator += dt * self.chars_per_second;
        let mut written = 0;
        while self.accumulator >= 1.0 || matches!(self.pending.front(), Some(Op::SetSpeed(_))) {
            let Some(op) = self.pending.pop_front() else {
                break;
            };
            match op {
                Op::Char(ch) => {
                    grid.write_char(ch);
                    written += 1;
                    self.accumulator -= 1.0;
                }
                Op::Cell(c) => {
                    grid.write_cell(c);
                    written += 1;
                    self.accumulator -= 1.0;
                }
                Op::SetSpeed(rate) => {
                    self.chars_per_second = rate;
                    // Re-accumulate against the new rate so we don't lose
                    // a fraction-of-a-character credit across the boundary.
                    // (No-op: accumulator is already in "characters" units.)
                }
            }
            if self.pending.is_empty() {
                self.accumulator = 0.0;
                break;
            }
        }
        written
    }

    /// Drop any queued output without writing it. Used when the user clears
    /// the screen mid-print.
    pub fn cancel(&mut self) {
        self.pending.clear();
        self.accumulator = 0.0;
        self.chars_per_second = self.default_cps;
    }

    /// Drain the entire queue into the grid in one shot. Used to skip the
    /// typing animation (commonly bound to Enter during printing). Speed
    /// changes still take effect in case there's any tail printing after
    /// — but in practice the queue is empty after this.
    pub fn flush(&mut self, grid: &mut Grid) {
        while let Some(op) = self.pending.pop_front() {
            match op {
                Op::Char(ch) => grid.write_char(ch),
                Op::Cell(c) => grid.write_cell(c),
                Op::SetSpeed(rate) => self.chars_per_second = rate,
            }
        }
        self.accumulator = 0.0;
    }
}
