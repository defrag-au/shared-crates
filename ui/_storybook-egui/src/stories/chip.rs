//! `Chip` storybook story — every variant + the removable affordance.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{Chip, ChipVariant};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Chip").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Small filled-tag label with optional × remove. Semantic variants pick a palette \
             so the call site says what the chip means rather than which colour to use.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── Every variant in one row ───────────────────────────────────────
    ui.label(egui::RichText::new("Variants").color(ACCENT).strong());
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        Chip::new("active").variant(ChipVariant::Success).show(ui);
        Chip::new("paused").variant(ChipVariant::Muted).show(ui);
        Chip::new("archived").variant(ChipVariant::Warning).show(ui);
        Chip::new("failed").variant(ChipVariant::Danger).show(ui);
        Chip::new("public").variant(ChipVariant::Tag).show(ui);
        Chip::new("preprod").variant(ChipVariant::Info).show(ui);
    });

    ui.add_space(16.0);

    // ── Removable (gate chip use case) ─────────────────────────────────
    ui.label(
        egui::RichText::new("Removable (gate chips)")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Removable chips emit `removed = true` on the × click. \
             The host drops the row from its model.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        Chip::new("public")
            .variant(ChipVariant::Tag)
            .removable(true)
            .show(ui);
        Chip::new("allowlist")
            .variant(ChipVariant::Tag)
            .removable(true)
            .show(ui);
        Chip::new("token_held(8532f316…, min 3)")
            .variant(ChipVariant::Tag)
            .removable(true)
            .show(ui);
    });

    ui.add_space(16.0);

    // ── Tooltip + upper-case ───────────────────────────────────────────
    ui.label(
        egui::RichText::new("Tooltip + upper-case")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "`upper_case(true)` matches the old `status_chip` rendering. \
             `on_hover_text` attaches a tooltip — hover the chip below.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        Chip::new("draft")
            .variant(ChipVariant::Muted)
            .upper_case(true)
            .on_hover_text("Status: draft — not minting yet")
            .show(ui);
        Chip::new("live")
            .variant(ChipVariant::Success)
            .upper_case(true)
            .on_hover_text("Status: live — actively minting")
            .show(ui);
        Chip::new("sold_out")
            .variant(ChipVariant::Info)
            .upper_case(true)
            .on_hover_text("Status: sold out — inventory exhausted")
            .show(ui);
    });
}
