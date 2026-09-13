//! Storybook demo for the FocusList widget from egui-widgets.

use egui_widgets::focus_list::{self, FocusListConfig};
use egui_widgets::slider_group::SliderGroup;

use crate::{accent, muted};

pub struct FocusListState {
    pub focus: usize,
    pub len: usize,
    pub visible_rows: usize,
}

impl Default for FocusListState {
    fn default() -> Self {
        Self {
            focus: 3,
            len: 24,
            visible_rows: 7,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut FocusListState) {
    // `focus`'s range depends on `len`, and a bank holds a mutable borrow of
    // every field it drives at once — so the range is read before the bank and
    // re-clamped after it, rather than mid-row. One frame of a stale upper bound
    // is invisible; a focus index past the end of the list is not, hence the
    // clamp on both sides.
    state.focus = state.focus.min(state.len - 1);
    let max_focus = state.len - 1;
    crate::controls(ui, |ui| {
        SliderGroup::new()
            .slider("items", &mut state.len, 1..=60)
            .slider("visible rows", &mut state.visible_rows, 3..=15)
            .slider("focus", &mut state.focus, 0..=max_focus)
            .show(ui);
    });
    state.focus = state.focus.min(state.len - 1);
    ui.add_space(8.0);

    egui::Frame::popup(ui.style()).show(ui, |ui| {
        focus_list::show(
            ui,
            state.len,
            state.focus,
            &FocusListConfig {
                visible_rows: state.visible_rows,
                ..Default::default()
            },
            |ui, pos, focused| {
                let value = 30 + (pos * 61) % 400;
                ui.label(
                    egui::RichText::new(format!("{value} ADA"))
                        .color(if focused {
                            accent(ui)
                        } else {
                            egui_widgets::theme::ThemeExt::tokens(ui).color.text_primary
                        })
                        .size(10.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(format!("Demo Asset #{:04}", pos * 37 % 10_000))
                        .color(muted(ui))
                        .size(9.0),
                );
            },
            |ui, pos| {
                ui.label(
                    egui::RichText::new(format!(
                        "Demo Asset #{:04} \u{2014} detail pane",
                        pos * 37 % 10_000
                    ))
                    .color(egui::Color32::from_rgb(220, 220, 235))
                    .size(11.0)
                    .strong(),
                );
                ui.label(
                    egui::RichText::new(
                        "The list above never reflows as the focus moves \u{2014} \
                         only the highlight slides and this pane swaps.",
                    )
                    .color(muted(ui))
                    .size(9.0),
                );
            },
        );
    });

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(
            "Fixed-geometry master-detail for constrained surfaces (chart tooltips): \
             windowed compact rows with reserved \u{2026}above/below marker slots, a sliding \
             highlight, and a detail pane. Drive `focus` from scroll (see Price Timeline) \
             or any other input.",
        )
        .color(egui::Color32::from_rgb(220, 220, 235))
        .small(),
    );
}
