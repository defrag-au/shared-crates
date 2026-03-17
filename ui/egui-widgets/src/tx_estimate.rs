//! Per-wallet transaction estimate widget — shows the local user's ADA impact.
//!
//! Displays a single-party breakdown of platform fee, network fee, min UTxO,
//! and net ADA change. Designed for the negotiation phase where both sides
//! haven't locked yet and the estimate is approximate.
//!
//! The existing [`crate::fee_report`] widget remains for the exact post-build
//! two-party report shown during signing.

use egui::RichText;

use crate::fee_report::format_lovelace;
use crate::icons::PhosphorIcon;
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Cost breakdown for the local user's side of a trade.
pub struct TxEstimateData {
    /// Platform fee in lovelace (0 if waived).
    pub platform_fee: u64,
    /// Estimated network TX fee share in lovelace.
    pub network_fee: u64,
    /// UTxO-related costs (min UTxO for receive outputs, change overhead, etc.).
    /// Outbound costs are negative ADA impact, inbound deposits are positive.
    /// Rolled up into summary lines in the display.
    pub utxo_costs: Vec<UtxoCost>,
    /// Net ADA impact on wallet (positive = gains ADA, negative = loses ADA).
    pub net_ada: i64,
    /// Whether the platform fee was waived (e.g. BF holder).
    pub waived: bool,
    /// Reason for waiver (e.g. "Black Flag holder").
    pub waiver_reason: Option<String>,
}

/// A single UTxO-related cost or deposit.
pub struct UtxoCost {
    /// Lovelace amount.
    pub lovelace: u64,
    /// Whether this is inbound (ADA arriving with peer's assets — not a cost).
    pub inbound: bool,
}

/// Configuration for the transaction estimate display.
pub struct TxEstimateConfig {
    /// Font size for line item text.
    pub font_size: f32,
    /// Font size for the rotated heading.
    pub heading_size: f32,
}

impl Default for TxEstimateConfig {
    fn default() -> Self {
        Self {
            font_size: 11.0,
            heading_size: 12.0,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the per-wallet transaction estimate panel.
///
/// Layout: a rotated "TX ESTIMATE" heading on the left edge with the cost
/// breakdown to its right. Uses a two-pass approach — first pass measures the
/// content height, second pass paints both the rotated text and the content.
pub fn show(ui: &mut egui::Ui, data: &TxEstimateData, config: &TxEstimateConfig) {
    let heading_width: f32 = 18.0;
    let heading_gap: f32 = 6.0;
    let content_margin = egui::Margin {
        left: 0,
        right: 14,
        top: 8,
        bottom: 8,
    };

    // Pass 1: measure content height using an invisible child UI
    let content_width = ui.available_width() - heading_width - heading_gap - content_margin.sum().x;
    let content_height = ui
        .scope(|ui| {
            let offscreen = egui::Rect::from_min_size(
                egui::pos2(-10000.0, -10000.0),
                egui::vec2(content_width, f32::INFINITY),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(offscreen));
            child.spacing_mut().item_spacing.y = 3.0;
            draw_cost_lines(&mut child, data, config);
            child.min_rect().height()
        })
        .inner;

    let total_height = content_height + content_margin.sum().y;
    let corner = 6.0;

    // Pass 2: paint for real — outer frame has no inner margin, we manage layout manually
    egui::Frame::new()
        .fill(theme::BG_SECONDARY)
        .corner_radius(corner)
        .inner_margin(egui::Margin {
            right: content_margin.right,
            ..Default::default()
        })
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;

                // Left strip: dark background with rotated heading, flush to frame edge
                let (heading_rect, _) = ui.allocate_exact_size(
                    egui::vec2(heading_width, total_height),
                    egui::Sense::hover(),
                );

                let strip_corner = corner as u8;
                // Deep indigo strip — distinct from the neutral BG tones
                const STRIP_BG: egui::Color32 = egui::Color32::from_rgb(24, 28, 56);
                ui.painter().rect_filled(
                    heading_rect,
                    egui::CornerRadius {
                        nw: strip_corner,
                        sw: strip_corner,
                        ..Default::default()
                    },
                    STRIP_BG,
                );

                // Layout the heading galley and paint it rotated -90°
                let font =
                    egui::FontId::new(config.heading_size - 1.0, egui::FontFamily::Proportional);
                let galley =
                    ui.painter()
                        .layout_no_wrap("TX ESTIMATE".into(), font, theme::TEXT_PRIMARY);

                // Rotate -90° around the text's center point.
                // with_angle_and_anchor keeps the anchor point (CENTER_CENTER)
                // at screen position `pos + a0` where a0 is the galley-local
                // center. So we set pos such that pos + a0 = heading_rect.center().
                let galley_center = galley.rect.center().to_vec2();
                let pos = heading_rect.center() - galley_center;
                let shape = egui::epaint::TextShape::new(pos, galley, theme::TEXT_PRIMARY)
                    .with_angle_and_anchor(
                        -std::f32::consts::FRAC_PI_2,
                        egui::Align2::CENTER_CENTER,
                    );
                ui.painter().add(shape);

                // Gap between strip and content
                ui.add_space(heading_gap);

                // Right side: cost lines with padding
                ui.vertical(|ui| {
                    ui.add_space(content_margin.top as f32);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    draw_cost_lines(ui, data, config);
                    ui.add_space(content_margin.bottom as f32);
                });
            });
        });
}

