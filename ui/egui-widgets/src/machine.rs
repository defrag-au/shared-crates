//! `Machine` — plain-enum UI state, hierarchical, with entry framing, frame-TTL auto-revert and eased transition progress.
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
//! Where a state is asserted by something that can repeat — a reconnect
//! handler, a poll that re-reports the same status — use [`Machine::enter`]
//! rather than `transition`, so entry effects fire once per genuine entry.
//!
//! ## Hierarchy, and why a flat machine feels clunky
//!
//! A flat enum can tell you *that* the state changed but not *how far* the UI
//! moved. Signing in and switching which wallet you are reading are both "one
//! transition", so both get the same hard cut — and a screen that cuts the
//! same way for a scene change and for a detail change reads as abrupt.
//!
//! Implement [`HState`] and a state names its position as a root→leaf
//! [`StatePath`] (`gate/signing_in`, `live/reading/story`). The machine can
//! then answer questions a flat one cannot:
//!
//! - [`Machine::is_in`] — "anywhere under `live`?", without listing leaves;
//! - [`Machine::change_depth`] — where the last transition happened. A change
//!   at the root is a scene change; a change at depth 2 is an edit;
//! - [`Machine::progress`] — eased 0→1 through the current transition, with a
//!   duration scaled by that depth, so big moves take longer than small ones.
//!
//! `progress` is the one that removes the clunk: render fades and offsets
//! content by it instead of swapping content between frames. It is built on
//! [`crate::motion`], so retargeting mid-flight is continuous — interrupt a
//! transition halfway and the next one starts from where the eye last saw
//! things, rather than snapping back.
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

use crate::motion::{self, Easing};
use egui::{Context, Id};

/// A state's position in the hierarchy, coarsest segment first.
///
/// Fixed capacity and `Copy` — a path is named once per frame in render, and
/// an allocation there would be a tax on every state read.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StatePath {
    segs: [&'static str; Self::MAX_DEPTH],
    len: u8,
}

impl StatePath {
    /// Deeper than this and the hierarchy is doing the job a data-carrying
    /// variant should be doing.
    pub const MAX_DEPTH: usize = 4;

    /// Start a path at the root: `StatePath::root("live")`.
    pub fn root(seg: &'static str) -> Self {
        let mut segs = [""; Self::MAX_DEPTH];
        segs[0] = seg;
        Self { segs, len: 1 }
    }

    /// Descend a level: `StatePath::root("live").then("reading")`. Segments
    /// past [`StatePath::MAX_DEPTH`] are dropped rather than panicking — a
    /// too-deep path degrades to a shallower one, which mis-sizes an
    /// animation but never takes down a frame.
    pub fn then(mut self, seg: &'static str) -> Self {
        if (self.len as usize) < Self::MAX_DEPTH {
            self.segs[self.len as usize] = seg;
            self.len += 1;
        }
        self
    }

    pub fn as_slice(&self) -> &[&'static str] {
        &self.segs[..self.len as usize]
    }

    pub fn depth(&self) -> usize {
        self.len as usize
    }

    /// How many leading segments two paths share — the depth at which they
    /// diverge, and so the magnitude of the move between them. `0` means they
    /// share nothing: a different branch entirely.
    pub fn common_prefix(&self, other: &Self) -> usize {
        self.as_slice()
            .iter()
            .zip(other.as_slice())
            .take_while(|(a, b)| a == b)
            .count()
    }

    /// Is `seg` anywhere on this path — i.e. is this state that one, or
    /// underneath it?
    pub fn contains(&self, seg: &str) -> bool {
        self.as_slice().iter().any(|s| *s == seg)
    }
}

