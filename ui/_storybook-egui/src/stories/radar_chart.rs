//! Storybook demo for the RadarChart widget from egui-widgets.

use egui_widgets::radar_chart::{RadarChartConfig, RadarPoint};

use crate::{ACCENT, TEXT_MUTED};

// ============================================================================
// State
// ============================================================================

pub struct RadarChartState {
    pub size: f32,
    pub tension: f32,
    pub ring_count: u32,
    pub dot_radius: f32,
    pub curve_width: f32,
    pub preset: usize,
}

impl Default for RadarChartState {
    fn default() -> Self {
        Self {
            size: 240.0,
            tension: 0.3,
            ring_count: 4,
            dot_radius: 3.0,
            curve_width: 1.5,
            preset: 0,
        }
    }
}

// ============================================================================
// Demo data presets
// ============================================================================

fn preset_balanced() -> Vec<RadarPoint> {
    vec![
        RadarPoint {
            label: "STR".into(),
            value: Some(0.75),
        },
        RadarPoint {
            label: "DEX".into(),
            value: Some(0.60),
        },
        RadarPoint {
            label: "CON".into(),
            value: Some(0.80),
        },
        RadarPoint {
            label: "INT".into(),
            value: Some(0.55),
        },
        RadarPoint {
            label: "WIS".into(),
            value: Some(0.70),
        },
        RadarPoint {
            label: "CHA".into(),
            value: Some(0.65),
        },
    ]
}

fn preset_glass_cannon() -> Vec<RadarPoint> {
    vec![
        RadarPoint {
            label: "STR".into(),
            value: Some(0.30),
        },
        RadarPoint {
            label: "DEX".into(),
            value: Some(0.45),
        },
        RadarPoint {
            label: "CON".into(),
            value: Some(0.15),
        },
        RadarPoint {
            label: "INT".into(),
            value: Some(0.95),
        },
        RadarPoint {
            label: "WIS".into(),
            value: Some(0.85),
        },
        RadarPoint {
            label: "CHA".into(),
            value: Some(0.40),
        },
    ]
}

fn preset_with_gaps() -> Vec<RadarPoint> {
    vec![
        RadarPoint {
            label: "ATK".into(),
            value: Some(0.90),
        },
        RadarPoint {
            label: "DEF".into(),
            value: Some(0.70),
        },
        RadarPoint {
            label: "SPD".into(),
            value: None,
        },
        RadarPoint {
            label: "MAG".into(),
            value: Some(0.40),
        },
        RadarPoint {
            label: "RES".into(),
            value: None,
        },
        RadarPoint {
            label: "LCK".into(),
            value: Some(0.55),
        },
        RadarPoint {
            label: "HP".into(),
            value: Some(0.80),
        },
        RadarPoint {
            label: "MP".into(),
            value: Some(0.35),
        },
    ]
}

fn preset_nft_rarity() -> Vec<RadarPoint> {
    vec![
        RadarPoint {
            label: "Background".into(),
            value: Some(0.12),
        },
        RadarPoint {
            label: "Body".into(),
            value: Some(0.45),
        },
        RadarPoint {
            label: "Eyes".into(),
            value: Some(0.88),
        },
        RadarPoint {
            label: "Mouth".into(),
            value: Some(0.33),
        },
        RadarPoint {
            label: "Headwear".into(),
            value: Some(0.72),
        },
        RadarPoint {
            label: "Accessory".into(),
            value: Some(0.95),
        },
        RadarPoint {
            label: "Clothing".into(),
            value: Some(0.58),
        },
    ]
}

const PRESET_NAMES: [&str; 4] = [
    "Balanced",
    "Glass Cannon",
    "Gaps (missing axes)",
    "NFT Rarity",
];

fn preset_data(index: usize) -> Vec<RadarPoint> {
    match index {
        1 => preset_glass_cannon(),
        2 => preset_with_gaps(),
        3 => preset_nft_rarity(),
        _ => preset_balanced(),
    }
}

// ============================================================================
// Show
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut RadarChartState) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.size, 120.0..=400.0).text("Size"));
        ui.add(egui::Slider::new(&mut state.tension, 0.0..=0.5).text("Tension"));
    });
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.ring_count, 2..=8).text("Rings"));
        ui.add(egui::Slider::new(&mut state.dot_radius, 1.0..=6.0).text("Dot size"));
        ui.add(egui::Slider::new(&mut state.curve_width, 0.5..=4.0).text("Line width"));
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
    let config = RadarChartConfig {
        tension: state.tension,
        ring_count: state.ring_count,
        dot_radius: state.dot_radius,
        curve_width: state.curve_width,
        ..Default::default()
    };

    egui_widgets::radar_chart::show(ui, &points, state.size, &config);

    ui.add_space(12.0);

    // Legend showing the data values
    ui.label(egui::RichText::new("Data points:").color(ACCENT).strong());
    for p in &points {
        let val_str = match p.value {
            Some(v) => format!("{:.0}%", v * 100.0),
            None => "—".into(),
        };
        ui.label(
            egui::RichText::new(format!("  {}: {val_str}", p.label))
                .color(if p.value.is_some() {
                    egui::Color32::from_rgb(220, 220, 235)
                } else {
                    TEXT_MUTED
                })
                .small(),
        );
    }
}
