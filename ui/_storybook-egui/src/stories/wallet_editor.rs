//! Storybook demo for the WalletEditor widget.

use egui::Color32;
use egui_widgets::wallet_editor::{
    self, WalletEditorConfig, WalletEditorEntry, WalletEditorState, WalletEntryStatus,
};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

// ============================================================================
// State
// ============================================================================

pub struct WalletEditorStoryState {
    pub editor: WalletEditorState,
    pub entries: Vec<WalletEditorEntry>,
    pub last_action: String,
}

impl Default for WalletEditorStoryState {
    fn default() -> Self {
        Self {
            editor: WalletEditorState::default(),
            entries: mock_entries(),
            last_action: String::new(),
        }
    }
}

fn mock_entries() -> Vec<WalletEditorEntry> {
    vec![
        WalletEditorEntry {
            display: "$boef".into(),
            status: WalletEntryStatus::Ready,
            is_browser_wallet: false,
            accent: egui_widgets::theme::ACCENT_GREEN,
        },
        WalletEditorEntry {
            display: "$djo".into(),
            status: WalletEntryStatus::Ready,
            is_browser_wallet: false,
            accent: egui_widgets::theme::ACCENT_GREEN,
        },
        WalletEditorEntry {
            display: "$perplord".into(),
            status: WalletEntryStatus::Ready,
            is_browser_wallet: true,
            accent: egui_widgets::theme::ACCENT_CYAN,
        },
        WalletEditorEntry {
            display: "$curiousfutures".into(),
            status: WalletEntryStatus::Resolving,
            is_browser_wallet: false,
            accent: egui_widgets::theme::ACCENT_GREEN,
        },
        WalletEditorEntry {
            display: "stake1q8x...m4xj".into(),
            status: WalletEntryStatus::Loading,
            is_browser_wallet: false,
            accent: egui_widgets::theme::ACCENT_GREEN,
        },
        WalletEditorEntry {
            display: "stake1qy2...k9fp".into(),
            status: WalletEntryStatus::Failed("address not found".into()),
            is_browser_wallet: false,
            accent: egui_widgets::theme::ACCENT_GREEN,
        },
    ]
}

// ============================================================================
// Show
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut WalletEditorStoryState) {
    ui.label(
        egui::RichText::new("WalletEditor Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Wallet bundle editor with input, status indicators, and remove actions. \
             The widget emits Add/Remove actions for the caller to handle.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // Constrain width to simulate a sidebar
    ui.allocate_ui(egui::vec2(320.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                let config = WalletEditorConfig {
                    subtitle: Some("Add wallets to analyze trait coverage"),
                    ..WalletEditorConfig::default()
                };
                let resp = wallet_editor::show(ui, &mut state.editor, &state.entries, &config);

                if let Some(action) = resp.action {
                    match action {
                        wallet_editor::WalletEditorAction::Add(input) => {
                            state.last_action = format!("Add: \"{input}\"");
                            // Simulate adding — in a real app this triggers async resolution
                            state.entries.push(WalletEditorEntry {
                                display: input,
                                status: WalletEntryStatus::Loading,
                                is_browser_wallet: false,
                                accent: egui_widgets::theme::ACCENT_GREEN,
                            });
                        }
                        wallet_editor::WalletEditorAction::Remove(idx) => {
                            state.last_action = format!(
                                "Remove: [{}] \"{}\"",
                                idx,
                                state.entries.get(idx).map_or("?", |e| &e.display)
                            );
                            if idx < state.entries.len() {
                                state.entries.remove(idx);
                            }
                        }
                    }
                }
            });
    });

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // Action log
    if state.last_action.is_empty() {
        ui.label(
            egui::RichText::new("No actions yet \u{2014} try adding or removing a wallet")
                .color(TEXT_MUTED)
                .size(11.0),
        );
    } else {
        ui.label(
            egui::RichText::new(format!("Last action: {}", state.last_action))
                .color(Color32::from_rgb(125, 207, 255))
                .size(11.0),
        );
    }

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!("{} entries", state.entries.len()))
            .color(TEXT_MUTED)
            .size(10.0),
    );

    ui.add_space(16.0);
    ui.label(
        egui::RichText::new("Entry States:")
            .color(ACCENT)
            .size(11.0)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "  \u{2022} Ready \u{2014} green dot, data loaded\n  \
             \u{2022} Resolving \u{2014} spinner, handle lookup\n  \
             \u{2022} Loading \u{2014} spinner, fetching bundle\n  \
             \u{2022} Failed \u{2014} red !, inline error message\n  \
             \u{2022} Browser wallet \u{2014} cyan accent, \"(browser)\" badge",
        )
        .color(TEXT_MUTED)
        .size(10.0),
    );

    // Reset button
    ui.add_space(12.0);
    if ui.button("Reset to mock data").clicked() {
        state.entries = mock_entries();
        state.last_action.clear();
    }
}
