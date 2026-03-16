//! Storybook demo for the CoverageDeltaBar widget.

use egui_widgets::coverage_delta_bar::{self, CoverageDeltaConfig};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("CoverageDeltaBar Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Before/after coverage bar showing trait coverage change from a trade. \
             Green region = gain, red region = loss.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    let config = CoverageDeltaConfig::default();

    ui.allocate_ui(egui::vec2(400.0, ui.available_height()), |ui| {
        // Positive delta
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Positive Delta (+4%)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);
                coverage_delta_bar::show(ui, 0.67, 0.71, Some("Coverage"), &config);
            });

        ui.add_space(12.0);

        // Large positive delta
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Large Positive Delta (+15%)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);
                coverage_delta_bar::show(ui, 0.45, 0.60, Some("Coverage"), &config);
            });

        ui.add_space(12.0);

        // Negative delta
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Negative Delta (-5%)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);
                coverage_delta_bar::show(ui, 0.80, 0.75, Some("Coverage"), &config);
            });

        ui.add_space(12.0);

        // No change
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("No Change (0%)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);
                coverage_delta_bar::show(ui, 0.50, 0.50, Some("Coverage"), &config);
            });

        ui.add_space(12.0);

        // No label variant
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Without Label")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);
                coverage_delta_bar::show(ui, 0.30, 0.42, None, &config);
            });
    });
}