/// Draw the cost breakdown lines (shared between measure pass and paint pass).
fn draw_cost_lines(ui: &mut egui::Ui, data: &TxEstimateData, config: &TxEstimateConfig) {
    // Platform fee
    if data.waived {
        cost_line_waived(ui, config.font_size, data.waiver_reason.as_deref());
    } else {
        cost_line(
            ui,
            "Platform fee",
            &format_lovelace(data.platform_fee),
            theme::ACCENT_YELLOW,
            config.font_size,
        );
    }

    // Network fee
    if data.network_fee > 0 {
        cost_line(
            ui,
            "Network fee",
            &format!("~{}", format_lovelace(data.network_fee)),
            theme::TEXT_SECONDARY,
            config.font_size,
        );
    }

    // UTxO overhead: sum outbound costs and inbound deposits separately
    let outbound_total: u64 = data
        .utxo_costs
        .iter()
        .filter(|c| !c.inbound)
        .map(|c| c.lovelace)
        .sum();
    let inbound_total: u64 = data
        .utxo_costs
        .iter()
        .filter(|c| c.inbound)
        .map(|c| c.lovelace)
        .sum();

    if outbound_total > 0 {
        cost_line(
            ui,
            "UTxO overhead",
            &format!("~{}", format_lovelace(outbound_total)),
            theme::TEXT_SECONDARY,
            config.font_size,
        );
    }

    // Inbound min UTxO (not a cost — ADA arriving locked with peer's assets)
    if inbound_total > 0 {
        cost_line(
            ui,
            "UTxO deposit (in)",
            &format!("+~{}", format_lovelace(inbound_total)),
            theme::ACCENT_CYAN,
            config.font_size,
        );
    }

    // Separator
    ui.add_space(2.0);
    let rect = ui.available_rect_before_wrap();
    let y = rect.min.y;
    ui.painter().line_segment(
        [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
        egui::Stroke::new(1.0, theme::BORDER),
    );
    ui.add_space(4.0);

    // Net ADA — the hero line
    draw_net_ada(ui, data.net_ada, config.font_size + 1.0);
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Draw a single cost line: label left, value right.
fn cost_line(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    value_color: egui::Color32,
    font_size: f32,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .color(theme::TEXT_SECONDARY)
                .size(font_size),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).color(value_color).size(font_size));
        });
    });
}

/// Draw the platform fee line when waived.
fn cost_line_waived(ui: &mut egui::Ui, font_size: f32, reason: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Platform fee")
                .color(theme::TEXT_SECONDARY)
                .size(font_size),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(reason) = reason {
                ui.label(
                    RichText::new(format!("({reason})"))
                        .color(theme::TEXT_MUTED)
                        .size(font_size),
                );
            }
            ui.label(
                RichText::new("FREE")
                    .color(theme::ACCENT_GREEN)
                    .size(font_size)
                    .strong(),
            );
        });
    });
}

/// Draw the net ADA line with color coding and Phosphor arrow icon.
fn draw_net_ada(ui: &mut egui::Ui, lovelace: i64, font_size: f32) {
    let ada = lovelace as f64 / 1_000_000.0;
    let (sign, icon, color) = if lovelace > 0 {
        ("+", Some(PhosphorIcon::ArrowUp), theme::ACCENT_GREEN)
    } else if lovelace < 0 {
        ("", Some(PhosphorIcon::ArrowDown), theme::ACCENT_RED)
    } else {
        ("", None, theme::TEXT_MUTED)
    };

    let value_text = if ada.abs().fract() == 0.0 {
        format!("{sign}{} ADA", ada as i64)
    } else {
        format!("{sign}{ada:.2} ADA")
    };

    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Net ADA")
                .color(theme::TEXT_PRIMARY)
                .size(font_size)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(icon) = icon {
                ui.label(icon.rich_text(font_size - 2.0, color));
            }
            ui.label(
                RichText::new(value_text)
                    .color(color)
                    .size(font_size)
                    .strong(),
            );
        });
    });
}
