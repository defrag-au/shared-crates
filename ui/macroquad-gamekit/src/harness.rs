//! `harness` — the review loop a game client is built around: a fixed clock,
//! an exact moment captured to a PNG *and* a text report of every line drawn,
//! scripted pointers, and a window camera that is the right way up.
//!
//! A client wires it once (see `fight-macroquad` in augminted-bots for the
//! reference) and gets these flags natively:
//!
//! ```sh
//! game --shot out.png [--at 1.5] [--size 390x844]     # capture a moment
//!      [--hover N | --drag N | --longpress N | --rclick N]  # with a scripted pointer
//! game --soak 20                                     # the game's own soak loop
//! game --sheet sheet.png                             # the game's own sheet of faces
//! game --frame-log                                   # print every frame over 50 ms
//! ```
//!
//! ## Stalls are invisible to captures
//!
//! Captures and soaks run on the fixed clock, so a frame that takes two
//! seconds of wall time looks like any other. Play with `--frame-log` (and
//! call [`Args::log_slow_frame`] once a frame) to see them: a text size that
//! animates, a bake each frame, a texture made in the loop.
//!
//! ## The window is not always the size asked for
//!
//! A tiling window manager resizes the window to its tile, so a capture
//! sized from the window ignores `--size`. [`Args::viewport`] sizes a
//! capture from `--size` when one is given.
//!
//! ## The report checks what a glance misses
//!
//! Each capture writes `out.png` and `out.txt`. The image is the review; the
//! text is a second check. It lists every line of text drawn, on the screen
//! and inside each baked face, with its box and size, and leads with the
//! problems that are easy to miss by eye: text off its surface, cut short with
//! an ellipsis, smaller than legible, or on top of other text.
//!
//! ## A capture is not the window
//!
//! A capture draws into a render target, and macroquad's cameras treat a
//! render target and the window differently: `Camera2D::from_display_rect`
//! runs y downwards into a target and *upwards* on the window. So a capture
//! that looks right says nothing about the window. [`window_camera`] flips it
//! back, and a unit test pins both orientations.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use macroquad::prelude::*;

/// The clock a capture runs on: sixty steps a second, whatever the machine.
pub const FIXED_DT: f32 = 1.0 / 60.0;

/// This frame's time step: [`FIXED_DT`] when capturing, so a moment is the
/// same moment every run; otherwise the real frame time, capped so a stall
/// does not teleport anything.
pub fn frame_dt(fixed: bool) -> f32 {
    if fixed {
        FIXED_DT
    } else {
        get_frame_time().min(0.1)
    }
}

// ---------------------------------------------------------------------------
// Viewport and cameras
// ---------------------------------------------------------------------------

/// How the window maps to the design's units: one uniform scale, and the side
/// with room to spare grows (a taller phone gets a longer screen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Screen points per design unit.
    pub u: f32,
    /// The screen in design units.
    pub size: Vec2,
    /// Device pixels per screen point.
    pub dpi: f32,
}

impl Viewport {
    pub fn fit(screen: Vec2, dpi: f32, design: Vec2) -> Self {
        let u = (screen.x / design.x).min(screen.y / design.y).max(0.05);
        Self {
            u,
            size: screen / u,
            dpi,
        }
    }

    /// The window as it is now.
    pub fn current(design: Vec2) -> Self {
        Self::fit(
            vec2(screen_width(), screen_height()),
            screen_dpi_scale(),
            design,
        )
    }

    /// Device pixels per design unit: the scale text rasterises at.
    pub fn px(&self) -> f32 {
        self.u * self.dpi
    }

    /// A screen point (as macroquad reports the mouse) in design units.
    pub fn to_design(self, p: Vec2) -> Vec2 {
        p / self.u
    }
}

/// Design units onto the window, origin top-left, y down, offset by `shake`.
pub fn window_camera(size: Vec2, shake: Vec2) -> Camera2D {
    let mut cam = Camera2D::from_display_rect(Rect::new(-shake.x, -shake.y, size.x, size.y));
    // `from_display_rect` is y-down only into a render target; macroquad
    // inverts y again for the window, so flip it back.
    cam.zoom.y = -cam.zoom.y;
    cam
}

/// Design units into `target`, origin top-left, y down, offset by `shake`.
/// Draw the target's texture with `flip_y: true` to show it upright.
pub fn target_camera(size: Vec2, shake: Vec2, target: &RenderTarget) -> Camera2D {
    let mut cam = Camera2D::from_display_rect(Rect::new(-shake.x, -shake.y, size.x, size.y));
    cam.render_target = Some(target.clone());
    cam
}

