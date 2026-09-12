//! rarity_target_editor — per-trait / per-value 0–100% rarity targets.
//!
//! A thin configuration of [`SliderGroup`](crate::slider_group::SliderGroup):
//! every row is a percentage, and the budget is however much of the trait is
//! being allocated (usually 100).
//!
//! It used to be its own hand-rolled stack of `egui::Slider`s with a hardcoded
//! `label_width: 140.0` and three literal colours for the over/under/balanced
//! cue. All three of those were the general problem in local disguise — the
//! labels did not line up, the width clipped under a larger type ramp, and the
//! cue could not follow a theme. What is left here is the only thing that was
//! ever specific to rarity: the rows are percentages.
//!
//! Mutates the rows in place; returns `true` when a value changed.

use egui::Ui;

use crate::slider_group::{Budget, Fader, SliderGroup};

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
    label_width: Option<f32>,
}

impl<'a> RarityTargetEditor<'a> {
    pub fn new(rows: &'a mut [RarityRow]) -> Self {
        Self {
            rows,
            budget: None,
            label_width: None,
        }
    }

    /// Show a running-total-vs-budget cue under the sliders.
    pub fn budget(mut self, budget: f32) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Pin the label column. Left alone it is measured from the labels, which
    /// is what the old flat `140.0` could not do.
    pub fn label_width(mut self, w: f32) -> Self {
        self.label_width = Some(w);
        self
    }

    pub fn show(self, ui: &mut Ui) -> bool {
        let mut group = self.rows.iter_mut().fold(SliderGroup::new(), |g, row| {
            g.fader(Fader::new(row.label.clone(), &mut row.percent, 0.0..=100.0).suffix("%"))
        });
        if let Some(w) = self.label_width {
            group = group.label_width(w);
        }
        if let Some(b) = self.budget {
            group = group.budget(Budget::new(b as f64));
        }
        group.show(ui).changed
    }
}
