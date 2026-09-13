use crate::{accent, muted};
use egui_widgets::wallet_identity_header::{WalletIdentityAction, WalletIdentityHeader};

const SAMPLE_STAKE: &str = "stake1u9pnzqcrvnt6njaqkexglkqtcktxrdc4lt7mdtsxafjzdfsgm5ex2";

#[derive(Default)]
pub struct WalletIdentityHeaderStoryState {
    pub last_action: Option<String>,
}

pub fn show(ui: &mut egui::Ui, state: &mut WalletIdentityHeaderStoryState) {
    ui.label(
        egui::RichText::new("Wallet Identity Header")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Big handle (when present) or shortened stake address, with a copy button \
             on the right. Used at the top of any wallet-profile view.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("With ADA Handle (most common case)")
            .color(accent(ui))
            .strong(),
    );
    ui.add_space(4.0);
    if let Some(WalletIdentityAction::CopyStake) = WalletIdentityHeader::new(SAMPLE_STAKE)
        .handle(Some("$djo"))
        .show(ui)
    {
        state.last_action = Some("Copy stake address".to_string());
    }
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("No handle — falls back to truncated stake")
            .color(accent(ui))
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
            .color(accent(ui))
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
                .color(crate::tok(ui, egui_widgets::theme::Token::AccentGreen))
                .small(),
        );
        if ui.small_button("Clear").clicked() {
            state.last_action = None;
        }
    }
}
