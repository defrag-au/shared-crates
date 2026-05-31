//! Scene abstraction — stackable game-loop participants.
//!
//! A [`Scene`] owns its state and knows how to update + render itself.
//! Scenes live in a [`SceneStack`]; the topmost runs every frame, with
//! lower scenes' state preserved but their `update`/`render` paused.
//! Pushing a new scene onto the stack lets a command (e.g. `wumpus`)
//! launch a game on top of a terminal; popping returns control to the
//! caller seamlessly.
//!
//! ## Lifecycle
//!
//! ```text
//!   tick(dt)                — for the topmost scene only
//!     ├─ update(dt, ctx)    — returns SceneOutcome
//!     │    Continue         → keep this scene next frame
//!     │    Exit             → on_exit(); pop from stack
//!     │    Push(new)        → push `new`; new.on_enter()
//!     │    Replace(new)     → on_exit(); pop self; push `new`; new.on_enter()
//!     └─ render(grid, ctx)  — same scene
//! ```
//!
//! Scenes never poll macroquad directly. The main render loop builds
//! a [`SceneInput`] snapshot once per frame and passes it via
//! [`SceneCtx`]; this makes scenes deterministic (the same input
//! sequence always yields the same state transitions) and
//! straightforward to feed synthetic input in tests.

use macroquad::input::KeyCode;
use macroquad::text::Font;
use macroquad::time::get_time;

use crate::grid::Grid;

// `Grid` is unused in this module's signatures (scenes own their own
// grids via `Scene::grid()` / `grid_mut()`); imported above so the
// type appears in this module's namespace for callers who `use
// macroquad_tui::scene::*` and want both surfaces in scope at once.
#[allow(unused_imports)]
use Grid as _;

/// Outcome of a `Scene::update` call — what the [`SceneStack`] should
/// do next.
pub enum SceneOutcome {
    /// Keep running this scene unchanged.
    Continue,
    /// Pop self off the stack; whatever was beneath becomes active.
    Exit,
    /// Push another scene onto the stack above self.
    Push(Box<dyn Scene>),
    /// Replace self with another scene (no net stack growth).
    Replace(Box<dyn Scene>),
}

/// A stackable game-loop participant.
///
/// Implementors own their state — including, optionally, a
/// [`Grid`]. The framework asks for that grid via [`grid`] /
/// [`grid_mut`] and paints it to the screen; scenes without a grid
/// (e.g. a sprite-only cutscene) return `None` and draw entirely
/// through macroquad's draw primitives in [`render`].
///
/// Lifecycle: only the topmost scene gets `update` / `render` each
/// frame. Lower scenes' state is preserved but their callbacks pause
/// until they're topmost again (at which point `on_enter` fires).
pub trait Scene {
    /// Per-frame update. `dt` is seconds since last tick. Return
    /// [`SceneOutcome::Continue`] to keep going, or one of the other
    /// variants to mutate the stack.
    fn update(&mut self, dt: f32, ctx: &SceneCtx) -> SceneOutcome;

    /// Per-frame render of anything that isn't the scene's grid —
    /// editor prompt, sprite overlays, status hints. The framework
    /// already painted [`grid`] before calling this.
    ///
    /// Default no-op for scenes that are pure grid-based.
    fn render(&mut self, _ctx: &SceneCtx) {}

    /// Borrow the scene's grid for painting by the framework. `None`
    /// for sprite-only scenes that don't have one.
    fn grid(&self) -> Option<&Grid> {
        None
    }

    /// Mutable borrow of the scene's grid. Lets the framework write
    /// into it for utilities (e.g. CRT screen-shake, transient
    /// effects); scenes typically write to their own grid via `self`
    /// rather than through this accessor.
    fn grid_mut(&mut self) -> Option<&mut Grid> {
        None
    }

    /// Fires when this scene becomes the active (topmost) scene —
    /// either freshly pushed, or revealed by the scene above popping.
    /// Default no-op.
    fn on_enter(&mut self, _ctx: &SceneCtx) {}

    /// Fires when this scene is about to be popped or replaced.
    /// Default no-op.
    fn on_exit(&mut self, _ctx: &SceneCtx) {}
}

