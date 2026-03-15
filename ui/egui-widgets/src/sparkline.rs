//! Sparkline widget — compact inline line chart for trend visualization.
//!
//! Renders a sequence of data points as a smooth line within a small area.
//! Supports optional fill gradient, reference line, and value labels.

use egui::{Color32, CornerRadius, Pos2, RichText, Sense, Stroke, Ui, Vec2};

use crate::theme;

/// Configuration for a sparkline chart.
pub struct Sparkline<'a> {
    /// Data points (y-values in order, equally spaced on x-axis).
    data: &'a [f64],
    /// Line color.
    line_color: Color32,
    /// Optional fill color below the line (semi-transparent recommended).
    fill_color: Option<Color32>,
    /// Line stroke width.
    line_width: f32,
    /// Chart height in pixels.
    height: f32,
    /// Chart width (None = use available width).
    width: Option<f32>,
    /// Optional label shown above the chart.
    label: Option<String>,
    /// Optional current value shown to the right of the label.
    value_text: Option<String>,
    /// Whether to show a horizontal reference line at the mean.
    show_mean_line: bool,
    /// Whether to highlight the last data point with a dot.
    show_endpoint: bool,
    /// Background color.
    bg_color: Color32,
    /// Corner rounding.
    rounding: u8,
}

impl<'a> Sparkline<'a> {
    /// Create a new sparkline from a slice of data points.
    pub fn new(data: &'a [f64]) -> Self {
        Self {
            data,
            line_color: theme::ACCENT,
            fill_color: None,
            line_width: 1.5,
            height: 40.0,
            width: None,
            label: None,
            value_text: None,
            show_mean_line: false,
            show_endpoint: true,
            bg_color: theme::BG_SECONDARY,
            rounding: 4,
        }
    }

    /// Set the line color.
    pub fn line_color(mut self, color: Color32) -> Self {
        self.line_color = color;
        self
    }

    /// Enable fill below the line with the given color.
    pub fn fill(mut self, color: Color32) -> Self {
        self.fill_color = Some(color);
        self
    }

    /// Set the line width.
    pub fn line_width(mut self, width: f32) -> Self {
        self.line_width = width;
        self
    }

    /// Set the chart height.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Set the chart width (None = use available width).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Set the label shown above the chart.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the current value text shown to the right of the label.
    pub fn value_text(mut self, text: impl Into<String>) -> Self {
        self.value_text = Some(text.into());
        self
    }

    /// Show a horizontal dashed reference line at the mean value.
    pub fn show_mean_line(mut self) -> Self {
        self.show_mean_line = true;
        self
    }

    /// Show/hide the endpoint dot on the last data point.
    pub fn show_endpoint(mut self, show: bool) -> Self {
        self.show_endpoint = show;
        self
    }

    /// Set the background color.
    pub fn bg_color(mut self, color: Color32) -> Self {
        self.bg_color = color;
        self
    }

