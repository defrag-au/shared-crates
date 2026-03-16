//! Storybook demo for the TraitDelta widget.

use egui_widgets::trait_delta::{self, TraitDeltaConfig, TraitItem};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("TraitDelta Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Trait chips showing what changes hands in a trade. \
             Green (+) for gains, red (-) for losses. No labels, just data.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    let config = TraitDeltaConfig::default();

    // Example 1: Both gains and losses
    ui.allocate_ui(egui::vec2(400.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Balanced Trade")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let gains = vec![
                    TraitItem::new("Background", "Purple"),
                    TraitItem::new("Hat", "Crown"),
                    TraitItem::new("Eyes", "Diamond"),
                ];
                let losses = vec![
                    TraitItem::new("Eyes", "Laser"),
                    TraitItem::new("Mouth", "Grin"),
                ];
                trait_delta::show(ui, &gains, &losses, &config);
            });
    });

    ui.add_space(16.0);

    // Example 2: Only gains
    ui.allocate_ui(egui::vec2(400.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Pure Gain (filling gaps)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let gains = vec![
                    TraitItem::new("Skin", "Gold"),
                    TraitItem::new("Accessory", "Monocle"),
                    TraitItem::new("Background", "Galaxy"),
                    TraitItem::new("Outfit", "Suit"),
                    TraitItem::new("Pet", "Dragon"),
                ];
                trait_delta::show(ui, &gains, &[], &config);
            });
    });

    ui.add_space(16.0);

    // Example 3: Only losses
    ui.allocate_ui(egui::vec2(400.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Pure Loss (trading away)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let losses = vec![
                    TraitItem::new("Hat", "Crown"),
                    TraitItem::new("Eyes", "Laser"),
                ];
                trait_delta::show(ui, &[], &losses, &config);
            });
    });
}
