//! Storybook demo for the AmountInput widget.

use egui_widgets::amount_input::{self, AmountInputAction, AmountInputConfig, AmountInputState};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub struct AmountInputStoryState {
    pub default_state: AmountInputState,
    pub with_max_state: AmountInputState,
    pub last_action: String,
}

impl Default for AmountInputStoryState {
    fn default() -> Self {
        Self {
            default_state: AmountInputState::new(),
            with_max_state: AmountInputState::new(),
            last_action: "None".into(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut AmountInputStoryState) {
    ui.label(
        egui::RichText::new("AmountInput Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "ADA amount input with preset buttons, optional MAX button, and validation warnings.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(450.0, ui.available_height()), |ui| {
        // Default presets
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Default Presets (100, 250, 500 ADA)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let config = AmountInputConfig::default();
                let resp = amount_input::show(ui, &mut state.default_state, &config);
                match resp.action {
                    AmountInputAction::Changed(lovelace) => {
                        state.last_action = format!(
                            "Changed to {} ADA ({lovelace} lovelace)",
                            lovelace as f64 / 1_000_000.0
                        );
                    }
                    AmountInputAction::Cleared => {
                        state.last_action = "Cleared".into();
                    }
                    AmountInputAction::None => {}
                }
            });

        ui.add_space(12.0);

        // With MAX button and custom presets
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("With MAX Button (balance: 1,234 ADA)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let config = AmountInputConfig {
                    presets: vec![50, 200, 500, 1000],
                    max_ada: Some(1234.0),
                    min_ada: 10.0,
                    accent: egui_widgets::theme::ACCENT_CYAN,
                };
                let resp = amount_input::show(ui, &mut state.with_max_state, &config);
                match resp.action {
                    AmountInputAction::Changed(lovelace) => {
                        state.last_action = format!(
                            "Changed to {} ADA ({lovelace} lovelace)",
                            lovelace as f64 / 1_000_000.0
                        );
                    }
                    AmountInputAction::Cleared => {
                        state.last_action = "Cleared".into();
                    }
                    AmountInputAction::None => {}
                }
            });
    });

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(format!("Last action: {}", state.last_action))
            .color(TEXT_MUTED)
            .size(10.0),
    );

    ui.add_space(8.0);
    ui.label(egui::RichText::new("Tips:").color(ACCENT).strong());
    ui.label("\u{2022} Type a value below 5 ADA (default min) to see the warning");
    ui.label("\u{2022} Type non-numeric text to see the invalid input warning");
    ui.label("\u{2022} The second example has a MAX button and 10 ADA minimum");

    ui.add_space(8.0);
    if ui.button("Reset").clicked() {
        *state = AmountInputStoryState::default();
    }
}
