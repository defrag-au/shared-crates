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
//!      [--keys "10:Up,20-40:Right"]                  # with scripted keys, by frame
//! game --plan plan.txt                               # many conditions and captures, one launch
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

/// The points a frame is timed at by [`FrameTimer`], in order.
#[derive(Clone, Copy, Debug)]
pub enum FrameMark {
    Update = 1,
    Draw = 2,
    Glyphs = 3,
    Present = 4,
}

/// Wall-clock timings of one frame, to say where a visible pause came from,
/// and the native frame cap. With [`frame_dt`], the only reads of the real
/// clock in the kit: game code sees `dt` and nothing else.
pub struct FrameTimer {
    marks: [f64; 5],
}

impl FrameTimer {
    /// At the top of the frame.
    pub fn start() -> Self {
        Self {
            marks: [get_time(); 5],
        }
    }

    pub fn mark(&mut self, mark: FrameMark) {
        self.marks[mark as usize] = get_time();
    }

    /// A frame-log line, if the frame was slower than [`SLOW_FRAME`].
    pub fn slow(&self) -> Option<String> {
        let [start, update, draw, glyphs, present] = self.marks;
        let ms = |a: f64, b: f64| (b - a) * 1000.0;
        (present - start > SLOW_FRAME as f64).then(|| {
            format!(
                "slow frame {:.1} ms: update {:.1}, draw {:.1}, glyphs {:.1}, present {:.1}",
                ms(start, present),
                ms(start, update),
                ms(update, draw),
                ms(draw, glyphs),
                ms(glyphs, present),
            )
        })
    }

    /// Sleep out the rest of a 60 Hz frame, natively.
    ///
    /// miniquad on macOS disables vsync and paces with CVDisplayLink; if that
    /// link is missing or the window is not presented, `next_frame` returns at
    /// once and a busy screen spins the machine. In the browser,
    /// requestAnimationFrame already paces the loop.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn pace(&self) {
        const FRAME: f64 = 1.0 / 60.0;
        let elapsed = get_time() - self.marks[0];
        if elapsed < FRAME {
            std::thread::sleep(std::time::Duration::from_secs_f64(FRAME - elapsed));
        }
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

/// Whether a native flag that takes no value (`--play`, `--debug`) was given;
/// false on the web. [`flag`] reads the argument *after* a name, so it cannot
/// see a bare switch.
pub fn switch(name: &str) -> bool {
    if cfg!(target_arch = "wasm32") {
        return false;
    }
    std::env::args().any(|a| a == name)
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
    /// `--keys "10:Up,20-40:Right"`: keys the loop presses for the game, by
    /// frame, merged into [`Frame::input`](crate::Frame::input) so the game
    /// cannot tell them from a player's. See [`KeyScript`].
    pub keys: Vec<KeyScript>,
    /// `--plan plan.txt`: many conditions and captures in one launch; see
    /// [`plan`](crate::plan).
    pub plan: Option<String>,
}

/// One key in a `--keys` script: held from frame `from` until before frame
/// `to`, and pressed on frame `from`. `10:Up` is `Up` tapped on frame 10
/// (held for that frame only); `20-40:Right` holds `Right` for frames 20 to
/// 39. Frames count from 0, as [`Frame::frames`](crate::Frame::frames) does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyScript {
    pub key: KeyCode,
    pub from: u32,
    pub to: u32,
}

impl KeyScript {
    /// Parse a whole `--keys` value: comma-separated `FRAME:Key` or
    /// `FROM-TO:Key`. Key names are macroquad's `KeyCode` names (`Up`,
    /// `Space`, `A`, `Key1`, `LeftShift`), matched without regard to case.
    pub fn parse_all(s: &str) -> Result<Vec<KeyScript>, String> {
        s.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| {
                let (frames, name) = part
                    .split_once(':')
                    .ok_or_else(|| format!("`{part}`: expected FRAME:Key or FROM-TO:Key"))?;
                let key = key_named(name.trim())
                    .ok_or_else(|| format!("`{part}`: no key named `{}`", name.trim()))?;
                let num = |n: &str| {
                    n.trim()
                        .parse::<u32>()
                        .map_err(|_| format!("`{part}`: `{n}` is not a frame number"))
                };
                let (from, to) = match frames.split_once('-') {
                    Some((a, b)) => (num(a)?, num(b)?),
                    None => {
                        let f = num(frames)?;
                        (f, f + 1)
                    }
                };
                if to <= from {
                    return Err(format!("`{part}`: the range ends before it starts"));
                }
                Ok(KeyScript { key, from, to })
            })
            .collect()
    }

    /// Whether the key is held on `frame`.
    pub fn down(&self, frame: u32) -> bool {
        (self.from..self.to).contains(&frame)
    }

    /// Whether the key goes down on `frame`.
    pub fn pressed(&self, frame: u32) -> bool {
        frame == self.from
    }
}