/// A plain (not multisampled) render target, `w` x `h` pixels. Use this, never
/// macroquad's `render_target`.
///
/// In macroquad 0.4, `render_target` defaults to `sample_count: 1`, and
/// `render_target_ex` treats every count but 0 as multisampled: each pass into
/// the target then ends in a framebuffer blit through `glReadBuffer`. WebGL 1,
/// miniquad's default in the browser, has no such call, so the first frame
/// that draws into a target dies with "gl.readBuffer is not a function" (and
/// every later event panics at miniquad's `wasm.rs:34` on top of it). Native
/// GL has the call, so a native build and every capture look fine. 0 is the
/// plain path, on WebGL 1 and 2 alike.
pub fn plain_target(w: u32, h: u32) -> RenderTarget {
    render_target_ex(
        w,
        h,
        RenderTargetParams {
            sample_count: 0,
            depth: false,
        },
    )
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

/// A native command-line flag's value; `None` on the web, which has no argv.
pub fn flag(name: &str) -> Option<String> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    let argv: Vec<String> = std::env::args().collect();
    flag_in(&argv, name)
}

/// [`flag`], parsed.
pub fn flag_num<T: std::str::FromStr>(name: &str) -> Option<T> {
    flag(name).and_then(|v| v.parse().ok())
}

fn flag_in(argv: &[String], name: &str) -> Option<String> {
    argv.iter()
        .position(|a| a == name)
        .and_then(|i| argv.get(i + 1))
        .cloned()
}

/// The harness's own flags. A game reads its own (seed, level) with [`flag`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// `--shot out.png`: capture one frame, write `out.png` and `out.txt`, exit.
    pub shot: Option<String>,
    /// `--at 1.5`: capture at this time on the fixed clock, rather than once
    /// the game says it is still.
    pub at: Option<f32>,
    /// `--soak 20`: the game's soak, over this many runs.
    pub soak: Option<u32>,
    /// `--sheet sheet.png`: the game's sheet of faces.
    pub sheet: Option<String>,
    /// `--hover N`, `--drag N`, `--longpress N` or `--rclick N`.
    pub script: Option<Script>,
    /// `--size 390x844`: the window, in points, and the size a capture is
    /// drawn at whatever the window became (see [`Args::viewport`]).
    pub size: Option<(i32, i32)>,
    /// `--frame-log`: print every frame slower than [`SLOW_FRAME`].
    pub frame_log: bool,
}

/// A frame slower than this, in seconds of wall time, is a stall worth
/// printing under `--frame-log`: three frames at 60 Hz.
pub const SLOW_FRAME: f32 = 0.05;

impl Args {
    pub fn read() -> Self {
        if cfg!(target_arch = "wasm32") {
            return Self::default();
        }
        let argv: Vec<String> = std::env::args().collect();
        Self::parse(&argv)
    }

    pub fn parse(argv: &[String]) -> Self {
        let num = |name: &str| flag_in(argv, name).and_then(|v| v.parse::<usize>().ok());
        let script = num("--hover")
            .map(Script::Hover)
            .or_else(|| num("--drag").map(Script::Drag))
            .or_else(|| num("--longpress").map(Script::LongPress))
            .or_else(|| num("--rclick").map(Script::RightClick));
        Self {
            shot: flag_in(argv, "--shot"),
            at: flag_in(argv, "--at").and_then(|v| v.parse().ok()),
            soak: flag_in(argv, "--soak").and_then(|v| v.parse().ok()),
            sheet: flag_in(argv, "--sheet"),
            script,
            size: flag_in(argv, "--size").and_then(|s| {
                let (w, h) = s.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            }),
            frame_log: argv.iter().any(|a| a == "--frame-log"),
        }
    }

    /// This frame's viewport. Capturing with `--size`, it is that size,
    /// whatever the window is: a tiling window manager (AeroSpace) resizes
    /// the window to its tile, and a capture draws into its own target
    /// anyway. Otherwise it is the window as it is now.
    pub fn viewport(&self, design: Vec2) -> Viewport {
        match self.size {
            Some((w, h)) if self.capturing() => {
                Viewport::fit(vec2(w as f32, h as f32), screen_dpi_scale(), design)
            }
            _ => Viewport::current(design),
        }
    }

