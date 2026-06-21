//! Storybook demo for the CollectionComposition widget.
//!
//! Two real-shaped collections: Hodlcroft (even per-slot distributions + variant_flow
//! brackets) and Squashua Chicken (per-asset weights via rarity-index R-tags, so the
//! swatch strips show overridden distributions; no variant_flow).

use egui::Color32;
use egui_widgets::collection_composition::{
    self, CollectionComposition, CompositionConfig, CompositionFlow, CompositionLayer,
    CompositionStat, ValueShare,
};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

// Distinct swatch colours, standing in for value thumbnails in the prototype.
fn swatch(i: usize) -> Color32 {
    const P: &[Color32] = &[
        Color32::from_rgb(122, 162, 247),
        Color32::from_rgb(158, 206, 106),
        Color32::from_rgb(187, 154, 247),
        Color32::from_rgb(255, 158, 100),
        Color32::from_rgb(125, 207, 255),
        Color32::from_rgb(224, 175, 104),
        Color32::from_rgb(247, 118, 142),
        Color32::from_rgb(140, 150, 190),
    ];
    P[i % P.len()]
}

// Swatch %s are the WITHIN-SLOT share (sums to 100% across the slot's values); the
// layer's presence % is the separate right-hand axis.

/// Even distribution: every value gets the same within-slot share.
fn even(n: usize) -> Vec<ValueShare> {
    let each = 100.0 / n as f32;
    (0..n).map(|i| ValueShare { pct: each, swatch: swatch(i) }).collect()
}

/// Overridden distribution from per-asset weights (R-tags): share = weight / Σweights.
fn weighted(weights: &[f32], total: f32) -> Vec<ValueShare> {
    weights
        .iter()
        .enumerate()
        .map(|(i, &w)| ValueShare { pct: w / total * 100.0, swatch: swatch(i) })
        .collect()
}

fn layer(z: &str, name: &str, present: f32, opts: usize, variants: &[&str], values: Vec<ValueShare>) -> CompositionLayer {
    CompositionLayer {
        z_label: z.into(),
        name: name.into(),
        present_pct: present,
        option_count: opts,
        variants: variants.iter().map(|s| s.to_string()).collect(),
        values,
    }
}

fn card(ui: &mut egui::Ui, comp: &CollectionComposition) {
    egui::Frame::new()
        .fill(BG_MAIN)
        .corner_radius(8.0)
        .inner_margin(16.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            collection_composition::show(ui, comp, &CompositionConfig::default());
        });
    ui.add_space(16.0);
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("CollectionComposition Widget").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Swatch %s are the within-slot distribution (uniform = even, varied = per-asset \
             overrides); the right-hand % is how often the layer is present. Rounded brackets \
             group variant_flow-coupled slots.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.allocate_ui(egui::vec2(640.0, f32::INFINITY), |ui| {
            // 1) Hodlcroft — even per-slot distributions + variant_flow.
            card(
                ui,
                &CollectionComposition {
                    title: "Hodlcroft (even distribution)".into(),
                    stats: vec![
                        CompositionStat { value: "10".into(), label: "traits".into() },
                        CompositionStat { value: "8".into(), label: "layers".into() },
                        CompositionStat { value: "~4.5B".into(), label: "combinations".into() },
                        CompositionStat { value: "skin b · 19%".into(), label: "rarest variant".into() },
                    ],
                    layers: vec![
                        layer("01", "backgrounds", 100.0, 4, &[], even(4)),
                        layer("02", "skin", 100.0, 8, &["a", "b"], even(8)),
                        layer("07", "neck", 80.0, 3, &[], even(3)),
                        layer("08", "eyes", 100.0, 31, &[], even(31)),
                        layer("09", "clothes", 100.0, 33, &["a", "b"], even(33)),
                        layer("10", "mouth", 100.0, 35, &[], even(35)),
                        layer("12", "eyewear", 20.0, 7, &[], even(7)),
                        layer("14", "headwear", 90.0, 33, &[], even(33)),
                    ],
                    flow: vec![
                        CompositionFlow { from: "skin".into(), to: "clothes".into() },
                        CompositionFlow { from: "skin".into(), to: "neck".into() },
                    ],
                },
            );

            // 2) Squashua Chicken — per-asset weights (rarity-index R-tags, linear curve).
            // Real weights pulled from the asset catalog.
            card(
                ui,
                &CollectionComposition {
                    title: "Squashua Chicken (per-asset weights)".into(),
                    stats: vec![
                        CompositionStat { value: "11".into(), label: "traits".into() },
                        CompositionStat { value: "16".into(), label: "layers".into() },
                        CompositionStat { value: "~2.1B".into(), label: "combinations".into() },
                        CompositionStat { value: "1/1s · 0.2%".into(), label: "rarest".into() },
                    ],
                    layers: vec![
                        layer("01", "Background", 100.0, 16, &[], weighted(&[10.0, 9.0, 9.0, 9.0, 8.0, 7.0, 7.0, 6.0], 100.0)),
                        layer("02", "Back", 20.0, 7, &[], weighted(&[10.0, 9.0, 8.0, 7.0, 6.0, 3.0, 1.0], 44.0)),
                        layer("03", "Bodies - Basic", 100.0, 11, &[], weighted(&[10.0, 9.0, 9.0, 9.0, 8.0, 8.0, 7.0, 6.0], 74.0)),
                        layer("04", "Lower Clothes", 65.0, 9, &[], weighted(&[10.0, 8.0, 7.0, 6.0, 6.0, 5.0, 4.0, 3.0], 51.0)),
                        layer("06", "Necklace", 20.0, 7, &[], weighted(&[9.0, 8.0, 7.0, 4.0, 3.0, 2.0, 1.0], 34.0)),
                        layer("08", "Hat", 70.0, 18, &[], weighted(&[9.0, 9.0, 8.0, 7.0, 7.0, 6.0, 6.0, 6.0], 97.0)),
                        layer("09", "Eyes-Beak", 100.0, 26, &[], weighted(&[9.0, 8.0, 7.0, 7.0, 6.0, 6.0, 6.0, 6.0], 113.0)),
                    ],
                    flow: vec![], // squashua uses mutual-exclusivity + multi-slot traits, not variant_flow
                },
            );
        });
    });
}
