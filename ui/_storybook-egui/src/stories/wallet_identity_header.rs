use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::wallet_identity_header::{WalletIdentityAction, WalletIdentityHeader};

const SAMPLE_STAKE: &str = "stake1u9pnzqcrvnt6njaqkexglkqtcktxrdc4lt7mdtsxafjzdfsgm5ex2";

pub struct WalletIdentityHeaderStoryState {
    pub last_action: Option<String>,
}

impl Default for WalletIdentityHeaderStoryState {
    fn default() -> Self {
        Self { last_action: None }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut WalletIdentityHeaderStoryState) {
    ui.label(
        egui::RichText::new("Wallet Identity Header")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Big handle (when present) or shortened stake address, with a copy button \
             on the right. Used at the top of any wallet-profile view.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("With ADA Handle (most common case)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    if let Some(WalletIdentityAction::CopyStake) =
        WalletIdentityHeader::new(SAMPLE_STAKE)
            .handle(Some("$djo"))
            .show(ui)
    {
        state.last_action = Some("Copy stake address".to_string());
    }
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("No handle — falls back to truncated stake")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    if let Some(WalletIdentityAction::CopyStake) = WalletIdentityHeader::new(SAMPLE_STAKE).show(ui)
    {
        state.last_action = Some("Copy stake address (no-handle variant)".to_string());
    }
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Copy disabled (read-only contexts)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    WalletIdentityHeader::new(SAMPLE_STAKE)
        .handle(Some("$alice"))
        .no_copy()
        .show(ui);
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    if let Some(ref action) = state.last_action {
        ui.label(
            egui::RichText::new(format!("Last action: {action}"))
                .color(egui_widgets::theme::ACCENT_GREEN)
                .small(),
        );
        if ui.small_button("Clear").clicked() {
            state.last_action = None;
        }
    }
}