/// Per-frame input snapshot. Built once by the main loop, passed into
/// scenes via [`SceneCtx`].
#[derive(Default)]
pub struct SceneInput {
    pub keys_pressed: Vec<KeyCode>,
    pub keys_down: Vec<KeyCode>,
    pub chars_pressed: Vec<char>,
    pub mouse_wheel_y: f32,
    pub mouse_x: f32,
    pub mouse_y: f32,
}

impl SceneInput {
    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    pub fn is_key_down(&self, key: KeyCode) -> bool {
        self.keys_down.contains(&key)
    }

    pub fn shift_held(&self) -> bool {
        self.is_key_down(KeyCode::LeftShift) || self.is_key_down(KeyCode::RightShift)
    }

    /// Snapshot from macroquad's input state. Call once per frame
    /// before passing to scenes. Helper so consumers don't have to
    /// repeat the same boilerplate.
    pub fn capture() -> Self {
        use macroquad::input::*;
        let mut chars = Vec::new();
        while let Some(c) = get_char_pressed() {
            chars.push(c);
        }
        // Compact list of keys we care about — exhaustive polling
        // every KeyCode would be wasteful. Covers terminal +
        // arcade-game vocabularies plus the full letter range for
        // hotkey use (game commands, viewer shortcuts). Letter polls
        // are 26 cheap calls per frame, well under the budget.
        let watch: &[KeyCode] = &[
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Space,
            KeyCode::Enter,
            KeyCode::Escape,
            KeyCode::Tab,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::LeftShift,
            KeyCode::RightShift,
            KeyCode::LeftControl,
            KeyCode::RightControl,
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            // Letters A–Z. macroquad's KeyCode lists them all
            // individually so we have to spell each out; the snapshot
            // then lets scenes treat any letter as a hotkey via the
            // standard `is_key_pressed` path.
            KeyCode::A,
            KeyCode::B,
            KeyCode::C,
            KeyCode::D,
            KeyCode::E,
            KeyCode::F,
            KeyCode::G,
            KeyCode::H,
            KeyCode::I,
            KeyCode::J,
            KeyCode::K,
            KeyCode::L,
            KeyCode::M,
            KeyCode::N,
            KeyCode::O,
            KeyCode::P,
            KeyCode::Q,
            KeyCode::R,
            KeyCode::S,
            KeyCode::T,
            KeyCode::U,
            KeyCode::V,
            KeyCode::W,
            KeyCode::X,
            KeyCode::Y,
            KeyCode::Z,
        ];
        let mut keys_pressed = Vec::new();
        let mut keys_down = Vec::new();
        for &k in watch {
            if is_key_pressed(k) {
                keys_pressed.push(k);
            }
            if is_key_down(k) {
                keys_down.push(k);
            }
        }
        let (mx, my) = mouse_position();
        Self {
            keys_pressed,
            keys_down,
            chars_pressed: chars,
            mouse_wheel_y: mouse_wheel().1,
            mouse_x: mx,
            mouse_y: my,
        }
    }
}

/// Per-frame context handed to scenes. Carries input, shared
/// resources, and timing. Scenes can pull what they need; new fields
/// can land here without churning every scene's signature.
pub struct SceneCtx<'a> {
    pub input: &'a SceneInput,
    pub font: Option<&'a Font>,
    /// Absolute seconds since macroquad started — useful for blink
    /// timing, sprite animation phases, etc.
    pub time: f32,
}

impl<'a> SceneCtx<'a> {
    pub fn new(input: &'a SceneInput, font: Option<&'a Font>) -> Self {
        Self {
            input,
            font,
            time: get_time() as f32,
        }
    }
}

/// Manages a stack of scenes. The topmost runs each frame; pushes /
/// pops happen between frames so a scene's `update` always completes
/// before the stack mutates.
pub struct SceneStack {
    scenes: Vec<Box<dyn Scene>>,
}

impl SceneStack {
    pub fn new(root: Box<dyn Scene>) -> Self {
        let mut s = Self {
            scenes: Vec::with_capacity(4),
        };
        s.scenes.push(root);
        s
    }

