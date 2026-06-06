//! `QuantityStepper` storybook story — clamping, sizing, and disabled edges.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::QuantityStepper;

/// Interactive state for the live steppers.
pub struct QuantityStepperStoryState {
    pub free: u32,
    pub capped: u32,
    pub big: u32,
}

impl Default for QuantityStepperStoryState {
    fn default() -> Self {
        Self {
            free: 1,
            capped: 3,
            big: 10,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut QuantityStepperStoryState) {
    ui.label(
        egui::RichText::new("QuantityStepper")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Compact −/[n]/+ control with min/max clamping. The caller owns the \
             value; show() returns the clamped value + whether it changed this \
             frame. Fully local — no async, no re-quote. − disables at min, + at max.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    section(ui, "Default — range 1..unbounded");
    let r = QuantityStepper::new(state.free).show(ui);
    if r.changed {
        state.free = r.value;
    }
    ui.add_space(12.0);

    section(ui, "Capped 1..5 — + disables at 5");
    let r = QuantityStepper::new(state.capped).range(1, 5).show(ui);
    if r.changed {
        state.capped = r.value;
    }
    ui.add_space(12.0);

    section(ui, "Larger buttons, blue accent — range 1..100");
    let r = QuantityStepper::new(state.big)
        .range(1, 100)
        .button_size(48.0)
        .accent(egui_widgets::theme::ACCENT_BLUE)
        .show(ui);
    if r.changed {
        state.big = r.value;
    }
    ui.add_space(12.0);

    section(ui, "At min — − disabled");
    let _ = QuantityStepper::new(1).range(1, 5).show(ui);
}

fn section(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).color(ACCENT).strong());
    ui.add_space(4.0);
}