/// A state that knows where it sits in the hierarchy.
///
/// ```
/// use egui_widgets::machine::{HState, StatePath};
///
/// enum Session { Gate, Live { reading: bool } }
///
/// impl HState for Session {
///     fn path(&self) -> StatePath {
///         match self {
///             Session::Gate => StatePath::root("gate"),
///             Session::Live { reading: false } => StatePath::root("live").then("idle"),
///             Session::Live { reading: true } => StatePath::root("live").then("reading"),
///         }
///     }
/// }
///
/// // Sibling leaves under `live` share one segment: a small move.
/// let a = Session::Live { reading: false }.path();
/// let b = Session::Live { reading: true }.path();
/// assert_eq!(a.common_prefix(&b), 1);
/// // Crossing branches shares nothing: a scene change.
/// assert_eq!(Session::Gate.path().common_prefix(&b), 0);
/// ```
pub trait HState {
    fn path(&self) -> StatePath;
}

/// Seconds for a transition at the root. Deeper changes scale down from here.
const BASE_TRANSITION_SECS: f32 = 0.28;

/// How much shorter each level of depth makes a transition.
const DEPTH_FALLOFF: f32 = 0.6;

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
    /// The state we came from, kept so a transition can be sized (how far did
    /// the UI move?) and, if the caller wants, drawn on its way out.
    prev: Option<S>,
    /// Transitions so far. Doubles as the tween target in [`Machine::progress`]
    /// — a monotonic counter means each transition retargets the tween by
    /// exactly 1.0, so "how far through the current one" is the fractional
    /// part, and an interrupted transition retargets from the eased position
    /// rather than snapping.
    generation: u32,
}

