//! `Scroll` — one scrolling container's offset, as a phone's lists move: the
//! content follows a finger that drags it, coasts on after a flick and slows to
//! a stop, and a wheel or trackpad moves it too.
//!
//! The pixel-offset counterpart of [`crate::Momentum`], which it is built on.
//! A list that steps a row at a time uses [`crate::Momentum::rows`] directly;
//! anything that scrolls by the pixel (a wall of tiles, a long panel, the
//! storybook's sidebar) holds a `Scroll` and reads [`Scroll::offset`] when it
//! lays out.
//!
//! ```ignore
//! // every frame, before drawing:
//! let input = ScrollInput::read(frame.input.gesture.drag, frame.pointer());
//! self.scroll.update(area, &input, Some(content_h - area.h), frame.dt);
//! let tap = self.scroll.filter_tap(frame.input.gesture.tap);
//! ```
//!
//! ## What it decides
//!
//! - **Which drag is its own.** A drag that starts inside the container's
//!   area is held until the pointer lifts, wherever the pointer goes. One that
//!   starts outside is not this container's.
//! - **The ends.** The offset stays within `0..=max`. Reaching an end stops a
//!   coast rather than letting it push on against the wall.
//! - **A tap that catches a coast.** A finger landing on a list that is still
//!   moving stops it, and that tap is not a tap on whatever was under it:
//!   [`Scroll::filter_tap`] drops it, as a phone does.
//!
//! The wheel moves the offset at once and stops any coast. Natively a mouse
//! wheel reports notches and a trackpad reports points; the browser reports
//! points. [`wheel_points`] reads both as points.

use macroquad::prelude::{Rect, Vec2, mouse_wheel};

use crate::Momentum;

/// Points a wheel notch scrolls: about two lines of body text.
pub const NOTCH_POINTS: f32 = 40.0;

/// The axis a container scrolls along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
}

/// One frame's pointer, for every [`Scroll`] on the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollInput {
    /// Where the pointer is while it is down (`Gesture::drag`), else `None`.
    pub drag: Option<Vec2>,
    /// Where the pointer is, down or not: the wheel scrolls what is under it.
    pub pointer: Vec2,
    /// This frame's wheel, in points (see [`wheel_points`]); positive is
    /// towards the start (up, or left), as macroquad reports it.
    pub wheel: Vec2,
}

impl ScrollInput {
    /// This frame's input, reading the wheel from macroquad.
    pub fn read(drag: Option<Vec2>, pointer: Vec2) -> Self {
        let (x, y) = mouse_wheel();
        Self {
            drag,
            pointer,
            wheel: Vec2::new(wheel_points(x), wheel_points(y)),
        }
    }

    /// The same input with no pointer at all: for a container behind a modal,
    /// which should keep coasting but take nothing new.
    pub fn none() -> Self {
        Self::default()
    }
}

/// A wheel's report as points: a notch (about ±1, from a native mouse wheel)
/// is [`NOTCH_POINTS`]; anything larger is already points (a trackpad, or the
/// browser, which reports pixels).
pub fn wheel_points(raw: f32) -> f32 {
    if raw.abs() <= 1.5 {
        raw * NOTCH_POINTS
    } else {
        raw
    }
}

/// See the [module docs](self).
#[derive(Clone, Debug, Default)]
pub struct Scroll {
    axis: ScrollAxis,
    offset: f32,
    momentum: Momentum,
    /// A drag that started inside the area is being followed.
    grabbed: bool,
    /// The grab began on a coasting list: its tap only caught it.
    caught: bool,
    /// The caught grab lifted this frame: [`Scroll::filter_tap`] drops the
    /// tap that lift made.
    swallow: bool,
}

impl Scroll {
    /// A vertical scroll at the top.
    pub fn new() -> Self {
        Self::default()
    }

    /// A scroll along `axis`, at its start.
    pub fn along(axis: ScrollAxis) -> Self {
        Self {
            axis,
            ..Self::default()
        }
    }

    /// How far the content is scrolled from its start, in points: what the
    /// container subtracts when it lays out.
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Jump to `offset` (a reveal, a key, a restored position), stopping any
    /// coast. Clamped on the next [`Scroll::update`].
    pub fn set(&mut self, offset: f32) {
        self.offset = offset;
        self.momentum.stop();
    }