/// The `KeyCode` called `name` (any case): letters, digits as `Key0`..`Key9`
/// or bare `0`..`9`, arrows, `F1`..`F12`, and the common named keys.
pub fn key_named(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    const KEYS: &[(&str, KeyCode)] = &[
        ("space", Space),
        ("enter", Enter),
        ("return", Enter),
        ("escape", Escape),
        ("esc", Escape),
        ("tab", Tab),
        ("backspace", Backspace),
        ("delete", Delete),
        ("pageup", PageUp),
        ("pagedown", PageDown),
        ("home", Home),
        ("end", End),
        ("leftbracket", LeftBracket),
        ("rightbracket", RightBracket),
        ("minus", Minus),
        ("equal", Equal),
        ("comma", Comma),
        ("period", Period),
        ("slash", Slash),
        ("up", Up),
        ("down", Down),
        ("left", Left),
        ("right", Right),
        ("leftshift", LeftShift),
        ("rightshift", RightShift),
        ("leftcontrol", LeftControl),
        ("rightcontrol", RightControl),
        ("leftalt", LeftAlt),
        ("rightalt", RightAlt),
        ("f1", F1),
        ("f2", F2),
        ("f3", F3),
        ("f4", F4),
        ("f5", F5),
        ("f6", F6),
        ("f7", F7),
        ("f8", F8),
        ("f9", F9),
        ("f10", F10),
        ("f11", F11),
        ("f12", F12),
    ];
    const LETTERS: [KeyCode; 26] = [
        A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    ];
    const DIGITS: [KeyCode; 10] = [Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9];
    let lower = name.to_ascii_lowercase();
    if let Some((_, k)) = KEYS.iter().find(|(n, _)| *n == lower) {
        return Some(*k);
    }
    let bytes = lower.as_bytes();
    match bytes {
        [c @ b'a'..=b'z'] => Some(LETTERS[usize::from(c - b'a')]),
        [d @ b'0'..=b'9'] | [b'k', b'e', b'y', d @ b'0'..=b'9'] => {
            Some(DIGITS[usize::from(d - b'0')])
        }
        _ => None,
    }
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
            keys: flag_in(argv, "--keys")
                .map(|s| {
                    KeyScript::parse_all(&s).unwrap_or_else(|e| {
                        eprintln!("--keys {e}");
                        std::process::exit(2)
                    })
                })
                .unwrap_or_default(),
            plan: flag_in(argv, "--plan"),
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
        self.shot.is_some() || self.sheet.is_some() || self.plan.is_some()
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
        write_capture(&self.target.texture, path, note)
    }
}

/// Write `texture` as a capture: the PNG at `path`, opaque, and the text
/// report beside it. The texture must be finished: draws into a render target
/// run when the camera next changes, so set another camera first (presenting
/// a pixel stage does). Returns the report's path.
pub fn write_capture(texture: &Texture2D, path: &str, note: &str) -> std::io::Result<PathBuf> {
    save_texture_png(texture, path);
    let txt = Path::new(path).with_extension("txt");
    std::fs::write(&txt, report::render(path, note))?;
    Ok(txt)
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

/// What the text report records. Off until [`report::enable`]; every line
/// drawn through `macroquad-text` — a game's rich text and a widget's label
/// alike — calls in, so nothing else has to. It lives with the text, below the
/// kit and the widgets; [`Capture::save`] writes it.
pub use macroquad_text::report;

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
    fn a_key_script_taps_and_holds_by_frame() {
        let keys = KeyScript::parse_all("10:Up, 20-23:right,5:space,7:key3,8:z").expect("parses");
        assert_eq!(
            keys[0],
            KeyScript {
                key: KeyCode::Up,
                from: 10,
                to: 11
            }
        );
        assert_eq!(keys[1].key, KeyCode::Right);
        assert_eq!(
            (19..25).map(|f| keys[1].down(f)).collect::<Vec<_>>(),
            [false, true, true, true, false, false]
        );
        assert!(keys[1].pressed(20) && !keys[1].pressed(21));
        assert_eq!(keys[2].key, KeyCode::Space);
        assert_eq!(keys[3].key, KeyCode::Key3);
        assert_eq!(keys[4].key, KeyCode::Z);
        let argv: Vec<String> = ["game", "--keys", "3:Left"].map(String::from).into();
        assert_eq!(Args::parse(&argv).keys[0].key, KeyCode::Left);
    }

    #[test]
    fn a_bad_key_script_says_which_part() {
        for (bad, says) in [
            ("10:Upp", "no key named `Upp`"),
            ("Up", "expected FRAME:Key"),
            ("x:Up", "not a frame number"),
            ("9-3:Up", "ends before it starts"),
        ] {
            let e = KeyScript::parse_all(bad).expect_err(bad);
            assert!(e.contains(says), "{bad}: {e}");
        }
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
