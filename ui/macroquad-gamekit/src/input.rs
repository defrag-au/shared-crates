//! `Actions` — a game's verbs (hop, fire, pause) bound to keys, swipes, taps and pad buttons, with key repeat and press buffering.
//!
//! The loop reads the devices once a frame into an [`InputSnapshot`] — keys,
//! the stage-unit gesture, and any `--keys` script merged in — and hands it to
//! the game as [`Frame::input`](crate::Frame::input). [`Actions::update`] turns
//! a snapshot into actions. It never reads macroquad, so it is tested without
//! a window, and a scripted key is indistinguishable from a player's.
//!
//! ## Buffering: why [`Actions::take`]
//!
//! A press is an instant; a game is often not ready for it. A frog mid-hop, a
//! fighter mid-swing, a tick that did not fall on this frame: polling
//! `is_key_pressed` there drops the press, and quick tapping feels sluggish.
//! Every press is kept for [`Actions::buffer`] seconds (0.15 by default), and
//! [`Actions::take`] spends it when the game is ready:
//!
//! ```ignore
//! // update: once a frame
//! self.actions.update(frame.input, frame.dt);
//! // tick: when the hop has landed
//! if self.hop.is_none() {
//!     for d in Dir4::ALL {
//!         if self.actions.take(Act::Hop(d)) { self.start_hop(d); break }
//!     }
//! }
//! ```
//!
//! [`Actions::pressed`] is the plain edge, for this frame only: right in
//! `update`, wrong in `tick`, which may run zero times on a frame.
//!
//! ## Pads
//!
//! The loop does not own the pads; a game that wants them owns a
//! [`Pads`](crate::Pads) (feature `gamepads`) and adds one to its copy of the
//! snapshot with [`InputSnapshot::with_pad`].

use macroquad::prelude::*;
use macroquad_widgets::Gesture;

use crate::dir::Dir4;
use crate::harness::KeyScript;
use crate::pointer::Swipes;

/// One frame's input, in stage units, as plain data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputSnapshot {
    /// Keys held this frame.
    pub keys_down: Vec<KeyCode>,
    /// Keys that went down this frame.
    pub keys_pressed: Vec<KeyCode>,
    /// Taps, drags and page-turn swipes, in stage units.
    pub gesture: Gesture,
    /// Every finger on the screen now, in stage units: the multi-touch a
    /// virtual pad needs, where one thumb steers while the other fires. The
    /// gesture follows only the first.
    pub touches: Vec<Vec2>,
    /// Pad buttons and directions, if the game added a pad.
    pub pad: PadState,
}

/// One pad's buttons and direction, as plain data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadState {
    /// Buttons held, by their [`PadButton`] number.
    pub down: Vec<PadButton>,
    /// Buttons that went down this frame.
    pub pressed: Vec<PadButton>,
    /// The d-pad while held, else the stick, y down, length ≤ 1.
    pub dir: Vec2,
}

/// A pad button. With feature `gamepads` this is the `gamepads` crate's
/// `Button`; without it, a placeholder no pad ever reports.
#[cfg(feature = "gamepads")]
pub type PadButton = crate::pads::Button;
#[cfg(not(feature = "gamepads"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadButton {}

