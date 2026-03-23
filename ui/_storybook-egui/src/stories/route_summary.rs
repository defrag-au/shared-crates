//! Storybook demo for the RouteSummary widget.

use egui_widgets::route_summary::{self, RouteLeg, RouteSummaryConfig, RouteSummaryData};
use egui_widgets::split_allocation_bar::dex_color;

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("RouteSummary Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Compact split routing result display. Per-leg breakdown with totals \
             and improvement percentage vs best single pool.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(400.0, ui.available_height()), |ui| {
        // Two-way split — Aliens at 1000 ADA
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Aliens \u{2014} 1000 ADA Split (78/22)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let data = RouteSummaryData {
                    legs: vec![
                        RouteLeg {
                            dex_label: "Splash".into(),
                            color: dex_color(0),
                            input_lovelace: 780_000_000,
                            expected_tokens: 6_912_743,
                            price_per_token: 0.000113,
                        },
                        RouteLeg {
                            dex_label: "CSWAP".into(),
                            color: dex_color(1),
                            input_lovelace: 220_000_000,
                            expected_tokens: 1_949_748,
                            price_per_token: 0.000113,
                        },
                    ],
                    total_tokens: 8_862_491,
                    token_name: "Aliens".into(),
                    best_single_pool_tokens: Some(8_784_200),
                    blended_price: 0.000113,
                };
                route_summary::show(ui, &data, &RouteSummaryConfig::default());
            });

        ui.add_space(12.0);

        // Three-way split — hypothetical
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("PERP COIN \u{2014} 2000 ADA Three-Way Split")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let data = RouteSummaryData {
                    legs: vec![
                        RouteLeg {
                            dex_label: "Splash".into(),
                            color: dex_color(0),
                            input_lovelace: 1_100_000_000,
                            expected_tokens: 6_500_000,
                            price_per_token: 0.000169,
                        },
                        RouteLeg {
                            dex_label: "Minswap".into(),
                            color: dex_color(1),
                            input_lovelace: 600_000_000,
                            expected_tokens: 3_450_000,
                            price_per_token: 0.000174,
                        },
                        RouteLeg {
                            dex_label: "CSWAP".into(),
                            color: dex_color(2),
                            input_lovelace: 300_000_000,
                            expected_tokens: 1_680_000,
                            price_per_token: 0.000179,
                        },
                    ],
                    total_tokens: 11_630_000,
                    token_name: "PERP COIN".into(),
                    best_single_pool_tokens: Some(11_400_000),
                    blended_price: 0.000172,
                };
                route_summary::show(ui, &data, &RouteSummaryConfig::default());
            });

        ui.add_space(12.0);

        // Single pool — no split advantage
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Single Pool (no split advantage)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let data = RouteSummaryData {
                    legs: vec![RouteLeg {
                        dex_label: "Splash".into(),
                        color: dex_color(0),
                        input_lovelace: 100_000_000,
                        expected_tokens: 892_000,
                        price_per_token: 0.000112,
                    }],
                    total_tokens: 892_000,
                    token_name: "Aliens".into(),
                    best_single_pool_tokens: None,
                    blended_price: 0.000112,
                };
                route_summary::show(ui, &data, &RouteSummaryConfig::default());
            });
    });
}
