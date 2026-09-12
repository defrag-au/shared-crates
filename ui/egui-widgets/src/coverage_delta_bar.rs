//! Coverage delta bar — before/after progress bar for trait coverage.
//!
//! Renders a horizontal bar showing current coverage percentage and the
//! projected coverage after a trade, with a delta indicator.

use egui::{CornerRadius, Rect, RichText, Ui, Vec2};

use crate::theme::{Ink, Space, SpaceExt, ThemeExt, Token};

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
    pub before_color: Ink,
    /// Color for the positive delta region.
    pub gain_color: Ink,
    /// Color for the negative delta region (shown as striped/faded).
    pub loss_color: Ink,
    /// Background color for the unfilled region.
    pub bg_color: Ink,
    /// Corner radius.
    pub corner_radius: u8,
}

impl Default for CoverageDeltaConfig {
    fn default() -> Self {
        Self {
            bar_height: 14.0,
            font_size: 10.0,
            before_color: Ink::Token(Token::AccentBlue),
            gain_color: Ink::Token(Token::AccentGreen),
            loss_color: Ink::Token(Token::AccentRed),
            bg_color: Ink::Token(Token::BgSecondary),
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
    crate::install_phosphor_font(ui.ctx());

    // Resolved once, ahead of the closure that reads them: a `Default` config
    // names its tokens, so the values arrive here.
    let t = ui.tokens();
    let gain_color = config.gain_color.resolve(&t);
    let loss_color = config.loss_color.resolve(&t);
    let before_color = config.before_color.resolve(&t);
    let bg_color = config.bg_color.resolve(&t);

    let before = before.clamp(0.0, 1.0);
    let after = after.clamp(0.0, 1.0);
    let delta = after - before;

    // Label row: "Coverage: 67% -> 71% (+4%)"
    ui.horizontal(|ui| {
        if let Some(lbl) = label {
            ui.label(
                RichText::new(format!("{lbl}:"))
                    .color(ui.tokens().color.text_secondary)
                    .size(config.font_size),
            );
        }

        ui.label(
            RichText::new(format!("{:.0}%", before * 100.0))
                .color(ui.tokens().color.text_primary)
                .size(config.font_size),
        );

        crate::PhosphorIcon::ArrowRight.show(ui, config.font_size, ui.tokens().color.text_muted);

        let after_color = if delta >= 0.0 { gain_color } else { loss_color };

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

    ui.gap(Space::Xs);

    // Bar
    let (rect, _response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), config.bar_height),
        egui::Sense::hover(),
    );

    let painter = ui.painter_at(rect);
    let rounding = CornerRadius::same(config.corner_radius);

    // Background
    painter.rect_filled(rect, rounding, bg_color);

    let min_frac = before.min(after);
    let max_frac = before.max(after);

    // Common (before) fill — up to the lesser of before/after
    if min_frac > 0.0 {
        let fill_rect = Rect::from_min_max(
            rect.min,
            egui::pos2(rect.min.x + rect.width() * min_frac, rect.max.y),
        );
        painter.rect_filled(fill_rect, rounding, before_color);
    }

    // Delta region
    if (delta).abs() > f32::EPSILON {
        let delta_rect = Rect::from_min_max(
            egui::pos2(rect.min.x + rect.width() * min_frac, rect.min.y),
            egui::pos2(rect.min.x + rect.width() * max_frac, rect.max.y),
        );

        if delta > 0.0 {
            // Gain: green region between before and after
            painter.rect_filled(delta_rect, rounding, gain_color.linear_multiply(0.6));
        } else {
            // Loss: red striped region between after and before
            painter.rect_filled(delta_rect, rounding, loss_color.linear_multiply(0.4));
        }
    }
}
