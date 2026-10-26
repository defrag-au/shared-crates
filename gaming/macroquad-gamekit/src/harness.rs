//! `harness` — the review loop a game client is built around: a fixed clock,
//! an exact moment captured to a PNG *and* a text report of every line drawn,
//! scripted pointers, and a window camera that is the right way up.
//!
//! A client wires it once (see `fight-macroquad` in augminted-bots for the
//! reference) and gets these flags natively:
//!
//! ```sh
//! game --shot out.png [--at 1.5] [--size 390x844]     # capture a moment
//!      [--px 3]                                      # at 3 pixels a unit, whatever the display
//!      [--hover N | --drag N | --longpress N | --rclick N]  # with a scripted pointer
//!      [--keys "10:Up,20-40:Right"]                  # with scripted keys, by frame
//! game --plan plan.txt                               # many conditions and captures, one launch
//! game --soak 20                                     # the game's own soak loop
//! game --sheet sheet.png                             # the game's own sheet of faces
//! game --sprites sprites.png [--only whale]          # every sprite (or one, larger) on a review sheet
//! game --sprites film.png --only whale --strip 24    # one sprite alone, played: frames in time order
//! game --view [whale]                                # the sprite viewer, for a person: tune and print
//! game --shot strip.png --at 9 --strip 12 [--every 2] [--crop x,y,w,h]  # frames in a row: motion in the scene
//! game --frame-log                                   # print every frame over 50 ms
//! ```
//!
//! ## Pixel art is reviewed on its own sheet
//!
//! A sprite in the scene overlaps others, moves, and shows whichever frame
//! the clock lands on, at whatever size a capture comes out. [`sprite_sheet`]
//! draws each one alone: every frame at a whole-number zoom and at its real
//! size, facing both ways, and as a solid silhouette (where a shape that does
//! not read shows at once), with each frame's ASCII onion skin against the
//! next in the report (see [`crate::pixel::Onion`]).
//!
//! A game does not write a command for it: it lists its art in
//! [`crate::Game::sprites`], and [`crate::app::run`] answers `--sprites` by
//! drawing that list and exiting, before the game's first frame.
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
//! ## A capture or soak's window says so in its title
//!
//! Both still open a window. [`Args::window_title`] appends
//! [`CAPTURE_TITLE_MARKER`] while capturing, or [`SOAK_TITLE_MARKER`] for a
//! soak, so a window manager can move the window out of the way; AeroSpace
//! matches both with `test %{window-title} ~= '\[(capture|soak)\]'`.
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
    /// `--sprites sprites.png`: the game's pixel art on a review sheet; see
    /// [`sprite_sheet`].
    pub sprites: Option<String>,
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
    /// `--px 3`: device pixels per design unit for a capture, whatever the
    /// window's. A headless or 1× display captures at 1, where a 36-pixel
    /// sprite is a smudge; text is rasterised at this scale too, so it stays
    /// sharp rather than enlarged. Ignored outside a capture and on a pixel
    /// stage, whose canvas is its own resolution.
    pub px: Option<f32>,
    /// `--strip 12` with `--shot`: from the capture's moment, take this many
    /// frames instead of one and write them as a [`crate::film::Film`]: the
    /// frames tiled, a trail and a change map, and a report of how far the
    /// mover moved each step.
    pub strip: Option<u32>,
    /// `--every 2`: display frames between a strip's captures (2, thirty a
    /// second, when not given).
    pub every: Option<u32>,
    /// `--crop x,y,w,h`: a strip's frames cut to this rectangle, in design
    /// units: one mover, large enough to read.
    pub crop: Option<(f32, f32, f32, f32)>,
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

/// Appended to a game's window title while it is capturing, so a window
/// manager can move the window out of the tiling tree. AeroSpace matches it
/// with `test %{window-title} ~= '\[(capture|soak)\]'`.
pub const CAPTURE_TITLE_MARKER: &str = "[capture]";

