//! `Sequencer` — plays queued cues one after another, each holding the stage
//! until its animation is done.
//!
//! A game turns what happened into cues (a card was played, the enemy was hit,
//! a turn began) and pushes them. Each frame it calls [`Sequencer::poll`]
//! with the frame's `dt` and whether anything it owns is still moving. A cue
//! comes back only when the previous one's hold has run out and the stage is
//! still; the game starts that cue's tweens and says how long it holds with
//! [`Sequencer::hold`].
//!
//! The animated values stay with their owner. The sequencer owns only the
//! order and the timing, so there is no shared state between the two.
//!
//! ```ignore
//! for e in step.events { seq.push(e); }
//! // each frame:
//! if let Some(cue) = seq.poll(dt, stage_moving) {
//!     start_animations_for(&cue);
//!     seq.hold(0.25);
//! }
//! let input_allowed = seq.idle() && !stage_moving;
//! ```

use std::collections::VecDeque;

#[derive(Debug)]
pub struct Sequencer<E> {
    queue: VecDeque<E>,
    /// Seconds the current cue still holds the stage.
    hold: f32,
}

impl<E> Default for Sequencer<E> {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            hold: 0.0,
        }
    }
}

impl<E> Sequencer<E> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, cue: E) {
        self.queue.push_back(cue);
    }

    pub fn extend(&mut self, cues: impl IntoIterator<Item = E>) {
        self.queue.extend(cues);
    }

    /// The next cue, once the last one's hold has passed and `moving` is
    /// false. Call once per frame.
    pub fn poll(&mut self, dt: f32, moving: bool) -> Option<E> {
        self.hold = (self.hold - dt.max(0.0)).max(0.0);
        if self.hold > 0.0 || moving {
            return None;
        }
        self.queue.pop_front()
    }

    /// How long the cue just taken holds the stage before the next may start,
    /// on top of waiting for everything to stop moving.
    pub fn hold(&mut self, seconds: f32) {
        self.hold = self.hold.max(seconds);
    }

    /// Nothing queued and nothing holding: input may resume once the stage is
    /// still too.
    pub fn idle(&self) -> bool {
        self.queue.is_empty() && self.hold <= 0.0
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Drop everything queued: a reset, or a skip.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.hold = 0.0;
    }
}

/// Delay for the `i`th item of a staggered group.
pub fn stagger(i: usize, step: f32) -> f32 {
    i as f32 * step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_come_out_in_order_one_hold_at_a_time() {
        let mut s = Sequencer::new();
        s.extend(["a", "b"]);
        assert_eq!(s.poll(0.016, false), Some("a"));
        s.hold(0.1);
        assert_eq!(s.poll(0.05, false), None);
        assert_eq!(s.poll(0.06, false), Some("b"));
        assert!(s.idle());
    }

    #[test]
    fn a_moving_stage_holds_the_next_cue() {
        let mut s = Sequencer::new();
        s.push(1);
        assert_eq!(s.poll(1.0, true), None);
        assert_eq!(s.poll(0.0, false), Some(1));
    }

    #[test]
    fn idle_waits_for_the_hold() {
        let mut s: Sequencer<u8> = Sequencer::new();
        s.hold(0.2);
        assert!(!s.idle());
        s.poll(0.2, false);
        assert!(s.idle());
    }

    #[test]
    fn stagger_spaces_a_group() {
        assert_eq!(stagger(0, 0.07), 0.0);
        assert!((stagger(3, 0.07) - 0.21).abs() < 1e-6);
    }
}