    /// Move by `delta` points, as a key or a button does, stopping any coast.
    pub fn nudge(&mut self, delta: f32) {
        self.set(self.offset + delta);
    }

    /// A finger is holding the content.
    pub fn held(&self) -> bool {
        self.grabbed
    }

    /// The content is moving on its own after a flick.
    pub fn coasting(&self) -> bool {
        self.momentum.coasting()
    }

    /// Advance a frame. `area` is the container on the stage; `max` is the
    /// furthest the content scrolls (its length less the area's), or `None`
    /// while that is not known yet — the offset is then left unclamped, so a
    /// position asked for before the content arrives is kept. Returns whether
    /// the offset moved.
    pub fn update(&mut self, area: Rect, input: &ScrollInput, max: Option<f32>, dt: f32) -> bool {
        let was = self.offset;
        self.swallow = false;
        let along = |v: Vec2| match self.axis {
            ScrollAxis::Vertical => v.y,
            ScrollAxis::Horizontal => v.x,
        };
        match input.drag {
            Some(at) if !self.grabbed && area.contains(at) => {
                self.grabbed = true;
                self.caught = self.momentum.coasting();
            }
            None if self.grabbed => {
                self.grabbed = false;
                self.swallow = std::mem::take(&mut self.caught);
            }
            _ => {}
        }
        // The wheel first: it stops a coast before the coast moves again.
        let wheel = match self.axis {
            ScrollAxis::Vertical => input.wheel.y,
            ScrollAxis::Horizontal if input.wheel.x != 0.0 => input.wheel.x,
            // A plain mouse wheel has only the one axis.
            ScrollAxis::Horizontal => input.wheel.y,
        };
        if wheel != 0.0 && !self.grabbed && area.contains(input.pointer) {
            self.momentum.stop();
            self.offset -= wheel;
        }

        let finger = input.drag.filter(|_| self.grabbed).map(along);
        // The content follows the finger: a finger moving up scrolls on.
        self.offset -= self.momentum.update(finger, dt);

        if let Some(max) = max {
            let clamped = self.offset.clamp(0.0, max.max(0.0));
            if clamped != self.offset {
                self.offset = clamped;
                // At an end: a coast stops here rather than pressing on.
                if !self.grabbed {
                    self.momentum.stop();
                }
            }
        }
        self.offset != was
    }

