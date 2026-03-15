//! MetricCard — a dashboard stat card with label, value, optional trend, and sparkline.
//!
//! Displays a key metric in a framed card. Supports:
//! - Large primary value with small label
//! - Optional trend indicator (up/down arrow with delta)
//! - Optional inline sparkline for recent history
//! - Configurable accent color for value/trend

use egui::{Color32, RichText, Ui};

use crate::theme;

/// Trend direction for the metric.
#[derive(Clone, Copy, PartialEq)]
pub enum Trend {
    /// Value is increasing (shown in green with up arrow).
    Up,
    /// Value is decreasing (shown in red with down arrow).
    Down,
    /// Value is stable (shown in muted with dash).
    Flat,
}

/// A dashboard metric card.
pub struct MetricCard<'a> {
    /// Small label above the value (e.g. "Total Accrued").
    label: &'a str,
    /// Large primary value (e.g. "12,345").
    value: &'a str,
    /// Optional subtitle below the value (e.g. "5.2/hr").
    subtitle: Option<String>,
    /// Optional trend indicator.
    trend: Option<(Trend, String)>,
    /// Optional sparkline data points.
    sparkline_data: Option<&'a [f64]>,
    /// Accent color for the value text.
    value_color: Color32,
    /// Card width (None = available width).
    width: Option<f32>,
    /// Card background color.
    bg_color: Color32,
    /// Border color.
    border_color: Color32,
}

impl<'a> MetricCard<'a> {
    /// Create a new metric card.
    pub fn new(label: &'a str, value: &'a str) -> Self {
        Self {
            label,
            value,
            subtitle: None,
            trend: None,
            sparkline_data: None,
            value_color: theme::TEXT_PRIMARY,
            width: None,
            bg_color: theme::BG_HIGHLIGHT,
            border_color: theme::BORDER,
        }
    }

    /// Set a subtitle shown below the value.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Set the trend indicator.
    pub fn trend(mut self, direction: Trend, delta: impl Into<String>) -> Self {
        self.trend = Some((direction, delta.into()));
        self
    }

    /// Set sparkline data to show a mini chart in the card.
    pub fn sparkline(mut self, data: &'a [f64]) -> Self {
        self.sparkline_data = Some(data);
        self
    }

    /// Set the value text color.
    pub fn value_color(mut self, color: Color32) -> Self {
        self.value_color = color;
        self
    }

    /// Set the card width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Set the background color.
    pub fn bg_color(mut self, color: Color32) -> Self {
        self.bg_color = color;
        self
    }

    /// Render the metric card.
    pub fn show(self, ui: &mut Ui) {
        let frame = egui::Frame::NONE
            .fill(self.bg_color)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, self.border_color));

        let add_contents = |ui: &mut Ui| {
            // Force vertical layout — cards may be placed inside ui.horizontal()
            ui.vertical(|ui| {
                // Label — top of card, distinct from value
                ui.label(
                    RichText::new(self.label)
                        .color(theme::TEXT_SECONDARY)
                        .size(12.0),
                );

                ui.add_space(4.0);

                // Value row — value + optional subtitle on same line
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(self.value)
                            .color(self.value_color)
                            .size(24.0)
                            .strong(),
                    );
                    if let Some(subtitle) = &self.subtitle {
                        ui.label(RichText::new(subtitle).color(theme::TEXT_MUTED).size(13.0));
                    }
                });

                // Trend indicator on its own line
                if let Some((direction, delta)) = &self.trend {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let color = match direction {
                            Trend::Up => theme::SUCCESS,
                            Trend::Down => theme::ERROR,
                            Trend::Flat => theme::TEXT_MUTED,
                        };

                        // Paint a small triangle arrow instead of unicode
                        let size = 8.0;
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                        let center = rect.center();
                        let painter = ui.painter();
                        match direction {
                            Trend::Up => {
                                let half = size / 2.0;
                                painter.add(egui::Shape::convex_polygon(
                                    vec![
                                        egui::pos2(center.x, center.y - half),
                                        egui::pos2(center.x + half, center.y + half),
                                        egui::pos2(center.x - half, center.y + half),
                                    ],
                                    color,
                                    egui::Stroke::NONE,
                                ));
                            }
                            Trend::Down => {
                                let half = size / 2.0;
                                painter.add(egui::Shape::convex_polygon(
                                    vec![
                                        egui::pos2(center.x - half, center.y - half),
                                        egui::pos2(center.x + half, center.y - half),
                                        egui::pos2(center.x, center.y + half),
                                    ],
                                    color,
                                    egui::Stroke::NONE,
                                ));
                            }
                            Trend::Flat => {
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x - size / 2.0, center.y),
                                        egui::pos2(center.x + size / 2.0, center.y),
                                    ],
                                    egui::Stroke::new(1.5, color),
                                );
                            }
                        }

                        ui.label(RichText::new(delta).color(color).size(11.0));
                    });
                }

                // Sparkline — own row with breathing room
                if let Some(data) = self.sparkline_data {
                    if data.len() >= 2 {
                        ui.add_space(8.0);
                        crate::Sparkline::new(data)
                            .height(32.0)
                            .line_width(1.5)
                            .line_color(self.value_color)
                            .fill(Color32::from_rgba_premultiplied(
                                self.value_color.r(),
                                self.value_color.g(),
                                self.value_color.b(),
                                25,
                            ))
                            .show_endpoint(false)
                            .bg_color(self.bg_color)
                            .show(ui);
                    }
                }
            });
        };

        if let Some(width) = self.width {
            ui.allocate_ui(egui::Vec2::new(width, 0.0), |ui| {
                frame.show(ui, add_contents);
            });
        } else {
            frame.show(ui, add_contents);
        }
    }
}
