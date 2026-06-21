//! Storybook demo for the CollectionComposition widget.
//!
//! Two real-shaped collections: Hodlcroft (even per-slot distributions + variant_flow
//! brackets) and Squashua Chicken (per-asset weights via rarity-index R-tags, linear
//! curve — so the value cells show an overridden distribution). All values shown as
//! left-aligned % cells; hover a cell to preview that asset top-right.

use egui_widgets::collection_composition::{
    self, CollectionComposition, CompositionConfig, CompositionFlow, CompositionLayer,
    CompositionStat, ValueShare,
};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

// Swatch %s are the WITHIN-SLOT share (sum to ~100% across a slot's values); the layer's
// presence % is the separate right-hand value in the left column.

/// Even distribution: every value gets the same within-slot share.
fn even(n: usize) -> Vec<ValueShare> {
    let each = 100.0 / n as f32;
    (0..n)
        .map(|i| ValueShare { pct: each, label: format!("#{}", i + 1), texture: None })
        .collect()
}

/// Per-asset weights, linear rarity curve (R10 = 10× R1): weights taper 10→1 across the
/// slot — the squashua-chicken case.
fn linear_curve(n: usize) -> Vec<ValueShare> {
    let weights: Vec<f32> = (0..n)
        .map(|i| if n <= 1 { 10.0 } else { 10.0 - 9.0 * i as f32 / (n - 1) as f32 })
        .collect();
    let total: f32 = weights.iter().sum();
    weights
        .iter()
        .enumerate()
        .map(|(i, &w)| ValueShare { pct: w / total * 100.0, label: format!("#{}", i + 1), texture: None })
        .collect()
}

fn layer(z: &str, name: &str, present: f32, variants: &[&str], values: Vec<ValueShare>) -> CompositionLayer {
    CompositionLayer {
        z_label: z.into(),
        name: name.into(),
        present_pct: present,
        option_count: values.len(),
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
            "All values shown as % cells (within-slot share): uniform = even, tapered = \
             per-asset overrides. Right-hand % is presence. Hover a cell to preview the \
             asset top-right. Rounded brackets group variant_flow-coupled slots.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.allocate_ui(egui::vec2(780.0, f32::INFINITY), |ui| {
            // 1) Hodlcroft — even per-slot distributions + variant_flow brackets.
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
                        layer("01", "backgrounds", 100.0, &[], even(4)),
                        layer("02", "skin", 100.0, &["a", "b"], even(8)),
                        layer("07", "neck", 80.0, &[], even(3)),
                        layer("08", "eyes", 100.0, &[], even(31)),
                        layer("09", "clothes", 100.0, &["a", "b"], even(33)),
                        layer("10", "mouth", 100.0, &[], even(35)),
                        layer("12", "eyewear", 20.0, &[], even(7)),
                        layer("14", "headwear", 90.0, &[], even(33)),
                    ],
                    flow: vec![
                        CompositionFlow { from: "skin".into(), to: "clothes".into() },
                        CompositionFlow { from: "skin".into(), to: "neck".into() },
                    ],
                },
            );

            // 2) Squashua Chicken — per-asset weights (rarity-index, linear curve).
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
                        layer("01", "Background", 100.0, &[], linear_curve(16)),
                        layer("02", "Back", 20.0, &[], linear_curve(7)),
                        layer("03", "Bodies - Basic", 100.0, &[], linear_curve(11)),
                        layer("04", "Lower Clothes", 65.0, &[], linear_curve(9)),
                        layer("06", "Necklace", 20.0, &[], linear_curve(7)),
                        layer("08", "Hat", 70.0, &[], linear_curve(18)),
                        layer("09", "Eyes-Beak", 100.0, &[], linear_curve(26)),
                    ],
                    flow: vec![],
                },
            );
        });
    });
}