    /// `tap`, unless it was the lift of a finger that landed on a coasting
    /// list: that finger stopped the list, and pressing what was under it
    /// as well would be a surprise. Call after [`Scroll::update`], on the same
    /// frame.
    pub fn filter_tap(&self, tap: Option<Vec2>) -> Option<Vec2> {
        tap.filter(|_| !self.swallow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;
    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 300.0,
        h: 600.0,
    };
    const MAX: Option<f32> = Some(2000.0);

    fn at(y: f32) -> ScrollInput {
        ScrollInput {
            drag: Some(Vec2::new(150.0, y)),
            pointer: Vec2::new(150.0, y),
            wheel: Vec2::ZERO,
        }
    }

    fn up() -> ScrollInput {
        ScrollInput {
            pointer: Vec2::new(150.0, 300.0),
            ..ScrollInput::none()
        }
    }

    /// A flick upward from `from`, `step` points a frame, then the lift.
    fn flick(s: &mut Scroll, from: f32, step: f32, frames: usize) {
        for i in 0..=frames {
            s.update(AREA, &at(from + step * i as f32), MAX, DT);
        }
        s.update(AREA, &up(), MAX, DT);
    }

    fn settle(s: &mut Scroll) {
        for _ in 0..600 {
            s.update(AREA, &up(), MAX, DT);
        }
    }

    #[test]
    fn the_content_follows_a_drag_that_starts_inside() {
        let mut s = Scroll::new();
        s.update(AREA, &at(500.0), MAX, DT);
        s.update(AREA, &at(400.0), MAX, DT);
        assert_eq!(s.offset(), 100.0, "a finger moving up scrolls on");
        assert!(s.held());
        // Still held off the area's edge.
        s.update(AREA, &at(-50.0), MAX, DT);
        assert_eq!(s.offset(), 550.0);
    }

    #[test]
    fn a_drag_that_starts_outside_is_not_its_own() {
        let mut s = Scroll::new();
        let outside = |y| ScrollInput {
            drag: Some(Vec2::new(400.0, y)),
            ..at(y)
        };
        s.update(AREA, &outside(500.0), MAX, DT);
        s.update(AREA, &outside(100.0), MAX, DT);
        assert_eq!(s.offset(), 0.0);
        assert!(!s.held());
    }

    #[test]
    fn a_flick_coasts_on_and_stops() {
        let mut s = Scroll::new();
        flick(&mut s, 500.0, -12.0, 12);
        let released = s.offset();
        assert!(s.coasting());
        settle(&mut s);
        assert!(
            s.offset() > released + 100.0,
            "{} vs {released}",
            s.offset()
        );
        assert!(!s.coasting());
    }

    #[test]
    fn a_coast_stops_at_the_end() {
        let mut s = Scroll::new();
        s.set(1950.0);
        flick(&mut s, 500.0, -20.0, 10);
        settle(&mut s);
        assert_eq!(s.offset(), 2000.0);
        assert!(!s.coasting());
        // And at the start, the other way.
        flick(&mut s, 0.0, 60.0, 20);
        settle(&mut s);
        assert_eq!(s.offset(), 0.0);
    }

    #[test]
    fn the_tap_that_catches_a_coast_is_dropped() {
        let mut s = Scroll::new();
        flick(&mut s, 500.0, -12.0, 12);
        assert!(s.coasting());
        s.update(AREA, &at(300.0), MAX, DT);
        assert!(!s.coasting(), "the finger caught it");
        s.update(AREA, &up(), MAX, DT);
        let tap = Some(Vec2::new(150.0, 300.0));
        assert_eq!(s.filter_tap(tap), None);
        // The next tap, on a still list, is a tap.
        s.update(AREA, &at(300.0), MAX, DT);
        s.update(AREA, &up(), MAX, DT);
        assert_eq!(s.filter_tap(tap), tap);
    }

    #[test]
    fn the_wheel_moves_what_is_under_the_pointer_and_stops_a_coast() {
        let mut s = Scroll::new();
        flick(&mut s, 500.0, -12.0, 12);
        let was = s.offset();
        let wheel = ScrollInput {
            // A notch towards the end: macroquad reports it negative.
            wheel: Vec2::new(0.0, wheel_points(-1.0)),
            ..up()
        };
        s.update(AREA, &wheel, MAX, DT);
        assert_eq!(s.offset(), was + NOTCH_POINTS, "a notch down");
        assert!(!s.coasting());
        let elsewhere = ScrollInput {
            pointer: Vec2::new(900.0, 300.0),
            ..wheel
        };
        s.update(AREA, &elsewhere, MAX, DT);
        assert_eq!(s.offset(), was + NOTCH_POINTS, "not over the area");
    }

    #[test]
    fn notches_and_points_both_read_as_points() {
        assert_eq!(wheel_points(1.0), NOTCH_POINTS);
        assert_eq!(wheel_points(-120.0), -120.0);
        assert_eq!(wheel_points(0.0), 0.0);
    }

    #[test]
    fn an_offset_asked_for_before_the_content_is_known_is_kept() {
        let mut s = Scroll::new();
        s.set(900.0);
        s.update(AREA, &up(), None, DT);
        assert_eq!(s.offset(), 900.0);
        s.update(AREA, &up(), Some(400.0), DT);
        assert_eq!(s.offset(), 400.0);
    }

    #[test]
    fn a_horizontal_scroll_reads_the_other_axis() {
        let mut s = Scroll::along(ScrollAxis::Horizontal);
        let x = |x: f32| ScrollInput {
            drag: Some(Vec2::new(x, 300.0)),
            pointer: Vec2::new(x, 300.0),
            wheel: Vec2::ZERO,
        };
        s.update(AREA, &x(250.0), MAX, DT);
        s.update(AREA, &x(200.0), MAX, DT);
        assert_eq!(s.offset(), 50.0);
    }
}
