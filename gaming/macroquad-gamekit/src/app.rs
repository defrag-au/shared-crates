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
//! - **Capture plans.** `--plan plan.txt` runs many conditions in one
//!   launch: per section, [`Game::reset`], the game's own [`Game::setup`]
//!   lines, scripted keys, and the frames to capture. See [`crate::plan`].
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
//!   Captures and scripted runs are not capped: on the fixed clock they come
//!   out the same at any speed, so `--at 90` takes seconds, not 90 of them.
//!
//! Modes that are not the game loop (a soak, a sheet of faces) stay in the
//! game's `main`, before it calls [`run`].
//!
//! The loop reads the wall clock only through the harness ([`frame_dt`] and
//! [`FrameTimer`]). A [`Game`] sees `dt` and nothing else, so it is stepped
//! and captured like everything else in the kit.

use macroquad::prelude::*;
use macroquad_widgets::{Gesture, Gestures};

use crate::harness::{
    self, Args, Capture, FrameMark, FrameTimer, KeyScript, Viewport, frame_dt, plain_target,
    target_camera, window_camera,
};
use crate::input::InputSnapshot;
use crate::plan::{Next, Plan, PlanRun};
use crate::startup::Startup;
use crate::step::{FixedStep, Tick};
use crate::text::enable_glyph_gate;

/// Where a game draws. Not `PartialEq`: [`Stage::PixelFit`] holds a function,
/// and function pointers do not compare reliably.
#[derive(Clone, Copy, Debug)]
pub enum Stage {
    /// Design units, scaled uniformly to fit the window (the roomier side
    /// grows) and drawn at its device pixels.
    Design(Vec2),
    /// A canvas of `width` × `height` pixels, drawn nearest-neighbour and
    /// scaled to the window by the largest whole number that fits (or shrunk,
    /// if the window is smaller), centred, letterboxed in black.
    Pixel { width: u32, height: u32 },
    /// A pixel canvas sized by the game: the function is given the window in
    /// points and its DPI scale, and returns the canvas size, every frame.
    /// When the answer changes (a phone rotated, a window resized) the loop
    /// makes a new canvas of that size; [`Frame::viewport`]'s `size` is the
    /// canvas a frame draws on, so a game reads its size from there.
    ///
    /// Return the screen in device pixels divided by a whole number, and the
    /// canvas fills the window edge to edge with no letterbox (Island Funk's
    /// `canvas_for`).
    PixelFit(fn(Vec2, f32) -> Vec2),
    /// A fixed play field of `size` design units, drawn at the window's device
    /// pixels like [`Stage::Design`], but fitted whole and placed by `align`
    /// in the window's spare room: `(0.5, 0.5)` centres it, `(0.5, 1.0)` sits
    /// it on the bottom edge. For an arcade board, which must not grow.
    ///
    /// The game still draws from (0, 0) to `size`, and anything it draws past
    /// the board (a car entering its lane) is hidden: after [`Game::draw`] the
    /// loop paints the margins in [`Backdrop::bottom`]. To put something in
    /// them (a frame, a score down the side) draw it in
    /// [`Game::draw_margin`], over [`Frame::bounds`], which reaches past the
    /// board to the window's edges (negative on the top and left).
    Board { size: Vec2, align: Vec2 },
}

impl Stage {
    /// A [`Stage::Board`] of `size`, centred.
    pub fn board(size: Vec2) -> Self {
        Stage::Board {
            size,
            align: vec2(0.5, 0.5),
        }
    }
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
    /// a letterboxed pixel stage and a placed board.
    pub origin: Vec2,
    /// Everything the game can draw on, in stage units: the stage itself on
    /// a design or pixel stage; a board plus its margins.
    pub bounds: Rect,
    /// This frame's input: keys held and pressed (with any `--keys` script
    /// merged in), and taps, drags and page-turn swipes in stage units. Feed
    /// it to [`Actions`](crate::Actions); feed `input.gesture` to a
    /// [`Pointer`](crate::Pointer) or [`Swipes`](crate::Swipes). The loop owns
    /// the one gesture recogniser. Its tap and drag thresholds stay in window
    /// points, because a finger's wobble is the same size at any scale.
    pub input: &'a InputSnapshot,
    /// How far into the next simulation tick the clock is, 0 to 1, for a game
    /// with a [`Game::tick_rate`]: draw moving things at
    /// `prev.lerp(now, alpha)`. 1 for a game without one.
    pub alpha: f32,
    /// Whether to draw text: false while the window settles.
    pub text: bool,
    /// A pixel stage's canvas. Finished only in [`Game::presented`]: draws into
    /// it run when the loop presents it.
    pub canvas: Option<&'a Texture2D>,
}

