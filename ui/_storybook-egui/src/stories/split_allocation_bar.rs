//! Storybook demo for the SplitAllocationBar widget.

use egui_widgets::split_allocation_bar::{self, AllocationSegment, SplitAllocationBarConfig};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("SplitAllocationBar Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Segmented horizontal bar showing ADA allocation across DEXes. \
             Hover for tooltips, percentage labels inside wide segments.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(450.0, ui.available_height()), |ui| {
        // Two-way split (typical: 78/22)
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Two-Way Split (78/22)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let segments = vec![
                    AllocationSegment {
                        label: "Splash".into(),
                        amount_lovelace: 780_000_000,
                        fraction: 0.78,
                        color: split_allocation_bar::dex_color(0),
                    },
                    AllocationSegment {
                        label: "CSWAP".into(),
                        amount_lovelace: 220_000_000,
                        fraction: 0.22,
                        color: split_allocation_bar::dex_color(1),
                    },
                ];
                split_allocation_bar::show(ui, &segments, &SplitAllocationBarConfig::default());
            });

        ui.add_space(12.0);

        // Three-way split
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Three-Way Split (55/30/15)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let segments = vec![
                    AllocationSegment {
                        label: "Splash".into(),
                        amount_lovelace: 550_000_000,
                        fraction: 0.55,
                        color: split_allocation_bar::dex_color(0),
                    },
                    AllocationSegment {
                        label: "Minswap".into(),
                        amount_lovelace: 300_000_000,
                        fraction: 0.30,
                        color: split_allocation_bar::dex_color(1),
                    },
                    AllocationSegment {
                        label: "CSWAP".into(),
                        amount_lovelace: 150_000_000,
                        fraction: 0.15,
                        color: split_allocation_bar::dex_color(2),
                    },
                ];
                split_allocation_bar::show(ui, &segments, &SplitAllocationBarConfig::default());
            });

        ui.add_space(12.0);

        // Single pool (100%)
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Single Pool (100%)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let segments = vec![AllocationSegment {
                    label: "Splash".into(),
                    amount_lovelace: 1_000_000_000,
                    fraction: 1.0,
                    color: split_allocation_bar::dex_color(0),
                }];
                split_allocation_bar::show(ui, &segments, &SplitAllocationBarConfig::default());
            });

        ui.add_space(12.0);

        // No legend variant
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Without Legend")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let segments = vec![
                    AllocationSegment {
                        label: "Splash".into(),
                        amount_lovelace: 600_000_000,
                        fraction: 0.60,
                        color: split_allocation_bar::dex_color(0),
                    },
                    AllocationSegment {
                        label: "CSWAP".into(),
                        amount_lovelace: 400_000_000,
                        fraction: 0.40,
                        color: split_allocation_bar::dex_color(1),
                    },
                ];
                let config = SplitAllocationBarConfig {
                    show_legend: false,
                    bar_height: 28.0,
                    ..Default::default()
                };
                split_allocation_bar::show(ui, &segments, &config);
            });
    });
}