impl<S> Machine<S> {
    pub fn new(initial: S) -> Self {
        Self {
            state: initial,
            frames_in_state: 0,
            revert: None,
            prev: None,
            generation: 0,
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
        self.prev = Some(std::mem::replace(&mut self.state, next));
        self.frames_in_state = 0;
        self.revert = None;
        self.generation = self.generation.saturating_add(1);
    }

    /// The state this one replaced, for as long as the transition into the
    /// current state is still running. `None` before the first transition.
    pub fn prev(&self) -> Option<&S> {
        self.prev.as_ref()
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

impl<S: PartialEq> Machine<S> {
    /// Transition only if `next` differs from the current state, and report
    /// whether it did.
    ///
    /// [`Machine::transition`] is unconditional: re-asserting the state you
    /// are already in resets entry framing, so `entered()` goes true again
    /// and any entry effect fires a second time. That is correct for a
    /// deliberate re-entry (a flash you want to restart) and wrong for the
    /// far more common shape — a state asserted from a source that can
    /// repeat, like a reconnect handler or a poll that re-reports a status.
    /// Reach for `enter` there: the first assertion transitions, later
    /// identical ones are inert.
    ///
    /// ```
    /// # use egui_widgets::machine::Machine;
    /// # #[derive(PartialEq)] enum P { Connecting, Live }
    /// let mut m = Machine::new(P::Connecting);
    /// assert!(m.enter(P::Live)); // first connect — run the arrival effect
    /// m.tick();
    /// assert!(!m.enter(P::Live)); // reconnect — already live, do nothing
    /// assert!(!m.entered());
    /// ```
    pub fn enter(&mut self, next: S) -> bool {
        if self.state == next {
            return false;
        }
        self.transition(next);
        true
    }
}

impl<S: HState> Machine<S> {
    /// The current state's path.
    pub fn path(&self) -> StatePath {
        self.state.path()
    }

    /// Is the machine in this state or anywhere beneath it? The ancestor test
    /// that lets `live` behaviour be written once instead of once per leaf.
    pub fn is_in(&self, seg: &str) -> bool {
        self.state.path().contains(seg)
    }

    /// The depth at which the last transition changed the path: `Some(0)` for
    /// a different root branch (a scene change), larger for smaller moves.
    /// `None` before the first transition.
    ///
    /// A transition between two states with the SAME path — different data,
    /// same position — reports the full depth, i.e. the smallest possible
    /// move, which is exactly right: only the payload changed.
    pub fn change_depth(&self) -> Option<usize> {
        let prev = self.prev.as_ref()?;
        Some(prev.path().common_prefix(&self.state.path()))
    }

    /// How long the current transition should take, given how far the UI
    /// moved. Root changes get the full duration; each level deeper is a
    /// smaller edit and earns a proportionally shorter one.
    pub fn transition_secs(&self) -> f32 {
        let depth = self.change_depth().unwrap_or(0);
        BASE_TRANSITION_SECS * DEPTH_FALLOFF.powi(depth as i32)
    }

    /// Eased 0→1 progress through the current transition — the value render
    /// fades and offsets by, instead of swapping content between frames.
    ///
    /// Call this EVERY frame with a stable `id`, including while settled: the
    /// tween behind it needs to have seen the key before the first transition,
    /// or that transition snaps instead of animating. Returns `1.0` when
    /// nothing is in flight, so the settled path needs no special case.
    pub fn progress(&self, ctx: &Context, id: Id) -> f32 {
        let raw = motion::tween(
            ctx,
            id,
            self.generation as f32,
            self.transition_secs(),
            Easing::InOutCubic,
        );
        if self.generation == 0 {
            return 1.0;
        }
        (raw - (self.generation - 1) as f32).clamp(0.0, 1.0)
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

    #[derive(Debug, PartialEq, Clone)]
    enum Sess {
        Gate,
        Live,
        Reading,
        Story,
    }

    impl HState for Sess {
        fn path(&self) -> StatePath {
            match self {
                Sess::Gate => StatePath::root("gate"),
                Sess::Live => StatePath::root("live"),
                Sess::Reading => StatePath::root("live").then("reading"),
                Sess::Story => StatePath::root("live").then("reading").then("story"),
            }
        }
    }

    #[test]
    fn change_depth_sizes_the_move() {
        let mut m = Machine::new(Sess::Gate);
        assert_eq!(m.change_depth(), None); // nothing has moved yet
        m.transition(Sess::Live);
        assert_eq!(m.change_depth(), Some(0)); // crossed branches: scene change
        m.transition(Sess::Reading);
        assert_eq!(m.change_depth(), Some(1)); // descended under live
        m.transition(Sess::Story);
        assert_eq!(m.change_depth(), Some(2)); // a smaller move still
    }

    #[test]
    fn deeper_transitions_are_quicker() {
        let mut m = Machine::new(Sess::Gate);
        m.transition(Sess::Live);
        let scene = m.transition_secs();
        m.transition(Sess::Reading);
        let edit = m.transition_secs();
        assert!(edit < scene, "a deeper change must not take longer");
    }

    #[test]
    fn ancestor_test_covers_every_leaf() {
        let mut m = Machine::new(Sess::Gate);
        assert!(!m.is_in("live"));
        for leaf in [Sess::Live, Sess::Reading, Sess::Story] {
            m.transition(leaf);
            assert!(m.is_in("live"), "every leaf under live must answer to it");
        }
    }

    #[test]
    fn path_depth_is_capped_not_panicking() {
        let deep = StatePath::root("a").then("b").then("c").then("d").then("e");
        assert_eq!(deep.depth(), StatePath::MAX_DEPTH);
        assert_eq!(deep.as_slice(), &["a", "b", "c", "d"]);
    }

    #[test]
    fn prev_is_retained_for_the_outgoing_draw() {
        let mut m = Machine::new(Sess::Gate);
        assert!(m.prev().is_none());
        m.transition(Sess::Live);
        assert_eq!(m.prev(), Some(&Sess::Gate));
    }

    #[test]
    fn enter_is_idempotent() {
        let mut m = Machine::new(Save::Clean);
        m.tick();
        assert!(m.enter(Save::Dirty));
        assert!(m.entered());
        m.tick();
        // The same state asserted again — the entry frame must NOT come back,
        // or a reconnect re-runs whatever hangs off it.
        assert!(!m.enter(Save::Dirty));
        assert!(!m.entered());
        assert_eq!(m.frames_in_state(), 1);
    }

    #[test]
    fn enter_leaves_an_armed_revert_alone() {
        let mut m = Machine::new(Save::Clean);
        m.transition_for(Save::Saved, 2, Save::Clean);
        assert!(!m.enter(Save::Saved)); // no-op: must not disarm the flash
        m.tick();
        m.tick();
        m.tick();
        assert!(matches!(m.get(), Save::Clean));
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