impl Frame<'_> {
    /// The mouse, in stage units.
    pub fn pointer(&self) -> Vec2 {
        self.to_stage(Vec2::from(mouse_position()))
    }

    /// A window point (as macroquad reports the mouse and touches) in stage
    /// units.
    pub fn to_stage(&self, p: Vec2) -> Vec2 {
        (p - self.origin) / self.viewport.u
    }
}

/// `g`, its points mapped by `to`.
fn gesture_in(g: Gesture, to: impl Fn(Vec2) -> Vec2) -> Gesture {
    Gesture {
        tap: g.tap.map(&to),
        drag: g.drag.map(&to),
        ..g
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
    /// Advance by `frame.dt` and read input. Runs once a frame, before any
    /// [`Game::tick`]: read input here, and leave it for the ticks to act on.
    fn update(&mut self, frame: &Frame);

    /// The simulation's fixed rate, in ticks a second, or `None` (the
    /// default) for a game that steps everything in [`Game::update`]. Asked
    /// once, before the first frame.
    ///
    /// With a rate, the loop calls [`Game::tick`] as many times as are due
    /// each frame, after `update` and before `draw`: 0 or 1 at 60 Hz on a
    /// 60 Hz display, more after a slow frame. On the capture clock a 60 Hz
    /// game ticks exactly once a frame, so a capture is the same moment every
    /// run.
    fn tick_rate(&self) -> Option<f32> {
        None
    }

    /// Advance the simulation by one tick of `tick.dt` seconds. Physics,
    /// collisions, spawns and rules belong here; tweens and effects stay in
    /// `update`.
    fn tick(&mut self, _tick: &Tick, _frame: &Frame) {}

    /// Draw the frame, background included, in stage units: the camera is
    /// already set, to the window or to the canvas. Draw text only when
    /// [`Frame::text`], at [`Frame::px`]. A game that binds render targets
    /// of its own saves and restores the camera around them
    /// (`push_camera_state`), so it ends where it began.
    fn draw(&mut self, frame: &Frame);

    /// On a [`Stage::Board`], draw in the margins around the board, after the
    /// loop has painted them over whatever [`Game::draw`] left there. The
    /// camera is the one `draw` had. By default, nothing: plain margins.
    fn draw_margin(&mut self, _frame: &Frame) {}

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

    /// A capture plan's `set` line (`--plan`, see [`crate::plan`]), without
    /// the `set`: put the game in a named condition, say `facing left` or
    /// `leader DaKing`. The words are the game's own. An `Err` stops the run,
    /// naming the plan's line. By default every line is refused: a game opts
    /// in by saying what it understands.
    fn setup(&mut self, line: &str) -> Result<(), String> {
        Err(format!("this game takes no `set` lines (`{line}`)"))
    }

    /// Before each section of a capture plan: back to how the game starts,
    /// so sections don't depend on one another. By default nothing, and each
    /// section runs on from where the last left off.
    fn reset(&mut self) {}
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
    let mut canvas = match fitted(stage, &args) {
        Stage::Pixel { width, height } => Some(pixel_canvas(width, height)),
        _ => None,
    };
    let mut startup = match (capturing, &canvas) {
        (true, _) => Startup::capture(),
        // A canvas's text never changes scale: nothing to settle.
        (false, Some(_)) => Startup::steady(),
        (false, None) => Startup::new(),
    };
    let exit_after = harness::flag_num::<f32>("--seconds");
    let mut sim = Sim {
        clock: 0.0,
        frames: 0,
        step: game.tick_rate().map(FixedStep::new),
        gestures: Gestures::new(),
    };
    // A `--shot --at T` capture skips drawing on its way to T: see
    // `fast_forward_to`.
    let fast_forward = fast_forward_to(&args, scripted);
    // A `--plan`: checked whole before anything runs.
    let mut plan = args.plan.as_deref().map(|path| match Plan::load(path) {
        Ok(plan) => PlanRun::new(plan),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2)
        }
    });

    loop {
        // A fitted stage resolves to a plain pixel stage each frame, and gets
        // a new canvas when its size changes.
        let stage = fitted(stage, &args);
        if let Stage::Pixel { width, height } = stage
            && canvas
                .as_ref()
                .is_none_or(|c| c.texture.size() != vec2(width as f32, height as f32))
        {
            canvas = Some(pixel_canvas(width, height));
        }
        if let Some(at) = fast_forward {
            while sim.frames >= SETTLE_FRAMES
                && sim.clock + harness::FIXED_DT * (DRAWN_BEFORE_AT as f32) < at
            {
                let placed = place(stage, &args);
                let text = startup.text();
                let keys = (&args.keys[..], sim.frames);
                simulate(
                    &mut game,
                    &mut sim,
                    &args,
                    keys,
                    placed,
                    harness::FIXED_DT,
                    text,
                );
                sim.frames += 1;
            }
        }
        if let Some(run) = plan.as_mut() {
            // A plan begins once the game can be captured, and sets each
            // section up as it reaches it: back to the start, then its `set`
            // lines in order.
            if !run.started() && sim.frames >= SETTLE_FRAMES && game.ready() {
                run.start();
            }
            if let Some(section) = run.take_setup() {
                game.reset();
                for (line, set) in &section.sets {
                    if let Err(e) = game.setup(set) {
                        eprintln!("plan line {line}: {e}");
                        std::process::exit(2);
                    }
                }
            }
            // As `--at` does: skip drawing on the way to the next capture,
            // drawing only the last few frames before it.
            while run.until_next().is_some_and(|n| n > DRAWN_BEFORE_AT) {
                let placed = place(stage, &args);
                let text = startup.text();
                simulate(
                    &mut game,
                    &mut sim,
                    &args,
                    run.keys(),
                    placed,
                    harness::FIXED_DT,
                    text,
                );
                sim.frames += 1;
                run.advance();
            }
        }

        let mut timer = FrameTimer::start();
        let dt = frame_dt(capturing);
        let placed = place(stage, &args);
        let Placed {
            viewport,
            px,
            origin,
            ..
        } = placed;
        let clock = sim.clock + dt;
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
        let text = startup.text();
        let keys = plan
            .as_ref()
            .map_or((&args.keys[..], sim.frames), PlanRun::keys);
        let (input, alpha) = simulate(&mut game, &mut sim, &args, keys, placed, dt, text);
        let frames = sim.frames;
        // Where a plan's capture of this frame goes, if it takes one.
        let plan_shot = plan.as_ref().and_then(|run| run.due().map(str::to_string));
        let frame = Frame {
            args: &args,
            dt,
            clock,
            frames,
            viewport,
            px,
            origin,
            bounds: placed.bounds,
            input: &input,
            alpha,
            text,
            canvas: canvas.as_ref().map(|target| &target.texture),
        };
        timer.mark(FrameMark::Update);

        let due =
            plan_shot.is_some() || (args.shot.is_some() && frames >= 2 && game.capture_due(&frame));
        // A board's offset into the window rides on the camera, with the shake.
        let shake = game.shake() + placed.offset;
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
            None if due => capture = Some(Capture::new(placed.window, px, shake)),
            None => set_camera(&window_camera(placed.window, shake)),
        }
        let board = matches!(stage, Stage::Board { .. });
        if board {
            clear_background(game.backdrop().bottom);
        }
        game.draw(&frame);
        if board {
            let fill = game.backdrop().bottom;
            for m in margins(placed.bounds, viewport.size) {
                draw_rectangle(m.x, m.y, m.w, m.h, fill);
            }
            game.draw_margin(&frame);
        }
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
            placed.bounds,
            backdrop.top,
            backdrop.bottom,
            backdrop.accent,
        );

        // A plan's capture is written and the run goes on; a `--shot` is
        // written and the run ends.
        let plan_note = |run: &PlanRun| {
            format!(
                "{} [{}] frame {}",
                game.capture_note(&frame),
                run.section_name(),
                run.frame()
            )
        };
        if let Some(target) = &canvas {
            present(target, viewport, origin);
            if let (Some(path), Some(run)) = (&plan_shot, plan.as_mut()) {
                let note = plan_note(run);
                wrote_planned(path, harness::write_capture(&target.texture, path, &note));
                run.wrote();
            } else if due && let Some(path) = args.shot.as_ref() {
                let note = game.capture_note(&frame);
                finish_capture(path, harness::write_capture(&target.texture, path, &note));
            }
        } else if let Some(capture) = capture {
            if let (Some(path), Some(run)) = (&plan_shot, plan.as_mut()) {
                let note = plan_note(run);
                wrote_planned(path, capture.save(path, &note));
                run.wrote();
            } else if let Some(path) = args.shot.as_ref() {
                save(capture, path, &game.capture_note(&frame));
            }
        }
        game.presented(&frame);

        next_frame().await;
        timer.mark(FrameMark::Present);
        if let Some(line) = timer.slow() {
            log(&line);
        }
        // A capture or scripted run is on the fixed clock, so it comes out the
        // same at any speed: no cap. (`next_frame` may still wait for the
        // display; a `--at` capture skips most frames instead.)
        #[cfg(not(target_arch = "wasm32"))]
        if !capturing {
            timer.pace();
        }

        if exit_after.is_some_and(|seconds| clock >= seconds) {
            std::process::exit(0);
        }
        sim.frames += 1;
        if let Some(run) = plan.as_mut()
            && run.advance() == Next::Done
        {
            println!("plan done: {} of {} captures", run.written(), run.total());
            std::process::exit(0);
        }
    }
}

