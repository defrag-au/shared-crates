//! `QuantityStepper` — a compact `−  [n]  +` control with min/max clamping.
//!
//! Self-contained, fixed-height row: two square buttons either side of a centred
//! readout box, all on one baseline. The `−` button disables at `min`, `+` at
//! `max`. Caller owns the value; `show` returns the (possibly clamped) new value
//! and whether it changed this frame — no internal state, no async.
//!
//! ```ignore
//! let resp = QuantityStepper::new(qty).range(1, max_per_wallet).show(ui);
//! if resp.changed { qty = resp.value; }
//! ```

use egui::{Color32, RichText, Ui, Vec2};

use crate::icons::PhosphorIcon;
use crate::theme;

/// A `−  [n]  +` quantity control.
pub struct QuantityStepper {
    value: u32,
    min: u32,
    max: u32,
    button_size: f32,
    readout_width: f32,
    accent: Color32,
}

/// What the caller does with the stepper's outcome.
pub struct QuantityStepperResponse {
    /// The value after this frame's interaction, clamped to `[min, max]`.
    pub value: u32,
    /// `true` if the value changed this frame (button click).
    pub changed: bool,
}

impl QuantityStepper {
    /// New stepper at `value`. Defaults: range `1..=u32::MAX`, 40px buttons,
    /// green readout.
    pub fn new(value: u32) -> Self {
        Self {
            value,
            min: 1,
            max: u32::MAX,
            button_size: 40.0,
            readout_width: 56.0,
            accent: theme::ACCENT_GREEN,
        }
    }

    /// Inclusive `[min, max]` clamp range. `max` is floored to `min`.
    pub fn range(mut self, min: u32, max: u32) -> Self {
        self.min = min;
        self.max = max.max(min);
        self
    }

    /// Square button edge length in px (default 40). The readout matches height.
    pub fn button_size(mut self, size: f32) -> Self {
        self.button_size = size;
        self
    }

    /// Readout box width in px (default 56).
    pub fn readout_width(mut self, width: f32) -> Self {
        self.readout_width = width;
        self
    }

    /// Readout number colour (default `ACCENT_GREEN`).
    pub fn accent(mut self, accent: Color32) -> Self {
        self.accent = accent;
        self
    }

    pub fn show(self, ui: &mut Ui) -> QuantityStepperResponse {
        crate::install_phosphor_font(ui.ctx());

        let mut value = self.value.clamp(self.min, self.max);
        let start = value;
        let btn = Vec2::splat(self.button_size);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            // − (disabled at min)
            let dec = ui.add_enabled(
                value > self.min,
                egui::Button::new(PhosphorIcon::Minus.rich_text(16.0, theme::TEXT_PRIMARY))
                    .min_size(btn)
                    .corner_radius(6.0),
            );
            if dec.clicked() {
                value = value.saturating_sub(1).max(self.min);
            }
            if dec.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }

            // [n] readout — fixed box, centred, same height as the buttons.
            egui::Frame::new()
                .fill(theme::BG_PRIMARY)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .corner_radius(6.0)
                .show(ui, |ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(self.readout_width, self.button_size),
                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            ui.label(
                                RichText::new(value.to_string())
                                    .size(20.0)
                                    .strong()
                                    .color(self.accent),
                            );
                        },
                    );
                });

            // + (disabled at max)
            let inc = ui.add_enabled(
                value < self.max,
                egui::Button::new(PhosphorIcon::Plus.rich_text(16.0, theme::TEXT_PRIMARY))
                    .min_size(btn)
                    .corner_radius(6.0),
            );
            if inc.clicked() {
                value = (value + 1).min(self.max);
            }
            if inc.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        });

        QuantityStepperResponse {
            value,
            changed: value != start,
        }
    }
}
