//! What every camera's motion shares: held keys that ease in and out, and
//! the easing that takes a value toward a target by a frame.

/// Seconds a held key's speed takes to come up or die away (to 1/e).
pub const EASE: f32 = 0.12;
/// Seconds a zoom takes to settle on its target (to 1/e).
pub const ZOOM_EASE: f32 = 0.09;
/// Seconds a released drag's momentum takes to die away (to 1/e).
pub const COAST: f32 = 0.35;

/// How far toward a target a value eases in `dt` seconds, when it takes
/// `time` seconds to close all but 1/e of the gap: frame-rate independent.
pub fn ease(dt: f32, time: f32) -> f32 {
    1.0 - (-dt / time.max(1e-6)).exp()
}

/// Smoothstep: 0 to 1, starting and stopping gently.
pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Held keys, each −1 to 1: travel ahead (up the screen) and right, turn the
/// heading clockwise, pitch toward the horizon, zoom in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    pub ahead: f32,
    pub right: f32,
    pub turn: f32,
    pub tilt: f32,
    pub zoom: f32,
}

impl Drive {
    /// Ease `self` (the speeds) toward `held` by `k` (0–1), snapping a speed
    /// that has all but died to zero so a camera can tell it has stopped.
    pub fn toward(self, held: Drive, k: f32) -> Drive {
        let step = |a: f32, b: f32| {
            let v = a + (b - a) * k;
            if b == 0.0 && v.abs() < 1e-4 {
                0.0
            } else {
                v
            }
        };
        Drive {
            ahead: step(self.ahead, held.ahead),
            right: step(self.right, held.right),
            turn: step(self.turn, held.turn),
            tilt: step(self.tilt, held.tilt),
            zoom: step(self.zoom, held.zoom),
        }
    }

    pub fn is_still(&self) -> bool {
        *self == Drive::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_does_not_depend_on_the_frame_rate() {
        // One 1/30 s frame or two 1/60 s frames close the same share of a gap.
        let once = ease(1.0 / 30.0, EASE);
        let twice = 1.0 - (1.0 - ease(1.0 / 60.0, EASE)).powi(2);
        assert!((once - twice).abs() < 1e-6, "{once} vs {twice}");
    }

    #[test]
    fn a_released_key_dies_away_to_exactly_still() {
        let mut speed = Drive {
            ahead: 1.0,
            ..Drive::default()
        };
        for _ in 0..120 {
            speed = speed.toward(Drive::default(), ease(1.0 / 60.0, EASE));
        }
        assert!(speed.is_still(), "{speed:?}");
    }
}
