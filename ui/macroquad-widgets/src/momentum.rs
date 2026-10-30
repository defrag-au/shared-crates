//! `Momentum` — inertial scrolling for a list a finger drags: the content follows the finger while it is down, and after a flick it keeps going and slows to a stop, as a phone's own lists do
//!
//! Host-owned state, like [`crate::Gestures`]: the host decides which list a
//! drag belongs to and feeds that list's `Momentum` the finger's position on
//! the list's axis each frame — `Some` while held, `None` once it lifts.
//! [`Momentum::update`] answers how far the content moved this frame, in the
//! finger's units and direction; [`Momentum::rows`] answers in whole rows, for
//! a list that scrolls a row at a time.
//!
//! The release speed is measured over the last [`SAMPLE_SECS`] of the drag,
//! so a finger that stops before it lifts does not fling. The coast decays
//! exponentially ([`COAST_SECS`]): a flick at `v` travels about
//! `v × COAST_SECS`.
//!
//! It knows nothing of the content's ends. A host whose content clamps calls
//! [`Momentum::stop`] when it reaches one, so the coast does not push on
//! against an end it cannot pass.

use std::collections::VecDeque;

/// How much of the drag the release speed is measured over, in seconds.
pub const SAMPLE_SECS: f32 = 0.1;

/// The coast's time constant, in seconds: its speed falls by `1/e` in this
/// long. A phone's lists sit near half a second.
pub const COAST_SECS: f32 = 0.45;

/// The fastest a fling starts, in units a second: a twitch at a frame's edge
/// can measure absurdly fast.
pub const MAX_SPEED: f32 = 6000.0;

/// Below this speed, in units a second, the coast has stopped.
const REST_SPEED: f32 = 30.0;

/// One list's scroll, between frames.
#[derive(Clone, Debug, Default)]
pub struct Momentum {
    /// The finger's position last frame, while it is down.
    held: Option<f32>,
    /// Recent finger positions, `(clock, position)`, oldest first.
    samples: VecDeque<(f32, f32)>,
    /// The coast's speed, in units a second, in the finger's direction.
    velocity: f32,
    clock: f32,
    /// What [`Momentum::rows`] has moved and not yet counted as a row.
    carry: f32,
}

impl Momentum {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance a frame: `finger` is the finger's position on the list's axis
    /// while it holds the list, `None` otherwise. Returns how far the content
    /// moved, in the finger's units: positive where the finger's positive is.
    ///
    /// A finger landing on a coasting list catches it.
    pub fn update(&mut self, finger: Option<f32>, dt: f32) -> f32 {
        self.clock += dt;
        match (finger, self.held) {
            (Some(at), None) => {
                self.velocity = 0.0;
                self.samples.clear();
                self.samples.push_back((self.clock, at));
                self.held = Some(at);
                0.0
            }
            (Some(at), Some(was)) => {
                self.samples.push_back((self.clock, at));
                while self
                    .samples
                    .front()
                    .is_some_and(|(t, _)| self.clock - t > SAMPLE_SECS)
                    && self.samples.len() > 2
                {
                    self.samples.pop_front();
                }
                self.held = Some(at);
                at - was
            }
            (None, Some(_)) => {
                self.held = None;
                self.velocity = self.release_speed();
                self.samples.clear();
                0.0
            }
            (None, None) => self.coast(dt),
        }
    }

    /// [`Momentum::update`] in whole rows of `row` units, the remainder kept
    /// for the next frame: a list that scrolls a row at a time.
    pub fn rows(&mut self, finger: Option<f32>, dt: f32, row: f32) -> i64 {
        self.carry += self.update(finger, dt);
        let rows = (self.carry / row.max(f32::EPSILON)).round();
        self.carry -= rows * row;
        rows as i64
    }

    /// Stop the coast where it is: the content reached an end, or the host
    /// moved it some other way (a wheel, a key, a jump).
    pub fn stop(&mut self) {
        self.velocity = 0.0;
        self.carry = 0.0;
    }

    /// A finger is on the list.
    pub fn held(&self) -> bool {
        self.held.is_some()
    }

    /// The list is still moving on its own after a flick.
    pub fn coasting(&self) -> bool {
        self.held.is_none() && self.velocity != 0.0
    }