impl InputSnapshot {
    /// Read the keyboard and the touches, merge in the `--keys` script for
    /// `frame`, and take `gesture`. `to_stage` maps a window point into stage
    /// units; `gesture` is already mapped. The loop calls this; a game reads
    /// the result from [`Frame::input`](crate::Frame::input).
    pub fn capture(
        gesture: Gesture,
        to_stage: impl Fn(Vec2) -> Vec2,
        script: &[KeyScript],
        frame: u32,
    ) -> Self {
        let mut s = Self {
            keys_down: get_keys_down().into_iter().collect(),
            keys_pressed: get_keys_pressed().into_iter().collect(),
            gesture,
            // An ended touch is still listed on the frame it lifts; counting it
            // would hold a pad button one tick after the finger left, and a
            // replay would reproduce that drift faithfully.
            touches: touches()
                .into_iter()
                .filter(|t| !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled))
                .map(|t| to_stage(t.position))
                .collect(),
            pad: PadState::default(),
        };
        s.script(script, frame);
        s
    }

    /// Add what `script` holds and presses on `frame`.
    pub fn script(&mut self, script: &[KeyScript], frame: u32) {
        for k in script {
            if k.down(frame) && !self.keys_down.contains(&k.key) {
                self.keys_down.push(k.key);
            }
            if k.pressed(frame) && !self.keys_pressed.contains(&k.key) {
                self.keys_pressed.push(k.key);
            }
        }
    }

    /// This snapshot with `pad`'s buttons and direction in it.
    #[cfg(feature = "gamepads")]
    pub fn with_pad(&self, pad: &crate::Pad) -> Self {
        use crate::pads::Button::*;
        const BUTTONS: [crate::pads::Button; 17] = [
            ActionDown,
            ActionRight,
            ActionLeft,
            ActionUp,
            FrontLeftUpper,
            FrontRightUpper,
            FrontLeftLower,
            FrontRightLower,
            LeftCenterCluster,
            RightCenterCluster,
            LeftStick,
            RightStick,
            DPadUp,
            DPadDown,
            DPadLeft,
            DPadRight,
            Mode,
        ];
        let mut s = self.clone();
        s.pad = PadState {
            down: BUTTONS.into_iter().filter(|b| pad.down(*b)).collect(),
            pressed: BUTTONS.into_iter().filter(|b| pad.pressed(*b)).collect(),
            dir: pad.direction(),
        };
        s
    }

    fn key_down(&self, k: KeyCode) -> bool {
        self.keys_down.contains(&k)
    }

    fn key_pressed(&self, k: KeyCode) -> bool {
        self.keys_pressed.contains(&k)
    }
}

/// How far a pad direction must lean to count as held that way.
const PAD_DIR_THRESHOLD: f32 = 0.5;

/// What triggers an action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Binding {
    /// A key: pressed when it goes down, held while it is down.
    Key(KeyCode),
    /// A four-way swipe: pressed on the frame it lifts; never held.
    Swipe(Dir4),
    /// A tap or click anywhere: pressed on the frame it lifts; never held.
    Tap,
    /// A pad button.
    Pad(PadButton),
    /// The pad's d-pad or stick leaning this way.
    PadDir(Dir4),
    /// A finger, or the mouse held down, inside this rectangle of the stage:
    /// a button of an on-screen pad. Pressed when one arrives, held while one
    /// stays.
    Zone(Rect),
}

/// One action's state.
#[derive(Clone, Copy, Debug, Default)]
struct State {
    held: bool,
    pressed: bool,
    /// Seconds since the buffered press, if one is waiting.
    buffered: Option<f32>,
    /// Seconds held, for repeat.
    held_for: f32,
    /// Repeats fired during this hold.
    repeats: u32,
}

/// A game's actions, `A` (an enum of its verbs), bound to inputs. Host-owned:
/// update once a frame, then ask.
#[derive(Clone, Debug)]
pub struct Actions<A> {
    bindings: Vec<(A, Binding)>,
    repeats: Vec<(A, f32, f32)>,
    buffer: f32,
    swipes: Swipes,
    states: Vec<(A, State)>,
}

/// Default seconds a press waits for [`Actions::take`].
pub const BUFFER: f32 = 0.15;

/// Default stage units a press must travel to be a [`Binding::Swipe`].
pub const SWIPE_MIN: f32 = 24.0;

impl<A: Copy + PartialEq> Actions<A> {
    /// Actions bound as listed; an action may have several bindings.
    pub fn new(bindings: &[(A, Binding)]) -> Self {
        let mut states: Vec<(A, State)> = Vec::new();
        for (a, _) in bindings {
            if !states.iter().any(|(s, _)| s == a) {
                states.push((*a, State::default()));
            }
        }
        Self {
            bindings: bindings.to_vec(),
            repeats: Vec::new(),
            buffer: BUFFER,
            swipes: Swipes::new(SWIPE_MIN),
            states,
        }
    }

    /// While `a` is held, press it again after `initial` seconds and every
    /// `interval` after that (a menu cursor, a hop that keeps going).
    pub fn repeat(mut self, a: A, initial: f32, interval: f32) -> Self {
        self.repeats.retain(|(r, _, _)| *r != a);
        self.repeats.push((a, initial, interval.max(0.001)));
        self
    }

