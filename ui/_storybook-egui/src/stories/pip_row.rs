//! Storybook demo for the PipRow widget from egui-widgets.

use egui_widgets::pip_row::{heat_color, HoverInfo, Pip, PipRowConfig, PipRowData, PipRowMode};

use crate::{ACCENT, TEXT_MUTED};

// ============================================================================
// State
// ============================================================================

pub struct PipRowState {
    pub label_width: f32,
    pub row_height: f32,
    pub bar_height: f32,
    pub pip_width: f32,
    pub preset: usize,
    pub use_density: bool,
    pub density_bins: usize,
    pub density_min_alpha: f32,
}

impl Default for PipRowState {
    fn default() -> Self {
        Self {
            label_width: 200.0,
            row_height: 26.0,
            bar_height: 18.0,
            pip_width: 4.0,
            preset: 0,
            use_density: false,
            density_bins: 40,
            density_min_alpha: 0.15,
        }
    }
}

// ============================================================================
// Demo data presets
// ============================================================================

struct DemoRow {
    label: &'static str,
    label_color: egui::Color32,
    values: Vec<f64>,
    empty_text: Option<&'static str>,
}

fn preset_market_depth() -> (Vec<DemoRow>, f64) {
    let max = 500.0;
    (
        vec![
            DemoRow {
                label: "1. Background: Red (2.1%) 45A",
                label_color: egui::Color32::from_rgb(187, 154, 247), // magenta — driver
                values: vec![45.0, 52.0, 58.0, 65.0, 72.0, 80.0, 95.0, 120.0],
                empty_text: None,
            },
            DemoRow {
                label: "2. Eyes: Laser (0.8%) 180A",
                label_color: egui::Color32::from_rgb(220, 220, 235),
                values: vec![180.0, 195.0, 210.0, 250.0, 310.0, 420.0, 500.0],
                empty_text: None,
            },
            DemoRow {
                label: "3. Headwear: Crown (1.5%)",
                label_color: egui::Color32::from_rgb(224, 175, 104), // yellow — no listings
                values: vec![],
                empty_text: Some("no listings"),
            },
            DemoRow {
                label: "4. Clothing: Hoodie (12.3%) 28A",
                label_color: egui::Color32::from_rgb(220, 220, 235),
                values: vec![
                    28.0, 30.0, 31.0, 32.0, 35.0, 38.0, 40.0, 42.0, 45.0, 50.0, 55.0, 60.0,
                ],
                empty_text: None,
            },
            DemoRow {
                label: "5. Accessory",
                label_color: egui::Color32::from_rgb(100, 100, 130), // muted — missing
                values: vec![],
                empty_text: Some("\u{2014}"),
            },
        ],
        max,
    )
}

fn preset_performance_metrics() -> (Vec<DemoRow>, f64) {
    let max = 100.0;
    (
        vec![
            DemoRow {
                label: "API Latency (ms)",
                label_color: egui::Color32::from_rgb(220, 220, 235),
                values: vec![12.0, 15.0, 18.0, 22.0, 25.0, 28.0, 35.0, 42.0, 55.0, 80.0],
                empty_text: None,
            },
            DemoRow {
                label: "DB Query (ms)",
                label_color: egui::Color32::from_rgb(220, 220, 235),
                values: vec![2.0, 3.0, 4.0, 5.0, 5.0, 6.0, 8.0],
                empty_text: None,
            },
            DemoRow {
                label: "Cache Hit Rate (%)",
                label_color: egui::Color32::from_rgb(158, 206, 106),
                values: vec![85.0, 88.0, 90.0, 92.0, 94.0, 95.0, 96.0, 97.0],
                empty_text: None,
            },
            DemoRow {
                label: "Error Rate (%)",
                label_color: egui::Color32::from_rgb(247, 118, 142),
                values: vec![0.1, 0.2, 0.5, 1.2],
                empty_text: None,
            },
        ],
        max,
    )
}

fn preset_sparse() -> (Vec<DemoRow>, f64) {
    let max = 1000.0;
    (
        vec![
            DemoRow {
                label: "Outlier cluster",
                label_color: egui::Color32::from_rgb(220, 220, 235),
                values: vec![50.0, 950.0],
                empty_text: None,
            },
            DemoRow {
                label: "Single point",
                label_color: egui::Color32::from_rgb(220, 220, 235),
                values: vec![500.0],
                empty_text: None,
            },
            DemoRow {
                label: "Empty row",
                label_color: egui::Color32::from_rgb(100, 100, 130),
                values: vec![],
                empty_text: Some("no data"),
            },
        ],
        max,
    )
}

const PRESET_NAMES: [&str; 3] = ["Market Depth", "Performance Metrics", "Sparse Data"];

