//! Gesture arbitration for value controls under a finger.
//!
//! ## The problem this exists for
//!
//! A control that senses drag is, in egui, the innermost widget under the
//! pointer — so it beats the `ScrollArea` behind it for every drag that starts
//! on it. With a mouse that is exactly right: a mouse scrolls with the wheel, so
//! nothing is competing. With a finger, scrolling IS dragging, and a control
//! that swallows vertical drags turns itself into a region of the page the
//! reader cannot get past. A bank of faders or a row of knobs is then a wall.
//!
//! The fix is to make the control **transparent until it is grabbed**: rest a
//! finger on it for [`HOLD`] and it takes over; flick and the page scrolls as if
//! the control were not there.
//!
//! ## Two ways to be transparent
//!
//! Both widgets that use this reach the same state by different routes, because
//! what they own differs:
//!
//! - [`crate::knob`] paints itself, so it simply allocates with `Sense::click()`
//!   while ungrabbed — no drag sense, nothing to steal.
//! - [`crate::slider_group`] wraps `egui::Slider`, whose sense is not ours to
//!   choose. It disables the slider instead: egui's hit test strips `CLICK` and
//!   `DRAG` from a disabled widget, which is the same transparency by another
//!   name, and `Ui::set_opacity` puts back the fade that `disable` applies so
//!   the rail still paints at full strength.
//!
//! Either way the rule is the same, which is the point of this module: one
//! vocabulary for "has the finger taken hold of this yet".
//!
//! ## Which controls should be gated — the rule, learned the hard way
//!
//! Not all of them, and the first version of this got it wrong by assuming the
//! knob's answer generalised. It does not. **Gate a control when contact has no
//! meaning; leave it alone when contact means something obvious.**
//!
//! | | [`crate::knob`] | [`crate::slider_group`] |
//! |---|---|---|
//! | model | relative — integrates drag | absolute — positions from x |
//! | what a tap means | nothing | *put the value here* |
//! | gesture axis | vertical, same as the page | horizontal, orthogonal to it |
//! | default | [`Grab::HoldToEngage`] | [`Grab::Direct`] |
//!
//! A knob loses nothing by waiting: there is no position to tap, and its drag
//! axis is the page's, so it is in direct competition. A rail loses its most
//! natural interaction by waiting, and barely competes for the gesture in the
//! first place — so gating it produces a slider that ignores you, which is a
//! worse failure than the one being prevented.
//!
//! The tell is the interaction model, not the platform: ask what a single tap
//! should do. If the answer is "nothing", a gate is free.

use egui::{Id, Ui};

/// Seconds a finger must rest on a control before it takes the gesture.
///
/// Long enough that a flick passes through, short enough to read as
/// responsiveness rather than as a stuck control. The usual long-press threshold
/// is ~500ms; this is a gate, not a menu.
pub const HOLD: f64 = 0.18;

/// How far the finger may wander during the hold and still count as still, in
/// px. A finger is never actually still.
pub const HOLD_SLOP: f32 = 6.0;

/// How a control behaves under a finger.
///
/// Only consulted on a touch gesture — a mouse always behaves as
/// [`Grab::Direct`], because a mouse scrolls with the wheel and so nothing is
/// competing for its drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grab {
    /// Take the gesture the moment the finger moves, exactly like a mouse.
    ///
    /// Correct when nothing else wants it — a control in a fixed panel, or one
    /// in a pane whose `ScrollArea` has `drag` cleared from its `ScrollSource`.
    Direct,
    /// Let a flick pass through to whatever is behind, and take the gesture only
    /// once the finger has rested on the control for [`HOLD`].
    ///
    /// The default, and the trade is deliberate: needing to hold for about a
    /// fifth of a second is a smaller cost than a page that traps the reader,
    /// and unlike the trap it is discoverable — the hold draws a progress ring.
    #[default]
    HoldToEngage,
}

/// Whether this control currently holds the gesture.
///
/// Read **before** allocating, because for a widget that owns its painting the
/// answer decides what sense to allocate with — and that choice is the whole
/// mechanism.
pub fn engaged(ui: &Ui, id: Id) -> bool {
    ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false)
}

/// What the gate did this pass.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hold {
    /// How far through the hold the finger is, `0.0..=1.0`. `0.0` means no hold
    /// is in progress — nothing pressed, the finger wandered past
    /// [`HOLD_SLOP`], or the control is already engaged.
    pub progress: f32,
    /// The hold completed on THIS pass. The moment to call
    /// [`take_the_drag`] — see there for why it cannot wait.
    pub just_engaged: bool,
}

