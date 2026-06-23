//! `SlotTable` story — slot list with enable / required toggles + z-order.

use egui_widgets::slot_table::{SlotRow, SlotTable};
use egui_widgets::theme;

pub struct SlotTableState {
    pub rows: Vec<SlotRow>,
}

impl Default for SlotTableState {
    fn default() -> Self {
        let row = |name: &str, enabled: bool, required: bool, z: &str| SlotRow {
            name: name.into(),
            enabled,
            required,
            z_order: z.into(),
        };
        Self {
            rows: vec![
                row("01 - backgrounds", true, true, "1"),
                row("02 - skin", true, true, "2"),
                row("08 - eyes", true, true, "11:5"),
                row("11 - hair a", true, false, "11"),
                row("13 - hair b", true, false, "13"),
                row("14 - headwear", true, false, "14"),
                row("04 - hand", false, false, "4"),
            ],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut SlotTableState) {
    ui.label(
        egui::RichText::new("Slot Table")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The slot list with enable / required toggles + z-order — backs \
             disabled_traits, defaults.required, and z_index_overrides. Required \
             is disabled for disabled slots.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset").clicked() {
        *state = SlotTableState::default();
    }
    ui.add_space(8.0);

    SlotTable::new(&mut state.rows).show(ui);
}
