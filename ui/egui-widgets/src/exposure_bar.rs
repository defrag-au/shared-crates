//! Exposure bar — stacked horizontal bar showing total ADA exposure
//! segmented by collateral token, colored by LTV risk.
//!
//! Each segment's width is proportional to its principal fraction. Fill color
//! encodes LTV risk (green < 50%, amber < 80%, red >= 80%). Token identity
//! color appears on the legend dots. Optional "Total Exposure" header above.

use egui::{Color32, CornerRadius, Rect, RichText, Sense, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// A single segment in the exposure bar.
pub struct ExposureSegment {
    /// Token label (e.g. "NIGHT", "SNEK").
    pub label: String,
    /// ADA principal deployed in this token (lovelace).
    pub principal_lovelace: u64,
    /// Fraction of total exposure (0.0..=1.0).
    pub fraction: f32,
    /// LTV percentage — determines bar segment fill color.
    pub ltv_pct: f64,
    /// Token identity color (used for legend dots).
    pub color: Color32,
}

/// Configuration for the exposure bar.
pub struct ExposureBarConfig {
    /// Bar height in pixels.
    pub bar_height: f32,
    /// Corner radius.
    pub corner_radius: u8,
    /// Background color for the track.
    pub bg_color: Color32,
    /// Whether to show the legend row below the bar.
    pub show_legend: bool,
    /// Whether to show "Total Exposure: X ADA" header above the bar.
    pub show_total: bool,
    /// Minimum segment width (pixels) to show percentage label inside.
    pub min_label_width: f32,
    /// Font size for percentage labels inside segments.
    pub label_size: f32,
    /// Font size for legend text.
    pub legend_size: f32,
    /// Font size for total exposure header.
    pub total_size: f32,
}

impl Default for ExposureBarConfig {
    fn default() -> Self {
        Self {
            bar_height: 24.0,
            corner_radius: 4,
            bg_color: theme::BG_SECONDARY,
            show_legend: true,
            show_total: true,
            min_label_width: 40.0,
            label_size: 10.0,
            legend_size: 10.0,
            total_size: 12.0,
        }
    }
}

// ============================================================================
// LTV risk color
// ============================================================================

/// Map LTV percentage to a risk color.
///
/// - Green (< 50%): well-collateralised
/// - Amber (< 80%): moderate risk
/// - Red (>= 80%): high risk / under-collateralised
pub fn ltv_risk_color(ltv_pct: f64) -> Color32 {
    if ltv_pct < 50.0 {
        theme::SUCCESS
    } else if ltv_pct < 80.0 {
        theme::WARNING
    } else {
        theme::ERROR
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the exposure bar.
pub fn show(ui: &mut Ui, segments: &[ExposureSegment], config: &ExposureBarConfig) {
    if segments.is_empty() {
        return;
    }

    // Total exposure header
    if config.show_total {
        let total_lovelace: u64 = segments.iter().map(|s| s.principal_lovelace).sum();
        ui.label(
            RichText::new(format!(
                "Total Exposure: {}",
                crate::utils::format_lovelace(total_lovelace as i64)
            ))
            .color(theme::TEXT_PRIMARY)
            .size(config.total_size)
            .strong(),
        );
        ui.add_space(4.0);
    }

    let available_width = ui.available_width();
    let desired_size = Vec2::new(available_width, config.bar_height);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let rounding = CornerRadius::same(config.corner_radius);

        // Background track
        painter.rect_filled(rect, rounding, config.bg_color);

        // Paint segments left to right — fill color from LTV risk
        let mut x = rect.min.x;
        for (i, seg) in segments.iter().enumerate() {
            if seg.fraction <= 0.0 {
                continue;
            }

            let seg_width = rect.width() * seg.fraction;
            let seg_rect = Rect::from_min_size(
                egui::pos2(x, rect.min.y),
                Vec2::new(seg_width, rect.height()),
            );

            // Corner rounding: first visible gets left, last visible gets right
            let is_first = i == 0 || segments[..i].iter().all(|s| s.fraction <= 0.0);
            let is_last =
                i == segments.len() - 1 || segments[i + 1..].iter().all(|s| s.fraction <= 0.0);

            let cr = config.corner_radius;
            let seg_rounding = CornerRadius {
                nw: if is_first { cr } else { 0 },
                sw: if is_first { cr } else { 0 },
                ne: if is_last { cr } else { 0 },
                se: if is_last { cr } else { 0 },
            };

            // Fill color from LTV risk — this is the key difference from SplitAllocationBar
            let fill_color = ltv_risk_color(seg.ltv_pct);
            painter.rect_filled(seg_rect, seg_rounding, fill_color);

            // Percentage label inside segment (if wide enough)
            if seg_width >= config.min_label_width {
                let pct_text = format!("{}%", (seg.fraction * 100.0).round() as u32);
                painter.text(
                    seg_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    pct_text,
                    egui::FontId::proportional(config.label_size),
                    theme::BG_PRIMARY,
                );
            }

            x += seg_width;
        }
    }

    // Hover tooltips — detect which segment the pointer is over
    if response.hovered() {
        if let Some(pointer) = ui.ctx().pointer_hover_pos() {
            let rel_x = pointer.x - rect.min.x;
            let frac_x = rel_x / rect.width();
            let mut cumulative = 0.0;
            for seg in segments {
                cumulative += seg.fraction;
                if frac_x <= cumulative {
                    let ada = seg.principal_lovelace as f64 / 1_000_000.0;
                    response.clone().on_hover_text(format!(
                        "{}: {ada:.0} ADA ({:.1}% LTV)",
                        seg.label, seg.ltv_pct,
                    ));
                    break;
                }
            }
        }
    }

    // Legend row — token identity color dots + label + ADA + LTV
    if config.show_legend {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for seg in segments {
                if seg.fraction <= 0.0 {
                    continue;
                }
                // Colored dot using token identity color
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                if ui.is_rect_visible(dot_rect) {
                    ui.painter()
                        .circle_filled(dot_rect.center(), 4.0, seg.color);
                }
                let ada = seg.principal_lovelace as f64 / 1_000_000.0;
                ui.label(
                    RichText::new(format!("{} {ada:.0} ADA ({:.1}%)", seg.label, seg.ltv_pct))
                        .color(theme::TEXT_SECONDARY)
                        .size(config.legend_size),
                );
                ui.add_space(8.0);
            }
        });
    }
}
