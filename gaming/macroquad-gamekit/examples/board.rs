//! A fixed play field on [`Stage::Board`], stepped by a fixed tick: the
//! smallest arcade game the kit can run, and the regression capture for a
//! board centred in a wide window.
//!
//! ```sh
//! cargo run -p macroquad-gamekit --example board
//! cargo run -p macroquad-gamekit --example board -- --shot .tmp/board.png --size 1280x800 --at 1
//! ```
//!
//! Arrow keys, WASD or a swipe hop the marker one cell, and holding a key
//! keeps hopping. A press during a hop is buffered by `Actions` and taken when
//! the hop lands, so quick taps are never lost. The bar along the top slides
//! at 240 units a second on the 60 Hz tick and is drawn between ticks with
//! `Frame::alpha`.
//!
//! A scripted run hops without a keyboard, for a capture mid-game:
//!
//! ```sh
//! cargo run -p macroquad-gamekit --example board -- --shot .tmp/hops.png --keys "5:Up,7:Up,9:Left" --at 1
//! ```

use macroquad::prelude::*;
use macroquad_gamekit::harness::Args;
use macroquad_gamekit::{Actions, Backdrop, Binding, Dir4, Frame, Game, Stage, Tick, app};

const CELL: f32 = 32.0;
const COLS: i32 = 7;
const ROWS: i32 = 8;
const BOARD: Vec2 = vec2(CELL * COLS as f32, CELL * ROWS as f32);
/// Seconds a hop takes; a press during one is kept for when it lands.
const HOP: f32 = 0.12;

struct Board {
    /// The hops, one action per direction.
    hops: Actions<Dir4>,
    cell: IVec2,
    hop: Option<(IVec2, f32)>,
    bar: f32,
    bar_prev: f32,
}

impl Board {
    fn new() -> Self {
        let mut bindings = Vec::new();
        for (d, arrow, letter) in [
            (Dir4::Up, KeyCode::Up, KeyCode::W),
            (Dir4::Down, KeyCode::Down, KeyCode::S),
            (Dir4::Left, KeyCode::Left, KeyCode::A),
            (Dir4::Right, KeyCode::Right, KeyCode::D),
        ] {
            bindings.extend([
                (d, Binding::Key(arrow)),
                (d, Binding::Key(letter)),
                (d, Binding::Swipe(d)),
            ]);
        }
        let mut hops = Actions::new(&bindings).swipe_min(CELL * 0.6);
        for d in Dir4::ALL {
            hops = hops.repeat(d, 0.2, 0.15);
        }
        Self {
            hops,
            cell: ivec2(COLS / 2, ROWS - 1),
            hop: None,
            bar: 0.0,
            bar_prev: 0.0,
        }
    }

    /// Where the marker is drawn: part way through a hop, or on its cell.
    fn marker(&self) -> Vec2 {
        let at = |c: IVec2| (c.as_vec2() + 0.5) * CELL;
        match self.hop {
            Some((from, t)) => at(from).lerp(at(self.cell), (t / HOP).min(1.0)),
            None => at(self.cell),
        }
    }
}

impl Game for Board {
    fn update(&mut self, frame: &Frame) {
        self.hops.update(frame.input, frame.dt);
    }

    fn tick_rate(&self) -> Option<f32> {
        Some(60.0)
    }

    fn tick(&mut self, tick: &Tick, _frame: &Frame) {
        self.bar_prev = self.bar;
        self.bar = (self.bar + 240.0 * tick.dt) % (BOARD.x + CELL * 2.0);
        if self.bar < self.bar_prev {
            self.bar_prev = self.bar;
        }
        if let Some((_, t)) = &mut self.hop {
            *t += tick.dt;
            if *t >= HOP {
                self.hop = None;
            }
        }
        if self.hop.is_none()
            && let Some(d) = Dir4::ALL.into_iter().find(|d| self.hops.take(*d))
        {
            let next =
                (self.cell + d.vec().as_ivec2()).clamp(IVec2::ZERO, ivec2(COLS - 1, ROWS - 1));
            if next != self.cell {
                self.hop = Some((self.cell, 0.0));
                self.cell = next;
            }
        }
    }

    fn draw(&mut self, frame: &Frame) {
        draw_rectangle(0.0, 0.0, BOARD.x, BOARD.y, Color::from_hex(0x1d2b3a));
        for row in 0..ROWS {
            for col in 0..COLS {
                if (row + col) % 2 == 0 {
                    let (x, y) = (col as f32 * CELL, row as f32 * CELL);
                    draw_rectangle(x, y, CELL, CELL, Color::from_hex(0x24364a));
                }
            }
        }
        draw_rectangle_lines(0.0, 0.0, BOARD.x, BOARD.y, 2.0, Color::from_hex(0x6fa8dc));
        let bar = self.bar_prev + (self.bar - self.bar_prev) * frame.alpha - CELL;
        draw_rectangle(
            bar,
            CELL * 0.25,
            CELL * 1.5,
            CELL * 0.5,
            Color::from_hex(0xe06666),
        );
        let at = self.marker();
        draw_circle(at.x, at.y, CELL * 0.35, Color::from_hex(0x93c47d));
    }

    fn backdrop(&self) -> Backdrop {
        Backdrop {
            top: Color::from_hex(0x0b1118),
            bottom: Color::from_hex(0x0b1118),
            accent: Color::from_hex(0x6fa8dc),
        }
    }
}

fn window_conf() -> Conf {
    let args = Args::read();
    let (w, h) = args.window_size((448, 512));
    Conf {
        window_title: args.window_title("gamekit board"),
        window_width: w,
        window_height: h,
        high_dpi: true,
        window_resizable: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    app::run_on(Stage::board(BOARD), Board::new()).await;
}
