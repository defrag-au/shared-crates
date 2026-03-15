//! Radar / spider chart widget for N-dimensional normalized data.
//!
//! Draws concentric web rings, axis lines, and a smooth bezier curve through
//! data points. Supports optional missing axes (skipped in the curve).

use egui::{Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};

// ============================================================================
// Public types
// ============================================================================

/// A single axis on the radar chart.
pub struct RadarPoint {
    /// Human-readable label for this axis (shown at the perimeter).
    pub label: String,
    /// Normalized value 0.0–1.0. `None` means "no data for this axis" —
    /// the axis line is still drawn but the curve skips it.
    pub value: Option<f32>,
}

/// Configuration for the radar chart appearance.
pub struct RadarChartConfig {
    /// Color of the data curve and dots.
    pub curve_color: Color32,
    /// Color of the concentric web rings and axis lines.
    pub web_color: Color32,
    /// Color for axis labels that have data.
    pub label_color: Color32,
    /// Color for axis labels with no data.
    pub label_muted_color: Color32,
    /// Curve line width.
    pub curve_width: f32,
    /// Dot radius on data points.
    pub dot_radius: f32,
    /// Catmull-Rom tension for the bezier curve (0.0–0.5 typical).
    pub tension: f32,
    /// Number of concentric rings (e.g. 4 → rings at 25%, 50%, 75%, 100%).
    pub ring_count: u32,
    /// Font size for axis labels.
    pub label_font_size: f32,
}

impl Default for RadarChartConfig {
    fn default() -> Self {
        Self {
            curve_color: Color32::from_rgb(125, 207, 255), // cyan
            web_color: Color32::from_rgba_premultiplied(86, 95, 137, 40),
            label_color: Color32::from_rgb(220, 220, 235),
            label_muted_color: Color32::from_rgb(100, 100, 130),
            curve_width: 1.5,
            dot_radius: 3.0,
            tension: 0.3,
            ring_count: 4,
            label_font_size: 10.0,
        }
    }
}

// ============================================================================
// Drawing
// ============================================================================

/// Draw a radar/spider chart.
///
/// `points` must have at least 3 entries for a meaningful chart.
/// `size` is the width and height of the allocated square.
pub fn show(ui: &mut egui::Ui, points: &[RadarPoint], size: f32, config: &RadarChartConfig) {
    let n = points.len();
    if n < 3 {
        return;
    }

    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let radius = size / 2.0 - 16.0; // room for labels

    let angle_step = std::f32::consts::TAU / n as f32;
    let start_angle = -std::f32::consts::FRAC_PI_2; // top

    let axis_point = |i: usize, frac: f32| -> Pos2 {
        let angle = start_angle + i as f32 * angle_step;
        Pos2::new(
            center.x + angle.cos() * radius * frac,
            center.y + angle.sin() * radius * frac,
        )
    };

    // Concentric web rings
    let web_stroke = Stroke::new(0.5, config.web_color);
    for r in 1..=config.ring_count {
        let frac = r as f32 / config.ring_count as f32;
        let ring_pts: Vec<Pos2> = (0..n).map(|i| axis_point(i, frac)).collect();
        for i in 0..n {
            painter.line_segment([ring_pts[i], ring_pts[(i + 1) % n]], web_stroke);
        }
    }

    // Axis lines
    for i in 0..n {
        painter.line_segment([center, axis_point(i, 1.0)], web_stroke);
    }

    // Collect present-data points for the curve
    let present_pts: Vec<Pos2> = points
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.value.map(|v| axis_point(i, v.clamp(0.0, 1.0))))
        .collect();

    let curve_stroke = Stroke::new(config.curve_width, config.curve_color);
    let m = present_pts.len();

    if m >= 3 {
        // Catmull-Rom → cubic bezier, closed loop
        let steps = 12;
        for i in 0..m {
            let p0 = present_pts[(i + m - 1) % m];
            let p1 = present_pts[i];
            let p2 = present_pts[(i + 1) % m];
            let p3 = present_pts[(i + 2) % m];

            let cp1 = Pos2::new(
                p1.x + (p2.x - p0.x) * config.tension,
                p1.y + (p2.y - p0.y) * config.tension,
            );
            let cp2 = Pos2::new(
                p2.x - (p3.x - p1.x) * config.tension,
                p2.y - (p3.y - p1.y) * config.tension,
            );

            let mut seg = Vec::with_capacity(steps + 1);
            for s in 0..=steps {
                let t = s as f32 / steps as f32;
                let inv = 1.0 - t;
                seg.push(Pos2::new(
                    inv * inv * inv * p1.x
                        + 3.0 * inv * inv * t * cp1.x
                        + 3.0 * inv * t * t * cp2.x
                        + t * t * t * p2.x,
                    inv * inv * inv * p1.y
                        + 3.0 * inv * inv * t * cp1.y
                        + 3.0 * inv * t * t * cp2.y
                        + t * t * t * p2.y,
                ));
            }
            for w in seg.windows(2) {
                painter.line_segment([w[0], w[1]], curve_stroke);
            }
        }
    } else if m == 2 {
        painter.line_segment([present_pts[0], present_pts[1]], curve_stroke);
    }

    // Dots on present data points
    for (i, p) in points.iter().enumerate() {
        if let Some(v) = p.value {
            let pt = axis_point(i, v.clamp(0.0, 1.0));
            painter.circle_filled(pt, config.dot_radius, config.curve_color);
        }
    }

    // Axis labels
    let label_font = FontId::proportional(config.label_font_size);
    for (i, p) in points.iter().enumerate() {
        let label_pos = axis_point(i, 1.15);
        let anchor = axis_label_anchor(i, n);
        let color = if p.value.is_some() {
            config.label_color
        } else {
            config.label_muted_color
        };
        painter.text(label_pos, anchor, &p.label, label_font.clone(), color);
    }
}

/// Choose text anchor based on axis position around the circle.
fn axis_label_anchor(index: usize, total: usize) -> Align2 {
    // Normalize position: 0.0 = top, 0.5 = bottom
    let frac = index as f32 / total as f32;
    if !(0.01..=0.99).contains(&frac) {
        Align2::CENTER_BOTTOM // top
    } else if frac < 0.25 {
        Align2::LEFT_BOTTOM
    } else if frac < 0.5 {
        Align2::LEFT_TOP
    } else if (frac - 0.5).abs() < 0.01 {
        Align2::CENTER_TOP // bottom
    } else if frac < 0.75 {
        Align2::RIGHT_TOP
    } else {
        Align2::RIGHT_BOTTOM
    }
}
