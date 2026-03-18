use crate::app::TEXT_MUTED;
use egui_widgets::egui_inbox::{UiInbox, UiInboxSender};
use egui_widgets::{
    classify_utxos, OptimizerPanel, ShelfConfig, ShelfData, ShelfState, WalletAction,
};

/// Messages from async wallet operations.
enum WalletMsg {
    Connected(
        Result<
            (
                egui_widgets::wallet::WalletConnectResult,
                egui_widgets::wallet::WalletApi,
            ),
            String,
        >,
    ),
    UtxosFetched(Result<Vec<cardano_assets::utxo::UtxoApi>, String>),
}

pub struct UtxoOptimizerStoryState {
    /// "Before" shelf state (static display of current wallet).
    before_shelf_state: ShelfState,
    before_data: Option<ShelfData>,
    /// Raw UTxOs for the optimizer.
    raw_utxos: Vec<cardano_assets::utxo::UtxoApi>,
    /// Optimizer panel (settings + animated preview).
    panel: OptimizerPanel,
    /// Async inbox for wallet messages.
    inbox: UiInbox<WalletMsg>,
    sender: UiInboxSender<WalletMsg>,
    fetching: bool,
    fetch_error: Option<String>,
}

impl Default for UtxoOptimizerStoryState {
    fn default() -> Self {
        let (sender, inbox) = UiInbox::channel();
        Self {
            before_shelf_state: ShelfState::default(),
            before_data: None,
            raw_utxos: Vec::new(),
            panel: OptimizerPanel::default(),
            inbox,
            sender,
            fetching: false,
            fetch_error: None,
        }
    }
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut UtxoOptimizerStoryState,
    wallet_btn: &mut egui_widgets::WalletButton,
    connector: &mut egui_widgets::wallet::WalletConnector,
) {
    // Drain inbox for async wallet messages
    for msg in state.inbox.read(ui) {
        match msg {
            WalletMsg::Connected(Ok((result, api))) => {
                connector.apply_connect_result(result);
                connector.api = Some(api);
            }
            WalletMsg::Connected(Err(e)) => {
                connector.set_error(e.clone());
                state.fetch_error = Some(e);
            }
            WalletMsg::UtxosFetched(Ok(utxos)) => {
                state.fetching = false;
                state.before_data = Some(classify_utxos(&utxos, 4310));
                state.before_shelf_state = ShelfState::default();
                state.raw_utxos = utxos;
                state.fetch_error = None;
            }
            WalletMsg::UtxosFetched(Err(e)) => {
                state.fetching = false;
                state.fetch_error = Some(e);
            }
        }
    }

    // --- Wallet connect ---
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(15, 15, 30))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, |ui| {
            ui.set_max_width(220.0);
            let action = wallet_btn.show(ui, connector);
            match action {
                WalletAction::Connect(provider) => {
                    connector.set_connecting();
                    let sender = state.sender.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let result = egui_widgets::wallet::connect_wallet(provider).await;
                        let _ = sender.send(WalletMsg::Connected(result));
                    });
                }
                WalletAction::Disconnect => {
                    connector.disconnect();
                    state.before_data = None;
                    state.raw_utxos.clear();
                    state.fetch_error = None;
                }
                WalletAction::None => {}
            }
        });

    ui.add_space(8.0);

    if connector.is_connected() {
        ui.horizontal(|ui| {
            let fetch_enabled = connector.has_api() && !state.fetching;
            if ui
                .add_enabled(
                    fetch_enabled,
                    egui::Button::new(if state.fetching {
                        "Fetching..."
                    } else {
                        "Fetch UTxOs"
                    }),
                )
                .clicked()
            {
                state.fetching = true;
                state.fetch_error = None;
                let api = connector.api.clone().unwrap();
                let sender = state.sender.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let result = egui_widgets::wallet::fetch_wallet_utxos(&api).await;
                    let _ = sender.send(WalletMsg::UtxosFetched(result));
                });
            }

            if let Some(ref addr) = connector.stake_address {
                ui.label(
                    egui::RichText::new(addr)
                        .color(TEXT_MUTED)
                        .monospace()
                        .size(10.0),
                );
            }
        });
    } else if connector.is_connecting() {
        ui.label(
            egui::RichText::new("Connecting...")
                .color(TEXT_MUTED)
                .small(),
        );
    } else {
        ui.label(
            egui::RichText::new("Connect a wallet to preview optimization")
                .color(TEXT_MUTED)
                .small(),
        );
    }

    if let Some(ref err) = state.fetch_error {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("Error: {err}"))
                .color(egui_widgets::theme::ACCENT_RED)
                .small(),
        );
    }

    ui.add_space(12.0);

    if !state.raw_utxos.is_empty() {
        // "Before" shelf — current wallet state
        ui.heading("Current Wallet");
        ui.add_space(4.0);
        if let Some(ref data) = state.before_data {
            let config = ShelfConfig {
                width: ui.available_width().min(600.0),
                ..ShelfConfig::default()
            };
            config.show(ui, data, &mut state.before_shelf_state);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // Optimizer panel with settings + animated "After" preview
        state.panel.show(ui, &state.raw_utxos);
    }
}
