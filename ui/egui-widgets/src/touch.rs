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

/// Advance the gate, and report how far through the hold the finger is.
///
/// Call **after** the control has been laid out, passing whether the pointer is
/// currently pressed on it. `0.0` means no hold is in progress — either nothing
/// is pressed, the finger has wandered past [`HOLD_SLOP`], or the control is
/// already engaged.
pub fn advance(ui: &Ui, id: Id, down_on_target: bool, engaged_now: bool) -> f32 {
    if !down_on_target {
        // Released — the next gesture starts from scratch.
        if engaged_now {
            ui.data_mut(|d| d.insert_temp(id, false));
        }
        return 0.0;
    }
    if engaged_now {
        return 0.0;
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
        return 0.0;
    }
    if held >= HOLD {
        ui.data_mut(|d| d.insert_temp(id, true));
    }
    // Nothing else is moving during a still hold, so without this the pass that
    // would complete it never runs.
    ui.ctx().request_repaint();
    (held / HOLD).clamp(0.0, 1.0) as f32
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
    fn hold_to_engage_is_the_default_because_the_other_way_traps_the_page() {
        // A control that senses drag beats the `ScrollArea` behind it, so
        // `Direct` inside a scrolling pane swallows every vertical gesture that
        // starts on it. Defaulting the other way makes a bank of controls into a
        // band the reader cannot scroll past.
        assert_eq!(Grab::default(), Grab::HoldToEngage);
    }
}
