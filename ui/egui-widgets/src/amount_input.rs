//! ADA amount input widget with preset buttons and validation.
//!
//! Text input field with "ADA" suffix, optional MAX button (from wallet
//! balance), quick-select preset buttons, and input validation (min amount
//! warning, invalid text). Returns [`AmountInputAction`] so the caller can
//! react to changes.

use egui::{Color32, RichText};

use crate::buttons::UiButtonExt;
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Caller-owned state for the amount input.
pub struct AmountInputState {
    /// Current text in the input field.
    pub text: String,
    /// Parsed amount in lovelace (None if text is empty or invalid).
    pub lovelace: Option<u64>,
    /// Index of the currently selected preset (None if custom text).
    pub selected_preset: Option<usize>,
}

impl AmountInputState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            lovelace: None,
            selected_preset: None,
        }
    }
}

impl Default for AmountInputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for the amount input.
pub struct AmountInputConfig {
    /// Preset ADA amounts shown as quick-select buttons.
    pub presets: Vec<u64>,
    /// Maximum ADA available (wallet balance). Enables MAX button when set.
    pub max_ada: Option<f64>,
    /// Minimum ADA required for a valid swap.
    pub min_ada: f64,
    /// Accent color for selected/active states.
    pub accent: Color32,
}

impl Default for AmountInputConfig {
    fn default() -> Self {
        Self {
            presets: vec![100, 250, 500],
            max_ada: None,
            min_ada: 5.0,
            accent: theme::ACCENT,
        }
    }
}

/// Actions returned by the amount input.
#[derive(Debug, Clone, PartialEq)]
pub enum AmountInputAction {
    /// No change.
    None,
    /// Amount changed to a valid lovelace value.
    Changed(u64),
    /// Input was cleared or became invalid.
    Cleared,
}

/// Response from the amount input widget.
pub struct AmountInputResponse {
    /// Action to handle.
    pub action: AmountInputAction,
    /// The inner egui response from the text input.
    pub response: egui::Response,
}

// ============================================================================
// Widget
// ============================================================================

/// Render the amount input widget. Returns an action and the text field response.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut AmountInputState,
    config: &AmountInputConfig,
) -> AmountInputResponse {
    let mut action = AmountInputAction::None;

    // Preset buttons row
    ui.horizontal(|ui| {
        for (idx, &ada) in config.presets.iter().enumerate() {
            let is_selected = state.selected_preset == Some(idx);
            let label = format!("{ada} ADA");
            let btn = toggle_button(&label, is_selected, config.accent);

            if ui.add_clickable(btn).clicked() {
                state.selected_preset = Some(idx);
                state.text = format!("{ada}");
                let lovelace = ada * 1_000_000;
                state.lovelace = Some(lovelace);
                action = AmountInputAction::Changed(lovelace);
            }
        }

        // MAX button (only if balance is provided)
        if let Some(max_ada) = config.max_ada {
            let is_max = state.selected_preset.is_none()
                && state.lovelace == Some((max_ada * 1_000_000.0) as u64);

            let btn = if is_max {
                egui::Button::new(
                    RichText::new("MAX")
                        .color(theme::BG_PRIMARY)
                        .strong()
                        .size(10.0),
                )
                .fill(config.accent)
                .corner_radius(4.0)
                .min_size(egui::vec2(40.0, 28.0))
            } else {
                egui::Button::new(RichText::new("MAX").color(config.accent).size(10.0))
                    .fill(Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(1.0, config.accent))
                    .corner_radius(4.0)
                    .min_size(egui::vec2(40.0, 28.0))
            };

            if ui.add_clickable(btn).clicked() {
                state.selected_preset = None;
                let lovelace = (max_ada * 1_000_000.0) as u64;
                state.text = format_ada(lovelace);
                state.lovelace = Some(lovelace);
                action = AmountInputAction::Changed(lovelace);
            }
        }
    });

    ui.add_space(4.0);

    // Text input row
    let text_response = ui
        .horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.text)
                    .desired_width(140.0)
                    .font(egui::FontId::monospace(14.0))
                    .hint_text("Amount"),
            );
            ui.label(RichText::new("ADA").color(theme::TEXT_MUTED).size(12.0));

            if response.changed() {
                state.selected_preset = None;
                match parse_ada_input(&state.text) {
                    Some(lovelace) => {
                        state.lovelace = Some(lovelace);
                        action = AmountInputAction::Changed(lovelace);
                    }
                    None => {
                        state.lovelace = None;
                        action = AmountInputAction::Cleared;
                    }
                }
            }

            response
        })
        .inner;

    // Validation warnings
    if let Some(lovelace) = state.lovelace {
        let ada = lovelace as f64 / 1_000_000.0;
        if ada < config.min_ada {
            ui.label(
                RichText::new(format!("Minimum {:.0} ADA required", config.min_ada))
                    .color(theme::WARNING)
                    .size(10.0),
            );
        }
    } else if !state.text.is_empty() {
        ui.label(
            RichText::new("Enter a valid ADA amount")
                .color(theme::ERROR)
                .size(10.0),
        );
    }

    AmountInputResponse {
        action,
        response: text_response,
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Create a toggle-style button (filled when selected, outline when not).
fn toggle_button(label: &str, selected: bool, accent: Color32) -> egui::Button<'_> {
    if selected {
        egui::Button::new(
            RichText::new(label)
                .color(theme::BG_PRIMARY)
                .strong()
                .size(11.0),
        )
        .fill(accent)
        .corner_radius(4.0)
        .min_size(egui::vec2(70.0, 28.0))
    } else {
        egui::Button::new(RichText::new(label).color(accent).size(11.0))
            .fill(Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(1.0, accent))
            .corner_radius(4.0)
            .min_size(egui::vec2(70.0, 28.0))
    }
}

/// Parse ADA text input to lovelace. Accepts integers and decimals.
fn parse_ada_input(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let val: f64 = trimmed.parse().ok()?;
    if val <= 0.0 {
        return None;
    }
    Some((val * 1_000_000.0) as u64)
}

/// Format lovelace as an ADA string for display in the text field.
fn format_ada(lovelace: u64) -> String {
    let ada = lovelace as f64 / 1_000_000.0;
    if ada.fract() == 0.0 {
        format!("{}", ada as u64)
    } else {
        format!("{ada:.2}")
    }
}
