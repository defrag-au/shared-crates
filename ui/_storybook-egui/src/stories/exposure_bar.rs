//! Storybook demo for the ExposureBar widget.

use egui_widgets::exposure_bar::{self, ExposureBarConfig, ExposureSegment};

use crate::{accent, bg, muted};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("ExposureBar Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Stacked horizontal bar showing total ADA exposure by collateral token, \
             colored by LTV risk. Green < 50%, amber < 80%, red >= 80%.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(500.0, ui.available_height()), |ui| {
        // ── Multi-token mixed risk ──
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(
                crate::highlight(ui),
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Multi-Token Mixed Risk")
                        .color(crate::secondary(ui))
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
                        color: crate::tok(ui, egui_widgets::theme::Token::AccentMagenta),
                    },
                    ExposureSegment {
                        label: "SNEK".into(),
                        principal_lovelace: 2_500_000_000,
                        fraction: 2_500_000_000.0 / total as f32,
                        ltv_pct: 45.2,
                        color: crate::tok(ui, egui_widgets::theme::Token::AccentGreen),
                    },
                    ExposureSegment {
                        label: "ANGELS".into(),
                        principal_lovelace: 2_000_000_000,
                        fraction: 2_000_000_000.0 / total as f32,
                        ltv_pct: 72.1,
                        color: crate::tok(ui, egui_widgets::theme::Token::AccentCyan),
                    },
                    ExposureSegment {
                        label: "HOSKY".into(),
                        principal_lovelace: 1_500_000_000,
                        fraction: 1_500_000_000.0 / total as f32,
                        ltv_pct: 35.0,
                        color: crate::tok(ui, egui_widgets::theme::Token::AccentYellow),
                    },
                ];
                exposure_bar::show(ui, &segments, &ExposureBarConfig::default());
            });

        ui.add_space(12.0);

        // ── All green (safe portfolio) ──
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(
                crate::highlight(ui),
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("All Green (Well-Collateralised)")
                        .color(crate::secondary(ui))
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
                        color: crate::tok(ui, egui_widgets::theme::Token::AccentBlue),
                    },
                    ExposureSegment {
                        label: "LENFI".into(),
                        principal_lovelace: 3_333_000_000,
                        fraction: 0.4,
                        ltv_pct: 41.5,
                        color: crate::tok(ui, egui_widgets::theme::Token::AccentOrange),
                    },
                ];
                exposure_bar::show(ui, &segments, &ExposureBarConfig::default());
            });

        ui.add_space(12.0);

        // ── Single token high risk ──
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(
                crate::highlight(ui),
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Single Token High Risk")
                        .color(crate::secondary(ui))
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let segments = vec![ExposureSegment {
                    label: "NIGHT".into(),
                    principal_lovelace: 6_000_000_000,
                    fraction: 1.0,
                    ltv_pct: 92.3,
                    color: crate::tok(ui, egui_widgets::theme::Token::AccentMagenta),
                }];
                exposure_bar::show(ui, &segments, &ExposureBarConfig::default());
            });

        ui.add_space(12.0);

        // ── Compact (no legend/total) ──
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(
                crate::highlight(ui),
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Compact (No Legend / No Total)")
                        .color(crate::secondary(ui))
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
                        color: crate::tok(ui, egui_widgets::theme::Token::AccentGreen),
                    },
                    ExposureSegment {
                        label: "NIGHT".into(),
                        principal_lovelace: 2_000_000_000,
                        fraction: 2_000_000_000.0 / total as f32,
                        ltv_pct: 85.0,
                        color: crate::tok(ui, egui_widgets::theme::Token::AccentMagenta),
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