/// Report a plan's capture, and stop the run if its report couldn't be
/// written: a plan's captures are its whole point.
fn wrote_planned(path: &str, written: std::io::Result<std::path::PathBuf>) {
    match written {
        Ok(report) => println!("wrote {path} and {}", report.display()),
        Err(error) => {
            eprintln!("wrote {path}, but not its report: {error}");
            std::process::exit(1);
        }
    }
}

/// Frames drawn before a capture fast-forwards: the start of the run, where
/// the preloader lifts and fonts warm.
const SETTLE_FRAMES: u32 = 3;

/// Frames drawn, not skipped, before a `--at` moment: enough for glyphs that
/// first appear late in the run to pass the glyph gate before the capture.
const DRAWN_BEFORE_AT: u32 = 30;

/// When a run skips drawing on its way to a moment: a `--shot` capture with
/// `--at`, which is not a scripted run (a scripted run saves frames as it
/// goes, so every one must be drawn).
///
/// The skipped frames are simulated exactly as drawn ones are, on the fixed
/// clock, with the same input and the same ticks: only `draw` is left out. A
/// game keeps its state in `update` and `tick`, never `draw`, so the moment
/// captured is the same one, reached in seconds instead of in real time.
/// That matters more than it looks: on macOS `next_frame` waits for the
/// display, so a drawn frame costs a sixtieth of a second however little it
/// draws.
fn fast_forward_to(args: &Args, scripted: bool) -> Option<f32> {
    if scripted || args.shot.is_none() {
        return None;
    }
    args.at
}

