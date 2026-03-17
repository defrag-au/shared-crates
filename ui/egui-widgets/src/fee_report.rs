//! Fee report widget — displays per-side fee breakdown for a trade.
//!
//! Pure display component with no user interaction. The caller maps their
//! domain data into [`FeeReportData`] and the widget renders the breakdown.

use egui::RichText;

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Fee for one side of the trade.
pub struct SideFeeData {
    /// Display label — "You" or the peer's name/handle.
    pub label: String,
    /// Platform fee in lovelace (0 if waived).
    pub fee_lovelace: u64,
    /// Whether this side's fee was waived (e.g. BF holder).
    pub waived: bool,
    /// Reason for waiver (e.g. "Black Flag holder").
    pub waiver_reason: Option<String>,
    /// This side's share of the network TX fee (if known).
    pub network_fee_share: Option<u64>,
    /// Min UTxO lovelace this side must fund for their asset output (if known).
    pub min_utxo_cost: Option<u64>,
    /// Net ADA gain/loss after all costs (if known).
    pub net_ada: Option<i64>,
}

/// Full fee report for the trade.
pub struct FeeReportData {
    pub sides: Vec<SideFeeData>,
    /// Total platform fee in lovelace.
    pub total_lovelace: u64,
    /// Total network TX fee in lovelace (if known).
    pub network_fee: Option<u64>,
}

/// Configuration for the fee report display.
pub struct FeeReportConfig {
    pub font_size: f32,
    pub heading_size: f32,
}

impl Default for FeeReportConfig {
    fn default() -> Self {
        Self {
            font_size: 10.0,
            heading_size: 12.0,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the fee report panel.
pub fn show(ui: &mut egui::Ui, data: &FeeReportData, config: &FeeReportConfig) {
    let has_breakdown = data.sides.iter().any(|s| s.net_ada.is_some());

    egui::Frame::new()
        .fill(theme::BG_SECONDARY)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .show(ui, |ui| {
            if has_breakdown {
                draw_detailed(ui, data, config);
            } else {
                draw_compact(ui, data, config);
            }
        });
}

/// Compact single-line display (backwards compat — before TX is built).
fn draw_compact(ui: &mut egui::Ui, data: &FeeReportData, config: &FeeReportConfig) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("FEES")
                .color(theme::TEXT_MUTED)
                .size(config.heading_size)
                .strong(),
        );

        ui.add_space(8.0);

        for (i, side) in data.sides.iter().enumerate() {
            if i > 0 {
                ui.label(
                    RichText::new("|")
                        .color(theme::TEXT_MUTED)
                        .size(config.font_size),
                );
            }
            draw_side_platform_fee(ui, side, config.font_size);
        }

        ui.add_space(8.0);

        if data.total_lovelace == 0 {
            ui.label(
                RichText::new("No platform fees!")
                    .color(theme::ACCENT_GREEN)
                    .size(config.font_size)
                    .strong(),
            );
        } else {
            ui.label(
                RichText::new(format!("Total: {}", format_lovelace(data.total_lovelace)))
                    .color(theme::TEXT_MUTED)
                    .size(config.font_size),
            );
        }
    });
}

/// Detailed multi-line display with full cost breakdown.
fn draw_detailed(ui: &mut egui::Ui, data: &FeeReportData, config: &FeeReportConfig) {
    ui.label(
        RichText::new("COSTS")
            .color(theme::TEXT_MUTED)
            .size(config.heading_size)
            .strong(),
    );

    ui.add_space(4.0);

    for side in &data.sides {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            ui.label(
                RichText::new(format!("{}:", side.label))
                    .color(theme::TEXT_PRIMARY)
                    .size(config.font_size)
                    .strong(),
            );

            // Platform fee
            draw_side_platform_fee(ui, side, config.font_size);

            // Network fee
            if let Some(net_fee) = side.network_fee_share {
                if net_fee > 0 {
                    ui.label(
                        RichText::new("·")
                            .color(theme::TEXT_MUTED)
                            .size(config.font_size),
                    );
                    ui.label(
                        RichText::new(format!("Network {}", format_lovelace(net_fee)))
                            .color(theme::TEXT_MUTED)
                            .size(config.font_size),
                    );
                }
            }

            // Min UTxO cost
            if let Some(utxo_cost) = side.min_utxo_cost {
                if utxo_cost > 0 {
                    ui.label(
                        RichText::new("·")
                            .color(theme::TEXT_MUTED)
                            .size(config.font_size),
                    );
                    ui.label(
                        RichText::new(format!("UTxO {}", format_lovelace(utxo_cost)))
                            .color(theme::TEXT_MUTED)
                            .size(config.font_size),
                    );
                }
            }
        });
    }

    // Net ADA line
    let any_net = data.sides.iter().any(|s| s.net_ada.is_some());
    if any_net {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            ui.label(
                RichText::new("Net:")
                    .color(theme::TEXT_MUTED)
                    .size(config.font_size)
                    .strong(),
            );

            for side in &data.sides {
                if let Some(net) = side.net_ada {
                    let (text, color) = format_net_ada(&side.label, net);
                    ui.label(RichText::new(text).color(color).size(config.font_size));
                }
            }
        });
    }
}

/// Draw the platform fee for a single side (shared between compact and detailed).
fn draw_side_platform_fee(ui: &mut egui::Ui, side: &SideFeeData, font_size: f32) {
    if side.waived {
        ui.label(
            RichText::new("FREE")
                .color(theme::ACCENT_GREEN)
                .size(font_size)
                .strong(),
        );
        if let Some(reason) = &side.waiver_reason {
            ui.label(
                RichText::new(format!("({reason})"))
                    .color(theme::TEXT_MUTED)
                    .size(font_size),
            );
        }
    } else {
        ui.label(
            RichText::new(format_lovelace(side.fee_lovelace))
                .color(theme::ACCENT_YELLOW)
                .size(font_size),
        );
    }
}

/// Format net ADA with color coding: green for positive, red for negative.
fn format_net_ada(label: &str, lovelace: i64) -> (String, egui::Color32) {
    let ada = lovelace as f64 / 1_000_000.0;
    let sign = if lovelace >= 0 { "+" } else { "" };
    let text = if ada.abs().fract() == 0.0 {
        format!("{label} {sign}{} ADA", ada as i64)
    } else {
        format!("{label} {sign}{ada:.2} ADA")
    };
    let color = if lovelace > 0 {
        theme::ACCENT_GREEN
    } else if lovelace < 0 {
        theme::ACCENT_RED
    } else {
        theme::TEXT_MUTED
    };
    (text, color)
}

fn format_lovelace(lovelace: u64) -> String {
    let ada = lovelace as f64 / 1_000_000.0;
    if ada.fract() == 0.0 {
        format!("{} ADA", ada as u64)
    } else {
        format!("{ada:.2} ADA")
    }
}
