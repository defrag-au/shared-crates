//! `Thumbs` — touch controls for a phone game: the first finger down
//! anywhere is a stick, any other tap is an activation, and a few reserved
//! areas (a map button) take their own taps.
//!
//! The default control style for a mobile game:
//!
//! - **The first held touch is the stick.** Wherever it lands (outside a
//!   reserved area) becomes the stick's centre, and dragging from there
//!   steers. There is no drawn pad to find first, and nothing covers the
//!   game until a thumb is down.
//! - **Every other tap activates.** A second finger touching down anywhere
//!   is an activation (jump, fire, use), on the frame it lands. So is a
//!   quick tap of the first finger that never pushed, on release, so one
//!   thumb can play a whole game.
//! - **Reserved areas** answer only their own taps, and never start the
//!   stick.
//!
//! The stick feels like the widgets crate's `FloatingStick`: a dead zone, a
//! squared response for fine steering near the middle, travel on a disc
//! (`macroquad_widgets::stick_response`).
//!
//! Everything is in stage units. [`Thumbs::read`] reads the fingers through
//! `macroquad_widgets::touch_points` (points, like the mouse) and maps them
//! with the frame; [`Thumbs::update`] is the recogniser itself, pure, so it is
//! tested without a window. With no touch seen yet, the mouse plays the first
//! finger, so a desktop can try the controls.

use macroquad::prelude::*;
use macroquad_widgets::{stick_response, touch_points};

use crate::app::Frame;

/// The knob's radius as a fraction of the stick's, as the widgets' stick.
const KNOB_FRACTION: f32 = 0.34;
/// The mouse, when it plays a finger: an id no platform touch has.
const MOUSE_ID: u64 = u64::MAX;

/// One finger's report for a frame, in stage units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finger {
    pub id: u64,
    pub phase: TouchPhase,
    pub at: Vec2,
}

/// The stick as drawn this frame, in stage units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickShown {
    /// Where the thumb landed: the centre.
    pub anchor: Vec2,
    /// Where the knob sits: the thumb, kept within the stick's travel.
    pub knob: Vec2,
    pub radius: f32,
    pub knob_radius: f32,
}

/// One frame of thumbs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThumbFrame {
    /// What the stick drives, each axis -1 to 1, y down: zero in the dead
    /// zone and when no thumb is on it.
    pub push: Vec2,
    pub stick: Option<StickShown>,
    /// A tap that was not the stick and not a reserved area: act.
    pub activated: bool,
    /// The reserved area touched this frame, by index.
    pub reserved: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct Held {
    id: u64,
    anchor: Vec2,
    at: Vec2,
    /// Has it ever left the dead zone? A thumb that pushed is not a tap.
    pushed: bool,
}

/// The recogniser. Host-owned: read it once a frame.
#[derive(Clone, Debug)]
pub struct Thumbs {
    /// The stick's radius, in stage units.
    pub radius: f32,
    stick: Option<Held>,
    /// Fingers that landed on a reserved area or as a tap, so their moves
    /// and lifts are not mistaken for a new stick.
    others: Vec<u64>,
    seen: bool,
}

impl Thumbs {
    /// A stick of `radius` stage units: about two thirds of it is the thumb's
    /// travel. Size it so the travel is a comfortable thumb movement, around
    /// 30 to 40 points.
    pub fn new(radius: f32) -> Self {
        Self {
            radius,
            stick: None,
            others: Vec::new(),
            seen: false,
        }
    }

    /// Whether a touch has been seen: draw touch hints only once one has.
    pub fn touched(&self) -> bool {
        self.seen
    }

    /// Count as touched before any touch: for captures of the controls.
    pub fn show(&mut self) {
        self.seen = true;
    }

    /// Read this frame's fingers from the window, `reserved` areas in stage
    /// units.
    pub fn read(&mut self, frame: &Frame, reserved: &[Rect]) -> ThumbFrame {
        let mut fingers: Vec<Finger> = touch_points()
            .into_iter()
            .map(|t| Finger {
                id: t.id,
                phase: t.phase,
                at: frame.to_stage(t.position),
            })
            .collect();
        if fingers.is_empty() && !self.seen {
            // macroquad also reports each touch as the mouse; only before any
            // touch does the mouse play a finger, so a phone never counts one
            // twice.
            let at = frame.pointer();
            let phase = if is_mouse_button_pressed(MouseButton::Left) {
                Some(TouchPhase::Started)
            } else if is_mouse_button_released(MouseButton::Left) {
                Some(TouchPhase::Ended)
            } else if is_mouse_button_down(MouseButton::Left) {
                Some(TouchPhase::Moved)
            } else {
                None
            };
            if let Some(phase) = phase {
                fingers.push(Finger {
                    id: MOUSE_ID,
                    phase,
                    at,
                });
            }
            let out = self.update(&fingers, reserved);
            self.seen = false;
            return out;
        }
        self.update(&fingers, reserved)
    }

