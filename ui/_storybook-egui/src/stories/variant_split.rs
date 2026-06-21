//! Storybook demo for the VariantSplit widget.

use egui_widgets::variant_split::{self, VariantSegment, VariantSplitConfig};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

fn seg(variant: &str, share: f32, assets: usize, i: usize) -> VariantSegment {
    VariantSegment {
        variant: variant.into(),
        share,
        asset_count: assets,
        color: variant_split::variant_color(i),
    }
}

fn card(ui: &mut egui::Ui, title: &str, slot: &str, segments: &[VariantSegment]) {
    egui::Frame::new()
        .fill(BG_MAIN)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .color(egui_widgets::theme::TEXT_SECONDARY)
                    .size(11.0)
                    .strong(),
            );
            ui.add_space(8.0);
            variant_split::show(ui, slot, segments, &VariantSplitConfig::default());
        });
    ui.add_space(12.0);
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("VariantSplit Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Explains a variant_flow source slot's DERIVED distribution: each variant's \
             share is weighted by downstream asset capacity, so the split isn't uniform. \
             Dotted ticks mark the naive uniform baseline; the gap is the cardinality skew.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(460.0, ui.available_height()), |ui| {
        // The real hodlcroft case: skin -> clothes, 26 `a` vs 7 `b` => ~81/19, not 50/50.
        card(
            ui,
            "skin -> clothes (real: 26 a vs 7 b, ~81/19)",
            "skin",
            &[seg("a", 0.81, 26, 0), seg("b", 0.19, 7, 1)],
        );

        // Three variants with uneven capacity.
        card(
            ui,
            "Three variants, uneven capacity",
            "body",
            &[
                seg("forest", 0.55, 22, 0),
                seg("desert", 0.30, 12, 1),
                seg("tundra", 0.15, 6, 2),
            ],
        );

        // Balanced — equal capacity lands near the uniform baseline (ticks align).
        card(
            ui,
            "Balanced capacity (lands near uniform)",
            "eyes",
            &[seg("open", 0.5, 15, 0), seg("closed", 0.5, 15, 1)],
        );
    });
}
