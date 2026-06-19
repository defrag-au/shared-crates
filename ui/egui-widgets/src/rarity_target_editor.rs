//! rarity_target_editor — a labelled list of 0–100% target sliders with an
//! optional budget indicator (running total vs a budget, coloured over/under/ok).
//! For per-trait None% and per-value rarity targets in the config editor.
//!
//! Mutates the rows in place; returns `true` when a value changed.

use egui::{Color32, Ui};

#[derive(Default, Debug, Clone)]
pub struct RarityRow {
    pub label: String,
    pub percent: f32,
}

pub struct RarityTargetEditor<'a> {
    rows: &'a mut [RarityRow],
    /// If set, show the running total against this budget (e.g. 100.0) with an
    /// over/under/ok colour cue.
    budget: Option<f32>,
    label_width: f32,
}

impl<'a> RarityTargetEditor<'a> {
    pub fn new(rows: &'a mut [RarityRow]) -> Self {
        Self {
            rows,
            budget: None,
            label_width: 140.0,
        }
    }

    /// Show a running-total-vs-budget cue under the sliders.
    pub fn budget(mut self, budget: f32) -> Self {
        self.budget = Some(budget);
        self
    }

    pub fn label_width(mut self, w: f32) -> Self {
        self.label_width = w;
        self
    }

    pub fn show(self, ui: &mut Ui) -> bool {
        let mut changed = false;
        for row in self.rows.iter_mut() {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [self.label_width, ui.spacing().interact_size.y],
                    egui::Label::new(row.label.as_str()).truncate(),
                );
                if ui
                    .add(egui::Slider::new(&mut row.percent, 0.0..=100.0).suffix("%"))
                    .changed()
                {
                    changed = true;
                }
            });
        }

        if let Some(budget) = self.budget {
            let sum: f32 = self.rows.iter().map(|r| r.percent).sum();
            let color = if sum > budget + 0.05 {
                Color32::from_rgb(230, 120, 90) // over
            } else if sum < budget - 0.05 {
                Color32::from_rgb(225, 185, 90) // under
            } else {
                Color32::from_rgb(120, 200, 120) // ok
            };
            let note = if sum > budget + 0.05 {
                "over budget"
            } else if sum < budget - 0.05 {
                "under budget"
            } else {
                "balanced"
            };
            ui.add_space(2.0);
            ui.colored_label(color, format!("{sum:.0}% / {budget:.0}% — {note}"));
        }

        changed
    }
}