/// The loop's own state across frames, drawn and skipped alike.
struct Sim {
    clock: f32,
    frames: u32,
    step: Option<FixedStep>,
    gestures: Gestures,
}

/// One frame of simulation: read the input, run `update` and the ticks due.
/// `keys` is the key script and which of its frames this is: `--keys` and
/// the loop's frame, or a plan section's and its own. Returns the input and
/// `alpha`, for the frame that is drawn after it.
fn simulate(
    game: &mut impl Game,
    sim: &mut Sim,
    args: &Args,
    (keys, key_frame): (&[KeyScript], u32),
    placed: Placed,
    dt: f32,
    text: bool,
) -> (InputSnapshot, f32) {
    sim.clock += dt;
    // The ticks due are counted before the frame is built, so `alpha` is where
    // drawing falls after them.
    let ticks = sim.step.as_mut().map(|s| s.advance(dt));
    let alpha = sim.step.as_ref().map_or(1.0, FixedStep::alpha);
    let Placed {
        viewport, origin, ..
    } = placed;
    let to_stage = |p: Vec2| (p - origin) / viewport.u;
    let input = InputSnapshot::capture(
        gesture_in(sim.gestures.update(), to_stage),
        to_stage,
        keys,
        key_frame,
    );
    let frame = Frame {
        args,
        dt,
        clock: sim.clock,
        frames: sim.frames,
        viewport,
        px: placed.px,
        origin,
        bounds: placed.bounds,
        input: &input,
        alpha,
        text,
        canvas: None,
    };
    game.update(&frame);
    for tick in ticks.into_iter().flatten() {
        game.tick(&tick, &frame);
    }
    (input, alpha)
}