/// Appended to a game's window title while its soak runs, for the same reason
/// as [`CAPTURE_TITLE_MARKER`]: a soak still opens a window and can hold it for
/// minutes.
pub const SOAK_TITLE_MARKER: &str = "[soak]";

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
            sprites: flag_in(argv, "--sprites"),
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
            px: flag_in(argv, "--px")
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|p| *p > 0.0 && *p <= 8.0),
            strip: flag_in(argv, "--strip")
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|n| (2..=64).contains(n)),
            every: flag_in(argv, "--every")
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|n| *n >= 1),
            crop: flag_in(argv, "--crop").and_then(|v| {
                let n: Vec<f32> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                match n[..] {
                    [x, y, w, h] if w > 0.0 && h > 0.0 => Some((x, y, w, h)),
                    _ => None,
                }
            }),
        }
    }

    /// The pixels per design unit a frame is drawn at: `--px` while
    /// capturing, else the window's own `px`.
    pub fn capture_px(&self, px: f32) -> f32 {
        match self.px {
            Some(p) if self.capturing() => p,
            _ => px,
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
        self.shot.is_some() || self.sheet.is_some() || self.sprites.is_some() || self.plan.is_some()
    }

    /// The window size: `--size`, else `default`.
    pub fn window_size(&self, default: (i32, i32)) -> (i32, i32) {
        self.size.unwrap_or(default)
    }

    /// The window title this run should use: `base`, with
    /// [`CAPTURE_TITLE_MARKER`] appended while capturing or
    /// [`SOAK_TITLE_MARKER`] while a soak runs.
    ///
    /// Set it from `window_conf`, where the title is part of the window as it
    /// is created. A title set after the first frame is too late for an
    /// `on-window-detected` rule, and a debug binary shares its bundle id with
    /// every other unbundled `cargo` binary.
    pub fn window_title(&self, base: &str) -> String {
        if self.capturing() {
            format!("{base} {CAPTURE_TITLE_MARKER}")
        } else if self.soak.is_some() {
            format!("{base} {SOAK_TITLE_MARKER}")
        } else {
            base.to_owned()
        }
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

    /// The finished frame as an image, upright, for a strip.
    pub fn image(self) -> Image {
        set_default_camera();
        self.target.texture.get_texture_data()
    }
}

/// A `--strip` in progress: frames gathered from the capture's moment on.
pub struct StripRun {
    film: Option<crate::film::Film>,
    count: u32,
    every: u32,
    next: u32,
    crop: Option<(f32, f32, f32, f32)>,
}

impl StripRun {
    /// A strip starting at display frame `from`.
    pub fn new(args: &Args, count: u32, from: u32) -> Self {
        Self {
            film: None,
            count,
            every: args.every.unwrap_or(2),
            next: from,
            crop: args.crop,
        }
    }

    /// Whether display frame `frame` is one the strip takes.
    pub fn due(&self, frame: u32) -> bool {
        frame == self.next
    }

    /// Add a frame drawn at `px` pixels a design unit; true once the strip
    /// has all its frames.
    pub fn add(&mut self, image: &Image, px: f32) -> bool {
        let (iw, ih) = (image.width as usize, image.height as usize);
        let (x0, y0, w, h) = match self.crop {
            Some((x, y, w, h)) => (
                ((x * px).round().max(0.0) as usize).min(iw),
                ((y * px).round().max(0.0) as usize).min(ih),
                (w * px).round() as usize,
                (h * px).round() as usize,
            ),
            None => (0, 0, iw, ih),
        };
        let (w, h) = (w.min(iw - x0).max(1), h.min(ih - y0).max(1));
        // A render target reads back bottom row first (`export_png` flips
        // it back): turn it top-down here, so the crop is measured from the
        // top as design units are.
        let mut bytes = vec![0u8; w * h * 4];
        for y in 0..h {
            let s = ((ih - 1 - (y0 + y)) * iw + x0) * 4;
            bytes[y * w * 4..(y + 1) * w * 4].copy_from_slice(&image.bytes[s..s + w * 4]);
        }
        let film = self
            .film
            .get_or_insert_with(|| crate::film::Film::new(w, h, self.every));
        film.push(&bytes, w, h);
        self.next += self.every;
        film.frames.len() as u32 >= self.count
    }

    /// Write the strip's sheet to `path` and its report beside it.
    pub fn save(&self, path: &str, note: &str) -> std::io::Result<PathBuf> {
        let film = self
            .film
            .as_ref()
            .ok_or_else(|| std::io::Error::other("the strip took no frames"))?;
        write_film(path, film, &format!("# Strip: {path}\n{note}"))
    }
}

/// Write `film`'s sheet to `path` and its report beside it, headed by
/// `header`. Returns the report's path.
pub fn write_film(path: &str, film: &crate::film::Film, header: &str) -> std::io::Result<PathBuf> {
    // As many a row as fit about 1900 px, so the sheet is read whole.
    let cols = (1900 / (film.w + 6)).clamp(2, 8);
    let (top_down, w, h) = film.sheet(cols);
    // `export_png` writes bottom row first, as a target reads back.
    let bytes: Vec<u8> = top_down
        .chunks_exact(w * 4)
        .rev()
        .flatten()
        .copied()
        .collect();
    Image {
        bytes,
        width: w as u16,
        height: h as u16,
    }
    .export_png(path);
    let txt = Path::new(path).with_extension("txt");
    std::fs::write(
        &txt,
        format!(
            "{header}\n\nFrames in reading order, {cols} a row, {} display frames apart at 60 Hz; the last row is the trail (every frame averaged) and the change map (what changed each step, blue early to yellow late).\n\n{}",
            film.every,
            film.text()
        ),
    )?;
    Ok(txt)
}

/// One sprite alone, played through its gait and moved by its body:
/// `count` frames `every` display frames apart, at a zoom that makes it
/// about 300 px wide, over `backdrop`. The report leads with the gait
/// cycle's problems ([`crate::motion::Cycle`]), then the film's. What
/// `--sprites out.png --only NAME --strip N` writes.
pub fn sprite_film(
    path: &str,
    sprite: &SheetSprite,
    count: usize,
    every: u32,
    backdrop: Color,
) -> std::io::Result<PathBuf> {
    let gait = sprite
        .gait
        .as_ref()
        .ok_or_else(|| std::io::Error::other(format!("{} has no gait to play", sprite.name)))?;
    let w = sprite.frames.first().map_or(1, |g| g.w.max(1));
    let zoom = (300 / w).clamp(2, 16);
    let bg = [
        (backdrop.r * 255.0) as u8,
        (backdrop.g * 255.0) as u8,
        (backdrop.b * 255.0) as u8,
        255,
    ];
    let film = crate::film::of_sprite(&sprite.frames, gait, sprite.body, zoom, count, every, bg);
    let cycle = crate::motion::Cycle::play(&sprite.frames, gait, sprite.scale);
    let header = format!(
        "# Sprite film: {} alone, {} frames at {zoom}x, body bob {:.1} and surge {:.1} art px\n\n## The gait cycle\n\n{}",
        sprite.name,
        sprite.frames.len(),
        sprite.body.bob,
        sprite.body.surge,
        cycle.text()
    );
    write_film(path, &film, &header)
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

/// One sprite on a [`sprite_sheet`]: what a game lists in
/// [`crate::Game::sprites`].
#[derive(Clone, Debug)]
pub struct SheetSprite {
    pub name: String,
    /// Its frames in order, all one size.
    pub frames: Vec<crate::pixel::Grid>,
    /// Design units an art pixel is drawn at in the game: its real size.
    pub scale: f32,
    /// How its frames play. With one, the sheet adds a strip of the cycle
    /// sampled across the beat, the frames rising and falling with its swing.
    pub gait: Option<crate::gait::Gait>,
    /// How the body moves with the gait, in art pixels, as the game moves
    /// it: what a sprite's film (`--strip` with `--sprites`) plays.
    pub body: crate::film::Body,
    /// The sprite file it was read from, when it was: where the viewer
    /// writes tuned values back.
    pub source: Option<String>,
}

impl SheetSprite {
    /// A sprite as its file gives it: frames in the file's palette, its
    /// scale, gait (for more than one frame) and body, and where it lives.
    pub fn from_file(file: &crate::sprite_file::SpriteFile) -> Self {
        let frames = file
            .grids()
            .unwrap_or_else(|e| panic!("{}: {e}", file.name));
        let mut s = if frames.len() > 1 {
            Self::moving(&file.name, frames, file.scale, file.gait())
        } else {
            Self::still(&file.name, frames, file.scale)
        };
        s.body = file.body();
        s.source = file.path.clone();
        s
    }

    /// The body's bob and surge with the gait, in art pixels: give the
    /// numbers the game moves it by, so the film is the game's motion.
    pub fn body(mut self, bob: f32, surge: f32) -> Self {
        self.body = crate::film::Body { bob, surge };
        self
    }

    /// A sprite that does not move: its frames at `scale`, no gait.
    pub fn still(name: &str, frames: Vec<crate::pixel::Grid>, scale: f32) -> Self {
        Self {
            name: name.to_string(),
            frames,
            scale,
            gait: None,
            body: crate::film::Body::default(),
            source: None,
        }
    }

    /// A sprite whose frames play by `gait`.
    pub fn moving(
        name: &str,
        frames: Vec<crate::pixel::Grid>,
        scale: f32,
        gait: crate::gait::Gait,
    ) -> Self {
        Self {
            gait: Some(gait),
            ..Self::still(name, frames, scale)
        }
    }
}

/// Draw `sprites` on one review sheet and write it to `path`, with a report
/// beside it: the `--sprites` flag's work. A game does not call this: it
/// lists its art in [`crate::Game::sprites`] and [`crate::app::run`] draws
/// the sheet when the flag is given.
///
/// Each sprite is a row: every frame at a whole-number zoom over a checker,
/// the same facing the other way, every frame as a white silhouette (where a
/// shape that does not read shows at once: a tail that is a stalk, a fin
/// that is a speck), every frame at its real size over `backdrop`, and, given
/// a gait, its cycle sampled across the beat. The report lists each sprite's
/// size and frames and the ASCII onion skin of each frame against the next
/// (the last against the first), so motion can be judged in text too.
///
/// Needs the window, for the textures; the art itself is checked by the
/// game's own tests on [`crate::pixel::Grid`].
pub fn sprite_sheet(
    path: &str,
    sprites: &[SheetSprite],
    backdrop: Color,
    note: &str,
) -> std::io::Result<PathBuf> {
    const MARGIN: f32 = 16.0;
    const GAP: f32 = 10.0;
    const LABEL: f32 = 22.0;
    const STRIP_SAMPLES: usize = 12;
    // The cycle strip is drawn at twice real size: still the size the game
    // draws, near enough to judge, but big enough to read the bob.
    const STRIP_K: f32 = 2.0;
    let cycle_of = |s: &SheetSprite| -> Option<crate::motion::Cycle> {
        let gait = s.gait.as_ref()?;
        (s.frames.len() > 1).then(|| crate::motion::Cycle::play(&s.frames, gait, s.scale))
    };
    let widest = sprites
        .iter()
        .filter_map(|s| s.frames.first().map(|g| g.w))
        .max()
        .unwrap_or(8)
        .max(1);
    // A whole-number zoom that makes the widest frame about 200 px.
    // Fewer sprites (`--only`), larger: the sheet is read by eye and by a
    // reviewer that sees images about 2000 px wide, so a lone sprite gets
    // the room.
    let target = if sprites.len() <= 2 { 300 } else { 200 };
    let zoom = (target / widest).clamp(2, 16);
    let bg = [
        (backdrop.r * 255.0) as u8,
        (backdrop.g * 255.0) as u8,
        (backdrop.b * 255.0) as u8,
        255,
    ];

    // Lay everything out first, wrapping at a width a reviewer sees whole,
    // then size the sheet to it and draw.
    let mut flow = Flow::new(MARGIN, SHEET_WIDTH - MARGIN);
    let mut ops: Vec<Op> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut body = String::new();
    let ink = Color::from_rgba(220, 224, 236, 255);
    let quiet = Color::from_rgba(150, 156, 170, 255);
    for s in sprites {
        let Some(first) = s.frames.first() else {
            continue;
        };
        let (w, h) = (first.w, first.h);
        let (zw, zh) = ((w * zoom) as f32, (h * zoom) as f32);
        let real = vec2(w as f32 * s.scale, h as f32 * s.scale);
        flow.gap(GAP);
        ops.push(Op::text(
            format!(
                "{}  {w}x{h}, {} frame{}, drawn {:.2} a pixel",
                s.name,
                s.frames.len(),
                if s.frames.len() == 1 { "" } else { "s" },
                s.scale
            ),
            flow.label(LABEL),
            18.0,
            ink,
        ));
        let section = |flow: &mut Flow, ops: &mut Vec<Op>, title: &str| {
            ops.push(Op::text(
                title.to_string(),
                flow.label(LABEL - 4.0),
                15.0,
                quiet,
            ));
        };
        section(&mut flow, &mut ops, "frames");
        for (i, g) in s.frames.iter().enumerate() {
            let at = flow.place(vec2(zw, zh));
            ops.push(Op::image(g.image(zoom, None), at, vec2(zw, zh), WHITE));
            ops.push(Op::text(format!("f{i}"), at + vec2(4.0, 16.0), 16.0, ink));
        }
        section(&mut flow, &mut ops, "flipped");
        for g in &s.frames {
            let at = flow.place(vec2(zw, zh));
            ops.push(Op::image(
                g.flipped().image(zoom, None),
                at,
                vec2(zw, zh),
                WHITE,
            ));
        }
        section(
            &mut flow,
            &mut ops,
            "silhouettes: does the white shape alone name it?",
        );
        for g in &s.frames {
            let at = flow.place(vec2(zw, zh));
            let img = g.silhouette(0xffffff).image(zoom, Some([24, 24, 32, 255]));
            ops.push(Op::image(img, at, vec2(zw, zh), WHITE));
        }
        section(
            &mut flow,
            &mut ops,
            "real size, then the cycle at twice real size",
        );
        for g in &s.frames {
            let at = flow.place(real);
            ops.push(Op::image(g.image(1, Some(bg)), at, real, WHITE));
        }
        if let Some(gait) = &s.gait {
            let big = real * STRIP_K;
            for i in 0..STRIP_SAMPLES {
                let stride = gait.at_beat(i as f32 / STRIP_SAMPLES as f32);
                let g = &s.frames[stride.frame.min(s.frames.len() - 1)];
                let cell = flow.place(vec2(big.x, big.y * 2.0));
                ops.push(Op::Rect(
                    Rect::new(cell.x, cell.y, big.x, big.y * 2.0),
                    backdrop,
                ));
                let rise = stride.swing * big.y * 0.25;
                let at = cell + vec2(0.0, big.y * 0.5 - rise);
                ops.push(Op::image(g.image(1, Some(bg)), at, big, WHITE));
            }
        }

        body.push_str(&format!(
            "\n\n## {} ({w}x{h}, {} frames)\n",
            s.name,
            s.frames.len()
        ));
        match cycle_of(s) {
            Some(cycle) => {
                for p in &cycle.problems {
                    problems.push(format!("{}: {p}", s.name));
                }
                body.push_str(&cycle.text());
                section(
                    &mut flow,
                    &mut ops,
                    "as played at 60 Hz (red: the pose before), then the trail with the moving part's path",
                );
                cycle_ops(&cycle, &s.frames, zoom, &mut flow, &mut ops);
                // The onion skins of the changes as played, each once.
                let mut seen: Vec<(usize, usize)> = Vec::new();
                for st in &cycle.steps {
                    if seen.contains(&(st.from, st.to)) {
                        continue;
                    }
                    seen.push((st.from, st.to));
                    body.push_str(&format!(
                        "\nframe {} to {}:\n{}\n",
                        st.from,
                        st.to,
                        s.frames[st.from].onion(&s.frames[st.to]).text()
                    ));
                }
            }
            None if s.frames.len() > 1 => {
                let n = s.frames.len();
                for i in 0..n {
                    let j = (i + 1) % n;
                    if n == 2 && i == 1 {
                        break;
                    }
                    body.push_str(&format!(
                        "\nframe {i} to {j}:\n{}\n",
                        s.frames[i].onion(&s.frames[j]).text()
                    ));
                }
            }
            None => {}
        }
        flow.gap(GAP * 2.0);
    }
    let size = vec2(flow.widest + MARGIN, flow.bottom() + MARGIN);
    let capture = Capture::new(size, 1.0, Vec2::ZERO);
    clear_background(Color::from_rgba(20, 20, 26, 255));
    for op in &ops {
        op.draw();
    }
    let mut report = String::from(note);
    report.push_str("\n\n## Problems\n\n");
    if problems.is_empty() {
        report.push_str("none\n");
    }
    for p in &problems {
        report.push_str(&format!("- {p}\n"));
    }
    report.push_str(&body);
    capture.save(path, &report)
}

/// The widest a review sheet is laid out: a reviewer that reads images
/// sees about 2000 px whole, and anything wider is shrunk until pixel art
/// is a smudge.
const SHEET_WIDTH: f32 = 1900.0;

/// Panels laid out left to right, wrapping at a width, each section on a
/// line of its own.
struct Flow {
    left: f32,
    right: f32,
    x: f32,
    y: f32,
    line: f32,
    /// The furthest right anything reached.
    widest: f32,
}

impl Flow {
    const GAP: f32 = 10.0;

    fn new(left: f32, right: f32) -> Self {
        Self {
            left,
            right,
            x: left,
            y: left,
            line: 0.0,
            widest: left,
        }
    }

    /// Finish the current line, if anything is on it.
    fn break_line(&mut self) {
        if self.line > 0.0 {
            self.y += self.line + Self::GAP;
        }
        self.x = self.left;
        self.line = 0.0;
    }

    /// Space before what comes next, on a line of its own.
    fn gap(&mut self, h: f32) {
        self.break_line();
        self.y += h;
    }

    /// A label's top-left on a line of its own, `h` tall.
    fn label(&mut self, h: f32) -> Vec2 {
        self.break_line();
        let at = vec2(self.x, self.y + h - 6.0);
        self.y += h;
        at
    }

    /// A panel's top-left, wrapping when it would pass the right edge.
    fn place(&mut self, size: Vec2) -> Vec2 {
        if self.x > self.left && self.x + size.x > self.right {
            self.break_line();
        }
        let at = vec2(self.x, self.y);
        self.x += size.x + Self::GAP;
        self.line = self.line.max(size.y);
        self.widest = self.widest.max(at.x + size.x);
        at
    }

    /// The bottom of everything placed.
    fn bottom(&self) -> f32 {
        self.y + self.line
    }
}

/// One thing to draw on a sheet, laid out before the sheet is sized.
enum Op {
    /// Text with its baseline's left at the point.
    Text(String, Vec2, f32, Color),
    /// An image stretched to a size, tinted, nearest-filtered.
    Image(Image, Vec2, Vec2, Color),
    Rect(Rect, Color),
    Line(Vec2, Vec2, Color),
    Dot(Vec2, Color),
}

impl Op {
    fn text(s: String, at: Vec2, size: f32, color: Color) -> Self {
        Op::Text(s, at, size, color)
    }

    fn image(img: Image, at: Vec2, size: Vec2, tint: Color) -> Self {
        Op::Image(img, at, size, tint)
    }

    fn draw(&self) {
        match self {
            Op::Text(s, at, size, color) => {
                draw_text(s, at.x, at.y, *size, *color);
            }
            Op::Image(img, at, size, tint) => {
                let t = Texture2D::from_image(img);
                t.set_filter(FilterMode::Nearest);
                draw_texture_ex(
                    &t,
                    at.x,
                    at.y,
                    *tint,
                    DrawTextureParams {
                        dest_size: Some(*size),
                        ..Default::default()
                    },
                );
            }
            Op::Rect(r, color) => draw_rectangle(r.x, r.y, r.w, r.h, *color),
            Op::Line(a, b, color) => draw_line(a.x, a.y, b.x, b.y, 2.0, *color),
            Op::Dot(at, color) => draw_circle(at.x, at.y, 4.0, *color),
        }
    }
}

/// A grid as an image with nothing behind it.
fn bare(g: &crate::pixel::Grid) -> Image {
    Image {
        bytes: g.rgba(),
        width: g.w as u16,
        height: g.h as u16,
    }
}

/// A cycle as played: each pose at `zoom` with the pose before it ghosted
/// in red behind it (where it no longer is), labelled with its frame and
/// hold, then a trail of every pose overlaid with the moving part's path
/// through them in order.
fn cycle_ops(
    cycle: &crate::motion::Cycle,
    frames: &[crate::pixel::Grid],
    zoom: usize,
    flow: &mut Flow,
    ops: &mut Vec<Op>,
) {
    let Some(first) = frames.first() else {
        return;
    };
    let size = vec2((first.w * zoom) as f32, (first.h * zoom) as f32);
    let ink = Color::from_rgba(220, 224, 236, 255);
    let n = cycle.runs.len();
    for (i, run) in cycle.runs.iter().enumerate() {
        let before = cycle.runs[(i + n - 1) % n].frame;
        let at = flow.place(size);
        ops.push(Op::Rect(
            Rect::new(at.x, at.y, size.x, size.y),
            Color::from_rgba(44, 44, 52, 255),
        ));
        if n > 1 {
            ops.push(Op::image(
                bare(&frames[before].silhouette(0xff5050)),
                at,
                size,
                Color::new(1.0, 1.0, 1.0, 0.55),
            ));
        }
        ops.push(Op::image(bare(&frames[run.frame]), at, size, WHITE));
        ops.push(Op::text(
            format!("f{} {}f", run.frame, run.ticks),
            at + vec2(4.0, 16.0),
            18.0,
            ink,
        ));
        // The change into this pose, marked where it is a problem.
        let warn = Color::from_rgba(255, 110, 100, 255);
        if let Some(step) = cycle.steps.get(i)
            && step.shift.travel > crate::motion::POP
        {
            ops.push(Op::text(
                format!("POP {:.1}px", step.shift.travel),
                at + vec2(4.0, size.y - 6.0),
                18.0,
                warn,
            ));
        }
        if run.ticks < crate::motion::FLICKER {
            ops.push(Op::text(
                "FLICKER".to_string(),
                at + vec2(4.0, 34.0),
                18.0,
                warn,
            ));
        }
    }
    // The trail: every pose faint, the path over them.
    let at = flow.place(size);
    ops.push(Op::Rect(
        Rect::new(at.x, at.y, size.x, size.y),
        Color::from_rgba(24, 24, 32, 255),
    ));
    let mut drawn: Vec<usize> = Vec::new();
    for run in &cycle.runs {
        if !drawn.contains(&run.frame) {
            drawn.push(run.frame);
        }
    }
    let fade = (1.6 / drawn.len().max(1) as f32).min(1.0);
    for f in &drawn {
        ops.push(Op::image(
            bare(&frames[*f]),
            at,
            size,
            Color::new(1.0, 1.0, 1.0, fade),
        ));
    }
    let path: Vec<Vec2> = cycle
        .path()
        .iter()
        .map(|(px, py)| at + vec2(px * zoom as f32, py * zoom as f32))
        .collect();
    let gold = Color::from_rgba(255, 215, 94, 255);
    for i in 0..path.len() {
        ops.push(Op::Line(path[i], path[(i + 1) % path.len()], gold));
    }
    for (i, p) in path.iter().enumerate() {
        ops.push(Op::Dot(*p, gold));
        ops.push(Op::text(format!("{i}"), *p + vec2(5.0, -4.0), 16.0, gold));
    }
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
    fn a_capture_or_soak_marks_the_window_title_and_play_does_not() {
        let capture = Args::parse(&["game".into(), "--shot".into(), "a.png".into()]);
        assert_eq!(capture.window_title("game"), "game [capture]");
        let soak = Args::parse(&["game".into(), "--soak".into(), "20".into()]);
        assert_eq!(soak.window_title("game"), "game [soak]");
        let play = Args::parse(&["game".into()]);
        assert_eq!(play.window_title("game"), "game");
    }

    /// `--px` only sets the scale of a capture, and only a sane one.
    #[test]
    fn px_sets_a_captures_scale_only() {
        let parse = |s: &str| Args::parse(&s.split(' ').map(String::from).collect::<Vec<_>>());
        assert_eq!(parse("game --shot a.png --px 3").capture_px(1.0), 3.0);
        assert_eq!(parse("game --px 3").capture_px(1.5), 1.5, "not capturing");
        assert_eq!(parse("game --shot a.png --px 40").px, None, "out of range");
        assert_eq!(parse("game --shot a.png").capture_px(2.0), 2.0);
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