    /// Keep each press this many seconds for [`take`](Self::take).
    pub fn buffer(mut self, seconds: f32) -> Self {
        self.buffer = seconds.max(0.0);
        self
    }

    /// Stage units a press must travel to be a swipe.
    pub fn swipe_min(mut self, min: f32) -> Self {
        self.swipes.min = min;
        self
    }

    /// Read this frame's input. Once a frame, in [`Game::update`](crate::Game::update).
    pub fn update(&mut self, input: &InputSnapshot, dt: f32) {
        let swipe = self.swipes.update(&input.gesture).map(|s| s.dir);
        let tap = input.gesture.tap.is_some() && swipe.is_none();
        let leaning = |d: Dir4| input.pad.dir.dot(d.vec()) >= PAD_DIR_THRESHOLD;
        for (a, st) in &mut self.states {
            let mut held = false;
            let mut edge = false;
            for (_, b) in self.bindings.iter().filter(|(x, _)| x == a) {
                match *b {
                    Binding::Key(k) => {
                        held |= input.key_down(k);
                        edge |= input.key_pressed(k);
                    }
                    Binding::Swipe(d) => edge |= swipe == Some(d),
                    Binding::Tap => edge |= tap,
                    Binding::Pad(btn) => {
                        held |= input.pad.down.contains(&btn);
                        edge |= input.pad.pressed.contains(&btn);
                    }
                    Binding::PadDir(d) => {
                        let now = leaning(d);
                        edge |= now && !st.held;
                        held |= now;
                    }
                    Binding::Zone(r) => {
                        let now = input
                            .touches
                            .iter()
                            .copied()
                            .chain(input.gesture.drag)
                            .any(|p| r.contains(p));
                        edge |= now && !st.held;
                        held |= now;
                    }
                }
            }
            let mut fired = edge;
            if edge {
                st.held_for = 0.0;
                st.repeats = 0;
            } else if held {
                st.held_for += dt;
                if let Some((_, initial, interval)) = self.repeats.iter().find(|(r, ..)| r == a) {
                    let due = if st.held_for >= *initial {
                        1 + ((st.held_for - initial) / interval) as u32
                    } else {
                        0
                    };
                    if due > st.repeats {
                        st.repeats = due;
                        fired = true;
                    }
                }
            } else {
                st.held_for = 0.0;
                st.repeats = 0;
            }
            st.held = held;
            st.pressed = fired;
            st.buffered = if fired {
                Some(0.0)
            } else {
                st.buffered
                    .map(|age| age + dt)
                    .filter(|age| *age <= self.buffer)
            };
        }
    }

    fn state(&self, a: A) -> Option<&State> {
        self.states.iter().find(|(s, _)| *s == a).map(|(_, st)| st)
    }

    /// Whether `a` was pressed (or repeated) this frame.
    pub fn pressed(&self, a: A) -> bool {
        self.state(a).is_some_and(|s| s.pressed)
    }

    /// Whether `a` is held. Swipes and taps are never held.
    pub fn held(&self, a: A) -> bool {
        self.state(a).is_some_and(|s| s.held)
    }

    /// Seconds `a` has been held since it went down; 0 when it is not.
    pub fn held_for(&self, a: A) -> f32 {
        self.state(a).filter(|s| s.held).map_or(0.0, |s| s.held_for)
    }

    /// Spend a buffered press of `a`, if one is waiting: true at most once
    /// per press. Call when the game is ready to act on it.
    pub fn take(&mut self, a: A) -> bool {
        match self.states.iter_mut().find(|(s, _)| *s == a) {
            Some((_, st)) if st.buffered.is_some() => {
                st.buffered = None;
                true
            }
            _ => false,
        }
    }

    /// Whether a press of `a` is waiting, without spending it.
    pub fn buffered(&self, a: A) -> bool {
        self.state(a).is_some_and(|s| s.buffered.is_some())
    }

    /// −1, 0 or 1: `pos` held minus `neg` held. A paddle, a turn.
    pub fn axis(&self, neg: A, pos: A) -> f32 {
        f32::from(i8::from(self.held(pos)) - i8::from(self.held(neg)))
    }

