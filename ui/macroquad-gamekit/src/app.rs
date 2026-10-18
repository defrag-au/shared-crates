//! `run` — the frame loop every gamekit client runs, written once: the clock,
//! the stage, startup and the preloader, the glyph gate, captures, the frame
//! log and the native frame cap, around a [`Game`]'s update and draw.
//!
//! ```ignore
//! #[macroquad::main(window_conf)]
//! async fn main() {
//!     let font = macroquad_gamekit::load_font(FONT).ok();
//!     macroquad_gamekit::app::run(DESIGN, MyGame::new(font)).await;
//! }
//! ```
//!
//! ## Stages
//!
//! A game draws on a [`Stage`]. [`Stage::Design`] is design units scaled to fit
//! the window and drawn at its device pixels, so text and art stay sharp at
//! any size ([`run`]). [`Stage::Pixel`] is a fixed canvas — 320×224, say —
//! drawn nearest-neighbour and scaled to the window by the largest whole
//! number that fits, letterboxed, so pixels stay square and chunky
//! ([`run_on`]). The loop owns the canvas: a game draws into it as it would
//! the window, and the loop presents it.
//!
//! ## What a game gets without writing it
//!
//! - **Startup.** On a design stage, text waits while the window settles. Then
//!   glyphs are warmed behind a [`Preloader`](crate::Preloader) cover drawn
//!   in the game's [`Backdrop`], which lifts on the first frame drawn whole
//!   (see [`Startup`]). The [glyph gate](crate::text::enable_glyph_gate) is
//!   on, so a font atlas uploads at most once a frame. Load fonts with
//!   [`load_font`](crate::text::load_font) so the gate can warm them.
//! - **Captures.** `--shot out.png [--at T] [--size WxH]` draws on the fixed
//!   clock and writes the PNG and its report when [`Game::capture_due`] says
//!   so, then exits. A pixel stage's capture is its canvas, at 1:1.
//! - **Scripted runs.** A game that reviews itself — bots playing to set
//!   moments, a screenshot at each — says so through [`Game::scripted`]: the
//!   loop runs the fixed clock without a cover, records text for reports, and
//!   hands it each finished frame in [`Game::presented`].
//! - **Diagnostics.** `--frame-log` prints viewport changes, glyph caching,
//!   the moment the screen went live, and every frame slower than
//!   [`SLOW_FRAME`](harness::SLOW_FRAME) with where its time went.
//!   `--seconds N` exits after `N` seconds, a safety valve for native runs.
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
    self, Args, Capture, FrameMark, FrameTimer, Viewport, frame_dt, plain_target, target_camera,
    window_camera,
};
use crate::startup::Startup;
use crate::text::enable_glyph_gate;

/// Where a game draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    /// Design units, scaled uniformly to fit the window (the roomier side
    /// grows) and drawn at its device pixels.
    Design(Vec2),
    /// A canvas of `width` × `height` pixels, drawn nearest-neighbour and
    /// scaled to the window by the largest whole number that fits (or shrunk,
    /// if the window is smaller), centred, letterboxed in black.
    Pixel { width: u32, height: u32 },
}

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
    /// The stage in its own units, and how it maps to the window: `size` is
    /// the design (or canvas) size, `u` window points per unit.
    pub viewport: Viewport,
    /// Device pixels per stage unit where the game draws: the pixel scale to
    /// draw text at. The window's on a design stage; 1 on a pixel stage, where
    /// text rasterises at the canvas's own pixels.
    pub px: f32,
    /// Where the stage's top-left sits in the window, in points: non-zero on
    /// a letterboxed pixel stage.
    pub origin: Vec2,
    /// Whether to draw text: false while the window settles.
    pub text: bool,
    /// A pixel stage's canvas. Finished only in [`Game::presented`]: draws into
    /// it run when the loop presents it.
    pub canvas: Option<&'a Texture2D>,
}

impl Frame<'_> {
    /// The mouse, in stage units.
    pub fn pointer(&self) -> Vec2 {
        (Vec2::from(mouse_position()) - self.origin) / self.viewport.u
    }
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

    /// Draw the frame, background included, in stage units: the camera is
    /// already set, to the window or to the canvas. Draw text only when
    /// [`Frame::text`], at [`Frame::px`]. A game that binds render targets
    /// of its own saves and restores the camera around them
    /// (`push_camera_state`), so it ends where it began.
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

    /// Camera shake this frame, in stage units.
    fn shake(&self) -> Vec2 {
        Vec2::ZERO
    }

    /// Whether this run is the game reviewing itself — bots playing to set
    /// moments and saving what they see. Asked once, before the first frame:
    /// a scripted run is on the fixed clock, has no preloader cover, and
    /// records text for reports, as a `--shot` capture does.
    fn scripted(&self) -> bool {
        false
    }

    /// After the frame is presented. On a pixel stage [`Frame::canvas`] is
    /// the finished frame: what a scripted run saves
    /// ([`harness::write_capture`]).
    fn presented(&mut self, _frame: &Frame) {}
}

/// Run `game` on a screen designed at `design` units, until the window
/// closes, a capture is written, or `--seconds` runs out.
pub async fn run(design: Vec2, game: impl Game) {
    run_on(Stage::Design(design), game).await;
}