    /// The recognition: this frame's fingers, in stage units.
    ///
    /// A finger seen for the first time *is* a landing, whatever its phase.
    /// macroquad keeps one phase per finger per frame, the last event's: a
    /// finger that lands and moves before the next frame arrives as `Moved`,
    /// and one that lands and lifts arrives only as `Ended`. Waiting for
    /// `Started` dropped those touches, which on a phone (where a landing
    /// thumb nearly always moves a little) made touch-and-hold hit and miss.
    pub fn update(&mut self, fingers: &[Finger], reserved: &[Rect]) -> ThumbFrame {
        let mut out = ThumbFrame::default();
        for f in fingers {
            self.seen = true;
            let known = self.stick.is_some_and(|s| s.id == f.id) || self.others.contains(&f.id);
            if !known && f.phase != TouchPhase::Cancelled {
                self.land(f, reserved, &mut out);
            }
            match f.phase {
                TouchPhase::Started => {}
                TouchPhase::Moved | TouchPhase::Stationary => {
                    if let Some(s) = &mut self.stick
                        && s.id == f.id
                    {
                        s.at = f.at;
                    }
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    if let Some(s) = self.stick.filter(|s| s.id == f.id) {
                        self.stick = None;
                        let still = !s.pushed && self.deflection(s).length() <= DEAD_ZONE;
                        if still && f.phase == TouchPhase::Ended {
                            out.activated = true;
                        }
                    }
                    self.others.retain(|id| *id != f.id);
                }
            }
        }
        let travel = self.travel();
        if let Some(s) = &mut self.stick {
            let d = clamp_unit((s.at - s.anchor) / travel);
            if d.length() > DEAD_ZONE {
                s.pushed = true;
            }
            out.push = stick_response(d);
            out.stick = Some(StickShown {
                anchor: s.anchor,
                knob: s.anchor + d * travel,
                radius: self.radius,
                knob_radius: self.radius * KNOB_FRACTION,
            });
        }
        out
    }

    /// A finger landing at `f.at`: a reserved area's tap, the stick if it is
    /// free, or else an activation.
    fn land(&mut self, f: &Finger, reserved: &[Rect], out: &mut ThumbFrame) {
        if let Some(i) = reserved.iter().position(|r| r.contains(f.at)) {
            out.reserved = Some(i);
            self.others.push(f.id);
        } else if self.stick.is_none() {
            self.stick = Some(Held {
                id: f.id,
                anchor: f.at,
                at: f.at,
                pushed: false,
            });
        } else {
            out.activated = true;
            self.others.push(f.id);
        }
    }

    fn travel(&self) -> f32 {
        (self.radius * (1.0 - KNOB_FRACTION)).max(1.0)
    }

    fn deflection(&self, s: Held) -> Vec2 {
        clamp_unit((s.at - s.anchor) / self.travel())
    }
}

/// The widgets' stick's dead zone: a resting thumb does not creep.
const DEAD_ZONE: f32 = 0.12;

