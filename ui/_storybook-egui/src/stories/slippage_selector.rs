//! Storybook demo for the SlippageSelector widget.

use egui_widgets::slippage_selector::{self, SlippageSelectorConfig, SlippageSelectorState};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub struct SlippageSelectorStoryState {
    pub default_state: SlippageSelectorState,
    pub custom_presets_state: SlippageSelectorState,
    pub last_action: String,
}

impl Default for SlippageSelectorStoryState {
    fn default() -> Self {
        Self {
            default_state: SlippageSelectorState::new(100),
            custom_presets_state: SlippageSelectorState::new(200),
            last_action: "None".into(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut SlippageSelectorStoryState) {
    ui.label(
        egui::RichText::new("SlippageSelector Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Preset buttons + custom input mode. Shows warnings for unusually high or low slippage.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(450.0, ui.available_height()), |ui| {
        // Default config
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Default Presets (0.5%, 1%, 3%)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let config = SlippageSelectorConfig::default();
                let action = slippage_selector::show(ui, &mut state.default_state, &config);
                if let slippage_selector::SlippageSelectorAction::Changed(bps) = action {
                    state.last_action =
                        format!("Changed to {bps} bps ({:.1}%)", bps as f64 / 100.0);
                }
            });

        ui.add_space(12.0);

        // Custom presets with different accent
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Custom Presets (0.1%, 0.5%, 2%, 5%)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let config = SlippageSelectorConfig {
                    presets: vec![
                        egui_widgets::SlippagePreset {
                            bps: 10,
                            label: "0.1%".into(),
                        },
                        egui_widgets::SlippagePreset {
                            bps: 50,
                            label: "0.5%".into(),
                        },
                        egui_widgets::SlippagePreset {
                            bps: 200,
                            label: "2%".into(),
                        },
                        egui_widgets::SlippagePreset {
                            bps: 500,
                            label: "5%".into(),
                        },
                    ],
                    accent: egui_widgets::theme::ACCENT_CYAN,
                    ..Default::default()
                };
                let action = slippage_selector::show(ui, &mut state.custom_presets_state, &config);
                if let slippage_selector::SlippageSelectorAction::Changed(bps) = action {
                    state.last_action =
                        format!("Changed to {bps} bps ({:.1}%)", bps as f64 / 100.0);
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
    ui.label("\u{2022} Click Custom then type a value to see custom input mode");
    ui.label("\u{2022} Try very low (<0.3%) or very high (>5%) values for warnings");

    ui.add_space(8.0);
    if ui.button("Reset").clicked() {
        *state = SlippageSelectorStoryState::default();
    }
}
