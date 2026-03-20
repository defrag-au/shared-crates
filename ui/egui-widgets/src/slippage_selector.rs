//! Reusable slippage selector widget.
//!
//! Preset buttons (e.g. 0.5%, 1%, 3%) plus a custom input mode.
//! Shows warnings for unusually high or low slippage values.
//! Returns [`SlippageSelectorAction`] so the caller can react to changes.

use egui::{Color32, RichText};

use crate::buttons::UiButtonExt;
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Preset slippage option.
#[derive(Clone)]
pub struct SlippagePreset {
    /// Value in basis points (e.g. 50 = 0.5%).
    pub bps: u32,
    /// Display label (e.g. "0.5%").
    pub label: String,
}

/// Caller-owned state for the slippage selector.
pub struct SlippageSelectorState {
    /// Current slippage in basis points.
    pub slippage_bps: u32,
    /// Whether custom input mode is active.
    pub custom_active: bool,
    /// Text in the custom input field.
    pub custom_text: String,
}

impl SlippageSelectorState {
    /// Create a new state with a default slippage value.
    pub fn new(default_bps: u32) -> Self {
        Self {
            slippage_bps: default_bps,
            custom_active: false,
            custom_text: String::new(),
        }
    }
}

/// Configuration for the slippage selector.
pub struct SlippageSelectorConfig {
    /// Preset slippage options.
    pub presets: Vec<SlippagePreset>,
    /// Accent color for selected state.
    pub accent: Color32,
    /// Threshold in bps below which a "low slippage" warning is shown.
    pub warn_low_bps: u32,
    /// Threshold in bps above which a "high slippage" warning is shown.
    pub warn_high_bps: u32,
}

impl Default for SlippageSelectorConfig {
    fn default() -> Self {
        Self {
            presets: vec![
                SlippagePreset {
                    bps: 50,
                    label: "0.5%".into(),
                },
                SlippagePreset {
                    bps: 100,
                    label: "1%".into(),
                },
                SlippagePreset {
                    bps: 300,
                    label: "3%".into(),
                },
            ],
            accent: theme::ACCENT,
            warn_low_bps: 30,
            warn_high_bps: 500,
        }
    }
}

/// Actions returned by the slippage selector.
#[derive(Debug, Clone, PartialEq)]
pub enum SlippageSelectorAction {
    /// No change.
    None,
    /// Slippage value changed (new value in basis points).
    Changed(u32),
}

// ============================================================================
// Widget
// ============================================================================

/// Render the slippage selector. Returns an action if the value changed.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut SlippageSelectorState,
    config: &SlippageSelectorConfig,
) -> SlippageSelectorAction {
    let mut action = SlippageSelectorAction::None;

    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Slippage:")
                .color(theme::TEXT_SECONDARY)
                .size(11.0),
        );

        // Preset buttons
        for preset in &config.presets {
            let is_selected = !state.custom_active && state.slippage_bps == preset.bps;
            let btn = toggle_button(&preset.label, is_selected, config.accent);
            if ui.add_clickable(btn).clicked() {
                state.custom_active = false;
                if state.slippage_bps != preset.bps {
                    state.slippage_bps = preset.bps;
                    action = SlippageSelectorAction::Changed(preset.bps);
                }
            }
        }

        // Custom button
        let custom_btn = toggle_button("Custom", state.custom_active, config.accent);
        if ui.add_clickable(custom_btn).clicked() {
            state.custom_active = true;
            if state.custom_text.is_empty() {
                state.custom_text = format_bps(state.slippage_bps);
            }
        }

        // Custom input field (inline, next to button)
        if state.custom_active {
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.custom_text)
                    .desired_width(48.0)
                    .font(egui::FontId::monospace(11.0))
                    .hint_text("1.0"),
            );
            ui.label(RichText::new("%").color(theme::TEXT_MUTED).size(11.0));

            if response.changed() {
                if let Some(bps) = parse_percent_to_bps(&state.custom_text) {
                    if state.slippage_bps != bps {
                        state.slippage_bps = bps;
                        action = SlippageSelectorAction::Changed(bps);
                    }
                }
            }
        }
    });

    // Warnings
    if state.slippage_bps > 0 && state.slippage_bps < config.warn_low_bps {
        ui.label(
            RichText::new("Low slippage may cause transaction failure")
                .color(theme::WARNING)
                .size(10.0),
        );
    } else if state.slippage_bps > config.warn_high_bps {
        ui.label(
            RichText::new("High slippage — you may receive significantly fewer tokens")
                .color(theme::WARNING)
                .size(10.0),
        );
    }

    action
}

// ============================================================================
// Helpers
// ============================================================================

/// Create a toggle-style button (filled when selected, outline when not).
fn toggle_button<'a>(label: &'a str, selected: bool, accent: Color32) -> egui::Button<'a> {
    if selected {
        egui::Button::new(
            RichText::new(label)
                .color(theme::BG_PRIMARY)
                .strong()
                .size(10.0),
        )
        .fill(accent)
        .corner_radius(4.0)
        .min_size(egui::vec2(36.0, 22.0))
    } else {
        egui::Button::new(RichText::new(label).color(theme::TEXT_MUTED).size(10.0))
            .fill(Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(1.0, theme::TEXT_MUTED))
            .corner_radius(4.0)
            .min_size(egui::vec2(36.0, 22.0))
    }
}

/// Parse a percentage string (e.g. "1.5") into basis points (150).
fn parse_percent_to_bps(text: &str) -> Option<u32> {
    let pct: f64 = text.trim().parse().ok()?;
    if !(0.0..=100.0).contains(&pct) {
        return None;
    }
    Some((pct * 100.0).round() as u32)
}

/// Format basis points as a percentage string (e.g. 150 -> "1.5").
fn format_bps(bps: u32) -> String {
    let pct = bps as f64 / 100.0;
    if pct.fract() == 0.0 {
        format!("{}", pct as u32)
    } else {
        format!("{pct:.1}")
    }
}