fn preset_data(index: usize) -> (Vec<DemoRow>, f64) {
    match index {
        1 => preset_performance_metrics(),
        2 => preset_sparse(),
        _ => preset_market_depth(),
    }
}

// ============================================================================
// Show
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut PipRowState) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.label_width, 100.0..=300.0).text("Label width"));
        ui.add(egui::Slider::new(&mut state.row_height, 16.0..=40.0).text("Row height"));
        ui.add(egui::Slider::new(&mut state.bar_height, 8.0..=32.0).text("Bar height"));
    });

    ui.horizontal(|ui| {
        ui.label("Mode:");
        let pips_text = if !state.use_density {
            egui::RichText::new("Pips").color(ACCENT).strong()
        } else {
            egui::RichText::new("Pips").color(TEXT_MUTED)
        };
        let density_text = if state.use_density {
            egui::RichText::new("Density").color(ACCENT).strong()
        } else {
            egui::RichText::new("Density").color(TEXT_MUTED)
        };
        if ui.selectable_label(!state.use_density, pips_text).clicked() {
            state.use_density = false;
        }
        if ui
            .selectable_label(state.use_density, density_text)
            .clicked()
        {
            state.use_density = true;
        }

        ui.separator();

        if state.use_density {
            ui.add(egui::Slider::new(&mut state.density_bins, 10..=80).text("Bins"));
            ui.add(egui::Slider::new(&mut state.density_min_alpha, 0.05..=0.5).text("Min alpha"));
        } else {
            ui.add(egui::Slider::new(&mut state.pip_width, 2.0..=10.0).text("Pip width"));
        }
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

    let (rows, global_max) = preset_data(state.preset);
    let mode = if state.use_density {
        PipRowMode::Density {
            bins: state.density_bins,
            color: egui::Color32::from_rgb(125, 207, 255),
            min_alpha: state.density_min_alpha,
        }
    } else {
        PipRowMode::Pips {
            pip_width: state.pip_width,
            pip_rounding: 1.0,
            overflow_color: egui::Color32::from_rgb(160, 160, 180),
        }
    };
    let config = PipRowConfig {
        mode,
        label_width: state.label_width,
        row_height: state.row_height,
        bar_height: state.bar_height,
        ..Default::default()
    };

    for row in &rows {
        let pips: Vec<Pip> = row
            .values
            .iter()
            .map(|&v| Pip {
                value: v,
                color: heat_color((v / global_max) as f32),
            })
            .collect();

        let data = PipRowData {
            label: row.label,
            label_color: row.label_color,
            pips: &pips,
            empty_text: row.empty_text,
            empty_color: if row.empty_text == Some("\u{2014}") {
                egui::Color32::from_rgb(100, 100, 130)
            } else {
                egui::Color32::from_rgb(224, 175, 104)
            },
        };

        let resp = egui_widgets::pip_row::show(ui, &data, global_max, &config);

        // Position-aware tooltip anchored at the crosshair
        resp.show_tooltip(|ui, hover| {
            ui.label(
                egui::RichText::new(row.label)
                    .color(row.label_color)
                    .size(11.0)
                    .strong(),
            );
            match hover {
                HoverInfo::Pips(hovered) => {
                    for hp in hovered.iter().take(6) {
                        ui.label(
                            egui::RichText::new(format!("{:.1}", hp.value))
                                .color(heat_color((hp.value / global_max) as f32))
                                .size(10.0),
                        );
                    }
                    if hovered.len() > 6 {
                        ui.label(
                            egui::RichText::new(format!("...and {} more", hovered.len() - 6))
                                .color(TEXT_MUTED)
                                .size(9.0),
                        );
                    }
                }
                HoverInfo::Bin(bin) => {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} values in {:.1}\u{2013}{:.1}",
                            bin.count, bin.range_lo, bin.range_hi
                        ))
                        .color(egui::Color32::from_rgb(125, 207, 255))
                        .size(10.0),
                    );
                }
            }
        });
    }

    ui.add_space(12.0);
    ui.label(egui::RichText::new("Features:").color(ACCENT).strong());
    let features = [
        "Two modes: Pips (individual marks) and Density (binned heatmap)",
        "Pips: heat_color() green\u{2192}yellow\u{2192}red gradient, auto-truncates with \"+N more\"",
        "Density: brightness encodes count per bin, clusters glow brighter",
        "Position-aware hover: crosshair + highlight + HoverInfo for nearby pips or bin details",
    ];
    for f in features {
        ui.label(
            egui::RichText::new(format!("  {f}"))
                .color(egui::Color32::from_rgb(220, 220, 235))
                .small(),
        );
    }
}