    /// Tick the topmost scene. Applies its [`SceneOutcome`] to the
    /// stack before returning. `on_enter` / `on_exit` callbacks fire
    /// at the appropriate transitions.
    ///
    /// Returns `true` while at least one scene remains. `false` once
    /// the last scene has exited (the main loop should break).
    pub fn tick(&mut self, dt: f32, ctx: &SceneCtx) -> bool {
        let outcome = match self.scenes.last_mut() {
            Some(s) => s.update(dt, ctx),
            None => return false,
        };
        match outcome {
            SceneOutcome::Continue => {}
            SceneOutcome::Exit => {
                if let Some(mut s) = self.scenes.pop() {
                    s.on_exit(ctx);
                }
                if let Some(s) = self.scenes.last_mut() {
                    s.on_enter(ctx);
                }
            }
            SceneOutcome::Push(mut new) => {
                new.on_enter(ctx);
                self.scenes.push(new);
            }
            SceneOutcome::Replace(mut new) => {
                if let Some(mut s) = self.scenes.pop() {
                    s.on_exit(ctx);
                }
                new.on_enter(ctx);
                self.scenes.push(new);
            }
        }
        !self.scenes.is_empty()
    }

    /// Run the topmost scene's `render` (overlay drawing). The
    /// framework should paint the scene's grid (via [`topmost_grid`])
    /// *before* calling this, so the overlay draws on top of the
    /// grid cells.
    pub fn render(&mut self, ctx: &SceneCtx) {
        if let Some(s) = self.scenes.last_mut() {
            s.render(ctx);
        }
    }

    /// Borrow the topmost scene's grid for painting.
    pub fn topmost_grid(&self) -> Option<&Grid> {
        self.scenes.last().and_then(|s| s.grid())
    }

    /// Mutable borrow of the topmost scene's grid. Useful when the
    /// framework provides utilities that touch grid contents directly
    /// (e.g. a CRT-effect overlay that ripples the buffer).
    pub fn topmost_grid_mut(&mut self) -> Option<&mut Grid> {
        self.scenes.last_mut().and_then(|s| s.grid_mut())
    }

    pub fn depth(&self) -> usize {
        self.scenes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }
}

/// Fixed-step accumulator — separates logic ticks from frame rate.
/// Most retro games want the logic to step at a fixed rate (Tetris
/// drops once per second; Snake moves every 80 ms) while the renderer
/// still runs at whatever the display refreshes at.
///
/// ```ignore
/// let mut step = FixedStep::new(5.0);  // 5 logic ticks per second
/// // in scene update:
/// for _ in 0..step.ticks(dt) {
///     game.advance_one_tick();
/// }
/// ```
pub struct FixedStep {
    interval: f32,
    accumulator: f32,
}

impl FixedStep {
    /// Construct a fixed step that fires at `hz` per second.
    pub fn new(hz: f32) -> Self {
        Self {
            interval: 1.0 / hz.max(0.001),
            accumulator: 0.0,
        }
    }

    /// Advance the accumulator; return how many full ticks should fire
    /// this frame. Usually `0` or `1`; can be more if the previous
    /// frame ran long.
    pub fn ticks(&mut self, dt: f32) -> u32 {
        self.accumulator += dt;
        let mut n = 0;
        while self.accumulator >= self.interval {
            self.accumulator -= self.interval;
            n += 1;
        }
        n
    }

    /// Reset the accumulator (e.g. when a scene resumes after pause).
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
    }

    /// Change the step rate at runtime — useful for Tetris-style
    /// "drop faster at higher levels". Doesn't reset the accumulator.
    pub fn set_hz(&mut self, hz: f32) {
        self.interval = 1.0 / hz.max(0.001);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_step_fires_at_expected_rate() {
        let mut s = FixedStep::new(10.0); // 10 Hz = 0.1 s interval
        assert_eq!(s.ticks(0.05), 0);
        assert_eq!(s.ticks(0.05), 1);
        assert_eq!(s.ticks(0.1), 1);
        // A long frame should catch up multiple ticks
        assert_eq!(s.ticks(0.5), 5);
    }

    #[test]
    fn fixed_step_reset_clears_accumulator() {
        let mut s = FixedStep::new(10.0);
        s.ticks(0.07);
        s.reset();
        assert_eq!(s.ticks(0.05), 0);
    }
}
