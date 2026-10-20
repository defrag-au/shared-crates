//! `Pads` — gamepads by player slot, with sticks deadzoned and running y-down like the screen (feature `gamepads`).
//!
//! Built on the `gamepads` crate: gilrs on desktop, the browser Gamepad API
//! on the web through a miniquad plugin. The page must load that plugin —
//! stamp it with [`crate::web::stamp`]`(dir, &[Plugin::Gamepads])` — or the
//! game sees no pads and says nothing.
//!
//! What the kit adds over the crate:
//!
//! - **Slots that stay put.** A pad keeps its player slot while connected; a
//!   newly connected pad takes the first free slot. Indexing the connected
//!   list instead would swap players when an earlier pad drops out.
//! - **Screen-direction sticks.** The crate reports up as positive y; a game
//!   draws with y down, so [`Pad::stick`] flips it, after a radial deadzone
//!   that rescales so movement starts from zero at the edge of it.
//! - **One direction.** [`Pad::direction`] is the d-pad when it is held and
//!   the stick otherwise, which is what a walking game wants.
//!
//! A browser only lists a pad after a button on it is pressed while the page
//! has focus. Rumble works natively through gilrs, and on the web where the
//! browser supports `vibrationActuator`.
//!
//! Nothing here reads the clock: call [`Pads::poll`] once per frame.

use gamepads::{Gamepad, GamepadId, Gamepads};
use macroquad::prelude::{Vec2, vec2};

pub use gamepads::Button;

/// Stick travel ignored around the centre, as a fraction of full.
pub const DEADZONE: f32 = 0.2;

/// Radial deadzone: zero inside `dz`, then rescaled so it ramps from 0 at
/// the deadzone's edge to 1 at full travel. Length capped at 1.
pub fn deadzone(v: Vec2, dz: f32) -> Vec2 {
    let len = v.length();
    if len <= dz {
        return Vec2::ZERO;
    }
    let scaled = ((len - dz) / (1.0 - dz)).min(1.0);
    v / len * scaled
}

/// Keep each slot's id while it is still connected; fill empty slots, in
/// order, with connected ids that hold no slot.
pub fn assign<T: Copy + PartialEq>(slots: &mut [Option<T>], connected: &[T]) {
    for slot in slots.iter_mut() {
        if slot.is_some_and(|id| !connected.contains(&id)) {
            *slot = None;
        }
    }
    for id in connected {
        if slots.contains(&Some(*id)) {
            continue;
        }
        if let Some(free) = slots.iter_mut().find(|s| s.is_none()) {
            *free = Some(*id);
        }
    }
}

/// One pad's state this frame.
#[derive(Clone, Copy, Debug)]
pub struct Pad(Gamepad);

impl Pad {
    /// Left stick, deadzoned, y down.
    pub fn stick(&self) -> Vec2 {
        let (x, y) = self.0.left_stick();
        deadzone(vec2(x, -y), DEADZONE)
    }

    /// Right stick, deadzoned, y down.
    pub fn right_stick(&self) -> Vec2 {
        let (x, y) = self.0.right_stick();
        deadzone(vec2(x, -y), DEADZONE)
    }

    /// The d-pad as a vector, y down; each axis -1, 0 or 1.
    pub fn dpad(&self) -> Vec2 {
        let axis =
            |neg: Button, pos: Button| self.down(pos) as i8 as f32 - self.down(neg) as i8 as f32;
        vec2(
            axis(Button::DPadLeft, Button::DPadRight),
            axis(Button::DPadUp, Button::DPadDown),
        )
    }

    /// Where the player is steering: the d-pad while held, else the stick.
    pub fn direction(&self) -> Vec2 {
        let d = self.dpad();
        if d != Vec2::ZERO {
            d.normalize()
        } else {
            self.stick()
        }
    }

    /// Held now.
    pub fn down(&self, b: Button) -> bool {
        self.0.is_currently_pressed(b)
    }

    /// Went down since the last poll.
    pub fn pressed(&self, b: Button) -> bool {
        self.0.is_just_pressed(b)
    }
}

/// The connected pads, held in player slots.
pub struct Pads {
    inner: Gamepads,
    slots: Vec<Option<GamepadId>>,
}

impl Pads {
    /// Room for `players` pads.
    pub fn new(players: usize) -> Self {
        Self {
            inner: Gamepads::new(),
            slots: vec![None; players],
        }
    }

    /// Read every pad and settle the slots. Once per frame, before reading.
    pub fn poll(&mut self) {
        self.inner.poll();
        let connected: Vec<GamepadId> = self.inner.all().map(|p| p.id()).collect();
        assign(&mut self.slots, &connected);
    }

    /// The pad in `slot`, if one is there.
    pub fn get(&self, slot: usize) -> Option<Pad> {
        let id = (*self.slots.get(slot)?)?;
        self.inner.get(id).map(Pad)
    }

    /// How many slots hold a pad.
    pub fn connected(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    /// Shake the pad in `slot`, if any. `strength` 0..1.
    pub fn rumble(&mut self, slot: usize, millis: u32, strength: f32) {
        if let Some(Some(id)) = self.slots.get(slot).copied() {
            self.inner.rumble(id, millis, 0, strength, strength * 0.6);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadzone_is_zero_inside_and_ramps_from_its_edge() {
        assert_eq!(deadzone(vec2(0.1, 0.1), 0.2), Vec2::ZERO);
        let just_out = deadzone(vec2(0.21, 0.0), 0.2);
        assert!(just_out.x > 0.0 && just_out.x < 0.02, "{just_out}");
        assert!((deadzone(vec2(1.0, 0.0), 0.2).x - 1.0).abs() < 1e-6);
        // Direction kept, length capped.
        let corner = deadzone(vec2(1.0, 1.0), 0.2);
        assert!((corner.length() - 1.0).abs() < 1e-6);
        assert!((corner.x - corner.y).abs() < 1e-6);
    }

    #[test]
    fn a_pad_keeps_its_slot_when_an_earlier_one_leaves() {
        let mut slots = [None; 2];
        assign(&mut slots, &[7, 9]);
        assert_eq!(slots, [Some(7), Some(9)]);
        // Pad 7 unplugs: player 2 keeps pad 9 rather than becoming player 1.
        assign(&mut slots, &[9]);
        assert_eq!(slots, [None, Some(9)]);
        // A new pad takes the free slot.
        assign(&mut slots, &[9, 4]);
        assert_eq!(slots, [Some(4), Some(9)]);
    }

    #[test]
    fn extra_pads_wait_for_a_slot() {
        let mut slots = [None; 1];
        assign(&mut slots, &[1, 2]);
        assert_eq!(slots, [Some(1)]);
        assign(&mut slots, &[2]);
        assert_eq!(slots, [Some(2)]);
    }
}
