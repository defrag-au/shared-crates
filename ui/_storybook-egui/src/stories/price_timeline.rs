//! Storybook demo for the PriceTimeline widget from egui-widgets.

use egui_widgets::price_timeline::{
    LogMode, PointEmphasis, PointShape, PriceTimelineConfig, ReferenceBand, ReferenceLine,
    TimelinePoint,
};

use crate::{ACCENT, TEXT_MUTED};

// ============================================================================
// State
// ============================================================================

pub struct PriceTimelineState {
    pub preset: usize,
    pub height: f32,
    pub log_mode: usize,
    pub connect: bool,
}

impl Default for PriceTimelineState {
    fn default() -> Self {
        Self {
            preset: 0,
            height: 160.0,
            log_mode: 1, // Auto
            connect: false,
        }
    }
}

// ============================================================================
// Demo data — deterministic (fixed "now", LCG pseudo-random), so the story
// renders identically every run.
// ============================================================================

/// Fixed reference clock for the demo (2026-07-01T00:00:00Z).
const DEMO_NOW: i64 = 1_782_864_000;
const DAY: i64 = 86_400;

const SALE_CYAN: egui::Color32 = egui::Color32::from_rgb(125, 207, 255);
const OFFER_MAGENTA: egui::Color32 = egui::Color32::from_rgb(187, 154, 247);
const CO_MUTED: egui::Color32 = egui::Color32::from_rgb(120, 126, 150);

struct Demo {
    points: Vec<TimelinePoint>,
    lines: Vec<ReferenceLine>,
    bands: Vec<ReferenceBand>,
    window_secs: i64,
    blurb: &'static str,
}

/// Sparse asset history: the asset's own trades highlighted over faint
/// collection context, with the realized band + a dashed listing line.
fn preset_asset_history() -> Demo {
    let own = [
        (62 * DAY, 38.0),
        (30 * DAY, 55.0),
        (9 * DAY, 74.0),
        (2 * DAY, 68.0),
    ];
    let mut points: Vec<TimelinePoint> = own
        .iter()
        .map(|&(age, v)| TimelinePoint {
            timestamp_secs: DEMO_NOW - age,
            value: v,
            color: SALE_CYAN,
            shape: PointShape::Circle,
            emphasis: PointEmphasis::Highlight,
        })
        .collect();
    // Faint comparable context around the subject's trades.
    let mut rng = Lcg(7);
    for _ in 0..40 {
        let age = rng.range(0, 90) * DAY + rng.range(0, DAY);
        let value = 25.0 + rng.range(0, 60) as f64;
        points.push(TimelinePoint {
            timestamp_secs: DEMO_NOW - age,
            value,
            color: SALE_CYAN,
            shape: PointShape::Circle,
            emphasis: PointEmphasis::Faint,
        });
    }
    Demo {
        points,
        lines: vec![ReferenceLine {
            value: 88.0,
            label: "listed 88".into(),
            color: egui::Color32::from_rgb(224, 175, 104),
            dashed: true,
        }],
        bands: vec![ReferenceBand {
            low: 42.0,
            high: 79.0,
            fill: egui::Color32::from_rgba_unmultiplied(125, 207, 255, 18),
        }],
        window_secs: 90 * DAY,
        blurb: "Sparse asset history: own trades highlighted, collection context faint, \
                realized p10\u{2013}p90 band, dashed listing line. Try connect for a path.",
    }
}

