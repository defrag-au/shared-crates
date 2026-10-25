//! `Gait` — how a creature's frames play over time: each frame held as long as it should be, an eased beat for the body to move with, and every instance its own rate and phase from a seed, so many clones of one sprite never beat in step.
//!
//! A sprite with two frames flipped on a fixed clock moves like a light
//! switch: every pose shown for the same time, the body still between them,
//! and a whole school flicking together. A gait fixes all three:
//!
//! - **Holds.** A cycle is a sequence of frames, each with a weight: the
//!   extremes of a stroke (tail up, tail down) held, the passing pose between
//!   them brief. A frame can appear more than once (up, level, down, level).
//! - **An eased beat.** [`Stride::swing`] is the cycle as a sine, slow at its
//!   ends like a real stroke, for the body to rise and fall or pitch with;
//!   [`Stride::surge`] peaks mid-stroke, for a push forward. Moving the body
//!   with these, rather than linearly, is what makes two frames look like
//!   swimming.
//! - **Each its own.** [`Gait::at`] takes the instance's seed: its rate
//!   varies by up to [`Gait::jitter`] either way and its phase is its own.
//!   The sprite's textures are shared, so a thousand clones cost a thousand
//!   seeds, not a thousand textures.
//!
//! Pure: no macroquad, no clock of its own. The caller passes the time.

use std::f32::consts::TAU;

/// A cycle of frames and how each instance plays it.
#[derive(Clone, Debug, PartialEq)]
pub struct Gait {
    /// `(frame, hold)` in cycle order; holds are relative weights.
    steps: Vec<(usize, f32)>,
    /// Seconds a cycle takes at the nominal rate.
    period: f32,
    /// How far an instance's rate may stray, as a fraction either way.
    spread: f32,
}

/// Where an instance is in its cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stride {
    /// The frame to draw.
    pub frame: usize,
    /// How far through the cycle, `0..1`.
    pub beat: f32,
    /// The stroke as a sine over the cycle, `-1..1`, slow at its ends: what
    /// the body rises and falls (or pitches) with.
    pub swing: f32,
    /// `0..1`, highest mid-stroke and nothing at the stroke's ends, twice a
    /// cycle: a push forward.
    pub surge: f32,
}

impl Gait {
    /// A cycle of `steps`, `(frame, hold)`, taking `period` seconds at the
    /// nominal rate. Holds are relative: `[(0, 2.0), (1, 1.0)]` shows frame 0
    /// for two thirds of the cycle.
    pub fn new(steps: &[(usize, f32)], period: f32) -> Self {
        let steps: Vec<(usize, f32)> = steps.iter().map(|(f, w)| (*f, w.max(0.0))).collect();
        Self {
            steps: if steps.is_empty() {
                vec![(0, 1.0)]
            } else {
                steps
            },
            period: period.max(1e-3),
            spread: 0.0,
        }
    }

    /// Frames `0..n` in order, held evenly.
    pub fn even(frames: usize, period: f32) -> Self {
        let steps: Vec<(usize, f32)> = (0..frames.max(1)).map(|f| (f, 1.0)).collect();
        Self::new(&steps, period)
    }

    /// Each instance's rate strays by up to `spread` either way (`0.15` is
    /// 15%), from its seed.
    pub fn jitter(mut self, spread: f32) -> Self {
        self.spread = spread.clamp(0.0, 0.9);
        self
    }

    /// The nominal seconds a cycle takes.
    pub fn period(&self) -> f32 {
        self.period
    }

    /// The cycle's `(frame, hold)` steps, in order.
    pub fn steps(&self) -> &[(usize, f32)] {
        &self.steps
    }

    /// How far an instance's rate may stray, as a fraction either way.
    pub fn spread(&self) -> f32 {
        self.spread
    }

    /// The rate `seed`'s instance plays at, as a multiple of the nominal.
    pub fn rate(&self, seed: u64) -> f32 {
        1.0 + self.spread * (unit(seed, 1) * 2.0 - 1.0)
    }

