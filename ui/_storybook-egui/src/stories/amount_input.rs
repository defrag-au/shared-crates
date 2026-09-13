//! Storybook demo for the AmountInput widget.

use egui_widgets::amount_input::{self, AmountInputAction, AmountInputConfig, AmountInputState};
use egui_widgets::theme::Token;

use crate::{accent, bg, muted};

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
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "ADA amount input with preset buttons, optional MAX button, and validation warnings.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(450.0, ui.available_height()), |ui| {
        // Default presets
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(crate::highlight(ui)))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Default Presets (100, 250, 500 ADA)")
                        .color(crate::secondary(ui))
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
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(crate::highlight(ui)))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("With MAX Button (balance: 1,234 ADA)")
                        .color(crate::secondary(ui))
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let config = AmountInputConfig {
                    presets: vec![50, 200, 500, 1000],
                    max_ada: Some(1234.0),
                    min_ada: 10.0,
                    // Overriding the accent is the point of this story — the
                    // story beside it shows the default. Naming the token is
                    // enough now; this used to need a `ui` in hand just to
                    // read the value back out of the theme.
                    accent: Token::AccentCyan.into(),
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
            .color(muted(ui))
            .size(10.0),
    );

    ui.add_space(8.0);
    ui.label(egui::RichText::new("Tips:").color(accent(ui)).strong());
    ui.label("\u{2022} Type a value below 5 ADA (default min) to see the warning");
    ui.label("\u{2022} Type non-numeric text to see the invalid input warning");
    ui.label("\u{2022} The second example has a MAX button and 10 ADA minimum");

    ui.add_space(8.0);
    if ui.button("Reset").clicked() {
        *state = AmountInputStoryState::default();
    }
}
