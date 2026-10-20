//! `FixedStep` — a simulation that steps at a fixed rate, whatever the frame
//! rate. A ball at 900 units a second tunnels through a brick on a 0.1 s
//! frame and not on a 1/120 s tick; a replay needs the same ticks every run.
//!
//! The loop drives one for a [`Game`](crate::Game) whose
//! [`tick_rate`](crate::Game::tick_rate) is `Some`, calling
//! [`Game::tick`](crate::Game::tick) as many times as are due each frame and
//! setting [`Frame::alpha`](crate::Frame::alpha) for drawing between the last
//! two. A game can own one too, for a second, slower clock (Tetris' drop).
//!
//! Exact on the capture clock: 60 Hz on [`FIXED_DT`](crate::harness::FIXED_DT)
//! is one tick every frame, never zero then two. The accumulator is `f64`, and
//! a tick within a millionth of an interval of its time counts as due, so the
//! rounding in an `f32` frame time never drops or delays one.

/// A tick this close to due, as a fraction of its interval, is due.
const SLACK: f64 = 1e-6;

/// One tick, as handed to [`Game::tick`](crate::Game::tick).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tick {
    /// Ticks before this one, since the loop started.
    pub n: u64,
    /// Seconds a tick lasts: `1 / tick_rate`.
    pub dt: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FixedStep {
    interval: f64,
    acc: f64,
    count: u64,
}

impl FixedStep {
    /// A step that fires `hz` times a second.
    pub fn new(hz: f32) -> Self {
        Self {
            interval: interval(hz),
            acc: 0.0,
            count: 0,
        }
    }

    /// Advance by `dt` seconds and return how many ticks are due: usually 0
    /// or 1, more after a long frame.
    pub fn ticks(&mut self, dt: f32) -> u32 {
        self.acc += f64::from(dt.max(0.0));
        let mut n = 0;
        while self.acc >= self.interval * (1.0 - SLACK) {
            self.acc = (self.acc - self.interval).max(0.0);
            n += 1;
        }
        self.count += u64::from(n);
        n
    }

    /// The due ticks as [`Tick`]s: what [`ticks`](Self::ticks) counted,
    /// numbered.
    pub fn advance(&mut self, dt: f32) -> impl Iterator<Item = Tick> + use<> {
        let first = self.count;
        let n = self.ticks(dt);
        let dt = self.interval as f32;
        (first..first + u64::from(n)).map(move |n| Tick { n, dt })
    }

    /// How far into the next tick the clock is, 0 to 1: draw at
    /// `prev.lerp(now, alpha)` for motion smoother than the tick rate.
    pub fn alpha(&self) -> f32 {
        (self.acc / self.interval).clamp(0.0, 1.0) as f32
    }

    /// Ticks fired since this step was made.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Seconds a tick lasts.
    pub fn dt(&self) -> f32 {
        self.interval as f32
    }

    /// Drop the time banked toward the next tick (a scene resuming after a
    /// pause). The count is kept.
    pub fn reset(&mut self) {
        self.acc = 0.0;
    }

    /// Change the rate (Tetris dropping faster each level). The time banked
    /// toward the next tick is kept.
    pub fn set_hz(&mut self, hz: f32) {
        self.interval = interval(hz);
    }
}

fn interval(hz: f32) -> f64 {
    1.0 / f64::from(hz.max(0.001))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::FIXED_DT;

    #[test]
    fn on_the_capture_clock_sixty_hertz_is_exactly_one_tick_a_frame() {
        let mut s = FixedStep::new(60.0);
        for frame in 0..100_000 {
            assert_eq!(s.ticks(FIXED_DT), 1, "frame {frame}");
        }
        let mut s = FixedStep::new(120.0);
        for frame in 0..100_000 {
            assert_eq!(s.ticks(FIXED_DT), 2, "frame {frame}");
        }
    }

    #[test]
    fn a_slower_rate_alternates_and_alpha_says_how_far_along() {
        let mut s = FixedStep::new(30.0);
        let ticks: Vec<u32> = (0..6).map(|_| s.ticks(FIXED_DT)).collect();
        assert_eq!(ticks, [0, 1, 0, 1, 0, 1]);
        s.ticks(FIXED_DT);
        assert!((s.alpha() - 0.5).abs() < 1e-4, "{}", s.alpha());
    }

    #[test]
    fn a_long_frame_catches_up_and_ticks_are_numbered() {
        let mut s = FixedStep::new(10.0);
        assert_eq!(s.ticks(0.05), 0);
        let ticks: Vec<Tick> = s.advance(0.35).collect();
        assert_eq!(ticks.len(), 4);
        assert_eq!(ticks[0], Tick { n: 0, dt: 0.1 });
        assert_eq!(ticks[3].n, 3);
        assert_eq!(s.count(), 4);
    }

    #[test]
    fn reset_drops_banked_time_and_set_hz_keeps_it() {
        let mut s = FixedStep::new(10.0);
        s.ticks(0.07);
        s.reset();
        assert_eq!(s.ticks(0.05), 0);
        s.set_hz(20.0);
        assert_eq!(s.ticks(0.0), 1, "0.05 s banked is one 20 Hz tick");
    }

    #[test]
    fn a_negative_dt_never_runs_the_clock_backwards() {
        let mut s = FixedStep::new(10.0);
        assert_eq!(s.ticks(-5.0), 0);
        assert_eq!(s.ticks(0.1), 1);
    }
}