    /// Where `seed`'s instance is at time `t` seconds.
    pub fn at(&self, t: f32, seed: u64) -> Stride {
        let cycles = t * self.rate(seed) / self.period + unit(seed, 2);
        self.at_beat(cycles.rem_euclid(1.0))
    }

    /// The stride at `beat` (`0..1`) through the cycle, the same for every
    /// instance: for drawing a cycle out, as the sprite sheet does.
    pub fn at_beat(&self, beat: f32) -> Stride {
        let beat = beat.rem_euclid(1.0);
        let total: f32 = self.steps.iter().map(|(_, w)| w).sum();
        let mut frame = self.steps[0].0;
        if total > 0.0 {
            let mut along = beat * total;
            for (f, w) in &self.steps {
                frame = *f;
                if along < *w {
                    break;
                }
                along -= w;
            }
        }
        Stride {
            frame,
            beat,
            swing: (TAU * beat).sin(),
            surge: 0.5 - 0.5 * (2.0 * TAU * beat).cos(),
        }
    }
}

/// A value in `0..1` from a seed and a salt, the same every time.
fn unit(seed: u64, salt: u64) -> f32 {
    // splitmix64
    let mut z = seed
        .wrapping_add(salt.wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each frame shows for its share of the cycle.
    #[test]
    fn holds_set_how_long_each_frame_shows() {
        let gait = Gait::new(&[(0, 3.0), (1, 1.0)], 1.0);
        let shown_0 = (0..1000)
            .filter(|i| gait.at_beat(*i as f32 / 1000.0).frame == 0)
            .count();
        assert!((740..=760).contains(&shown_0), "{shown_0}");

        // A frame can come round twice: up, level, down, level.
        let stroke = Gait::new(&[(0, 1.0), (1, 1.0), (2, 1.0), (1, 1.0)], 1.0);
        let frames: Vec<usize> = [0.1, 0.35, 0.6, 0.85]
            .iter()
            .map(|b| stroke.at_beat(*b).frame)
            .collect();
        assert_eq!(frames, vec![0, 1, 2, 1]);
    }

    /// The swing is a sine: still at its ends, fastest through the middle;
    /// the surge is nothing at the ends and most mid-stroke.
    #[test]
    fn the_beat_eases() {
        let g = Gait::even(2, 1.0);
        assert!(g.at_beat(0.25).swing > 0.99);
        assert!(g.at_beat(0.75).swing < -0.99);
        let near_end = g.at_beat(0.25).swing - g.at_beat(0.24).swing;
        let mid = g.at_beat(0.01).swing - g.at_beat(0.0).swing;
        assert!(mid > near_end * 10.0, "slow at the extreme, quick through");
        assert!(g.at_beat(0.25).surge > 0.99 && g.at_beat(0.0).surge < 1e-6);
    }

    /// Instances do not beat in step, and each is the same every time.
    #[test]
    fn every_instance_keeps_its_own_rhythm() {
        let g = Gait::even(2, 0.6).jitter(0.15);
        let rates: Vec<f32> = (0..64).map(|s| g.rate(s)).collect();
        assert!(rates.iter().all(|r| (0.85..=1.15).contains(r)));
        let distinct = rates
            .iter()
            .filter(|r| (*r - rates[0]).abs() > 0.01)
            .count();
        assert!(distinct > 40, "rates differ across a school");
        let frames: Vec<usize> = (0..64).map(|s| g.at(3.3, s).frame).collect();
        assert!(
            frames.contains(&0) && frames.contains(&1),
            "not all on one frame"
        );
        assert_eq!(g.at(3.3, 7), g.at(3.3, 7));

        let steady = Gait::even(2, 0.6);
        assert_eq!(steady.rate(1), 1.0);
        assert_eq!(steady.rate(99), 1.0);
    }
}