/// Advance the gate.
///
/// Call **after** the control has been laid out, passing whether the pointer is
/// currently pressed on it.
pub fn advance(ui: &Ui, id: Id, down_on_target: bool, engaged_now: bool) -> Hold {
    if !down_on_target {
        // Released — the next gesture starts from scratch.
        if engaged_now {
            ui.data_mut(|d| d.insert_temp(id, false));
        }
        return Hold::default();
    }
    if engaged_now {
        return Hold::default();
    }

    let (held, wander) = ui.input(|i| {
        let held = i.pointer.press_start_time().map_or(0.0, |t| i.time - t);
        let wander = i
            .pointer
            .press_origin()
            .zip(i.pointer.latest_pos())
            .map_or(0.0, |(o, p)| (p - o).length());
        (held, wander)
    });
    if wander > HOLD_SLOP {
        return Hold::default();
    }
    let done = held >= HOLD;
    if done {
        ui.data_mut(|d| d.insert_temp(id, true));
    }
    // Nothing else is moving during a still hold, so without this the pass that
    // would complete it never runs.
    ui.ctx().request_repaint();
    Hold {
        progress: (held / HOLD).clamp(0.0, 1.0) as f32,
        just_engaged: done,
    }
}

/// Take the drag away from whatever is currently holding it.
///
/// # Why changing the sense is not enough
///
/// This is the correction to the first version of this gate, and the device
/// found it. egui picks the drag candidate **once, at press time**, and never
/// revisits it:
///
/// ```ignore
/// PointerEvent::Pressed { .. } => {
///     if interaction.potential_drag_id.is_none() {
///         interaction.potential_drag_id = hits.drag.map(|w| w.id);
///     }
/// }
/// ```
///
/// The candidate is cleared only on release. So at the moment the finger lands,
/// the ungrabbed control is deliberately not sensing drag — and the `ScrollArea`
/// behind it becomes the candidate. Completing the hold afterwards makes the
/// control drag-sensitive, but the `is_none()` guard means egui will not look
/// again, and every subsequent movement scrolls the page.
///
/// Transparency is therefore only half the mechanism: the control has to be
/// transparent *before* the gesture is claimed, and then explicitly claim it.
pub fn take_the_drag(ctx: &egui::Context, id: Id) {
    ctx.set_dragged_id(id);
}

/// Whether this context is being driven by a touch screen.
///
/// **Sticky** — `has_touch_screen`, meaning "a touch has ever arrived", not
/// `any_touches`, meaning "a finger is down right now". The difference is the
/// whole correctness of the gate, and it is not obvious:
///
/// egui resolves an interaction at the start of a pass against the widget rects
/// the PREVIOUS pass registered. A control decides whether to be transparent
/// during its own pass. So with `any_touches` the decision is always one frame
/// behind the gesture that needs it — the first contact of a touch lands on a
/// control that was registered while the context still assumed a mouse, and it
/// is attributed and committed before the control ever learns a finger is
/// involved. Sticky detection moves that cost to once per context instead of
/// once per gesture.
///
/// # Known limit
///
/// The very first touch of a session still arrives before anything knows the
/// device is touch. If that first touch lands directly on a rail, it behaves as
/// a mouse click. In practice a reader has scrolled or tapped something before
/// reaching a bank of faders, so the flag is set long before it matters — but it
/// is a real hole and worth knowing rather than discovering.
pub fn is_touch(ui: &Ui) -> bool {
    ui.input(|i| i.has_touch_screen())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hold_is_a_gate_not_a_menu() {
        // Long enough that a flick passes through, short enough that it reads as
        // responsiveness. These are invariants of the constants themselves, so a
        // compile-time failure beats a test-run one.
        const { assert!(HOLD >= 0.1 && HOLD <= 0.25) };
        const { assert!(HOLD_SLOP > 0.0) };
    }

    #[test]
    fn taking_the_drag_moves_it_off_whatever_held_it() {
        // The device found this: sensing drag from the next pass does nothing,
        // because egui picks the drag candidate at PRESS time and the
        // `is_none()` guard stops it ever looking again. So engaging has to
        // claim the gesture explicitly, and claiming it has to dislodge the
        // scrolling container that legitimately owns it.
        let ctx = egui::Context::default();
        let scroller = Id::new("a-scroll-area");
        let control = Id::new("a-knob");

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 400.0),
            )),
            ..Default::default()
        });
        ctx.set_dragged_id(scroller);
        assert_eq!(ctx.dragged_id(), Some(scroller), "precondition");

        take_the_drag(&ctx, control);
        assert_eq!(ctx.dragged_id(), Some(control), "the control now holds it");
        assert_eq!(
            ctx.drag_stopped_id(),
            Some(scroller),
            "and the previous holder is told, so it stops scrolling"
        );
        let _ = ctx.end_pass();
    }

    #[test]
    fn hold_to_engage_is_the_default_because_the_other_way_traps_the_page() {
        // A control that senses drag beats the `ScrollArea` behind it, so
        // `Direct` inside a scrolling pane swallows every vertical gesture that
        // starts on it. Defaulting the other way makes a bank of controls into a
        // band the reader cannot scroll past.
        assert_eq!(Grab::default(), Grab::HoldToEngage);
    }
}
