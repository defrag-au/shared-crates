//! `QuantityStepper` molecule — `[−] [ n ] [+]`, clamped to `[min, max]`.
//!
//! Stateless: the host owns `qty` and re-projects it; the widget reports the
//! requested new value. The `−`/`+` are [`Button`]s (tonal), so they inherit the
//! hover/press feel; the value sits in a rounded track between them.

use macroquad::prelude::*;

use crate::button::{Button, ButtonVariant};
use crate::painter::{draw_rounded_rect, Painter};

pub struct QuantityStepperVm {
    pub qty: u32,
    pub min: u32,
    pub max: u32,
}

pub enum StepperAction {
    Changed(u32),
}

pub struct StepperResponse {
    /// Total width drawn — hosts lay out to the right of this.
    pub width: f32,
    pub action: Option<StepperAction>,
}

/// Draw the stepper at top-left `(x, y)` with row height `h`. `enabled = false`
/// greys both buttons (e.g. while a mint is in flight).
pub fn quantity_stepper(
    p: &Painter,
    vm: &QuantityStepperVm,
    x: f32,
    y: f32,
    h: f32,
    enabled: bool,
) -> StepperResponse {
    let gap = 6.0;
    let bw = h; // square +/- cells
    let val_w = 54.0;
    let radius = (h * 0.22).min(10.0);
    let mut action = None;

    if Button::new("-")
        .variant(ButtonVariant::Tonal)
        .font_size(h * 0.55)
        .enabled(enabled && vm.qty > vm.min)
        .show(p, Rect::new(x, y, bw, h))
    {
        action = Some(StepperAction::Changed(vm.qty.saturating_sub(1).max(vm.min)));
    }

    let vx = x + bw + gap;
    draw_rounded_rect(vx, y, val_w, h, radius, p.theme.track);
    let s = vm.qty.to_string();
    let dim = p.measure(&s, 16.0);
    let baseline = p.centre_baseline(y, h, 16.0);
    p.text(&s, vx + (val_w - dim.width) * 0.5, baseline, 16.0, p.theme.fg);

    let px = vx + val_w + gap;
    if Button::new("+")
        .variant(ButtonVariant::Tonal)
        .font_size(h * 0.55)
        .enabled(enabled && vm.qty < vm.max)
        .show(p, Rect::new(px, y, bw, h))
    {
        action = Some(StepperAction::Changed((vm.qty + 1).min(vm.max)));
    }

    StepperResponse {
        width: px + bw - x,
        action,
    }
}
