//! `Machine` — plain-enum UI state with entry-frame detection and frame-TTL auto-revert.
//!
//! The "model state, not flags" rule, given machinery. Immediate-mode UIs
//! keep growing the same three boolean weeds around one underlying state:
//!
//! - **entry transients** (`just_opened: bool`) — behaviour that differs on
//!   the frame a state was entered (suppress the click that opened a modal
//!   from also dismissing it);
//! - **flash states** (`saved_flash: bool`) — a state that should show
//!   briefly and then revert by itself;
//! - **in-flight markers** (`pending: Option<OpId>`) — really a `Saving(op)`
//!   state whose data is the op to match the ack against.
//!
//! `Machine<S>` wraps YOUR enum — states stay plain data-carrying variants
//! you `match` in render — and adds exactly those three affordances:
//! [`Machine::entered`], [`Machine::transition_for`] (auto-revert after N
//! frames), and the convention that in-flight data lives IN the variant
//! (`Saving { op }`), so acks are matched with a `matches!` guard and no
//! parallel `Option` field exists to drift.
//!
//! ## The tick convention (load-bearing)
//!
//! Call [`Machine::tick`] **once per frame, AFTER rendering** (last thing in
//! `update`/`ui`). Transitions can then happen anywhere in the frame —
//! input handling, event polling, render callbacks — and `entered()` is true
//! from the transition until that end-of-frame tick, i.e. for exactly the
//! remainder of the frame that entered the state. Frame-counted (like
//! `ToastQueue`), not wall-clock: identical behaviour native and wasm.
//!
//! ## Example
//!
//! ```
//! use egui_widgets::machine::Machine;
//!
//! #[derive(Debug, PartialEq)]
//! enum Save {
//!     Clean,
//!     Dirty,
//!     Saving { op: u64 },
//!     Saved,
//! }
//!
//! let mut save = Machine::new(Save::Clean);
//! save.transition(Save::Dirty);                       // an edit happened
//! save.transition(Save::Saving { op: 7 });            // sent
//! // ... ack for op 7 arrives:
//! if matches!(save.get(), Save::Saving { op } if *op == 7) {
//!     save.transition_for(Save::Saved, 180, Save::Clean); // ~3s flash
//! }
//! assert!(save.entered());                            // this frame only
//! assert!(matches!(save.get(), Save::Saved));
//! for _ in 0..181 { save.tick(); }
//! assert!(matches!(save.get(), Save::Clean));         // auto-reverted
//! ```

/// A UI state machine over a caller-defined state enum. See the module docs
/// for the tick convention.
pub struct Machine<S> {
    state: S,
    /// End-of-frame ticks since the current state was entered. 0 means the
    /// state was entered during the current (un-ticked) frame.
    frames_in_state: u32,
    /// Auto-revert armed by [`Machine::transition_for`]: after the current
    /// state has been shown for N ticks, transition to the stored state.
    revert: Option<(u32, S)>,
}

impl<S> Machine<S> {
    pub fn new(initial: S) -> Self {
        Self {
            state: initial,
            frames_in_state: 0,
            revert: None,
        }
    }

    /// The current state, for `match`ing in render.
    pub fn get(&self) -> &S {
        &self.state
    }

    /// Mutable access for edits WITHIN a state (e.g. typing into a field a
    /// variant carries). Not a transition — entry framing is untouched.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.state
    }

    /// Enter a new state. Resets entry framing and disarms any pending
    /// auto-revert (an explicit transition always wins over a scheduled one).
    pub fn transition(&mut self, next: S) {
        self.state = next;
        self.frames_in_state = 0;
        self.revert = None;
    }

    /// Enter `temp`, and auto-revert to `then` after it has been shown for
    /// `frames` end-of-frame ticks — the flash-state affordance
    /// (`saved ✓` for ~3s at 60fps = 180 frames).
    pub fn transition_for(&mut self, temp: S, frames: u32, then: S) {
        self.transition(temp);
        self.revert = Some((frames, then));
    }

    /// True from a transition until the end-of-frame tick — i.e. exactly the
    /// remainder of the frame that entered the state. The `just_opened`
    /// killer: "suppress dismissal on the frame the modal opened" is
    /// `if !machine.entered() { … }`.
    pub fn entered(&self) -> bool {
        self.frames_in_state == 0
    }

    /// End-of-frame ticks spent in the current state.
    pub fn frames_in_state(&self) -> u32 {
        self.frames_in_state
    }

    /// Advance one frame. Call once per frame, AFTER rendering. Applies any
    /// armed auto-revert whose frames have elapsed.
    pub fn tick(&mut self) {
        self.frames_in_state = self.frames_in_state.saturating_add(1);
        if let Some((frames, _)) = &self.revert {
            if self.frames_in_state > *frames {
                let (_, then) = self.revert.take().expect("checked above");
                self.transition(then);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum Save {
        Clean,
        Dirty,
        Saving { op: u64 },
        Saved,
    }

    #[test]
    fn entry_framing() {
        let mut m = Machine::new(Save::Clean);
        assert!(m.entered()); // initial state counts as entered
        m.tick();
        assert!(!m.entered());
        m.transition(Save::Dirty);
        assert!(m.entered()); // true for the remainder of this frame
        m.tick();
        assert!(!m.entered());
        assert_eq!(m.frames_in_state(), 1);
    }

    #[test]
    fn flash_auto_revert() {
        let mut m = Machine::new(Save::Dirty);
        m.transition_for(Save::Saved, 3, Save::Clean);
        assert!(matches!(m.get(), Save::Saved));
        m.tick();
        m.tick();
        m.tick();
        assert!(matches!(m.get(), Save::Saved)); // shown for 3 full frames
        m.tick();
        assert!(matches!(m.get(), Save::Clean));
        assert!(m.entered()); // the revert is itself an entry
    }

    #[test]
    fn explicit_transition_disarms_revert() {
        let mut m = Machine::new(Save::Clean);
        m.transition_for(Save::Saved, 2, Save::Clean);
        m.transition(Save::Dirty); // user edited during the flash
        for _ in 0..10 {
            m.tick();
        }
        assert!(matches!(m.get(), Save::Dirty)); // no zombie revert fired
    }

    #[test]
    fn op_in_variant_matches_ack() {
        let mut m = Machine::new(Save::Saving { op: 7 });
        // A stale ack for a different op leaves the state alone.
        if matches!(m.get(), Save::Saving { op } if *op == 3) {
            m.transition(Save::Saved);
        }
        assert!(matches!(m.get(), Save::Saving { op: 7 }));
        // The matching ack transitions.
        if matches!(m.get(), Save::Saving { op } if *op == 7) {
            m.transition_for(Save::Saved, 1, Save::Clean);
        }
        assert!(matches!(m.get(), Save::Saved));
    }
}
