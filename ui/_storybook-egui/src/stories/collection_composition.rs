//! Storybook demo for the CollectionComposition widget.

use egui_widgets::collection_composition::{
    self, CollectionComposition, CompositionConfig, CompositionFlow, CompositionLayer,
    CompositionStat,
};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

fn layer(z: &str, name: &str, present: f32, opts: usize, variants: &[&str]) -> CompositionLayer {
    CompositionLayer {
        z_label: z.into(),
        name: name.into(),
        present_pct: present,
        option_count: opts,
        variants: variants.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("CollectionComposition Widget").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "A promotable \"how this collection is generated\" infographic: the z-ordered \
             layer stack (front on top) with per-layer presence, options, variant badges, \
             and variant_flow connectors, under a headline stats band.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    let comp = CollectionComposition {
        title: "Hodlcroft".into(),
        stats: vec![
            CompositionStat { value: "10".into(), label: "traits".into() },
            CompositionStat { value: "8".into(), label: "layers".into() },
            CompositionStat { value: "~4.5B".into(), label: "combinations".into() },
            CompositionStat { value: "skin b · 19%".into(), label: "rarest variant".into() },
        ],
        layers: vec![
            layer("01", "backgrounds", 100.0, 4, &[]),
            layer("02", "skin", 100.0, 8, &["a", "b"]),
            layer("07", "neck", 80.0, 3, &[]),
            layer("08", "eyes", 100.0, 31, &[]),
            layer("09", "clothes", 100.0, 33, &["a", "b"]),
            layer("10", "mouth", 100.0, 35, &[]),
            layer("12", "eyewear", 20.0, 7, &[]),
            layer("14", "headwear", 90.0, 33, &[]),
        ],
        flow: vec![
            CompositionFlow { from: "skin".into(), to: "clothes".into() },
            CompositionFlow { from: "skin".into(), to: "neck".into() },
        ],
    };

    ui.allocate_ui(egui::vec2(560.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(8.0)
            .inner_margin(16.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                collection_composition::show(ui, &comp, &CompositionConfig::default());
            });
    });
}
