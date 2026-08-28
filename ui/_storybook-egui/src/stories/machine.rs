//! `Machine` story — the save lifecycle as one state, no flag trio.

use egui_widgets::machine::Machine;
use egui_widgets::theme;

#[derive(Debug)]
pub enum DemoSave {
    Clean,
    Dirty,
    Saving { op: u64 },
    Saved,
}

pub struct MachineState {
    pub save: Machine<DemoSave>,
    pub next_op: u64,
}

impl Default for MachineState {
    fn default() -> Self {
        Self {
            save: Machine::new(DemoSave::Clean),
            next_op: 1,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut MachineState) {
    ui.label(egui::RichText::new("Machine").color(theme::ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Plain-enum UI state with entry-frame detection and frame-TTL \
             auto-revert — replaces the dirty/pending/flash boolean trio with \
             one matchable state. Tick once per frame, after rendering.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // Drive the lifecycle. "ack"/"fail" stand in for the async response.
    ui.horizontal(|ui| {
        if ui.button("edit").clicked() {
            state.save.transition(DemoSave::Dirty);
        }
        let can_send = matches!(state.save.get(), DemoSave::Dirty);
        if ui
            .add_enabled(can_send, egui::Button::new("save"))
            .clicked()
        {
            let op = state.next_op;
            state.next_op += 1;
            state.save.transition(DemoSave::Saving { op });
        }
        let in_flight = matches!(state.save.get(), DemoSave::Saving { .. });
        if ui
            .add_enabled(in_flight, egui::Button::new("ack"))
            .clicked()
        {
            // ~2s flash, then back to Clean by itself.
            state
                .save
                .transition_for(DemoSave::Saved, 120, DemoSave::Clean);
        }
        if ui
            .add_enabled(in_flight, egui::Button::new("fail"))
            .clicked()
        {
            state.save.transition(DemoSave::Dirty);
        }
    });
    ui.add_space(12.0);

    let (label, color) = match state.save.get() {
        DemoSave::Clean => ("Clean".to_string(), theme::TEXT_MUTED),
        DemoSave::Dirty => ("Dirty — unsaved edits".to_string(), theme::ACCENT_YELLOW),
        DemoSave::Saving { op } => (format!("Saving op {op}…"), theme::ACCENT_CYAN),
        DemoSave::Saved => ("Saved ✓ (auto-reverts)".to_string(), theme::SUCCESS),
    };
    ui.label(egui::RichText::new(label).color(color).strong());
    ui.label(
        egui::RichText::new(format!(
            "frames_in_state: {} · entered(): {}",
            state.save.frames_in_state(),
            state.save.entered()
        ))
        .color(theme::TEXT_SECONDARY)
        .small(),
    );

    // The convention: end-of-frame tick, and keep frames flowing so the
    // flash countdown is visible without input.
    state.save.tick();
    ui.ctx().request_repaint();
}