    /// Render the sparkline.
    pub fn show(self, ui: &mut Ui) -> egui::Response {
        // Header row with label + value
        if self.label.is_some() || self.value_text.is_some() {
            ui.horizontal(|ui| {
                if let Some(label) = &self.label {
                    ui.label(RichText::new(label).color(theme::TEXT_SECONDARY).small());
                }
                if let Some(value) = &self.value_text {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(value)
                                .color(theme::TEXT_PRIMARY)
                                .small()
                                .strong(),
                        );
                    });
                }
            });
            ui.add_space(2.0);
        }

        let chart_width = self.width.unwrap_or(ui.available_width());
        let desired_size = Vec2::new(chart_width, self.height);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if ui.is_rect_visible(rect) && self.data.len() >= 2 {
            let painter = ui.painter();
            let rounding = CornerRadius::same(self.rounding);

            // Background
            painter.rect_filled(rect, rounding, self.bg_color);

            // Compute data bounds with padding
            let padding = 4.0;
            let plot_rect = rect.shrink(padding);

            let min_val = self.data.iter().copied().fold(f64::INFINITY, f64::min);
            let max_val = self.data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let range = if (max_val - min_val).abs() < f64::EPSILON {
                1.0 // Avoid division by zero for flat lines
            } else {
                max_val - min_val
            };

            // Map data points to pixel positions
            let n = self.data.len();
            let points: Vec<Pos2> = self
                .data
                .iter()
                .enumerate()
                .map(|(i, &val)| {
                    let x = plot_rect.left() + (i as f32 / (n - 1) as f32) * plot_rect.width();
                    let y =
                        plot_rect.bottom() - ((val - min_val) / range) as f32 * plot_rect.height();
                    Pos2::new(x, y)
                })
                .collect();

            // Fill area below line
            if let Some(fill_color) = self.fill_color {
                for window in points.windows(2) {
                    let p0 = window[0];
                    let p1 = window[1];
                    // Draw a filled trapezoid from the line segment down to the bottom
                    let mesh = egui::Mesh {
                        indices: vec![0, 1, 2, 0, 2, 3],
                        vertices: vec![
                            egui::epaint::Vertex {
                                pos: p0,
                                uv: egui::epaint::WHITE_UV,
                                color: fill_color,
                            },
                            egui::epaint::Vertex {
                                pos: p1,
                                uv: egui::epaint::WHITE_UV,
                                color: fill_color,
                            },
                            egui::epaint::Vertex {
                                pos: Pos2::new(p1.x, plot_rect.bottom()),
                                uv: egui::epaint::WHITE_UV,
                                color: Color32::TRANSPARENT,
                            },
                            egui::epaint::Vertex {
                                pos: Pos2::new(p0.x, plot_rect.bottom()),
                                uv: egui::epaint::WHITE_UV,
                                color: Color32::TRANSPARENT,
                            },
                        ],
                        texture_id: egui::TextureId::default(),
                    };
                    painter.add(egui::Shape::mesh(mesh));
                }
            }

            // Mean reference line
            if self.show_mean_line && !self.data.is_empty() {
                let mean = self.data.iter().sum::<f64>() / n as f64;
                let mean_y =
                    plot_rect.bottom() - ((mean - min_val) / range) as f32 * plot_rect.height();

                // Dashed line via segments
                let dash_len = 4.0;
                let gap_len = 3.0;
                let mut x = plot_rect.left();
                while x < plot_rect.right() {
                    let end_x = (x + dash_len).min(plot_rect.right());
                    painter.line_segment(
                        [Pos2::new(x, mean_y), Pos2::new(end_x, mean_y)],
                        Stroke::new(1.0, theme::TEXT_MUTED),
                    );
                    x += dash_len + gap_len;
                }
            }

            // Line
            let line_stroke = Stroke::new(self.line_width, self.line_color);
            for window in points.windows(2) {
                painter.line_segment([window[0], window[1]], line_stroke);
            }

            // Endpoint dot
            if self.show_endpoint {
                if let Some(&last) = points.last() {
                    painter.circle_filled(last, 3.0, self.line_color);
                    painter.circle_stroke(last, 3.0, Stroke::new(1.0, theme::BG_PRIMARY));
                }
            }

            // Hover: show nearest value
            if let Some(hover_pos) = response.hover_pos() {
                let rel_x = (hover_pos.x - plot_rect.left()) / plot_rect.width();
                let idx = (rel_x * (n - 1) as f32).round() as usize;
                if idx < n {
                    let val = self.data[idx];
                    let point = points[idx];

                    // Highlight dot
                    painter.circle_filled(point, 4.0, self.line_color);
                    painter.circle_stroke(point, 4.0, Stroke::new(1.5, theme::TEXT_PRIMARY));

                    // Vertical crosshair
                    painter.line_segment(
                        [
                            Pos2::new(point.x, plot_rect.top()),
                            Pos2::new(point.x, plot_rect.bottom()),
                        ],
                        Stroke::new(0.5, theme::TEXT_MUTED),
                    );

                    // Value tooltip
                    let text = if val.abs() >= 1_000_000.0 {
                        format!("{:.1}M", val / 1_000_000.0)
                    } else if val.abs() >= 1_000.0 {
                        format!("{:.1}K", val / 1_000.0)
                    } else {
                        format!("{val:.1}")
                    };
                    response.clone().on_hover_text(text);
                }
            }
        } else if ui.is_rect_visible(rect) {
            // Not enough data — show placeholder
            let painter = ui.painter();
            painter.rect_filled(rect, CornerRadius::same(self.rounding), self.bg_color);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "no data",
                egui::FontId::proportional(10.0),
                theme::TEXT_MUTED,
            );
        }

        response
    }
}
