//! `ChoiceStepper` molecule — `LABEL  [‹] value [›]`, for picking one of a list too long for a row of buttons.
//!
//! Stateless: the host owns the index and re-projects it; the widget reports
//! the index asked for. The arrows are [`Button`]s (tonal), so they share the
//! hover/press feel; the value sits in a rounded track between them, and the
//! whole row is one rect the host lays out. A list of fifty hairstyles steps
//! as well as a list of three, which a row of chips does not.

use macroquad::prelude::*;
use ui_theme::{TextRole, TextSize};

use crate::button::{Button, ButtonVariant};
use crate::painter::{Painter, draw_rounded_rect};

pub struct ChoiceStepperVm<'a> {
    /// What is being picked, drawn to the left.
    pub label: &'a str,
    /// The current choice, as the user should read it.
    pub value: &'a str,
    pub index: usize,
    pub count: usize,
    /// Whether stepping past either end comes round to the other.
    pub wrap: bool,
}

/// Draw the stepper filling `rect`: the label in its left `label_frac`, the
/// arrows and value in the rest. Returns the index asked for this frame, if
/// an arrow was tapped. `enabled = false` greys the arrows.
pub fn choice_stepper(
    p: &Painter,
    vm: &ChoiceStepperVm,
    rect: Rect,
    label_frac: f32,
    enabled: bool,
) -> Option<usize> {
    let label_size = p.role(TextRole::Label);
    p.text(
        vm.label,
        rect.x,
        p.centre_baseline(rect.y, rect.h, label_size),
        label_size,
        p.theme.color.text_muted,
    );

    let gap = 6.0;
    let bw = rect.h;
    let x = rect.x + rect.w * label_frac;
    let track_w = rect.right() - x - 2.0 * (bw + gap);
    let last = vm.count.saturating_sub(1);
    let can_prev = enabled && vm.count > 1 && (vm.wrap || vm.index > 0);
    let can_next = enabled && vm.count > 1 && (vm.wrap || vm.index < last);
    let mut asked = None;

    if Button::new("<")
        .variant(ButtonVariant::Tonal)
        .text_size(TextSize::Lg)
        .enabled(can_prev)
        .show(p, Rect::new(x, rect.y, bw, rect.h))
    {
        asked = Some(if vm.index == 0 { last } else { vm.index - 1 });
    }

    let vx = x + bw + gap;
    let radius = (rect.h * 0.22).min(10.0);
    draw_rounded_rect(
        vx,
        rect.y,
        track_w,
        rect.h,
        radius,
        p.theme.color.bg_highlight,
    );
    let value_size = p.size(TextSize::Md);
    let dim = p.measure(vm.value, value_size);
    p.text(
        vm.value,
        vx + ((track_w - dim.width) * 0.5).max(4.0),
        p.centre_baseline(rect.y, rect.h, value_size),
        value_size,
        p.theme.color.text_primary,
    );

    if Button::new(">")
        .variant(ButtonVariant::Tonal)
        .text_size(TextSize::Lg)
        .enabled(can_next)
        .show(p, Rect::new(vx + track_w + gap, rect.y, bw, rect.h))
    {
        asked = Some(if vm.index >= last { 0 } else { vm.index + 1 });
    }
    asked
}

#[cfg(test)]
mod tests {
    /// The stepping rule, without a window: the same arithmetic as the
    /// arrows, wrapping or clamping at the ends.
    fn step(index: usize, count: usize, forward: bool) -> usize {
        let last = count - 1;
        match forward {
            true if index >= last => 0,
            true => index + 1,
            false if index == 0 => last,
            false => index - 1,
        }
    }

    #[test]
    fn stepping_wraps_round_both_ends() {
        assert_eq!(step(0, 3, false), 2);
        assert_eq!(step(2, 3, true), 0);
        assert_eq!(step(1, 3, true), 2);
    }
}
