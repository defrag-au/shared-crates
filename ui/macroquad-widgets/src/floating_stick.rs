//! `FloatingStick` — a thumbstick that appears where the thumb lands, for
//! touch surfaces with no keyboard.
//!
//! A fixed pad sits where the designer guessed the thumb would be. A floating
//! stick has no position until a finger lands inside the zone the host gives
//! it; that point becomes the centre, and the push is measured from there. The
//! thumb never has to find the control, and the control covers nothing until it
//! is in use.
//!
//! ## Feel
//!
//! Taken from egui-widgets' `TransformStick`, the livery editor's stick, so
//! the two renderers push the same way:
//!
//! - a **dead zone** around the centre, so a resting thumb does not creep and
//!   a release is clean;
//! - a **squared response**, so the middle of the travel is the precise part
//!   and the last few percent are not the most violent;
//! - the knob rides a **disc**, not a square, so a diagonal push is no longer
//!   than a straight one.
//!
//! The visuals are the same anatomy too: a well, the dead-zone ring, a faint
//! cross so "up" means something before the push, and an accent knob.
//!
//! ## Taps still work
//!
//! A touch that never leaves the dead zone is a tap, reported on release as
//! [`Release::Tap`] at the point it landed. A host can give the stick the
//! whole play area and still take taps there (tap to move, tap a monster)
//! without the two fighting: a push is a push, a tap is a tap, and one touch is
//! never both.
//!
//! ## State is the host's
//!
//! Like [`crate::Gestures`], the recogniser needs memory across frames (which
//! finger, where it landed), so the host owns a [`FloatingStick`] and calls
//! [`FloatingStick::update`] once a frame. Drawing is the stateless half:
//! [`floating_stick`] takes the [`StickView`] the update returned.

use macroquad::prelude::*;

use crate::painter::Painter;

/// Below this fraction of the travel nothing happens: a released stick must
/// not drift, and a resting thumb must not creep. Matches `TransformStick`.
pub const DEAD_ZONE: f32 = 0.12;

/// A push just past the dead zone is this fraction of the full rate, so the
/// first movement is gentle rather than nothing-then-sudden. Matches
/// `TransformStick`. `theme-exempt`: a feel parameter, not a size.
const FINE_FLOOR: f32 = 0.10;

/// The knob's radius as a fraction of the stick's. `theme-exempt`: a pointer
/// affordance, scaled with the stick the host sizes.
const KNOB_FRACTION: f32 = 0.34;

/// The mouse is one pointer; this id cannot collide with a platform touch id.
/// Same convention as [`crate::Gestures`].
const MOUSE_ID: u64 = u64::MAX;

/// Where a push lands, in `−1..=1` on both axes, with the dead zone removed
/// and the travel curved for a fine hand. `push` is the raw deflection in the
/// unit disc.
pub fn response(push: Vec2) -> Vec2 {
    let length = push.length();
    if length <= DEAD_ZONE {
        return Vec2::ZERO;
    }
    let travel = ((length - DEAD_ZONE) / (1.0 - DEAD_ZONE)).min(1.0);
    let curved = FINE_FLOOR + (1.0 - FINE_FLOOR) * travel * travel;
    push / length * curved
}

/// One of four directions, for a host on a grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cardinal {
    Up,
    Down,
    Left,
    Right,
}

/// How a touch ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Release {
    /// It never left the dead zone: a tap, where it landed.
    Tap(Vec2),
    /// It pushed; the push has already been reported frame by frame.
    Pushed,
}

/// What the stick looks like this frame, for [`floating_stick`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickView {
    /// Where the thumb landed: the centre of the well.
    pub anchor: Vec2,
    /// The raw deflection, in the unit disc (before the dead zone and curve).
    pub deflection: Vec2,
    /// The well's radius, in pixels.
    pub radius: f32,
}

impl StickView {
    /// Where the knob is drawn.
    pub fn knob(&self) -> Vec2 {
        self.anchor + self.deflection * self.travel()
    }

    /// How far the knob's centre can move from the anchor.
    pub fn travel(&self) -> f32 {
        self.radius * (1.0 - KNOB_FRACTION)
    }
}

/// One frame of the stick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StickFrame {
    /// Present while a thumb is down on the stick.
    pub view: Option<StickView>,
    /// What to drive this frame: [`response`] of the deflection. Zero inside
    /// the dead zone and the instant the thumb lifts.
    pub push: Vec2,
    /// Set on the frame the thumb lifts.
    pub released: Option<Release>,
}

