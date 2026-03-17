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
    /// Fee in lovelace (0 if waived).
    pub fee_lovelace: u64,
    /// Whether this side's fee was waived (e.g. BF holder).
    pub waived: bool,
    /// Reason for waiver (e.g. "Black Flag holder").
    pub waiver_reason: Option<String>,
}

/// Full fee report for the trade.
pub struct FeeReportData {
    pub sides: Vec<SideFeeData>,
    /// Total platform fee in lovelace.
    pub total_lovelace: u64,
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
    egui::Frame::new()
        .fill(theme::BG_SECONDARY)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .show(ui, |ui| {
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
                    draw_side_inline(ui, side, config.font_size);
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
                        RichText::new(format!(
                            "Total: {}",
                            format_lovelace(data.total_lovelace)
                        ))
                        .color(theme::TEXT_MUTED)
                        .size(config.font_size),
                    );
                }
            });
        });
}

fn draw_side_inline(ui: &mut egui::Ui, side: &SideFeeData, font_size: f32) {
    ui.label(
        RichText::new(format!("{}:", side.label))
            .color(theme::TEXT_PRIMARY)
            .size(font_size),
    );

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

fn format_lovelace(lovelace: u64) -> String {
    let ada = lovelace as f64 / 1_000_000.0;
    if ada.fract() == 0.0 {
        format!("{} ADA", ada as u64)
    } else {
        format!("{ada:.2} ADA")
    }
}