/// Dense collection scatter with a premium tail — Auto log kicks in.
fn preset_collection_scatter() -> Demo {
    let mut rng = Lcg(42);
    let mut points = Vec::with_capacity(400);
    for i in 0..400 {
        let age = rng.range(0, 90) * DAY + rng.range(0, DAY);
        // Mostly floor-ish, occasional premium fills up to ~100x.
        let roll = rng.range(0, 100);
        let value = if roll < 70 {
            30.0 + rng.range(0, 25) as f64
        } else if roll < 93 {
            60.0 + rng.range(0, 140) as f64
        } else {
            300.0 + rng.range(0, 2700) as f64
        };
        let (shape, color) = match i % 9 {
            0 => (PointShape::Diamond, OFFER_MAGENTA),
            1 => (PointShape::Square, CO_MUTED),
            _ => (PointShape::Circle, SALE_CYAN),
        };
        points.push(TimelinePoint {
            timestamp_secs: DEMO_NOW - age,
            value,
            color,
            shape,
            emphasis: PointEmphasis::Normal,
        });
    }
    Demo {
        points,
        lines: vec![ReferenceLine {
            value: 34.0,
            label: "floor 34".into(),
            color: egui::Color32::from_rgb(158, 206, 106),
            dashed: false,
        }],
        bands: Vec::new(),
        window_secs: 90 * DAY,
        blurb: "400-point collection scatter with a premium tail \u{2014} LogMode::Auto switches \
                to log y. Circle = sale, diamond = offer accept, square = collection offer. \
                Pinch / ctrl+scroll zooms the time axis, drag or horizontal scroll pans, \
                double-click resets (animated); deep-cluster hovers expand one member \
                (scroll cycles it).",
    }
}

/// Degenerate cases: a single point, then none.
fn preset_degenerate() -> Demo {
    Demo {
        points: vec![TimelinePoint {
            timestamp_secs: DEMO_NOW - 5 * DAY,
            value: 120.0,
            color: SALE_CYAN,
            shape: PointShape::Circle,
            emphasis: PointEmphasis::Normal,
        }],
        lines: Vec::new(),
        bands: Vec::new(),
        window_secs: 30 * DAY,
        blurb: "Degenerate data: one point renders sanely; an empty set shows \"no data\" \
                (second chart below).",
    }
}

const PRESET_NAMES: [&str; 3] = ["Asset History", "Collection Scatter", "Degenerate"];
const LOG_NAMES: [&str; 3] = ["Off", "Auto", "On"];

/// Tiny deterministic LCG so the story needs no rand dep and never changes.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo).max(1) as u64) as i64
    }
}

