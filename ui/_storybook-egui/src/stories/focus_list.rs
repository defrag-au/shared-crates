//! Storybook demo for the FocusList widget from egui-widgets.

use egui_widgets::focus_list::{self, FocusListConfig};

use crate::{ACCENT, TEXT_MUTED};

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
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.len, 1..=60).text("items"));
        ui.add(egui::Slider::new(&mut state.visible_rows, 3..=15).text("visible rows"));
        state.focus = state.focus.min(state.len - 1);
        ui.add(egui::Slider::new(&mut state.focus, 0..=state.len - 1).text("focus"));
    });
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
                            ACCENT
                        } else {
                            egui::Color32::from_rgb(220, 220, 235)
                        })
                        .size(10.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(format!("Demo Asset #{:04}", pos * 37 % 10_000))
                        .color(TEXT_MUTED)
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
                    .color(TEXT_MUTED)
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