    /// Under `--frame-log`, print the last frame if it took longer than
    /// [`SLOW_FRAME`] of wall time. Call once a frame; `frame` is the
    /// game's own count, `t` its clock, both only for the message.
    pub fn log_slow_frame(&self, frame: u32, t: f32) {
        let took = get_frame_time();
        if self.frame_log && took > SLOW_FRAME {
            println!("slow frame {frame} at t={t:.2}s: {:.0} ms", took * 1000.0);
        }
    }

    /// Whether this run captures, so runs on the fixed clock and records text.
    pub fn capturing(&self) -> bool {
        self.shot.is_some() || self.sheet.is_some()
    }

    /// The window size: `--size`, else `default`.
    pub fn window_size(&self, default: (i32, i32)) -> (i32, i32) {
        self.size.unwrap_or(default)
    }
}

// ---------------------------------------------------------------------------
// Pointer scripts
// ---------------------------------------------------------------------------

/// A scripted pointer on target `N`; which thing `N` is (a card in the hand,
/// a tile) is the game's to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    Hover(usize),
    Drag(usize),
    LongPress(usize),
    RightClick(usize),
}

/// The pointer on one frame of a script, in design units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scripted {
    pub hover: Option<Vec2>,
    /// The button is held here.
    pub down: Option<Vec2>,
    /// A secondary click, on this frame only.
    pub right_click: Option<Vec2>,
}

impl Script {
    pub fn target(self) -> usize {
        match self {
            Script::Hover(i) | Script::Drag(i) | Script::LongPress(i) | Script::RightClick(i) => i,
        }
    }

    /// Frames the script runs before its capture.
    pub fn frames(self) -> u32 {
        match self {
            Script::Hover(_) => 30,
            Script::Drag(_) => 40,
            Script::LongPress(_) => 45,
            Script::RightClick(_) => 12,
        }
    }

    /// The pointer on frame `f`, the target resting at `at`. A drag travels
    /// by `drag_by` over its first twenty frames, then holds.
    pub fn pointer(self, f: u32, at: Vec2, drag_by: Vec2) -> Scripted {
        let mut p = Scripted {
            hover: Some(at),
            ..Default::default()
        };
        match self {
            Script::Hover(_) => {}
            Script::Drag(_) => {
                let to = at + drag_by * (f as f32 / 20.0).min(1.0);
                p.down = Some(to);
                p.hover = Some(to);
            }
            Script::LongPress(_) => p.down = Some(at),
            Script::RightClick(_) => {
                if f == 0 {
                    p.right_click = Some(at);
                }
            }
        }
        p
    }
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

/// One frame drawn into a render target and written out as a PNG and its
/// text report. Begin before drawing the frame; save after.
pub struct Capture {
    target: RenderTarget,
}

impl Capture {
    /// The window's frame: `vp.size` design units at the window's device
    /// pixels, bound as the camera.
    pub fn window(vp: &Viewport, shake: Vec2) -> Self {
        Self::new(vp.size, vp.px(), shake)
    }

    /// Any surface: `size` design units at `px` pixels per unit, bound as the
    /// camera. Clears nothing; the caller clears as its frame does.
    pub fn new(size: Vec2, px: f32, shake: Vec2) -> Self {
        let target = plain_target(
            (size.x * px).ceil().max(1.0) as u32,
            (size.y * px).ceil().max(1.0) as u32,
        );
        target.texture.set_filter(FilterMode::Linear);
        set_camera(&target_camera(size, shake, &target));
        report::begin_screen(size);
        Self { target }
    }

