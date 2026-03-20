//! Route summary widget — compact display of split routing results.
//!
//! Per-leg breakdown rows (colored dot + DEX label + ADA input + expected
//! tokens), separator, total output line, improvement percentage vs best
//! single pool. Follows the framed data-row pattern from [`crate::tx_estimate`].

use egui::{Color32, RichText, Ui};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// A single leg in the split route.
pub struct RouteLeg {
    /// DEX label (e.g. "Splash", "CSWAP").
    pub dex_label: String,
    /// Color for the dot indicator.
    pub color: Color32,
    /// ADA input for this leg in lovelace.
    pub input_lovelace: u64,
    /// Expected tokens received from this leg.
    pub expected_tokens: u64,
    /// Price per token in ADA for this leg.
    pub price_per_token: f64,
}

/// Complete route summary data.
pub struct RouteSummaryData {
    /// Individual routing legs.
    pub legs: Vec<RouteLeg>,
    /// Total tokens received across all legs.
    pub total_tokens: u64,
    /// Output token display name.
    pub token_name: String,
    /// Tokens that would be received from the best single pool (for comparison).
    /// None if no comparison available.
    pub best_single_pool_tokens: Option<u64>,
    /// Blended price per token across all legs.
    pub blended_price: f64,
}

/// Configuration for the route summary display.
pub struct RouteSummaryConfig {
    /// Font size for data rows.
    pub font_size: f32,
    /// Font size for the total/hero line.
    pub total_size: f32,
}

impl Default for RouteSummaryConfig {
    fn default() -> Self {
        Self {
            font_size: 11.0,
            total_size: 12.0,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the route summary panel.
pub fn show(ui: &mut Ui, data: &RouteSummaryData, config: &RouteSummaryConfig) {
    egui::Frame::new()
        .fill(theme::BG_SECONDARY)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .show(ui, |ui| {
            // Per-leg rows
            for leg in &data.legs {
                ui.horizontal(|ui| {
                    // Colored dot
                    let (dot_rect, _) =
                        ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    if ui.is_rect_visible(dot_rect) {
                        ui.painter()
                            .circle_filled(dot_rect.center(), 4.0, leg.color);
                    }

                    // DEX label
                    ui.label(
                        RichText::new(&leg.dex_label)
                            .color(theme::TEXT_SECONDARY)
                            .size(config.font_size),
                    );

                    // ADA input
                    let ada = leg.input_lovelace as f64 / 1_000_000.0;
                    ui.label(
                        RichText::new(format!("{ada:.0} ADA"))
                            .color(theme::TEXT_MUTED)
                            .size(config.font_size),
                    );

                    // Right-aligned: expected tokens
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format_tokens(leg.expected_tokens))
                                .color(theme::TEXT_PRIMARY)
                                .size(config.font_size),
                        );
                    });
                });
            }

            // Separator
            ui.add_space(4.0);
            let rect = ui.available_rect_before_wrap();
            let y = rect.min.y;
            ui.painter().line_segment(
                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                egui::Stroke::new(1.0, theme::BORDER),
            );
            ui.add_space(6.0);

            // Total output line
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Total")
                        .color(theme::TEXT_PRIMARY)
                        .strong()
                        .size(config.total_size),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(&data.token_name)
                            .color(theme::TEXT_MUTED)
                            .size(config.total_size),
                    );
                    ui.label(
                        RichText::new(format_tokens(data.total_tokens))
                            .color(theme::ACCENT_GREEN)
                            .strong()
                            .size(config.total_size),
                    );
                });
            });

            // Blended price
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Blended price")
                        .color(theme::TEXT_MUTED)
                        .size(config.font_size),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.6} ADA", data.blended_price))
                            .color(theme::TEXT_SECONDARY)
                            .size(config.font_size),
                    );
                });
            });

            // Improvement vs best single pool
            if let Some(single_tokens) = data.best_single_pool_tokens {
                if single_tokens > 0 && data.total_tokens > single_tokens {
                    let improvement =
                        (data.total_tokens as f64 / single_tokens as f64 - 1.0) * 100.0;
                    let extra = data.total_tokens - single_tokens;

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Split advantage")
                                .color(theme::TEXT_MUTED)
                                .size(config.font_size),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "+{} ({improvement:.2}%)",
                                    format_tokens(extra),
                                ))
                                .color(theme::ACCENT_GREEN)
                                .size(config.font_size),
                            );
                        });
                    });
                }
            }
        });
}

// ============================================================================
// Helpers
// ============================================================================

/// Format a token count with comma separators.
fn format_tokens(amount: u64) -> String {
    if amount == 0 {
        return "0".into();
    }
    let s = amount.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}
