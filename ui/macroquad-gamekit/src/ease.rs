//! `Ease` — named easing curves, so an animation says how it moves rather than
//! writing the maths at each call site.
//!
//! Every curve maps 0 to 0 and 1 to 1. Input outside 0..=1 is clamped, so a
//! tween that overshoots its duration by a frame still lands exactly.
//!
//! For card-game feel: [`Ease::ExpoOut`] is the settle (fast start, no
//! overshoot; about 150 ms reads as crisp), [`Ease::CubicIn`] is a card being
//! thrown at a target, and [`Ease::BackOut`] is a badge popping in.

use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ease {
    Linear,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    /// Fast start, long gentle landing, no overshoot: the settle.
    ExpoOut,
    SineInOut,
    /// Overshoots slightly and comes back: a pop.
    BackOut,
}

impl Ease {
    pub const ALL: [Ease; 10] = [
        Ease::Linear,
        Ease::QuadIn,
        Ease::QuadOut,
        Ease::QuadInOut,
        Ease::CubicIn,
        Ease::CubicOut,
        Ease::CubicInOut,
        Ease::ExpoOut,
        Ease::SineInOut,
        Ease::BackOut,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Ease::Linear => "linear",
            Ease::QuadIn => "quad in",
            Ease::QuadOut => "quad out",
            Ease::QuadInOut => "quad in-out",
            Ease::CubicIn => "cubic in",
            Ease::CubicOut => "cubic out",
            Ease::CubicInOut => "cubic in-out",
            Ease::ExpoOut => "expo out",
            Ease::SineInOut => "sine in-out",
            Ease::BackOut => "back out",
        }
    }

    /// Progress `t` (0..=1) mapped through the curve.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::QuadIn => t * t,
            Ease::QuadOut => 1.0 - (1.0 - t) * (1.0 - t),
            Ease::QuadInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Ease::CubicIn => t * t * t,
            Ease::CubicOut => 1.0 - (1.0 - t).powi(3),
            Ease::CubicInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Ease::ExpoOut => {
                if t >= 1.0 {
                    1.0
                } else {
                    1.0 - 2f32.powf(-10.0 * t)
                }
            }
            Ease::SineInOut => -((PI * t).cos() - 1.0) / 2.0,
            Ease::BackOut => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_starts_at_zero_and_ends_at_one() {
        for e in Ease::ALL {
            assert!(e.apply(0.0).abs() < 1e-6, "{} at 0", e.name());
            assert!((e.apply(1.0) - 1.0).abs() < 1e-6, "{} at 1", e.name());
        }
    }

    #[test]
    fn progress_outside_the_range_is_clamped() {
        for e in Ease::ALL {
            assert_eq!(e.apply(-1.0), e.apply(0.0));
            assert_eq!(e.apply(2.0), e.apply(1.0));
        }
    }

    #[test]
    fn the_settle_never_overshoots_and_the_pop_does() {
        let samples = (0..=100).map(|i| i as f32 / 100.0);
        assert!(samples.clone().all(|t| Ease::ExpoOut.apply(t) <= 1.0));
        assert!(samples.clone().any(|t| Ease::BackOut.apply(t) > 1.0));
    }

    #[test]
    fn out_curves_are_ahead_of_linear_and_in_curves_behind() {
        for t in [0.25, 0.5, 0.75] {
            assert!(Ease::ExpoOut.apply(t) > t);
            assert!(Ease::CubicOut.apply(t) > t);
            assert!(Ease::CubicIn.apply(t) < t);
        }
    }
}