    /// Forget every waiting press and swipe in progress (a scene change, a
    /// death), so nothing pressed before it acts after.
    pub fn clear(&mut self) {
        self.swipes.cancel();
        for (_, st) in &mut self.states {
            st.buffered = None;
            st.pressed = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Act {
        Hop(Dir4),
        Fire,
        Start,
    }

    fn actions() -> Actions<Act> {
        Actions::new(&[
            (Act::Hop(Dir4::Up), Binding::Key(KeyCode::Up)),
            (Act::Hop(Dir4::Up), Binding::Key(KeyCode::W)),
            (Act::Hop(Dir4::Up), Binding::Swipe(Dir4::Up)),
            (Act::Hop(Dir4::Up), Binding::PadDir(Dir4::Up)),
            (Act::Fire, Binding::Key(KeyCode::Space)),
            (Act::Start, Binding::Tap),
        ])
    }

    fn keys(down: &[KeyCode], pressed: &[KeyCode]) -> InputSnapshot {
        InputSnapshot {
            keys_down: down.to_vec(),
            keys_pressed: pressed.to_vec(),
            ..Default::default()
        }
    }

    fn press(k: KeyCode) -> InputSnapshot {
        keys(&[k], &[k])
    }

    fn hold(k: KeyCode) -> InputSnapshot {
        keys(&[k], &[])
    }

    fn none() -> InputSnapshot {
        InputSnapshot::default()
    }

    #[test]
    fn a_key_presses_once_and_holds_while_down_through_any_binding() {
        let mut a = actions();
        a.update(&press(KeyCode::W), DT);
        assert!(a.pressed(Act::Hop(Dir4::Up)) && a.held(Act::Hop(Dir4::Up)));
        a.update(&hold(KeyCode::W), DT);
        assert!(!a.pressed(Act::Hop(Dir4::Up)) && a.held(Act::Hop(Dir4::Up)));
        assert!((a.held_for(Act::Hop(Dir4::Up)) - DT).abs() < 1e-6);
        a.update(&none(), DT);
        assert!(!a.held(Act::Hop(Dir4::Up)));
        assert!(!a.pressed(Act::Fire), "other actions untouched");
    }

    #[test]
    fn a_press_during_a_cooldown_still_lands_when_taken() {
        let mut a = actions();
        a.update(&press(KeyCode::Up), DT);
        // Mid-hop: the game is not ready, so nothing takes it. 0.1 s passes.
        for _ in 0..6 {
            a.update(&none(), DT);
        }
        assert!(a.take(Act::Hop(Dir4::Up)), "within the buffer");
        assert!(!a.take(Act::Hop(Dir4::Up)), "spent: once per press");
    }

    #[test]
    fn a_press_not_taken_in_time_expires() {
        let mut a = actions().buffer(0.1);
        a.update(&press(KeyCode::Space), DT);
        for _ in 0..7 {
            a.update(&none(), DT);
        }
        assert!(!a.take(Act::Fire), "0.117 s is past a 0.1 s buffer");
    }

    #[test]
    fn repeat_fires_after_the_delay_then_every_interval() {
        let mut a = actions().repeat(Act::Fire, 0.25, 0.1);
        a.update(&press(KeyCode::Space), DT);
        let mut fired = Vec::new();
        for f in 1..=40 {
            a.update(&hold(KeyCode::Space), DT);
            if a.pressed(Act::Fire) {
                fired.push(f);
            }
        }
        // 0.25 s is frame 15, then every 6 frames.
        assert_eq!(fired, [15, 21, 27, 33, 39]);
    }

    #[test]
    fn a_swipe_and_a_tap_press_on_release_and_are_never_held() {
        let mut a = actions();
        let drag = |at: Vec2| InputSnapshot {
            gesture: Gesture {
                drag: Some(at),
                ..Default::default()
            },
            ..Default::default()
        };
        a.update(&drag(vec2(50.0, 100.0)), DT);
        a.update(&drag(vec2(52.0, 40.0)), DT);
        assert!(!a.pressed(Act::Hop(Dir4::Up)));
        a.update(&none(), DT);
        assert!(a.pressed(Act::Hop(Dir4::Up)) && !a.held(Act::Hop(Dir4::Up)));
        assert!(!a.pressed(Act::Start), "a swipe is not also a tap");

        let tap = InputSnapshot {
            gesture: Gesture {
                tap: Some(vec2(1.0, 1.0)),
                ..Default::default()
            },
            ..Default::default()
        };
        a.update(&tap, DT);
        assert!(a.pressed(Act::Start) && !a.held(Act::Start));
    }

    #[test]
    fn a_pad_direction_presses_when_it_starts_leaning() {
        let mut a = actions();
        let lean = |dir: Vec2| InputSnapshot {
            pad: PadState {
                dir,
                ..Default::default()
            },
            ..Default::default()
        };
        a.update(&lean(vec2(0.0, -0.3)), DT);
        assert!(!a.held(Act::Hop(Dir4::Up)), "under the threshold");
        a.update(&lean(vec2(0.1, -0.9)), DT);
        assert!(a.pressed(Act::Hop(Dir4::Up)));
        a.update(&lean(vec2(0.1, -0.9)), DT);
        assert!(!a.pressed(Act::Hop(Dir4::Up)) && a.held(Act::Hop(Dir4::Up)));
    }

    #[test]
    fn a_zone_holds_under_any_finger_so_one_steers_while_another_fires() {
        let left = Rect::new(0.0, 80.0, 20.0, 20.0);
        let fire = Rect::new(80.0, 80.0, 20.0, 20.0);
        let mut a = Actions::new(&[
            (Act::Hop(Dir4::Left), Binding::Zone(left)),
            (Act::Fire, Binding::Zone(fire)),
        ]);
        let fingers = |at: &[Vec2]| InputSnapshot {
            touches: at.to_vec(),
            ..Default::default()
        };
        a.update(&fingers(&[vec2(10.0, 90.0)]), DT);
        assert!(a.pressed(Act::Hop(Dir4::Left)) && !a.held(Act::Fire));
        a.update(&fingers(&[vec2(10.0, 90.0), vec2(90.0, 90.0)]), DT);
        assert!(a.held(Act::Hop(Dir4::Left)) && !a.pressed(Act::Hop(Dir4::Left)));
        assert!(a.pressed(Act::Fire));
        a.update(&fingers(&[vec2(90.0, 90.0)]), DT);
        assert!(!a.held(Act::Hop(Dir4::Left)) && a.held(Act::Fire));
        // The mouse held down counts too, through the gesture's drag.
        let mouse = InputSnapshot {
            gesture: Gesture {
                drag: Some(vec2(5.0, 85.0)),
                ..Default::default()
            },
            ..Default::default()
        };
        a.update(&mouse, DT);
        assert!(a.held(Act::Hop(Dir4::Left)));
    }

    #[test]
    fn axis_and_clear() {
        let mut a = Actions::new(&[
            (Act::Hop(Dir4::Left), Binding::Key(KeyCode::Left)),
            (Act::Hop(Dir4::Right), Binding::Key(KeyCode::Right)),
        ]);
        let (l, r) = (Act::Hop(Dir4::Left), Act::Hop(Dir4::Right));
        a.update(&press(KeyCode::Left), DT);
        assert_eq!(a.axis(l, r), -1.0);
        a.update(
            &keys(&[KeyCode::Left, KeyCode::Right], &[KeyCode::Right]),
            DT,
        );
        assert_eq!(a.axis(l, r), 0.0);
        a.clear();
        assert!(!a.take(l) && !a.take(r));
        assert!(!a.pressed(Act::Fire), "an unbound action is never pressed");
    }

    #[test]
    fn a_script_merges_into_the_snapshot() {
        let script = KeyScript::parse_all("2:Up,3-5:Space").expect("parses");
        let mut s = hold(KeyCode::Space);
        s.script(&script, 2);
        assert_eq!(s.keys_pressed, [KeyCode::Up]);
        assert_eq!(s.keys_down, [KeyCode::Space, KeyCode::Up]);
        let mut s = none();
        s.script(&script, 4);
        assert_eq!(s.keys_down, [KeyCode::Space]);
        assert!(s.keys_pressed.is_empty());
    }
}