impl StickFrame {
    /// The dominant direction of the push, or `None` inside the dead zone.
    /// Screen axes: down is positive y.
    pub fn cardinal(&self) -> Option<Cardinal> {
        let p = self.push;
        if p == Vec2::ZERO {
            return None;
        }
        Some(if p.x.abs() >= p.y.abs() {
            if p.x > 0.0 {
                Cardinal::Right
            } else {
                Cardinal::Left
            }
        } else if p.y > 0.0 {
            Cardinal::Down
        } else {
            Cardinal::Up
        })
    }
}

/// One thumb on the stick.
#[derive(Clone, Copy, Debug)]
struct Active {
    id: u64,
    anchor: Vec2,
    at: Vec2,
    /// Has this touch ever left the dead zone? Decides tap or push on release,
    /// and is kept even if the thumb drifts back to the centre: a touch that
    /// pushed and came home was a push, not a tap.
    pushed: bool,
}

/// The host-owned recogniser. Call [`Self::update`] once per frame, before
/// drawing.
#[derive(Debug)]
pub struct FloatingStick {
    radius: f32,
    active: Option<Active>,
}

impl FloatingStick {
    /// A stick whose well is `radius` pixels. Size it from the shorter screen
    /// edge so it stays thumb-sized in both orientations.
    pub fn new(radius: f32) -> Self {
        Self {
            radius,
            active: None,
        }
    }

    pub fn set_radius(&mut self, radius: f32) {
        self.radius = radius;
    }

    /// Is a thumb down on the stick?
    pub fn engaged(&self) -> bool {
        self.active.is_some()
    }

    /// Read this frame's pointers. A touch (or mouse press) that *starts*
    /// inside `zone` takes the stick; once taken it is followed anywhere on
    /// screen until it lifts.
    pub fn update(&mut self, zone: Rect) -> StickFrame {
        self.update_from(
            zone,
            &crate::gesture::touch_points(),
            is_mouse_button_pressed(MouseButton::Left),
            is_mouse_button_down(MouseButton::Left),
            Vec2::from(mouse_position()),
        )
    }

    /// The recognition, with inputs passed in so it can be tested without a
    /// window.
    fn update_from(
        &mut self,
        zone: Rect,
        touches: &[Touch],
        mouse_pressed: bool,
        mouse_down: bool,
        mouse_at: Vec2,
    ) -> StickFrame {
        let mut released = None;

        // ── touch ───────────────────────────────────────────────────────
        for t in touches {
            match t.phase {
                // One thumb. A second finger is a different control's (a
                // button, a card), not a second stick.
                TouchPhase::Started if self.active.is_none() && zone.contains(t.position) => {
                    self.active = Some(Active {
                        id: t.id,
                        anchor: t.position,
                        at: t.position,
                        pushed: false,
                    });
                }
                // macroquad keeps one phase per touch per frame, the last
                // event's: a thumb that landed and moved before this frame
                // arrives as `Moved`, never `Started`. A free stick takes it
                // anyway, from where it is now.
                TouchPhase::Moved | TouchPhase::Stationary
                    if self.active.is_none() && zone.contains(t.position) =>
                {
                    self.active = Some(Active {
                        id: t.id,
                        anchor: t.position,
                        at: t.position,
                        pushed: false,
                    });
                }
                TouchPhase::Moved | TouchPhase::Stationary => {
                    if let Some(a) = &mut self.active
                        && a.id == t.id
                    {
                        a.at = t.position;
                    }
                }
                // Landed and lifted between frames: a tap, if the stick was
                // free to take it.
                TouchPhase::Ended if self.active.is_none() && zone.contains(t.position) => {
                    released = Some(Release::Tap(t.position));
                }
                TouchPhase::Ended => {
                    if let Some(a) = self.active.filter(|a| a.id == t.id) {
                        self.active = None;
                        released = Some(release(a, self.radius));
                    }
                }
                TouchPhase::Cancelled => {
                    // The platform took the touch: neither a tap nor a push.
                    if self.active.is_some_and(|a| a.id == t.id) {
                        self.active = None;
                        released = Some(Release::Pushed);
                    }
                }
                TouchPhase::Started => {}
            }
        }

        // ── mouse ───────────────────────────────────────────────────────
        // The same stick under a mouse, so a desktop build and the storybook
        // can feel it. A touch that macroquad also reports as a mouse press is
        // ignored here because the touch already holds the stick.
        if mouse_pressed && self.active.is_none() && zone.contains(mouse_at) {
            self.active = Some(Active {
                id: MOUSE_ID,
                anchor: mouse_at,
                at: mouse_at,
                pushed: false,
            });
        } else if let Some(a) = &mut self.active
            && a.id == MOUSE_ID
        {
            if mouse_down {
                a.at = mouse_at;
            } else {
                let a = *a;
                self.active = None;
                released = Some(release(a, self.radius));
            }
        }

        let view = self.active.as_mut().map(|a| {
            let deflection = deflection(a.anchor, a.at, self.radius);
            if deflection.length() > DEAD_ZONE {
                a.pushed = true;
            }
            StickView {
                anchor: a.anchor,
                deflection,
                radius: self.radius,
            }
        });
        StickFrame {
            push: view.map_or(Vec2::ZERO, |v| response(v.deflection)),
            view,
            released,
        }
    }
}