    /// Write `path` (PNG) and the report beside it (`.txt`), headed by
    /// `note` (the seed, the moment). Returns the report's path.
    ///
    /// The PNG is written opaque. macroquad blends alpha like colour, so
    /// every translucent layer (grain, a shadow, a skid mark) wears the
    /// target's alpha down, and the image shows paler than the window
    /// against a viewer's white, though its colours are right. A window has
    /// no transparency, so a capture of one should not either.
    pub fn save(self, path: &str, note: &str) -> std::io::Result<PathBuf> {
        set_default_camera();
        save_texture_png(&self.target.texture, path);
        let txt = Path::new(path).with_extension("txt");
        std::fs::write(&txt, report::render(path, note))?;
        Ok(txt)
    }
}

/// Write a render target's texture to `path` as an opaque PNG: the way to
/// screenshot a game that composes its frame into a target of its own (a
/// pixel-art canvas scaled to the window) without adopting [`Capture`].
///
/// Never `get_screen_data` for this: on macOS it returns black when the
/// window is unfocused or not yet composited, so a bot run in the
/// background saves black frames. A target reads back the same anywhere.
/// It comes out upright for a target drawn through a
/// `Camera2D::from_display_rect` camera ([`target_camera`]), and opaque for
/// the reason [`Capture::save`] gives.
pub fn save_texture_png(texture: &Texture2D, path: &str) {
    let mut image = texture.get_texture_data();
    opaque(&mut image.bytes);
    image.export_png(path);
}

/// Set every pixel of RGBA `bytes` fully opaque.
fn opaque(bytes: &mut [u8]) {
    for px in bytes.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
}

// ---------------------------------------------------------------------------
// The text report
// ---------------------------------------------------------------------------

/// What the text report records. Off until [`report::enable`]; the text
/// module calls in on every line it draws, so nothing else has to.
pub mod report {
    use super::*;

    /// Below this many design units, text is reported as too small to read.
    pub const MIN_TEXT: f32 = 8.0;

    #[derive(Clone, Debug, PartialEq)]
    pub struct Line {
        pub text: String,
        /// Its box: left, top, width, height, in the surface's units.
        pub rect: Rect,
        pub size: f32,
        pub alpha: f32,
    }

    /// A surface text is drawn on: the screen, or a baked face.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Surface {
        pub size: Vec2,
        pub lines: Vec<Line>,
    }

    #[derive(Default)]
    struct State {
        on: bool,
        screen: Surface,
        /// Faces baked since recording began, each once, in bake order.
        bakes: Vec<Surface>,
        /// The bake being drawn, if one is.
        baking: Option<Surface>,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
    }

    /// Start recording. Call before anything is baked, so faces are caught.
    pub fn enable() {
        STATE.with(|s| s.borrow_mut().on = true);
    }

    pub fn on() -> bool {
        STATE.with(|s| s.borrow().on)
    }