    /// The finger's speed over the last [`SAMPLE_SECS`] it was down.
    fn release_speed(&self) -> f32 {
        let (Some(&(t0, p0)), Some(&(t1, p1))) = (self.samples.front(), self.samples.back()) else {
            return 0.0;
        };
        // Held still since the last movement: no fling.
        if self.clock - t1 > SAMPLE_SECS || t1 - t0 <= f32::EPSILON {
            return 0.0;
        }
        ((p1 - p0) / (t1 - t0)).clamp(-MAX_SPEED, MAX_SPEED)
    }

    fn coast(&mut self, dt: f32) -> f32 {
        if self.velocity == 0.0 {
            return 0.0;
        }
        // Exact over the frame, so a long frame coasts as far as short ones.
        let decay = (-dt / COAST_SECS).exp();
        let moved = self.velocity * COAST_SECS * (1.0 - decay);
        self.velocity *= decay;
        if self.velocity.abs() < REST_SPEED {
            self.velocity = 0.0;
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// Drag from `from` by `step` a frame for `frames`, returning the
    /// distance the content followed.
    fn drag(m: &mut Momentum, from: f32, step: f32, frames: usize) -> f32 {
        let mut moved = m.update(Some(from), DT);
        for i in 1..=frames {
            moved += m.update(Some(from + step * i as f32), DT);
        }
        moved
    }

    /// Let it coast until it stops, returning how far it went.
    fn coast(m: &mut Momentum) -> f32 {
        let mut moved = m.update(None, DT);
        for _ in 0..600 {
            moved += m.update(None, DT);
        }
        moved
    }

    #[test]
    fn the_content_follows_the_finger_while_it_is_down() {
        let mut m = Momentum::new();
        assert_eq!(drag(&mut m, 100.0, -5.0, 10), -50.0);
        assert!(m.held());
    }

    #[test]
    fn a_flick_coasts_on_the_same_way_and_stops() {
        let mut m = Momentum::new();
        // 10 units a frame: 600 a second, upward.
        drag(&mut m, 500.0, -10.0, 12);
        let went = coast(&mut m);
        assert!(went < 0.0, "the way the finger went");
        // About the speed times the time constant.
        let expect = -600.0 * COAST_SECS;
        assert!(
            (went - expect).abs() < expect.abs() * 0.15,
            "{went} vs {expect}"
        );
        assert!(!m.coasting(), "and it stops");
    }

    #[test]
    fn a_finger_that_stops_before_it_lifts_does_not_fling() {
        let mut m = Momentum::new();
        drag(&mut m, 500.0, -10.0, 12);
        for _ in 0..12 {
            m.update(Some(380.0), DT);
        }
        assert_eq!(coast(&mut m), 0.0);
    }

    #[test]
    fn a_finger_landing_catches_a_coasting_list() {
        let mut m = Momentum::new();
        drag(&mut m, 500.0, -20.0, 10);
        m.update(None, DT);
        assert!(m.coasting());
        assert_eq!(m.update(Some(200.0), DT), 0.0);
        assert!(!m.coasting());
        m.update(None, DT);
        assert_eq!(coast(&mut m), 0.0, "and lifting again does not restart it");
    }

    #[test]
    fn stop_ends_the_coast() {
        let mut m = Momentum::new();
        drag(&mut m, 500.0, -20.0, 10);
        m.update(None, DT);
        m.stop();
        assert_eq!(coast(&mut m), 0.0);
    }

    #[test]
    fn rows_count_whole_rows_and_keep_the_rest() {
        let mut m = Momentum::new();
        m.rows(Some(0.0), DT, 36.0);
        // 20 units: past half a row, so one.
        assert_eq!(m.rows(Some(20.0), DT, 36.0), 1);
        // 16 more: 36 in all, still one row.
        assert_eq!(m.rows(Some(36.0), DT, 36.0), 0);
        assert_eq!(m.rows(Some(-36.0), DT, 36.0), -2);
    }

    #[test]
    fn a_long_frame_coasts_as_far_as_short_ones() {
        let fling = || {
            let mut m = Momentum::new();
            drag(&mut m, 0.0, 10.0, 12);
            m.update(None, DT);
            m
        };
        let mut short = fling();
        let mut long = fling();
        let a: f32 = (0..30).map(|_| short.update(None, DT)).sum();
        let b = long.update(None, DT * 30.0);
        assert!((a - b).abs() < 1.0, "{a} vs {b}");
    }
}
