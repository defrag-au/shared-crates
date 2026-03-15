//! Storybook demo for the RangeBar widget from egui-widgets.

use egui_widgets::range_bar::{RangeBarConfig, RangePoint};

use crate::{ACCENT, TEXT_MUTED};

// ============================================================================
// State
// ============================================================================

pub struct RangeBarState {
    pub max_width: f32,
    pub bar_height: f32,
    pub fill_opacity: f32,
    pub preset: usize,
}

impl Default for RangeBarState {
    fn default() -> Self {
        Self {
            max_width: 500.0,
            bar_height: 16.0,
            fill_opacity: 0.35,
            preset: 0,
        }
    }
}

// ============================================================================
// Demo data presets
// ============================================================================

fn preset_nft_pricing() -> Vec<RangePoint> {
    vec![
        RangePoint {
            value: 45.0,
            label: "Floor".into(),
            color: egui::Color32::from_rgb(158, 206, 106), // green
        },
        RangePoint {
            value: 68.0,
            label: "Rarity Model".into(),
            color: egui::Color32::from_rgb(125, 207, 255), // cyan
        },
        RangePoint {
            value: 82.0,
            label: "Market Model".into(),
            color: egui::Color32::from_rgb(224, 175, 104), // yellow
        },
        RangePoint {
            value: 120.0,
            label: "Trait Floor".into(),
            color: egui::Color32::from_rgb(187, 154, 247), // magenta
        },
    ]
}

fn preset_tight_spread() -> Vec<RangePoint> {
    vec![
        RangePoint {
            value: 98.0,
            label: "Bid".into(),
            color: egui::Color32::from_rgb(158, 206, 106),
        },
        RangePoint {
            value: 100.0,
            label: "Mid".into(),
            color: egui::Color32::from_rgb(125, 207, 255),
        },
        RangePoint {
            value: 102.0,
            label: "Ask".into(),
            color: egui::Color32::from_rgb(247, 118, 142),
        },
    ]
}

fn preset_wide_range() -> Vec<RangePoint> {
    vec![
        RangePoint {
            value: 10.0,
            label: "Min".into(),
            color: egui::Color32::from_rgb(158, 206, 106),
        },
        RangePoint {
            value: 150.0,
            label: "P25".into(),
            color: egui::Color32::from_rgb(125, 207, 255),
        },
        RangePoint {
            value: 500.0,
            label: "Median".into(),
            color: egui::Color32::from_rgb(224, 175, 104),
        },
        RangePoint {
            value: 1200.0,
            label: "P75".into(),
            color: egui::Color32::from_rgb(187, 154, 247),
        },
        RangePoint {
            value: 5000.0,
            label: "Max".into(),
            color: egui::Color32::from_rgb(247, 118, 142),
        },
    ]
}

fn preset_single() -> Vec<RangePoint> {
    vec![RangePoint {
        value: 42.0,
        label: "Estimate".into(),
        color: egui::Color32::from_rgb(125, 207, 255),
    }]
}

const PRESET_NAMES: [&str; 4] = ["NFT Pricing", "Tight Spread", "Wide Range", "Single Point"];

fn preset_data(index: usize) -> Vec<RangePoint> {
    match index {
        1 => preset_tight_spread(),
        2 => preset_wide_range(),
        3 => preset_single(),
        _ => preset_nft_pricing(),
    }
}

// ============================================================================
// Show
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut RangeBarState) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.max_width, 200.0..=800.0).text("Max width"));
        ui.add(egui::Slider::new(&mut state.bar_height, 8.0..=32.0).text("Bar height"));
    });
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.fill_opacity, 0.0..=1.0).text("Fill opacity"));
    });

    ui.horizontal(|ui| {
        ui.label("Preset:");
        for (i, name) in PRESET_NAMES.iter().enumerate() {
            let text = if state.preset == i {
                egui::RichText::new(*name).color(ACCENT).strong()
            } else {
                egui::RichText::new(*name).color(TEXT_MUTED)
            };
            if ui.selectable_label(state.preset == i, text).clicked() {
                state.preset = i;
            }
        }
    });

    ui.add_space(8.0);

    let points = preset_data(state.preset);
    let config = RangeBarConfig {
        max_width: state.max_width,
        bar_height: state.bar_height,
        fill_opacity: state.fill_opacity,
        ..Default::default()
    };

    egui_widgets::range_bar::show(ui, &points, &config);

    ui.add_space(12.0);

    // Legend
    ui.label(egui::RichText::new("Data points:").color(ACCENT).strong());
    for p in &points {
        ui.label(
            egui::RichText::new(format!("  {}: {:.0}", p.label, p.value))
                .color(p.color)
                .small(),
        );
    }
}
