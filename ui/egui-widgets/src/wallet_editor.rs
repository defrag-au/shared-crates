//! Wallet bundle editor widget.
//!
//! A reusable component for managing a list of wallet entries (stake addresses
//! or $handles). The widget is UI-only — it emits [`WalletEditorAction`]s and
//! the caller handles async resolution, persistence, and data fetching.

use egui::{Color32, RichText};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Visual state of a wallet entry.
#[derive(Clone, Debug)]
pub enum WalletEntryStatus {
    /// Handle lookup in progress.
    Resolving,
    /// Fetching bundle data.
    Loading,
    /// Bundle processed — data ready.
    Ready,
    /// Something went wrong.
    Failed(String),
}

/// A single wallet entry for display.
#[derive(Clone)]
pub struct WalletEditorEntry {
    /// Display text — "$handle" or truncated stake address.
    pub display: String,
    /// Current visual state.
    pub status: WalletEntryStatus,
    /// True for auto-connected browser wallets.
    pub is_browser_wallet: bool,
    /// Accent color for the label (green for manual, cyan for browser).
    pub accent: Color32,
}

/// Configuration for the editor appearance.
pub struct WalletEditorConfig {
    pub heading: &'static str,
    pub subtitle: Option<&'static str>,
    pub placeholder: &'static str,
    pub font_size: f32,
    pub heading_size: f32,
}

impl Default for WalletEditorConfig {
    fn default() -> Self {
        Self {
            heading: "My Wallets",
            subtitle: None,
            placeholder: "stake1... or $handle",
            font_size: 10.0,
            heading_size: 12.0,
        }
    }
}

/// Persistent widget state.
#[derive(Default)]
pub struct WalletEditorState {
    /// Text input buffer.
    pub input: String,
}

/// Action emitted by the widget.
#[derive(Debug)]
pub enum WalletEditorAction {
    /// User submitted a new wallet input (stake address or $handle).
    Add(String),
    /// User clicked remove on entry at the given index.
    Remove(usize),
}

/// Response from a single frame.
pub struct WalletEditorResponse {
    /// Action to handle, if any.
    pub action: Option<WalletEditorAction>,
}

// ============================================================================
// Widget
// ============================================================================

/// Render the wallet bundle editor.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut WalletEditorState,
    entries: &[WalletEditorEntry],
    config: &WalletEditorConfig,
) -> WalletEditorResponse {
    crate::install_phosphor_font(ui.ctx());

    let mut action: Option<WalletEditorAction> = None;

    // Heading
    ui.label(
        RichText::new(config.heading)
            .color(theme::TEXT_SECONDARY)
            .size(config.heading_size),
    );
    if let Some(subtitle) = config.subtitle {
        ui.add_space(2.0);
        ui.label(
            RichText::new(subtitle)
                .color(theme::TEXT_MUTED)
                .size(config.font_size),
        );
    }
    ui.add_space(6.0);

    // Input row
    let mut submitted = false;
    ui.horizontal(|ui| {
        let input_resp = ui.add(
            egui::TextEdit::singleline(&mut state.input)
                .hint_text(config.placeholder)
                .desired_width(ui.available_width() - 32.0)
                .font(egui::FontId::monospace(config.font_size)),
        );
        if input_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            submitted = true;
        }
        if ui
            .add(
                egui::Button::new(crate::PhosphorIcon::Plus.rich_text(14.0, theme::ACCENT_CYAN))
                    .frame(false),
            )
            .clicked()
        {
            submitted = true;
        }
    });

    if submitted && !state.input.trim().is_empty() {
        let trimmed = state.input.trim();
        // Auto-prepend $ for handle lookups — anything that isn't a bech32 address
        let value = if trimmed.starts_with('$')
            || trimmed.starts_with("stake1")
            || trimmed.starts_with("addr1")
        {
            trimmed.to_string()
        } else {
            format!("${trimmed}")
        };
        state.input.clear();
        action = Some(WalletEditorAction::Add(value));
    }

    ui.add_space(4.0);

    // Entry list
    for (i, entry) in entries.iter().enumerate() {
        let row_rect = ui.available_rect_before_wrap();
        let row_rect =
            egui::Rect::from_min_size(row_rect.min, egui::vec2(ui.available_width(), 20.0));

        // Hover highlight
        let hovered = ui.rect_contains_pointer(row_rect);
        if hovered {
            ui.painter().rect_filled(row_rect, 2.0, theme::BG_HIGHLIGHT);
        }

        ui.horizontal(|ui| {
            ui.set_height(20.0);

            // Status indicator
            match &entry.status {
                WalletEntryStatus::Resolving | WalletEntryStatus::Loading => {
                    ui.spinner();
                }
                WalletEntryStatus::Ready => {
                    ui.label(
                        RichText::new("\u{2022}")
                            .color(theme::ACCENT_GREEN)
                            .size(14.0),
                    );
                }
                WalletEntryStatus::Failed(_) => {
                    ui.label(
                        RichText::new("!")
                            .color(theme::ACCENT_RED)
                            .size(12.0)
                            .strong(),
                    );
                }
            }

            // Display label
            ui.label(
                RichText::new(&entry.display)
                    .color(entry.accent)
                    .size(config.font_size),
            );

            // Browser wallet badge
            if entry.is_browser_wallet {
                ui.label(
                    RichText::new("(browser)")
                        .color(theme::TEXT_MUTED)
                        .size(8.0),
                );
            }

            // Error text
            if let WalletEntryStatus::Failed(ref msg) = entry.status {
                ui.label(
                    RichText::new(msg.as_str())
                        .color(theme::ACCENT_RED)
                        .size(9.0),
                );
            }

            // Spacer to push remove button right (but not under scrollbar)
            let remaining = ui.available_width() - 20.0;
            if remaining > 0.0 {
                ui.add_space(remaining);
            }

            // Remove button
            let remove_color = if hovered {
                theme::TEXT_SECONDARY
            } else {
                theme::TEXT_MUTED
            };
            if ui
                .add(
                    egui::Button::new(crate::PhosphorIcon::X.rich_text(10.0, remove_color))
                        .frame(false),
                )
                .clicked()
            {
                action = Some(WalletEditorAction::Remove(i));
            }
        });
    }

    WalletEditorResponse { action }
}