fn clamp_unit(v: Vec2) -> Vec2 {
    let length = v.length();
    if length <= 1.0 { v } else { v / length }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: u64, phase: TouchPhase, x: f32, y: f32) -> Finger {
        Finger {
            id,
            phase,
            at: vec2(x, y),
        }
    }

    const MAP: Rect = Rect {
        x: 300.0,
        y: 0.0,
        w: 40.0,
        h: 20.0,
    };

    #[test]
    fn the_first_finger_anywhere_is_the_stick_and_dragging_steers() {
        let mut t = Thumbs::new(50.0);
        let out = t.update(&[f(1, TouchPhase::Started, 200.0, 400.0)], &[MAP]);
        assert_eq!(out.push, Vec2::ZERO, "no push where it landed");
        assert!(out.stick.is_some() && !out.activated);
        let out = t.update(&[f(1, TouchPhase::Moved, 230.0, 400.0)], &[MAP]);
        assert!(out.push.x > 0.3 && out.push.y == 0.0, "{:?}", out.push);
        // Far past the travel: full push, the knob held at the rim.
        let out = t.update(&[f(1, TouchPhase::Moved, 400.0, 400.0)], &[MAP]);
        assert!((out.push.x - 1.0).abs() < 1e-4);
        let s = out.stick.expect("held");
        assert!((s.knob.x - s.anchor.x - 33.0).abs() < 0.1);
    }

    #[test]
    fn a_second_finger_anywhere_activates_and_never_steals_the_stick() {
        let mut t = Thumbs::new(50.0);
        t.update(&[f(1, TouchPhase::Started, 100.0, 400.0)], &[MAP]);
        t.update(&[f(1, TouchPhase::Moved, 120.0, 400.0)], &[MAP]);
        let out = t.update(&[f(2, TouchPhase::Started, 300.0, 600.0)], &[MAP]);
        assert!(out.activated);
        assert!(out.push.x > 0.0, "still steering");
        // The second finger moving and lifting does nothing more.
        let out = t.update(&[f(2, TouchPhase::Moved, 320.0, 610.0)], &[MAP]);
        assert!(!out.activated);
        let out = t.update(&[f(2, TouchPhase::Ended, 320.0, 610.0)], &[MAP]);
        assert!(!out.activated && out.stick.is_some());
    }

    #[test]
    fn a_quick_tap_with_one_finger_activates_on_release() {
        let mut t = Thumbs::new(50.0);
        t.update(&[f(1, TouchPhase::Started, 100.0, 400.0)], &[MAP]);
        let out = t.update(&[f(1, TouchPhase::Ended, 102.0, 401.0)], &[MAP]);
        assert!(out.activated);
        assert!(out.stick.is_none());
    }

    #[test]
    fn a_drag_that_comes_home_is_not_a_tap() {
        let mut t = Thumbs::new(50.0);
        t.update(&[f(1, TouchPhase::Started, 100.0, 400.0)], &[MAP]);
        t.update(&[f(1, TouchPhase::Moved, 140.0, 400.0)], &[MAP]);
        t.update(&[f(1, TouchPhase::Moved, 100.0, 400.0)], &[MAP]);
        let out = t.update(&[f(1, TouchPhase::Ended, 100.0, 400.0)], &[MAP]);
        assert!(!out.activated);
    }

    /// macroquad reports one phase per finger per frame: a landing that
    /// moved before the frame arrives as `Moved`, a quick tap as only
    /// `Ended`. Both still count.
    #[test]
    fn a_landing_seen_only_as_moved_or_ended_still_counts() {
        let mut t = Thumbs::new(50.0);
        let out = t.update(&[f(1, TouchPhase::Moved, 100.0, 400.0)], &[MAP]);
        assert!(out.stick.is_some(), "a first sight as Moved is the stick");
        let out = t.update(&[f(1, TouchPhase::Moved, 130.0, 400.0)], &[MAP]);
        assert!(out.push.x > 0.3, "and it steers");
        // A second finger that landed and lifted between frames: a tap.
        let out = t.update(&[f(2, TouchPhase::Ended, 300.0, 600.0)], &[MAP]);
        assert!(out.activated && out.stick.is_some());
        // So is a lone finger's, with the stick free.
        let mut t = Thumbs::new(50.0);
        let out = t.update(&[f(3, TouchPhase::Ended, 100.0, 400.0)], &[MAP]);
        assert!(out.activated && out.stick.is_none());
        // And one on a reserved area is that area's.
        let out = t.update(&[f(4, TouchPhase::Ended, 310.0, 10.0)], &[MAP]);
        assert_eq!(out.reserved, Some(0));
        assert!(!out.activated);
    }

    #[test]
    fn a_reserved_area_takes_its_own_tap_and_never_starts_the_stick() {
        let mut t = Thumbs::new(50.0);
        let out = t.update(&[f(1, TouchPhase::Started, 310.0, 10.0)], &[MAP]);
        assert_eq!(out.reserved, Some(0));
        assert!(out.stick.is_none() && !out.activated);
        // The stick is still free for the next finger.
        let out = t.update(&[f(2, TouchPhase::Started, 100.0, 400.0)], &[MAP]);
        assert!(out.stick.is_some());
        let out = t.update(&[f(1, TouchPhase::Ended, 310.0, 10.0)], &[MAP]);
        assert!(!out.activated);
    }
}
