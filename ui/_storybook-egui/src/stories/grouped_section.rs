use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::grouped_section::{GroupedSection, GroupedSectionAction};
use egui_widgets::theme;

/// 64×64 placeholder hero baked into the storybook binary so the
/// section visualises with a real image without needing network
/// access. Production callers pass a URL via `hero_url`.
const PLACEHOLDER_HERO: egui::ImageSource<'_> =
    egui::include_image!("../../assets/placeholders/section_hero_64.png");

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Grouped Section")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Header pattern (hero icon + title + verified badge + bulk-action button) \
             with caller-rendered body. Common shape for any view that groups items by \
             collection / wallet / pool / account.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("With hero + verified + subtitle + bulk button")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    let (action, _) = GroupedSection::new("Black Flag")
        .hero_image(PLACEHOLDER_HERO.clone())
        .verified(true)
        .subtitle("12 unspent")
        .bulk_action(true, "Add all (12)")
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("[caller-rendered body — tiles, rows, charts, …]")
                    .color(theme::TEXT_MUTED)
                    .size(11.0),
            );
        });
    if let Some(GroupedSectionAction::BulkAction) = action {
        ui.label(
            egui::RichText::new("→ BulkAction fired")
                .color(theme::ACCENT_GREEN)
                .size(10.0),
        );
    }
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Without hero (placeholder reserves space so titles align)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    let _ = GroupedSection::new("Unknown collection")
        .subtitle("3 offer(s) — 3 unspent")
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("[no hero, title aligns with the row above]")
                    .color(theme::TEXT_MUTED)
                    .size(11.0),
            );
        });
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Bulk button hidden when not eligible")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    let _ = GroupedSection::new("Derp Birds")
        .verified(true)
        .subtitle("1 offer — 0 eligible")
        .bulk_action(false, "Add all")
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("[passing visible=false hides the button]")
                    .color(theme::TEXT_MUTED)
                    .size(11.0),
            );
        });
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Without subtitle, no badge")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    let _ = GroupedSection::new("Frigid")
        .hero_image(PLACEHOLDER_HERO.clone())
        .bulk_action(true, "Add all (5)")
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "[minimal config — title + bulk action only, no badge or subtitle]",
                )
                .color(theme::TEXT_MUTED)
                .size(11.0),
            );
        });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("Test cases:")
            .color(ACCENT)
            .strong(),
    );
    ui.label("\u{2022} Hero image renders at 32×32 with rounded corners");
    ui.label("\u{2022} Verified badge appears next to title when `verified(true)`");
    ui.label("\u{2022} Subtitle below title in muted text, optional");
    ui.label("\u{2022} Bulk button right-aligned, hidden when first arg is `false`");
    ui.label("\u{2022} `BulkAction` fires once per click; consumer dispatches its own action");
    ui.label("\u{2022} Body is caller-rendered — accepts any closure");
    ui.label("\u{2022} Hero placeholder reserves the same width so titles line up");
}