// ============================================================================
// Show
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut PriceTimelineState) {
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
        ui.separator();
        ui.label("Log y:");
        for (i, name) in LOG_NAMES.iter().enumerate() {
            let text = if state.log_mode == i {
                egui::RichText::new(*name).color(ACCENT).strong()
            } else {
                egui::RichText::new(*name).color(TEXT_MUTED)
            };
            if ui.selectable_label(state.log_mode == i, text).clicked() {
                state.log_mode = i;
            }
        }
        ui.separator();
        ui.checkbox(&mut state.connect, "connect");
        ui.add(egui::Slider::new(&mut state.height, 80.0..=280.0).text("height"));
    });
    ui.add_space(8.0);

    let demo = match state.preset {
        1 => preset_collection_scatter(),
        2 => preset_degenerate(),
        _ => preset_asset_history(),
    };
    let config = PriceTimelineConfig {
        height: state.height,
        now_secs: DEMO_NOW,
        window_secs: demo.window_secs,
        log_y: match state.log_mode {
            0 => LogMode::Off,
            2 => LogMode::On,
            _ => LogMode::Auto,
        },
        connect: state
            .connect
            .then_some(egui::Color32::from_rgba_unmultiplied(125, 207, 255, 90)),
        ..Default::default()
    };

    let resp =
        egui_widgets::price_timeline::show(ui, &demo.points, &demo.lines, &demo.bands, &config);
    // The widget hands back point INDICES — the caller owns the tooltip, so a
    // real app renders asset thumbnails/traits here. This demo fakes that
    // shape: swatch standing in for the thumbnail, name, price line, traits.
    // Deep clusters expand ONE member (scroll cycles it) over compact lines.
    let pinned = resp.is_pinned();
    let focus = resp.focus;
    // One rich row (swatch + name + price + traits column).
    let rich_row = |ui: &mut egui::Ui, i: usize| {
        let p = &demo.points[i];
        let age_days = (DEMO_NOW - p.timestamp_secs) / DAY;
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(34.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 4.0, p.color.gamma_multiply(0.4));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "img",
                egui::FontId::proportional(9.0),
                TEXT_MUTED,
            );
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Demo Asset #{:04}", i * 37 % 10_000))
                            .color(egui::Color32::from_rgb(220, 220, 235))
                            .size(11.0)
                            .strong(),
                    );
                    // Interactive content only works once pinned — this is
                    // where a real app puts its marketplace link.
                    if pinned && ui.button("W\u{2197} open (demo)").clicked() {
                        log::info!("demo: would open marketplace listing");
                    }
                });
                ui.label(
                    egui::RichText::new(format!("{:.0} ADA", p.value))
                        .color(p.color)
                        .size(10.0),
                );
                ui.label(
                    egui::RichText::new(format!("{age_days}d ago"))
                        .color(TEXT_MUTED)
                        .size(9.0),
                );
            });
            // Traits: rightmost column, one row per trait, all shown.
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                // Pure labels — drop the interactive min-height so the
                // rows pack tightly.
                ui.spacing_mut().interact_size.y = 0.0;
                for (cat, val) in [
                    ("Background", "Red"),
                    ("Eyes", "Laser"),
                    ("Headwear", "Crown"),
                    ("Clothing", "Hoodie"),
                    ("Accessory", "Chain"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{cat}:"))
                                .color(TEXT_MUTED)
                                .size(9.0),
                        );
                        ui.label(
                            egui::RichText::new(val)
                                .color(egui::Color32::from_rgb(180, 185, 205))
                                .size(9.0),
                        );
                    });
                }
            });
        });
    };
    let divider = |ui: &mut egui::Ui| {
        ui.add_space(4.0);
        ui.scope(|ui| {
            ui.visuals_mut().widgets.noninteractive.bg_stroke.color =
                egui::Color32::from_rgba_unmultiplied(120, 126, 150, 50);
            ui.separator();
        });
        ui.add_space(4.0);
    };
    resp.show_tooltip(|ui, hovered| {
        if hovered.len() > 3 {
            // Fixed-geometry master-detail via focus_list: stable order
            // (value desc), scroll slides the highlight, detail pane below.
            let mut order: Vec<usize> = hovered.to_vec();
            order.sort_by(|&a, &b| {
                demo.points[b]
                    .value
                    .partial_cmp(&demo.points[a].value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let focus_pos = focus.min(order.len() - 1);
            ui.label(
                egui::RichText::new(format!(
                    "{}/{} \u{00b7} scroll to cycle",
                    focus_pos + 1,
                    order.len()
                ))
                .color(TEXT_MUTED)
                .size(8.0),
            );
            egui_widgets::focus_list::show(
                ui,
                order.len(),
                focus_pos,
                &egui_widgets::focus_list::FocusListConfig::default(),
                |ui, pos, _focused| {
                    let i = order[pos];
                    let p = &demo.points[i];
                    let age_days = (DEMO_NOW - p.timestamp_secs) / DAY;
                    ui.label(
                        egui::RichText::new(format!("{:.0} ADA", p.value))
                            .color(p.color)
                            .size(10.0)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{age_days}d ago \u{00b7} Demo Asset #{:04}",
                            i * 37 % 10_000
                        ))
                        .color(TEXT_MUTED)
                        .size(9.0),
                    );
                },
                |ui, pos| rich_row(ui, order[pos]),
            );
            ui.label(
                egui::RichText::new(if pinned {
                    "pinned \u{2014} Esc or click elsewhere to close"
                } else {
                    "click to pin \u{00b7} zoom in (pinch / ctrl+scroll) for detail"
                })
                .color(TEXT_MUTED)
                .size(8.0),
            );
            return;
        }
        for (n, &i) in hovered.iter().enumerate() {
            if n > 0 {
                divider(ui);
            }
            rich_row(ui, i);
        }
    });

    if state.preset == 2 {
        ui.add_space(8.0);
        egui_widgets::price_timeline::show(
            ui,
            &[],
            &[],
            &[],
            &PriceTimelineConfig {
                height: 80.0,
                now_secs: DEMO_NOW,
                ..Default::default()
            },
        );
    }

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(demo.blurb)
            .color(egui::Color32::from_rgb(220, 220, 235))
            .small(),
    );
}