    /// A new screen frame of `size` units: forget the last frame's lines.
    pub fn begin_screen(size: Vec2) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.screen = Surface {
                size,
                lines: Vec::new(),
            };
        });
    }

    /// Text drawn from here until [`end_bake`] lands on a face of `size`.
    pub fn begin_bake(size: Vec2) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.on {
                s.baking = Some(Surface {
                    size,
                    lines: Vec::new(),
                });
            }
        });
    }

    /// Forget what this bake has drawn so far: it is drawing again.
    pub fn restart_bake() {
        STATE.with(|s| {
            if let Some(b) = s.borrow_mut().baking.as_mut() {
                b.lines.clear();
            }
        });
    }

    pub fn end_bake() {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if let Some(b) = s.baking.take()
                && !b.lines.is_empty()
                && !s.bakes.contains(&b)
            {
                s.bakes.push(b);
            }
        });
    }

    /// A line of text was drawn. Called by the text module.
    pub fn line(text: &str, rect: Rect, size: f32, alpha: f32) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.on {
                return;
            }
            let line = Line {
                text: text.to_owned(),
                rect,
                size,
                alpha,
            };
            match s.baking.as_mut() {
                Some(b) => b.lines.push(line),
                None => s.screen.lines.push(line),
            }
        });
    }

    /// The screen and the faces, as recorded so far.
    pub fn surfaces() -> (Surface, Vec<Surface>) {
        STATE.with(|s| {
            let s = s.borrow();
            (s.screen.clone(), s.bakes.clone())
        })
    }

    /// One problem a reviewer would flag.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Problem {
        /// Not wholly on its surface.
        Off { line: Line },
        /// Cut short with an ellipsis.
        Cut { line: Line },
        /// Below [`MIN_TEXT`].
        Small { line: Line },
        /// On top of another line.
        Overlap { a: Line, b: Line },
    }

    /// Text that can be seen: fully transparent lines are animation states.
    fn visible(l: &Line) -> bool {
        l.alpha > 0.05
    }

    /// The visible text on `surface` as a reader sees it: a layout places
    /// each word on its own, so words on one baseline, in one size and a
    /// space apart, are joined back into the line they make.
    pub fn joined(surface: &Surface) -> Vec<Line> {
        let mut words: Vec<&Line> = surface.lines.iter().filter(|l| visible(l)).collect();
        words.sort_by(|a, b| {
            (a.rect.y.round(), a.rect.x)
                .partial_cmp(&(b.rect.y.round(), b.rect.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut out: Vec<Line> = Vec::new();
        for w in words {
            if let Some(last) = out.last_mut() {
                let gap = w.rect.x - last.rect.right();
                let same_line = (last.rect.y - w.rect.y).abs() < 0.5
                    && (last.size - w.size).abs() < 0.01
                    && gap >= -0.5
                    && gap < w.size * 0.6;
                if same_line {
                    last.text = format!("{} {}", last.text, w.text);
                    last.rect.w = w.rect.right() - last.rect.x;
                    last.rect.h = last.rect.h.max(w.rect.h);
                    continue;
                }
            }
            out.push(w.clone());
        }
        out
    }

    /// What is wrong with the text on `surface`.
    pub fn problems(surface: &Surface) -> Vec<Problem> {
        let mut out = Vec::new();
        let bounds = Rect::new(0.0, 0.0, surface.size.x, surface.size.y);
        let joined = joined(surface);
        let lines: Vec<&Line> = joined.iter().collect();
        for l in &lines {
            let r = l.rect;
            let inside = r.x >= bounds.x - 0.5
                && r.y >= bounds.y - 0.5
                && r.right() <= bounds.right() + 0.5
                && r.bottom() <= bounds.bottom() + 0.5;
            if !inside {
                out.push(Problem::Off { line: (*l).clone() });
            }
            if l.text.contains('…') {
                out.push(Problem::Cut { line: (*l).clone() });
            }
            if l.size < MIN_TEXT {
                out.push(Problem::Small { line: (*l).clone() });
            }
        }
        for (i, a) in lines.iter().enumerate() {
            for b in &lines[i + 1..] {
                // A shadow or outline copy is not a second line.
                if a.text == b.text && a.rect.overlaps(&b.rect) {
                    continue;
                }
                if let Some(o) = a.rect.intersect(b.rect) {
                    let smaller = (a.rect.w * a.rect.h).min(b.rect.w * b.rect.h).max(1e-3);
                    if o.w * o.h / smaller > 0.25 {
                        out.push(Problem::Overlap {
                            a: (*a).clone(),
                            b: (*b).clone(),
                        });
                    }
                }
            }
        }
        out
    }

    fn rect(r: Rect) -> String {
        format!("({:.0}, {:.0}, {:.0}×{:.0})", r.x, r.y, r.w, r.h)
    }

    fn describe(p: &Problem, surface: &str) -> String {
        match p {
            Problem::Off { line } => format!(
                "off its surface: {:?} at {} on {surface}",
                line.text,
                rect(line.rect)
            ),
            Problem::Cut { line } => format!("cut short: {:?} on {surface}", line.text),
            Problem::Small { line } => format!(
                "too small: {:?} at {:.1} (minimum {MIN_TEXT}) on {surface}",
                line.text, line.size
            ),
            Problem::Overlap { a, b } => format!(
                "overlapping: {:?} at {} and {:?} at {} on {surface}",
                a.text,
                rect(a.rect),
                b.text,
                rect(b.rect)
            ),
        }
    }

    fn list(out: &mut String, surface: &Surface) {
        let mut lines = joined(surface);
        lines.dedup_by(|a, b| a.text == b.text && a.rect.overlaps(&b.rect));
        for l in lines {
            let _ = writeln!(out, "- {} {:.1} {:?}", rect(l.rect), l.size, l.text);
        }
    }

    /// The report for the capture written to `png`.
    pub fn render(png: &str, note: &str) -> String {
        let (screen, bakes) = surfaces();
        let mut found: Vec<String> = problems(&screen)
            .iter()
            .map(|p| describe(p, "the screen"))
            .collect();
        for (i, b) in bakes.iter().enumerate() {
            let name = format!("face #{}", i + 1);
            found.extend(problems(b).iter().map(|p| describe(p, &name)));
        }
        let mut out = String::new();
        let _ = writeln!(out, "# Frame report: {png}");
        if !note.is_empty() {
            let _ = writeln!(out, "{note}");
        }
        let _ = writeln!(
            out,
            "\nBoxes are (left, top, width×height) in design units, then the text size and the text. \
             The screen is {:.0}×{:.0}. A face's text is in the face's own units; the face is drawn \
             scaled, so its on-screen size is its size times that scale.",
            screen.size.x, screen.size.y
        );
        let _ = writeln!(out, "\n## Problems ({})", found.len());
        if found.is_empty() {
            let _ = writeln!(out, "None found.");
        }
        for p in &found {
            let _ = writeln!(out, "- {p}");
        }
        let _ = writeln!(out, "\n## Screen text, top to bottom");
        list(&mut out, &screen);
        let _ = writeln!(out, "\n## Baked faces ({})", bakes.len());
        for (i, b) in bakes.iter().enumerate() {
            let _ = writeln!(
                out,
                "\n### Face #{} ({:.0}×{:.0})",
                i + 1,
                b.size.x,
                b.size.y
            );
            list(&mut out, b);
        }
        out
    }

    #[cfg(test)]
    pub(crate) fn reset() {
        STATE.with(|s| *s.borrow_mut() = State::default());
    }
}

#[cfg(test)]
mod tests {
    use super::report::{Line, Problem, Surface, problems};
    use super::*;
    use macroquad::camera::Camera;

    fn ndc(cam: &Camera2D, p: Vec2) -> Vec2 {
        let v = cam.matrix().project_point3(vec3(p.x, p.y, 0.0));
        vec2(v.x, v.y)
    }

    #[test]
    fn the_window_camera_puts_the_origin_top_left() {
        let size = vec2(390.0, 844.0);
        let cam = window_camera(size, Vec2::ZERO);
        // Normalised device coordinates run y up: the top-left is (-1, 1).
        assert!(ndc(&cam, Vec2::ZERO).abs_diff_eq(vec2(-1.0, 1.0), 1e-5));
        assert!(ndc(&cam, size).abs_diff_eq(vec2(1.0, -1.0), 1e-5));
        // Unflipped, the window is upside down: the bug this exists for.
        let raw = Camera2D::from_display_rect(Rect::new(0.0, 0.0, size.x, size.y));
        assert!(ndc(&raw, Vec2::ZERO).abs_diff_eq(vec2(-1.0, -1.0), 1e-5));
    }

    #[test]
    fn a_capture_is_written_opaque_and_its_colours_kept() {
        let mut bytes = vec![159, 101, 52, 204, 10, 20, 30, 0];
        opaque(&mut bytes);
        assert_eq!(bytes, [159, 101, 52, 255, 10, 20, 30, 255]);
    }

    #[test]
    fn the_design_size_fits_at_scale_one() {
        let design = vec2(390.0, 844.0);
        let v = Viewport::fit(design, 2.0, design);
        assert_eq!(v.u, 1.0);
        assert_eq!(v.size, design);
        assert_eq!(v.px(), 2.0);
        assert_eq!(v.to_design(vec2(39.0, 84.4)), vec2(39.0, 84.4));
    }

    #[test]
    fn landscape_grows_wide_and_keeps_the_design_height() {
        let design = vec2(390.0, 844.0);
        let v = Viewport::fit(vec2(1280.0, 800.0), 1.0, design);
        assert!((v.size.y - design.y).abs() < 1e-3);
        assert!(v.size.x > 1300.0);
    }

    #[test]
    fn shake_moves_the_view_not_the_orientation() {
        let cam = window_camera(vec2(100.0, 100.0), vec2(10.0, 0.0));
        assert!(ndc(&cam, vec2(-10.0, 0.0)).abs_diff_eq(vec2(-1.0, 1.0), 1e-5));
    }

    #[test]
    fn flags_parse_and_unknown_ones_are_left_alone() {
        let argv: Vec<String> = "game --shot a.png --at 1.5 --drag 2 --size 1280x800 --seed 7"
            .split(' ')
            .map(String::from)
            .collect();
        let a = Args::parse(&argv);
        assert_eq!(a.shot.as_deref(), Some("a.png"));
        assert_eq!(a.at, Some(1.5));
        assert_eq!(a.script, Some(Script::Drag(2)));
        assert_eq!(a.size, Some((1280, 800)));
        assert!(a.capturing());
        assert!(!a.frame_log);
        assert!(Args::parse(&["game".into(), "--frame-log".into()]).frame_log);
        assert_eq!(flag_in(&argv, "--seed").as_deref(), Some("7"));
        assert_eq!(Args::parse(&["game".into()]), Args::default());
    }

    #[test]
    fn a_drag_travels_then_holds_and_a_right_click_is_one_frame() {
        let at = vec2(100.0, 700.0);
        let up = vec2(0.0, -200.0);
        assert_eq!(Script::Drag(0).pointer(0, at, up).down, Some(at));
        assert_eq!(
            Script::Drag(0).pointer(10, at, up).down,
            Some(vec2(100.0, 600.0))
        );
        assert_eq!(
            Script::Drag(0).pointer(39, at, up).down,
            Some(vec2(100.0, 500.0))
        );
        assert_eq!(
            Script::RightClick(0).pointer(0, at, up).right_click,
            Some(at)
        );
        assert_eq!(Script::RightClick(0).pointer(1, at, up).right_click, None);
    }

    fn line(text: &str, x: f32, y: f32, w: f32, size: f32) -> Line {
        Line {
            text: text.into(),
            rect: Rect::new(x, y, w, size),
            size,
            alpha: 1.0,
        }
    }

    #[test]
    fn the_report_flags_what_a_reviewer_would() {
        let s = Surface {
            size: vec2(100.0, 100.0),
            lines: vec![
                line("fine", 10.0, 10.0, 30.0, 12.0),
                line("off the edge", 80.0, 40.0, 40.0, 12.0),
                line("Scrap-Iron…", 10.0, 60.0, 30.0, 12.0),
                line("tiny", 60.0, 10.0, 10.0, 6.0),
                line("under", 12.0, 12.0, 30.0, 12.0),
            ],
        };
        let p = problems(&s);
        let has = |f: &dyn Fn(&Problem) -> bool| p.iter().any(f);
        assert!(has(
            &|p| matches!(p, Problem::Off { line } if line.text == "off the edge")
        ));
        assert!(has(
            &|p| matches!(p, Problem::Cut { line } if line.text == "Scrap-Iron…")
        ));
        assert!(has(
            &|p| matches!(p, Problem::Small { line } if line.text == "tiny")
        ));
        assert!(has(
            &|p| matches!(p, Problem::Overlap { a, b } if a.text == "fine" && b.text == "under")
        ));
        assert_eq!(p.len(), 4, "{p:?}");
    }

    #[test]
    fn words_on_one_baseline_read_as_one_line() {
        let s = Surface {
            size: vec2(200.0, 100.0),
            lines: vec![
                line("Thing", 64.0, 10.0, 40.0, 12.0),
                line("Stripped", 10.0, 10.0, 50.0, 12.0),
                line("below", 10.0, 30.0, 30.0, 12.0),
                line("far", 150.0, 10.0, 20.0, 12.0),
            ],
        };
        let texts: Vec<String> = report::joined(&s).into_iter().map(|l| l.text).collect();
        assert_eq!(texts, ["Stripped Thing", "far", "below"]);
    }

    #[test]
    fn invisible_text_and_shadow_copies_are_not_problems() {
        let mut faded = line("fading", 90.0, 90.0, 40.0, 5.0);
        faded.alpha = 0.0;
        let s = Surface {
            size: vec2(100.0, 100.0),
            lines: vec![
                line("Title", 10.0, 10.0, 30.0, 12.0),
                line("Title", 10.5, 10.8, 30.0, 12.0),
                faded,
            ],
        };
        assert_eq!(problems(&s), Vec::new());
    }

    #[test]
    fn bakes_are_recorded_once_and_a_redraw_replaces_the_first_pass() {
        report::reset();
        report::enable();
        for _ in 0..2 {
            report::begin_bake(vec2(112.0, 157.0));
            report::line("Shredder", Rect::new(30.0, 8.0, 50.0, 12.0), 12.0, 1.0);
            report::restart_bake();
            report::line("Shredder", Rect::new(30.0, 8.0, 50.0, 12.0), 12.0, 1.0);
            report::end_bake();
        }
        report::begin_screen(vec2(390.0, 844.0));
        report::line("Turn 1", Rect::new(170.0, 600.0, 40.0, 12.0), 12.0, 1.0);
        let (screen, bakes) = report::surfaces();
        assert_eq!(bakes.len(), 1, "the same face baked twice is one face");
        assert_eq!(bakes[0].lines.len(), 1, "the first pass is forgotten");
        assert_eq!(screen.lines.len(), 1);
        let text = report::render("out.png", "seed 1");
        assert!(text.contains("## Problems (0)"), "{text}");
        assert!(text.contains("\"Shredder\""), "{text}");
        report::reset();
    }
}
