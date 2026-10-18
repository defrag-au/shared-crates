//! `run` — the frame loop every gamekit client runs, written once: the clock,
//! the viewport, startup and the preloader, the glyph gate, captures, the
//! frame log and the native frame cap, around a [`Game`]'s update and draw.
//!
//! ```ignore
//! #[macroquad::main(window_conf)]
//! async fn main() {
//!     let font = macroquad_gamekit::load_font(FONT).ok();
//!     macroquad_gamekit::app::run(DESIGN, MyGame::new(font)).await;
//! }
//! ```
//!
//! What a game gets without writing it:
//!
//! - **Startup.** Text waits while the window settles, then glyphs are warmed
//!   behind a [`Preloader`](crate::Preloader) cover drawn in the game's
//!   [`Backdrop`], which lifts on the first frame drawn whole (see
//!   [`Startup`]). The [glyph gate](crate::text::enable_glyph_gate) is on, so a
//!   font atlas uploads at most once a frame. Load fonts with
//!   [`load_font`](crate::text::load_font) so the gate can warm them.
//! - **Captures.** `--shot out.png [--at T] [--size WxH]` draws into a target
//!   at the fixed clock and writes the PNG and its report when
//!   [`Game::capture_due`] says so, then exits.
//! - **Diagnostics.** `--frame-log` prints viewport changes, glyph caching,
//!   the moment the screen went live, and every frame slower than
//!   [`SLOW_FRAME`](harness::SLOW_FRAME) with where its time went. `--seconds N` exits after `N`
//!   seconds, a safety valve for native runs.
//! - **Native pacing.** miniquad on macOS disables vsync and paces with
//!   CVDisplayLink; if that link is missing, `next_frame` returns at once and
//!   spins the machine. The loop is capped at 60 frames a second natively;
//!   the browser's requestAnimationFrame already paces the web build.
//!
//! Modes that are not the game loop (a soak, a sheet of faces) stay in the
//! game's `main`, before it calls [`run`].
//!
//! The loop reads the wall clock only through the harness ([`frame_dt`] and
//! [`FrameTimer`]). A [`Game`] sees `dt` and nothing else, so it is stepped
//! and captured like everything else in the kit.

use macroquad::prelude::*;

use crate::harness::{
    self, Args, Capture, FrameMark, FrameTimer, Viewport, frame_dt, window_camera,
};
use crate::startup::Startup;
use crate::text::enable_glyph_gate;

/// What a [`Game`] sees each frame.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub args: &'a Args,
    /// Seconds since the last frame: [`harness::FIXED_DT`] when capturing.
    pub dt: f32,
    /// Seconds since the loop started, on the same clock.
    pub clock: f32,
    /// Frames drawn before this one.
    pub frames: u32,
    pub viewport: Viewport,
    /// Whether to draw text: false while the window settles.
    pub text: bool,
}

/// The colours the preloader's cover is drawn in: the game's own background,
/// so lifting it reads as the content arriving rather than a curtain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Backdrop {
    pub top: Color,
    pub bottom: Color,
    /// The spinner.
    pub accent: Color,
}

/// A screen the loop drives.
pub trait Game {
    /// Advance by `frame.dt` and read input.
    fn update(&mut self, frame: &Frame);

    /// Draw the frame, background included, in design units: the camera is
    /// already set to [`Frame::viewport`]. Draw text only when
    /// [`Frame::text`].
    fn draw(&mut self, frame: &Frame);

    /// The preloader's colours.
    fn backdrop(&self) -> Backdrop;

    /// Whether the game has what it needs to draw whole: its data is in, its
    /// assets are loaded. The preloader stays up until it is.
    fn ready(&self) -> bool {
        true
    }

    /// Whether a `--shot` capture should be taken this frame. The loop
    /// already waits for the third frame. By default: once ready, and at
    /// `--at` if one was given.
    fn capture_due(&self, frame: &Frame) -> bool {
        self.ready() && frame.args.at.is_none_or(|at| frame.clock >= at)
    }

    /// The line a capture's report opens with.
    fn capture_note(&self, frame: &Frame) -> String {
        format!("t={:.2}s", frame.clock)
    }

    /// Camera shake this frame, in design units.
    fn shake(&self) -> Vec2 {
        Vec2::ZERO
    }
}

/// Run `game` on a screen designed at `design` units, until the window
/// closes, a capture is written, or `--seconds` runs out.
pub async fn run(design: Vec2, mut game: impl Game) {
    let args = Args::read();
    let capturing = args.capturing();
    if capturing {
        harness::report::enable();
    }
    enable_glyph_gate();
    let mut startup = if capturing {
        Startup::capture()
    } else {
        Startup::new()
    };
    let exit_after = harness::flag_num::<f32>("--seconds");
    let mut clock = 0.0f32;
    let mut frames = 0u32;

    loop {
        let mut timer = FrameTimer::start();
        let dt = frame_dt(capturing);
        clock += dt;
        let viewport = args.viewport(design);
        let log = |line: &str| {
            if args.frame_log {
                eprintln!("t={clock:.2} {line}");
            }
        };
        if startup.begin(dt, viewport.px()) {
            log(&format!(
                "viewport {:.0}x{:.0} points, {:.2} px per design unit",
                screen_width(),
                screen_height(),
                viewport.px()
            ));
        }
        let frame = Frame {
            args: &args,
            dt,
            clock,
            frames,
            viewport,
            text: startup.text(),
        };

        game.update(&frame);
        timer.mark(FrameMark::Update);

        let due = args.shot.is_some() && frames >= 2 && game.capture_due(&frame);
        let capture = due.then(|| Capture::window(&viewport, game.shake()));
        if capture.is_none() {
            set_camera(&window_camera(viewport.size, game.shake()));
        }
        game.draw(&frame);
        timer.mark(FrameMark::Draw);

        let finished = startup.finish(dt, game.ready());
        timer.mark(FrameMark::Glyphs);
        let flushed = finished.flushed;
        if flushed.cached > 0 {
            log(&format!(
                "cached glyphs for {} new sizes or characters, {} left",
                flushed.cached, flushed.remaining
            ));
        }
        if finished.went_live {
            log("screen whole, now live");
        }
        let backdrop = game.backdrop();
        startup.draw_cover(
            Rect::new(0.0, 0.0, viewport.size.x, viewport.size.y),
            backdrop.top,
            backdrop.bottom,
            backdrop.accent,
        );

        if let (Some(path), Some(capture)) = (args.shot.as_ref(), capture) {
            save(capture, path, &game.capture_note(&frame));
        }

        next_frame().await;
        timer.mark(FrameMark::Present);
        if let Some(line) = timer.slow() {
            log(&line);
        }
        #[cfg(not(target_arch = "wasm32"))]
        timer.pace();

        if exit_after.is_some_and(|seconds| clock >= seconds) {
            std::process::exit(0);
        }
        frames += 1;
    }
}

/// Write a capture and its report, say where, and exit.
pub fn save(capture: Capture, path: &str, note: &str) -> ! {
    match capture.save(path, note) {
        Ok(report) => println!("wrote {path} and {}", report.display()),
        Err(error) => {
            eprintln!("wrote {path}, but not its report: {error}");
            std::process::exit(1);
        }
    }
    std::process::exit(0);
}
