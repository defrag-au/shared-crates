//! Horizontal range bar widget for visualizing a set of labeled price/value
//! points along a gradient.
//!
//! Draws a bar with colored gradient fill, tick marks, dots, and labels that
//! auto-stagger into multiple rows when they would overlap.

use egui::{Color32, Pos2, Sense, Stroke, Vec2};

// ============================================================================
// Public types
// ============================================================================

/// A single labeled point on the range bar.
pub struct RangePoint {
    /// Numeric value determining position along the bar.
    pub value: f64,
    /// Display label shown below the tick mark.
    pub label: String,
    /// Color for the tick, dot, and label.
    pub color: Color32,
}

/// Configuration for the range bar appearance.
pub struct RangeBarConfig {
    /// Background color of the bar track.
    pub track_color: Color32,
    /// Height of the main bar in pixels.
    pub bar_height: f32,
    /// Height of each label row below the bar.
    pub label_row_height: f32,
    /// Height of tick marks extending below the bar.
    pub tick_height: f32,
    /// Maximum bar width (will be clamped to available width).
    pub max_width: f32,
    /// Horizontal margin on each side for label overflow room.
    pub margin: f32,
    /// Corner radius of the bar track.
    pub rounding: f32,
    /// Gradient fill opacity (0.0–1.0).
    pub fill_opacity: f32,
    /// Font size for labels.
    pub label_font_size: f32,
    /// Dot radius on the bar center line.
    pub dot_radius: f32,
}

impl Default for RangeBarConfig {
    fn default() -> Self {
        Self {
            track_color: Color32::from_rgb(40, 43, 55),
            bar_height: 16.0,
            label_row_height: 14.0,
            tick_height: 8.0,
            max_width: 500.0,
            margin: 40.0,
            rounding: 4.0,
            fill_opacity: 0.35,
            label_font_size: 10.0,
            dot_radius: 4.0,
        }
    }
}

// ============================================================================
// Drawing
// ============================================================================

/// Draw a horizontal range bar with labeled tick marks.
///
/// Points are sorted internally by value. If fewer than 2 points are provided
/// the widget degrades gracefully: a single point renders as a label, zero
/// points render nothing.
pub fn show(ui: &mut egui::Ui, points: &[RangePoint], config: &RangeBarConfig) {
    if points.is_empty() {
        return;
    }

    // Sort by value
    let mut sorted: Vec<&RangePoint> = points.iter().collect();
    sorted.sort_by(|a, b| {
        a.value
            .partial_cmp(&b.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.dedup_by(|a, b| (a.value - b.value).abs() < f64::EPSILON);

    // Single point — just show as a label
    if sorted.len() < 2 {
        let p = sorted[0];
        ui.label(
            egui::RichText::new(format!("{}: {:.0}", p.label, p.value))
                .color(p.color)
                .size(13.0)
                .strong(),
        );
        return;
    }

    let min_val = sorted.first().unwrap().value;
    let max_val = sorted.last().unwrap().value;
    let val_range = (max_val - min_val).max(f64::EPSILON);

    let label_font = egui::FontId::proportional(config.label_font_size);
    let bar_width = ui.available_width().min(config.max_width);
    let inner_width = bar_width - 2.0 * config.margin;

    // Pre-compute x positions and detect label overlap to stagger rows
    struct Layout {
        x: f32,
        label_half_w: f32,
        row: usize,
    }

    let mut layout: Vec<Layout> = sorted
        .iter()
        .map(|p| {
            let x = if (max_val - min_val).abs() < f64::EPSILON {
                config.margin + inner_width / 2.0
            } else {
                config.margin + ((p.value - min_val) / val_range) as f32 * inner_width
            };
            let galley =
                ui.painter()
                    .layout_no_wrap(p.label.clone(), label_font.clone(), Color32::WHITE);
            Layout {
                x,
                label_half_w: galley.rect.width() / 2.0 + 4.0,
                row: 0,
            }
        })
        .collect();

    // Stagger into rows to avoid overlap
    for i in 1..layout.len() {
        let cur_left = layout[i].x - layout[i].label_half_w;
        let cur_right = layout[i].x + layout[i].label_half_w;
        for row in 0..4 {
            let overlaps = layout[..i].iter().any(|prev| {
                prev.row == row && {
                    let prev_right = prev.x + prev.label_half_w;
                    let prev_left = prev.x - prev.label_half_w;
                    cur_left < prev_right && cur_right > prev_left
                }
            });
            if !overlaps {
                layout[i].row = row;
                break;
            }
            layout[i].row = row + 1;
        }
    }

    let max_rows = layout.iter().map(|l| l.row).max().unwrap_or(0) + 1;
    let total_height =
        config.bar_height + config.tick_height + config.label_row_height * max_rows as f32 + 2.0;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_width, total_height), Sense::hover());
    let bar_left = rect.min.x + config.margin;
    let bar_y = rect.min.y;

    // Bar background
    let bar_rect = egui::Rect::from_min_size(
        Pos2::new(bar_left, bar_y),
        Vec2::new(inner_width, config.bar_height),
    );
    ui.painter()
        .rect_filled(bar_rect, config.rounding, config.track_color);

    // Gradient fill between first and last color
    let first_color = sorted.first().unwrap().color;
    let last_color = sorted.last().unwrap().color;
    let segments = 20;
    let seg_width = inner_width / segments as f32;
    for i in 0..segments {
        let t = i as f32 / segments as f32;
        let color = lerp_color(first_color, last_color, t);
        let seg_rect = egui::Rect::from_min_size(
            Pos2::new(bar_left + i as f32 * seg_width, bar_y + 1.0),
            Vec2::new(seg_width + 1.0, config.bar_height - 2.0),
        );
        ui.painter()
            .rect_filled(seg_rect, 0.0, color.gamma_multiply(config.fill_opacity));
    }

    // Tick marks, dots, and labels
    for (point, lp) in sorted.iter().zip(layout.iter()) {
        let x = rect.min.x + lp.x;

        // Tick
        ui.painter().line_segment(
            [
                Pos2::new(x, bar_y),
                Pos2::new(x, bar_y + config.bar_height + config.tick_height),
            ],
            Stroke::new(2.0, point.color),
        );

        // Dot on bar center
        ui.painter().circle_filled(
            Pos2::new(x, bar_y + config.bar_height / 2.0),
            config.dot_radius,
            point.color,
        );

        // Label
        let label_y = bar_y
            + config.bar_height
            + config.tick_height
            + 2.0
            + lp.row as f32 * config.label_row_height;
        ui.painter().text(
            Pos2::new(x, label_y),
            egui::Align2::CENTER_TOP,
            &point.label,
            label_font.clone(),
            point.color,
        );
    }
}

/// Linear interpolation between two colors.
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}
