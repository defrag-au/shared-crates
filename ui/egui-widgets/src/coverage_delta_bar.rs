//! Coverage delta bar — before/after progress bar for trait coverage.
//!
//! Renders a horizontal bar showing current coverage percentage and the
//! projected coverage after a trade, with a delta indicator.

use egui::{Color32, CornerRadius, Rect, RichText, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Configuration for the coverage delta bar.
pub struct CoverageDeltaConfig {
    /// Height of the progress bar in pixels.
    pub bar_height: f32,
    /// Font size for labels.
    pub font_size: f32,
    /// Color for the "before" fill.
    pub before_color: Color32,
    /// Color for the positive delta region.
    pub gain_color: Color32,
    /// Color for the negative delta region (shown as striped/faded).
    pub loss_color: Color32,
    /// Background color for the unfilled region.
    pub bg_color: Color32,
    /// Corner radius.
    pub corner_radius: u8,
}

impl Default for CoverageDeltaConfig {
    fn default() -> Self {
        Self {
            bar_height: 14.0,
            font_size: 10.0,
            before_color: theme::ACCENT_BLUE,
            gain_color: theme::ACCENT_GREEN,
            loss_color: theme::ACCENT_RED,
            bg_color: theme::BG_SECONDARY,
            corner_radius: 3,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render a coverage delta bar.
///
/// - `before`: current coverage as a fraction (0.0..=1.0)
/// - `after`: projected coverage after trade (0.0..=1.0)
/// - `label`: optional label prefix (e.g. "Coverage")
pub fn show(
    ui: &mut Ui,
    before: f32,
    after: f32,
    label: Option<&str>,
    config: &CoverageDeltaConfig,
) {
    let before = before.clamp(0.0, 1.0);
    let after = after.clamp(0.0, 1.0);
    let delta = after - before;

    // Label row: "Coverage: 67% → 71% (+4%)"
    ui.horizontal(|ui| {
        if let Some(lbl) = label {
            ui.label(
                RichText::new(format!("{lbl}:"))
                    .color(theme::TEXT_SECONDARY)
                    .size(config.font_size),
            );
        }

        ui.label(
            RichText::new(format!("{:.0}%", before * 100.0))
                .color(theme::TEXT_PRIMARY)
                .size(config.font_size),
        );

        ui.label(
            RichText::new("\u{2192}")
                .color(theme::TEXT_MUTED)
                .size(config.font_size),
        );

        let after_color = if delta >= 0.0 {
            config.gain_color
        } else {
            config.loss_color
        };

        ui.label(
            RichText::new(format!("{:.0}%", after * 100.0))
                .color(after_color)
                .size(config.font_size),
        );

        let sign = if delta >= 0.0 { "+" } else { "" };
        ui.label(
            RichText::new(format!("({sign}{:.0}%)", delta * 100.0))
                .color(after_color)
                .size(config.font_size),
        );
    });

    ui.add_space(2.0);

    // Bar
    let (rect, _response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), config.bar_height),
        egui::Sense::hover(),
    );

    let painter = ui.painter_at(rect);
    let rounding = CornerRadius::same(config.corner_radius);

    // Background
    painter.rect_filled(rect, rounding, config.bg_color);

    let min_frac = before.min(after);
    let max_frac = before.max(after);

    // Common (before) fill — up to the lesser of before/after
    if min_frac > 0.0 {
        let fill_rect = Rect::from_min_max(
            rect.min,
            egui::pos2(rect.min.x + rect.width() * min_frac, rect.max.y),
        );
        painter.rect_filled(fill_rect, rounding, config.before_color);
    }

    // Delta region
    if (delta).abs() > f32::EPSILON {
        let delta_rect = Rect::from_min_max(
            egui::pos2(rect.min.x + rect.width() * min_frac, rect.min.y),
            egui::pos2(rect.min.x + rect.width() * max_frac, rect.max.y),
        );

        if delta > 0.0 {
            // Gain: green region between before and after
            painter.rect_filled(delta_rect, rounding, config.gain_color.linear_multiply(0.6));
        } else {
            // Loss: red striped region between after and before
            painter.rect_filled(delta_rect, rounding, config.loss_color.linear_multiply(0.4));
        }
    }
}