/// Where a stage sits in the window this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placed {
    /// What the game sees: the stage's own size, and points per unit.
    viewport: Viewport,
    px: f32,
    /// The stage's top-left in the window, in points.
    origin: Vec2,
    /// Everything drawable, in stage units.
    bounds: Rect,
    /// The window camera's extent, in stage units: the whole window.
    window: Vec2,
    /// The stage's top-left inside that extent, in stage units.
    offset: Vec2,
}

/// A pixel stage's canvas: a plain target, drawn nearest-neighbour.
fn pixel_canvas(width: u32, height: u32) -> RenderTarget {
    let target = plain_target(width.max(1), height.max(1));
    target.texture.set_filter(FilterMode::Nearest);
    target
}

/// `stage` as it draws this frame: a [`Stage::PixelFit`] resolved to the
/// pixel stage its function asks for, every other stage as it is. A capture
/// with `--size` fits that size, whatever the window became.
fn fitted(stage: Stage, args: &Args) -> Stage {
    let Stage::PixelFit(fit) = stage else {
        return stage;
    };
    let points = match args.size {
        Some((w, h)) if args.capturing() => vec2(w as f32, h as f32),
        _ => vec2(screen_width(), screen_height()),
    };
    let size = fit(points, screen_dpi_scale()).max(Vec2::ONE);
    Stage::Pixel {
        width: size.x as u32,
        height: size.y as u32,
    }
}

fn place(stage: Stage, args: &Args) -> Placed {
    match stage {
        Stage::Design(design) => {
            let vp = args.viewport(design);
            Placed {
                viewport: vp,
                px: vp.px(),
                origin: Vec2::ZERO,
                bounds: Rect::new(0.0, 0.0, vp.size.x, vp.size.y),
                window: vp.size,
                offset: Vec2::ZERO,
            }
        }
        Stage::Pixel { width, height } => {
            let size = vec2(width as f32, height as f32);
            let screen = vec2(screen_width(), screen_height());
            let dpi = screen_dpi_scale();
            let (u, origin) = pixel_fit(size, screen, dpi);
            Placed {
                viewport: Viewport { u, size, dpi },
                px: 1.0,
                origin,
                bounds: Rect::new(0.0, 0.0, size.x, size.y),
                window: size,
                offset: Vec2::ZERO,
            }
        }
        Stage::Board { size, align } => board_fit(args.viewport(size), size, align),
        // Resolved by `fitted` before it gets here; placed as its pixel stage.
        Stage::PixelFit(_) => place(fitted(stage, args), args),
    }
}

/// A board of `size` design units in a window that `full` fits the design
/// into (the roomier side grown), placed by `align` in the spare room, its
/// top-left on a whole device pixel so its edges stay crisp.
fn board_fit(full: Viewport, size: Vec2, align: Vec2) -> Placed {
    let px = full.px();
    let spare = (full.size - size).max(Vec2::ZERO);
    let offset = (spare * align.clamp(Vec2::ZERO, Vec2::ONE) * px).floor() / px;
    Placed {
        viewport: Viewport {
            u: full.u,
            size,
            dpi: full.dpi,
        },
        px,
        origin: offset * full.u,
        bounds: Rect::new(-offset.x, -offset.y, full.size.x, full.size.y),
        window: full.size,
        offset,
    }
}