/// Run `game` on `stage`; see [`run`].
pub async fn run_on(stage: Stage, mut game: impl Game) {
    let args = Args::read();
    let scripted = game.scripted();
    let capturing = args.capturing() || scripted;
    if capturing {
        harness::report::enable();
    }
    enable_glyph_gate();
    let canvas = match stage {
        Stage::Pixel { width, height } => {
            let target = plain_target(width, height);
            target.texture.set_filter(FilterMode::Nearest);
            Some(target)
        }
        Stage::Design(_) => None,
    };
    let mut startup = match (capturing, &canvas) {
        (true, _) => Startup::capture(),
        // A canvas's text never changes scale: nothing to settle.
        (false, Some(_)) => Startup::steady(),
        (false, None) => Startup::new(),
    };
    let exit_after = harness::flag_num::<f32>("--seconds");
    let mut clock = 0.0f32;
    let mut frames = 0u32;

    loop {
        let mut timer = FrameTimer::start();
        let dt = frame_dt(capturing);
        clock += dt;
        let (viewport, px, origin) = match stage {
            Stage::Design(design) => {
                let vp = args.viewport(design);
                (vp, vp.px(), Vec2::ZERO)
            }
            Stage::Pixel { width, height } => {
                let size = vec2(width as f32, height as f32);
                let screen = vec2(screen_width(), screen_height());
                let dpi = screen_dpi_scale();
                let (u, origin) = pixel_fit(size, screen, dpi);
                (Viewport { u, size, dpi }, 1.0, origin)
            }
        };
        let log = |line: &str| {
            if args.frame_log {
                eprintln!("t={clock:.2} {line}");
            }
        };
        if startup.begin(dt, px) {
            log(&format!(
                "viewport {:.0}x{:.0} points, {:.2} px per stage unit",
                screen_width(),
                screen_height(),
                viewport.u * viewport.dpi
            ));
        }
        let frame = Frame {
            args: &args,
            dt,
            clock,
            frames,
            viewport,
            px,
            origin,
            text: startup.text(),
            canvas: canvas.as_ref().map(|target| &target.texture),
        };

        game.update(&frame);
        timer.mark(FrameMark::Update);

        let due = args.shot.is_some() && frames >= 2 && game.capture_due(&frame);
        let shake = game.shake();
        // The design stage's capture draws into its own target; the pixel
        // stage's is the canvas.
        let mut capture = None;
        match &canvas {
            Some(target) => {
                if capturing {
                    harness::report::begin_screen(viewport.size);
                }
                set_camera(&target_camera(viewport.size, shake, target));
            }
            None if due => capture = Some(Capture::window(&viewport, shake)),
            None => set_camera(&window_camera(viewport.size, shake)),
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

        if let Some(target) = &canvas {
            present(target, viewport, origin);
            if due && let Some(path) = args.shot.as_ref() {
                let note = game.capture_note(&frame);
                finish_capture(path, harness::write_capture(&target.texture, path, &note));
            }
        } else if let (Some(path), Some(capture)) = (args.shot.as_ref(), capture) {
            save(capture, path, &game.capture_note(&frame));
        }
        game.presented(&frame);

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

/// Window points per canvas pixel, and where the canvas's top-left lands, for
/// a `canvas` shown in a window of `screen` points at `dpi` device pixels a
/// point: the largest whole number of device pixels per canvas pixel that
/// fits (so every canvas pixel is the same square), or a fraction when even
/// one does not.
fn pixel_fit(canvas: Vec2, screen: Vec2, dpi: f32) -> (f32, Vec2) {
    let dpi = dpi.max(0.01);
    let fit = (screen * dpi / canvas).min_element();
    let device = if fit >= 1.0 { fit.floor() } else { fit };
    let u = device / dpi;
    let origin = ((screen - canvas * u) / 2.0).floor();
    (u, origin)
}

/// Draw the canvas onto the window, scaled and letterboxed.
fn present(target: &RenderTarget, viewport: Viewport, origin: Vec2) {
    set_default_camera();
    clear_background(BLACK);
    draw_texture_ex(
        &target.texture,
        origin.x,
        origin.y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(viewport.size * viewport.u),
            // A render target comes out upside down under the window camera.
            flip_y: true,
            ..Default::default()
        },
    );
}

/// Write a capture and its report, say where, and exit.
pub fn save(capture: Capture, path: &str, note: &str) -> ! {
    finish_capture(path, capture.save(path, note))
}

fn finish_capture(path: &str, written: std::io::Result<std::path::PathBuf>) -> ! {
    match written {
        Ok(report) => println!("wrote {path} and {}", report.display()),
        Err(error) => {
            eprintln!("wrote {path}, but not its report: {error}");
            std::process::exit(1);
        }
    }
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pixel_canvas_scales_by_whole_device_pixels_and_centres() {
        // 320x224 in a 960x672-point window at 1x: exactly 3x, no border.
        let (u, origin) = pixel_fit(vec2(320.0, 224.0), vec2(960.0, 672.0), 1.0);
        assert_eq!(u, 3.0);
        assert_eq!(origin, Vec2::ZERO);
        // A wider window: still 3x, centred with bars at the sides.
        let (u, origin) = pixel_fit(vec2(320.0, 224.0), vec2(1200.0, 700.0), 1.0);
        assert_eq!(u, 3.0);
        assert_eq!(origin, vec2(120.0, 14.0));
    }

    #[test]
    fn on_a_retina_window_the_whole_number_is_in_device_pixels() {
        // 849x976 points at 2x is 1698x1952 device pixels: 5 device pixels
        // per canvas pixel, 2.5 points.
        let (u, _) = pixel_fit(vec2(320.0, 224.0), vec2(849.0, 976.0), 2.0);
        assert_eq!(u, 2.5);
    }

    #[test]
    fn a_window_smaller_than_the_canvas_shrinks_it_rather_than_cropping() {
        let (u, _) = pixel_fit(vec2(320.0, 224.0), vec2(160.0, 112.0), 1.0);
        assert_eq!(u, 0.5);
    }
}