/// The thumb's offset from the anchor, as a deflection in the unit disc.
fn deflection(anchor: Vec2, at: Vec2, radius: f32) -> Vec2 {
    let travel = (radius * (1.0 - KNOB_FRACTION)).max(1.0);
    clamp_unit((at - anchor) / travel)
}

/// How a touch that is lifting ended.
fn release(a: Active, radius: f32) -> Release {
    if a.pushed || deflection(a.anchor, a.at, radius).length() > DEAD_ZONE {
        Release::Pushed
    } else {
        Release::Tap(a.anchor)
    }
}

/// `v` clamped into the unit disc.
fn clamp_unit(v: Vec2) -> Vec2 {
    let length = v.length();
    if length <= 1.0 { v } else { v / length }
}

/// Draw the stick: a well, the dead-zone ring, a faint cross, and the knob.
/// Draw it last, over whatever the thumb is pushing.
pub fn floating_stick(p: &Painter, view: &StickView) {
    let c = p.theme.color;
    let line = (view.radius * 0.02).max(1.0);
    let travel = view.travel();
    let anchor = view.anchor;

    // The well is translucent: it sits over the game, which should still read
    // through it.
    draw_circle(
        anchor.x,
        anchor.y,
        view.radius,
        Color {
            a: 0.55,
            ..c.bg_secondary
        },
    );
    draw_circle_lines(anchor.x, anchor.y, view.radius, line, c.border);
    draw_circle_lines(anchor.x, anchor.y, travel * DEAD_ZONE, line, c.border);

    let guide = Color {
        a: 0.3,
        ..c.text_muted
    };
    draw_line(
        anchor.x - travel,
        anchor.y,
        anchor.x + travel,
        anchor.y,
        line,
        guide,
    );
    draw_line(
        anchor.x,
        anchor.y - travel,
        anchor.x,
        anchor.y + travel,
        line,
        guide,
    );

    let knob = view.knob();
    let knob_r = view.radius * KNOB_FRACTION;
    draw_circle(knob.x, knob.y, knob_r, c.accent);
    draw_circle_lines(knob.x, knob.y, knob_r, line * 1.5, c.bg_primary);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONE: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 400.0,
        h: 800.0,
    };
    const RADIUS: f32 = 100.0;

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> Touch {
        Touch {
            id,
            phase,
            position: vec2(x, y),
        }
    }

    fn frame(stick: &mut FloatingStick, touches: &[Touch]) -> StickFrame {
        stick.update_from(ZONE, touches, false, false, Vec2::ZERO)
    }

    #[test]
    fn a_released_stick_does_nothing() {
        assert_eq!(response(Vec2::ZERO), Vec2::ZERO);
        assert_eq!(response(vec2(0.05, -0.05)), Vec2::ZERO);
        let full = response(vec2(1.0, 0.0)).length();
        assert!((full - 1.0).abs() < 1e-4, "full push gave {full}");
    }

    #[test]
    fn it_appears_where_the_thumb_lands() {
        let mut s = FloatingStick::new(RADIUS);
        let f = frame(&mut s, &[touch(7, TouchPhase::Started, 120.0, 600.0)]);
        let v = f.view.expect("engaged");
        assert_eq!(v.anchor, vec2(120.0, 600.0));
        assert_eq!(f.push, Vec2::ZERO);
    }

    #[test]
    fn a_touch_outside_the_zone_is_not_the_sticks() {
        let mut s = FloatingStick::new(RADIUS);
        let f = frame(&mut s, &[touch(7, TouchPhase::Started, 500.0, 100.0)]);
        assert!(f.view.is_none() && !s.engaged());
    }

    #[test]
    fn a_push_is_measured_from_the_anchor_and_points_its_way() {
        let mut s = FloatingStick::new(RADIUS);
        frame(&mut s, &[touch(7, TouchPhase::Started, 200.0, 400.0)]);
        let f = frame(&mut s, &[touch(7, TouchPhase::Moved, 200.0, 340.0)]);
        assert_eq!(f.cardinal(), Some(Cardinal::Up));
        let f = frame(&mut s, &[touch(7, TouchPhase::Moved, 260.0, 410.0)]);
        assert_eq!(f.cardinal(), Some(Cardinal::Right));
    }

    #[test]
    fn the_knob_rides_a_disc() {
        let mut s = FloatingStick::new(RADIUS);
        frame(&mut s, &[touch(7, TouchPhase::Started, 200.0, 400.0)]);
        let f = frame(&mut s, &[touch(7, TouchPhase::Moved, 600.0, 800.0)]);
        let v = f.view.unwrap();
        assert!((v.deflection.length() - 1.0).abs() < 1e-5);
        assert!((f.push.length() - 1.0).abs() < 1e-4);
        assert!(v.knob().distance(v.anchor) <= v.travel() + 1e-3);
    }

    #[test]
    fn a_touch_that_never_pushes_is_a_tap_where_it_landed() {
        let mut s = FloatingStick::new(RADIUS);
        frame(&mut s, &[touch(7, TouchPhase::Started, 200.0, 400.0)]);
        frame(&mut s, &[touch(7, TouchPhase::Moved, 202.0, 401.0)]);
        let f = frame(&mut s, &[touch(7, TouchPhase::Ended, 202.0, 401.0)]);
        assert_eq!(f.released, Some(Release::Tap(vec2(200.0, 400.0))));
        assert!(f.view.is_none() && f.push == Vec2::ZERO);
    }

    #[test]
    fn a_push_that_comes_home_is_still_a_push() {
        let mut s = FloatingStick::new(RADIUS);
        frame(&mut s, &[touch(7, TouchPhase::Started, 200.0, 400.0)]);
        frame(&mut s, &[touch(7, TouchPhase::Moved, 260.0, 400.0)]);
        frame(&mut s, &[touch(7, TouchPhase::Moved, 200.0, 400.0)]);
        let f = frame(&mut s, &[touch(7, TouchPhase::Ended, 200.0, 400.0)]);
        assert_eq!(f.released, Some(Release::Pushed));
    }

    #[test]
    fn a_second_finger_does_not_steal_the_stick() {
        let mut s = FloatingStick::new(RADIUS);
        frame(&mut s, &[touch(7, TouchPhase::Started, 200.0, 400.0)]);
        let f = frame(
            &mut s,
            &[
                touch(7, TouchPhase::Moved, 200.0, 460.0),
                touch(8, TouchPhase::Started, 50.0, 50.0),
            ],
        );
        assert_eq!(f.view.unwrap().anchor, vec2(200.0, 400.0));
        assert_eq!(f.cardinal(), Some(Cardinal::Down));
        let f = frame(&mut s, &[touch(8, TouchPhase::Ended, 50.0, 50.0)]);
        assert!(f.released.is_none() && s.engaged());
    }

    #[test]
    fn the_mouse_drives_the_same_stick() {
        let mut s = FloatingStick::new(RADIUS);
        s.update_from(ZONE, &[], true, true, vec2(100.0, 100.0));
        let f = s.update_from(ZONE, &[], false, true, vec2(30.0, 100.0));
        assert_eq!(f.cardinal(), Some(Cardinal::Left));
        let f = s.update_from(ZONE, &[], false, false, vec2(30.0, 100.0));
        assert_eq!(f.released, Some(Release::Pushed));
        assert!(!s.engaged());
    }

    #[test]
    fn a_touch_echoed_as_a_mouse_press_is_one_pointer() {
        // macroquad reports a touch as a mouse press too; the touch already
        // holds the stick, so the echo must not re-anchor it.
        let mut s = FloatingStick::new(RADIUS);
        let f = s.update_from(
            ZONE,
            &[touch(7, TouchPhase::Started, 200.0, 400.0)],
            true,
            true,
            vec2(200.0, 400.0),
        );
        assert_eq!(f.view.unwrap().anchor, vec2(200.0, 400.0));
        let f = s.update_from(
            ZONE,
            &[touch(7, TouchPhase::Moved, 200.0, 340.0)],
            false,
            true,
            vec2(200.0, 340.0),
        );
        assert_eq!(f.cardinal(), Some(Cardinal::Up));
    }
}
