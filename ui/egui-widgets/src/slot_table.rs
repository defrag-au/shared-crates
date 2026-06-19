//! slot_table — the trait/slot list with enable / required toggles and an
//! optional z-order field. Backs `disabled_traits`, `defaults.required`, and
//! `order.z_index_overrides` in the config editor.
//!
//! Mutates the rows in place; returns `true` when a toggle/value changed.

use egui::Ui;

#[derive(Default, Debug, Clone)]
pub struct SlotRow {
    pub name: String,
    pub enabled: bool,
    pub required: bool,
    /// Z-index (string, e.g. "11" or "11:5"); shown only when z editing is on.
    pub z_order: String,
}

pub struct SlotTable<'a> {
    rows: &'a mut [SlotRow],
    show_z: bool,
}

impl<'a> SlotTable<'a> {
    pub fn new(rows: &'a mut [SlotRow]) -> Self {
        Self {
            rows,
            show_z: true,
        }
    }

    /// Show the z-order column (default true).
    pub fn show_z(mut self, show: bool) -> Self {
        self.show_z = show;
        self
    }

    pub fn show(self, ui: &mut Ui) -> bool {
        let Self { rows, show_z } = self;
        let mut changed = false;

        let num_columns = if show_z { 4 } else { 3 };
        egui::Grid::new("slot_table")
            .num_columns(num_columns)
            .striped(true)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Slot").strong());
                ui.label(egui::RichText::new("Enabled").strong());
                ui.label(egui::RichText::new("Required").strong());
                if show_z {
                    ui.label(egui::RichText::new("Z").strong());
                }
                ui.end_row();

                for row in rows.iter_mut() {
                    // Disabled slots read dimmed; required is moot when disabled.
                    let label = if row.enabled {
                        egui::RichText::new(row.name.as_str())
                    } else {
                        egui::RichText::new(row.name.as_str()).weak()
                    };
                    ui.label(label);
                    if ui.checkbox(&mut row.enabled, "").changed() {
                        changed = true;
                    }
                    let req = ui.add_enabled(
                        row.enabled,
                        egui::Checkbox::new(&mut row.required, ""),
                    );
                    if req.changed() {
                        changed = true;
                    }
                    if show_z
                        && ui
                            .add(
                                egui::TextEdit::singleline(&mut row.z_order)
                                    .desired_width(54.0),
                            )
                            .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();
                }
            });

        changed
    }
}
