//! Key-repeat helper — macroquad's `is_key_pressed` only fires on the
//! initial press, so held keys (backspace, arrows, delete) need explicit
//! handling. This module wraps the typical "initial delay → repeat at
//! interval" pattern keyboards exhibit.
//!
//! Usage:
//! ```ignore
//! let mut repeat = KeyRepeat::new(0.4, 0.04);
//! loop {
//!     let time = get_time() as f32;
//!     if repeat.fires(KeyCode::Backspace, time) {
//!         editor.backspace();
//!     }
//!     next_frame().await;
//! }
//! ```
//!
//! The `fires` method returns `true` when the key was pressed this frame
//! *or* has been held long enough to repeat. Callers don't need to
//! distinguish — they just respond to the event.

use std::collections::HashMap;

use macroquad::input::{is_key_down, is_key_pressed, KeyCode};

#[derive(Debug, Clone, Copy)]
struct KeyState {
    pressed_at: f32,
    last_fired_at: f32,
}

pub struct KeyRepeat {
    initial_delay: f32,
    repeat_interval: f32,
    state: HashMap<KeyCode, KeyState>,
}

impl KeyRepeat {
    /// Typical defaults: `initial_delay = 0.4s`, `repeat_interval = 0.04s`
    /// (matches the macOS / GNOME standard pretty closely).
    pub fn new(initial_delay: f32, repeat_interval: f32) -> Self {
        Self {
            initial_delay,
            repeat_interval,
            state: HashMap::new(),
        }
    }

    /// Returns true when the key should fire this frame — either it was
    /// just pressed, or it's been held past `initial_delay` and the last
    /// fire was more than `repeat_interval` ago.
    pub fn fires(&mut self, key: KeyCode, time: f32) -> bool {
        // Initial press — always fires; reset bookkeeping.
        if is_key_pressed(key) {
            self.state.insert(
                key,
                KeyState { pressed_at: time, last_fired_at: time },
            );
            return true;
        }

        // Held — check repeat schedule.
        if is_key_down(key) {
            let Some(s) = self.state.get_mut(&key) else {
                // We missed the press event (e.g. focus loss); start
                // tracking from now without firing.
                self.state.insert(
                    key,
                    KeyState { pressed_at: time, last_fired_at: time },
                );
                return false;
            };
            let held_for = time - s.pressed_at;
            if held_for < self.initial_delay {
                return false;
            }
            let since_last = time - s.last_fired_at;
            if since_last >= self.repeat_interval {
                s.last_fired_at = time;
                return true;
            }
            return false;
        }

        // Released — clear bookkeeping.
        self.state.remove(&key);
        false
    }
}
