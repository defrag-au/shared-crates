//! Trait delta widget — shows traits gained and lost in a trade.
//!
//! Renders two sections: gains (green, +) and losses (red, −), each as a list
//! of `Category: Value` chips. Designed for the Trade Desk to visualize the
//! trait impact of a proposed swap at a glance.

use egui::{Color32, RichText, Ui, Vec2};

use crate::theme::{Radius, Space, SpaceExt, ThemeExt};

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
    /// `None` asks the theme at render time — a `Default` has no `Ui` to ask, and
    /// baking a colour here would put this widget beyond a theme's reach.
    pub gain_color: Option<Color32>,
    /// Color for loss chips.
    pub loss_color: Option<Color32>,
    /// Gutter between chips. `None` takes the theme's [`Space::Sm`] — same
    /// reasoning as the colours above.
    pub chip_spacing: Option<Space>,
}

impl Default for TraitDeltaConfig {
    fn default() -> Self {
        Self {
            font_size: 10.0,
            gain_color: None,
            loss_color: None,
            chip_spacing: None,
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
        let gain = config.gain_color.unwrap_or(ui.tokens().color.accent_green);
        draw_chips(ui, "+", gains, gain, config);
    }

    if !gains.is_empty() && !losses.is_empty() {
        ui.gap(Space::Sm);
    }

    if !losses.is_empty() {
        let loss = config.loss_color.unwrap_or(ui.tokens().color.accent_red);
        draw_chips(ui, "-", losses, loss, config);
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
    // One value, because the wrap arithmetic below has to agree with the gap
    // egui actually lays out.
    let gap = ui.space(config.chip_spacing.unwrap_or(Space::Sm));

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(gap);

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
                .corner_radius(ui.tokens().corner(Radius::Base))
                .inner_margin(ui.tokens().margin_xy(Space::Base, Space::Xs))
                .stroke(egui::Stroke::new(1.0_f32, color.linear_multiply(0.3)))
                .show(ui, |ui| {
                    ui.label(chip_text);
                });

            cursor_x += approx_width + gap;
        }
    });
}
