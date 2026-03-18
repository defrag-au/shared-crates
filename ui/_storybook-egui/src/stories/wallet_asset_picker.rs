//! Storybook demo for the WalletAssetPicker widget — modal asset browser
//! with accordion policy groups and card grid selection.
//!
//! Connects to a real CIP-30 wallet via `WalletConnector` and converts
//! the wallet balance into picker groups. Falls back to mock data when
//! no wallet is connected.

use egui_widgets::egui_inbox::{UiInbox, UiInboxSender};
use egui_widgets::wallet_asset_picker::{
    self, PickerAsset, PickerPolicyGroup, WalletAssetPickerConfig, WalletAssetPickerState,
};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

// ── Wallet → Picker conversion ────────────────────────────────────────────

/// Convert a `WalletBalance` into picker groups (NFTs only).
fn balance_to_groups(balance: &egui_widgets::wallet::WalletBalance) -> Vec<PickerPolicyGroup> {
    balance
        .nft_policy_groups()
        .into_iter()
        .map(|pg| {
            let assets = pg
                .nfts()
                .into_iter()
                .map(|token| PickerAsset {
                    asset_name_hex: token.asset_name_hex.clone(),
                    display_name: token.display_name(),
                    rarity_rank: None,
                    total_ranked: None,
                    traits: Vec::new(),
                    quantity: token.quantity,
                })
                .collect();
            PickerPolicyGroup {
                policy_id: pg.policy_id.clone(),
                label: pg.policy_id.clone(),
                assets,
                is_token_group: false,
            }
        })
        .collect()
}

// ── Async message ──────────────────────────────────────────────────────────

enum WalletMsg {
    Connected(
        egui_widgets::wallet::WalletConnectResult,
        egui_widgets::wallet::WalletApi,
    ),
    BalanceFetched(egui_widgets::wallet::WalletBalance),
    Error(String),
}

// ── Story state ────────────────────────────────────────────────────────────

pub struct WalletAssetPickerStoryState {
    pub picker_state: WalletAssetPickerState,
    pub groups: Vec<PickerPolicyGroup>,
    pub last_selection: String,
    pub wallet_btn: egui_widgets::WalletButton,
    pub connector: egui_widgets::wallet::WalletConnector,
    inbox: UiInbox<WalletMsg>,
    sender: UiInboxSender<WalletMsg>,
    pub status_msg: String,
}

impl Default for WalletAssetPickerStoryState {
    fn default() -> Self {
        let (sender, inbox) = UiInbox::channel();
        Self {
            picker_state: WalletAssetPickerState::default(),
            groups: Vec::new(),
            last_selection: String::new(),
            wallet_btn: egui_widgets::WalletButton::new(),
            connector: egui_widgets::wallet::WalletConnector::new(),
            inbox,
            sender,
            status_msg: "Connect a wallet to browse your NFTs".into(),
        }
    }
}

// ── Show ───────────────────────────────────────────────────────────────────

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut WalletAssetPickerStoryState) {
    // ── Process async messages ──
    for msg in state.inbox.read(ui) {
        match msg {
            WalletMsg::Connected(result, api) => {
                state.connector.apply_connect_result(result);
                state.connector.api = Some(api.clone());
                state.status_msg = "Connected! Fetching balance...".into();

                // Fetch balance
                let sender = state.sender.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match egui_widgets::wallet::fetch_wallet_balance(&api).await {
                        Ok(balance) => {
                            let _ = sender.send(WalletMsg::BalanceFetched(balance));
                        }
                        Err(e) => {
                            let _ = sender.send(WalletMsg::Error(e));
                        }
                    }
                });
            }
            WalletMsg::BalanceFetched(balance) => {
                state.groups = balance_to_groups(&balance);
                let nft_count: usize = state.groups.iter().map(|g| g.assets.len()).sum();
                state.status_msg = format!(
                    "Loaded {nft_count} NFTs across {} policies",
                    state.groups.len()
                );
                state.connector.apply_balance(balance);
            }
            WalletMsg::Error(e) => {
                state.connector.set_error(e.clone());
                state.status_msg = format!("Error: {e}");
            }
        }
    }

    // ── Header ──
    ui.label(
        egui::RichText::new("WalletAssetPicker Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Modal asset browser with accordion policy groups, \
             search filter, and card grid selection. Connect a wallet to browse real NFTs.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // ── Wallet connection ──
    egui::Frame::new()
        .fill(BG_MAIN)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            ui.set_max_width(260.0);
            let action = state.wallet_btn.show(ui, &state.connector);
            match action {
                egui_widgets::WalletAction::Connect(provider) => {
                    state.connector.set_connecting();
                    state.status_msg = "Connecting...".into();

                    let sender = state.sender.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        match egui_widgets::wallet::connect_wallet(provider).await {
                            Ok((result, api)) => {
                                let _ = sender.send(WalletMsg::Connected(result, api));
                            }
                            Err(e) => {
                                let _ = sender.send(WalletMsg::Error(e));
                            }
                        }
                    });
                }
                egui_widgets::WalletAction::Disconnect => {
                    state.connector.disconnect();
                    state.groups.clear();
                    state.status_msg = "Disconnected".into();
                }
                egui_widgets::WalletAction::None => {}
            }
        });

    ui.add_space(8.0);

    // ── Status ──
    ui.label(
        egui::RichText::new(&state.status_msg)
            .color(TEXT_MUTED)
            .size(10.0),
    );

    ui.add_space(12.0);

    // ── Picker trigger + result ──
    egui::Frame::new()
        .fill(BG_MAIN)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            let has_assets = !state.groups.is_empty();
            let btn = ui.add_enabled(has_assets, egui::Button::new("Open Asset Picker"));
            if btn.clicked() {
                state.picker_state.open = true;
            }
            if !has_assets {
                ui.label(
                    egui::RichText::new("Connect a wallet first")
                        .color(TEXT_MUTED)
                        .size(10.0),
                );
            }

            ui.add_space(8.0);

            if state.last_selection.is_empty() {
                ui.label(
                    egui::RichText::new("No asset selected yet")
                        .color(TEXT_MUTED)
                        .size(11.0),
                );
            } else {
                ui.label(
                    egui::RichText::new(format!("Selected: {}", state.last_selection))
                        .color(egui_widgets::theme::ACCENT_CYAN)
                        .size(11.0),
                );
            }
        });

    // ── Render the modal ──
    let config = WalletAssetPickerConfig::default();
    let resp = wallet_asset_picker::show(ctx, &mut state.picker_state, &state.groups, &config);

    if let Some(action) = resp.action {
        match action {
            wallet_asset_picker::WalletAssetPickerAction::Confirmed(assets) => {
                let names: Vec<String> = assets.iter().map(|a| a.asset_id.asset_name()).collect();
                state.last_selection = format!("{} asset(s): {}", assets.len(), names.join(", "));
            }
            wallet_asset_picker::WalletAssetPickerAction::Closed => {
                state.last_selection = "Closed without selecting".into();
            }
        }
    }
}
