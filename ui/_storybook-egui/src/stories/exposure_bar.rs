//! Storybook demo for the ExposureBar widget.

use egui_widgets::exposure_bar::{self, ExposureBarConfig, ExposureSegment};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("ExposureBar Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Stacked horizontal bar showing total ADA exposure by collateral token, \
             colored by LTV risk. Green < 50%, amber < 80%, red >= 80%.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(500.0, ui.available_height()), |ui| {
        // ── Multi-token mixed risk ──
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Multi-Token Mixed Risk")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let total: u64 = 3_355_000_000 + 2_500_000_000 + 2_000_000_000 + 1_500_000_000;
                let segments = vec![
                    ExposureSegment {
                        label: "NIGHT".into(),
                        principal_lovelace: 3_355_000_000,
                        fraction: 3_355_000_000.0 / total as f32,
                        ltv_pct: 89.8,
                        color: egui_widgets::theme::ACCENT_MAGENTA,
                    },
                    ExposureSegment {
                        label: "SNEK".into(),
                        principal_lovelace: 2_500_000_000,
                        fraction: 2_500_000_000.0 / total as f32,
                        ltv_pct: 45.2,
                        color: egui_widgets::theme::ACCENT_GREEN,
                    },
                    ExposureSegment {
                        label: "ANGELS".into(),
                        principal_lovelace: 2_000_000_000,
                        fraction: 2_000_000_000.0 / total as f32,
                        ltv_pct: 72.1,
                        color: egui_widgets::theme::ACCENT_CYAN,
                    },
                    ExposureSegment {
                        label: "HOSKY".into(),
                        principal_lovelace: 1_500_000_000,
                        fraction: 1_500_000_000.0 / total as f32,
                        ltv_pct: 35.0,
                        color: egui_widgets::theme::ACCENT_YELLOW,
                    },
                ];
                exposure_bar::show(ui, &segments, &ExposureBarConfig::default());
            });

        ui.add_space(12.0);

        // ── All green (safe portfolio) ──
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("All Green (Well-Collateralised)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let segments = vec![
                    ExposureSegment {
                        label: "WMT".into(),
                        principal_lovelace: 5_000_000_000,
                        fraction: 0.6,
                        ltv_pct: 32.0,
                        color: egui_widgets::theme::ACCENT_BLUE,
                    },
                    ExposureSegment {
                        label: "LENFI".into(),
                        principal_lovelace: 3_333_000_000,
                        fraction: 0.4,
                        ltv_pct: 41.5,
                        color: egui_widgets::theme::ACCENT_ORANGE,
                    },
                ];
                exposure_bar::show(ui, &segments, &ExposureBarConfig::default());
            });

        ui.add_space(12.0);

        // ── Single token high risk ──
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Single Token High Risk")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let segments = vec![ExposureSegment {
                    label: "NIGHT".into(),
                    principal_lovelace: 6_000_000_000,
                    fraction: 1.0,
                    ltv_pct: 92.3,
                    color: egui_widgets::theme::ACCENT_MAGENTA,
                }];
                exposure_bar::show(ui, &segments, &ExposureBarConfig::default());
            });

        ui.add_space(12.0);

        // ── Compact (no legend/total) ──
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Compact (No Legend / No Total)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let total: u64 = 3_000_000_000 + 2_000_000_000;
                let segments = vec![
                    ExposureSegment {
                        label: "SNEK".into(),
                        principal_lovelace: 3_000_000_000,
                        fraction: 3_000_000_000.0 / total as f32,
                        ltv_pct: 55.0,
                        color: egui_widgets::theme::ACCENT_GREEN,
                    },
                    ExposureSegment {
                        label: "NIGHT".into(),
                        principal_lovelace: 2_000_000_000,
                        fraction: 2_000_000_000.0 / total as f32,
                        ltv_pct: 85.0,
                        color: egui_widgets::theme::ACCENT_MAGENTA,
                    },
                ];
                let config = ExposureBarConfig {
                    show_legend: false,
                    show_total: false,
                    bar_height: 28.0,
                    ..Default::default()
                };
                exposure_bar::show(ui, &segments, &config);
            });
    });
}
