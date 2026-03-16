//! Trait delta widget — shows traits gained and lost in a trade.
//!
//! Renders two sections: gains (green, +) and losses (red, −), each as a list
//! of `Category: Value` chips. Designed for the Trade Desk to visualize the
//! trait impact of a proposed swap at a glance.

use egui::{Color32, RichText, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// A single trait with category and value labels.
#[derive(Clone, Debug)]
pub struct TraitItem {
    /// E.g. "Background"
    pub category: String,
    /// E.g. "Purple"
    pub value: String,
}

impl TraitItem {
    pub fn new(category: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            value: value.into(),
        }
    }
}

/// Configuration for the trait delta display.
pub struct TraitDeltaConfig {
    /// Font size for trait chips.
    pub font_size: f32,
    /// Color for gain chips.
    pub gain_color: Color32,
    /// Color for loss chips.
    pub loss_color: Color32,
    /// Spacing between chips.
    pub chip_spacing: f32,
}

impl Default for TraitDeltaConfig {
    fn default() -> Self {
        Self {
            font_size: 10.0,
            gain_color: theme::ACCENT_GREEN,
            loss_color: theme::ACCENT_RED,
            chip_spacing: 4.0,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the trait delta display.
///
/// Shows gained and lost traits as colored chips. Gains are prefixed with `+`
/// in the gain color, losses with `-` in the loss color. No headings or
/// informational text — just the data.
pub fn show(ui: &mut Ui, gains: &[TraitItem], losses: &[TraitItem], config: &TraitDeltaConfig) {
    if !gains.is_empty() {
        draw_chips(ui, "+", gains, config.gain_color, config);
    }

    if !gains.is_empty() && !losses.is_empty() {
        ui.add_space(4.0);
    }

    if !losses.is_empty() {
        draw_chips(ui, "-", losses, config.loss_color, config);
    }
}

fn draw_chips(
    ui: &mut Ui,
    prefix: &str,
    traits: &[TraitItem],
    color: Color32,
    config: &TraitDeltaConfig,
) {
    // Flow-wrap trait chips horizontally
    let available = ui.available_width();
    let mut cursor_x = 0.0_f32;

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(config.chip_spacing, config.chip_spacing);

        for item in traits {
            let label = format!("{prefix} {}: {}", item.category, item.value);
            let chip_text = RichText::new(&label).color(color).size(config.font_size);

            // Estimate chip width for wrapping
            let approx_width = label.len() as f32 * config.font_size * 0.55 + 16.0;
            if cursor_x + approx_width > available && cursor_x > 0.0 {
                ui.end_row();
                cursor_x = 0.0;
            }

            let bg =
                Color32::from_rgba_premultiplied(color.r() / 6, color.g() / 6, color.b() / 6, 40);

            egui::Frame::new()
                .fill(bg)
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 2))
                .stroke(egui::Stroke::new(1.0, color.linear_multiply(0.3)))
                .show(ui, |ui| {
                    ui.label(chip_text);
                });

            cursor_x += approx_width + config.chip_spacing;
        }
    });
}