/// The parts of `bounds` outside a board of `size` at the origin: left and
/// right full height, top and bottom between them. Empty where there is no
/// margin. Padded by a unit outward, so nothing drawn past the board's edge
/// shows through a rounding seam.
fn margins(bounds: Rect, size: Vec2) -> Vec<Rect> {
    const PAD: f32 = 1.0;
    let (top, bottom) = (bounds.y - PAD, bounds.bottom() + PAD);
    [
        Rect::new(bounds.x - PAD, top, -bounds.x + PAD, bottom - top),
        Rect::new(size.x, top, bounds.right() - size.x + PAD, bottom - top),
        Rect::new(0.0, top, size.x, -bounds.y + PAD),
        Rect::new(0.0, size.y, size.x, bottom - size.y),
    ]
    .into_iter()
    .filter(|r| r.w > PAD && r.h > PAD)
    .collect()
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

    const BOARD: Vec2 = vec2(448.0, 512.0);

    #[test]
    fn a_board_is_fitted_whole_and_centred_in_a_wide_window() {
        // 1280x800 at 1x: 800/512 = 1.5625 points a unit; the window is
        // 819.2 units wide, so 371.2 spare, 185.6 each side.
        let full = Viewport::fit(vec2(1280.0, 800.0), 1.0, BOARD);
        let p = board_fit(full, BOARD, vec2(0.5, 0.5));
        assert_eq!(p.viewport.size, BOARD, "the game sees its board");
        assert_eq!(p.viewport.u, 1.5625);
        assert_eq!(p.origin.y, 0.0);
        assert!((p.origin.x - 290.0).abs() < 1.6, "{:?}", p.origin);
        // The origin is on a whole device pixel.
        assert_eq!(p.origin.x * full.dpi, (p.origin.x * full.dpi).round());
        // Bounds reach the window's edges either side of the board.
        assert!((p.bounds.x + p.offset.x).abs() < 1e-4);
        assert_eq!(p.bounds.w, full.size.x);
        assert!(p.bounds.x < 0.0 && p.bounds.right() > BOARD.x);
    }

    #[test]
    fn align_places_a_board_in_the_spare_room() {
        let full = Viewport::fit(vec2(400.0, 1000.0), 2.0, BOARD);
        let top = board_fit(full, BOARD, vec2(0.5, 0.0));
        let bottom = board_fit(full, BOARD, vec2(0.5, 1.0));
        assert_eq!(top.offset, Vec2::ZERO, "a narrow window has no spare width");
        assert!(bottom.offset.y > 0.0);
        let spare = full.size.y - BOARD.y;
        assert!((bottom.offset.y - spare).abs() <= 1.0 / full.px());
    }

    #[test]
    fn a_window_exactly_the_boards_shape_has_no_margin() {
        let full = Viewport::fit(BOARD * 2.0, 1.0, BOARD);
        let p = board_fit(full, BOARD, vec2(0.5, 0.5));
        assert_eq!(p.offset, Vec2::ZERO);
        assert_eq!(p.bounds, Rect::new(0.0, 0.0, BOARD.x, BOARD.y));
    }

    #[test]
    fn the_margins_cover_everything_outside_the_board_and_nothing_inside() {
        let full = Viewport::fit(vec2(1280.0, 800.0), 1.0, BOARD);
        let p = board_fit(full, BOARD, vec2(0.5, 0.5));
        let m = margins(p.bounds, BOARD);
        assert_eq!(m.len(), 2, "a wide window has side margins only: {m:?}");
        let board = Rect::new(0.0, 0.0, BOARD.x, BOARD.y);
        for r in &m {
            assert!(r.intersect(board).is_none_or(|i| i.w * i.h == 0.0), "{r:?}");
        }
        let covered = |p: Vec2| m.iter().any(|r| r.contains(p));
        assert!(covered(vec2(-1.0, 10.0)) && covered(vec2(BOARD.x + 1.0, BOARD.y - 1.0)));
        assert!(covered(p.bounds.point()) && covered(p.bounds.point() + p.bounds.size() - 0.5));

        let full = Viewport::fit(vec2(400.0, 1000.0), 1.0, BOARD);
        let p = board_fit(full, BOARD, vec2(0.5, 0.5));
        assert_eq!(
            margins(p.bounds, BOARD).len(),
            2,
            "a tall window: top and bottom"
        );
        assert!(margins(Rect::new(0.0, 0.0, BOARD.x, BOARD.y), BOARD).is_empty());
    }

    #[test]
    fn a_gesture_is_mapped_into_stage_units() {
        let g = Gesture {
            tap: Some(vec2(110.0, 20.0)),
            drag: Some(vec2(30.0, 40.0)),
            swipe: None,
        };
        let m = gesture_in(g, |p| (p - vec2(10.0, 0.0)) / 2.0);
        assert_eq!(m.tap, Some(vec2(50.0, 10.0)));
        assert_eq!(m.drag, Some(vec2(10.0, 20.0)));
    }
}
