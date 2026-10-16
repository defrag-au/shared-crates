use crate::muted;
use crate::stories::wallet_fixture;
use egui_widgets::egui_inbox::{UiInbox, UiInboxSender};

/// Messages from the async connect task.
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
}

/// This story's async plumbing. The connector itself stays on the app: the
/// UtxoShelf story draws the same one, so connecting here leaves that story
/// connected too.
pub struct WalletButtonStoryState {
    inbox: UiInbox<WalletMsg>,
    sender: UiInboxSender<WalletMsg>,
    /// A second connector with wallets seeded into it, because a test browser
    /// announces none — and the multichain picker is the part of this widget
    /// worth looking at, so it must not depend on what happens to be installed.
    seeded: egui_widgets::wallet::WalletConnector,
    /// What a seeded pick would do, in the host's words.
    seeded_note: String,
}

impl Default for WalletButtonStoryState {
    fn default() -> Self {
        let (sender, inbox) = UiInbox::channel();
        let mut seeded = egui_widgets::wallet::WalletConnector::new();
        seeded.available_wallets = vec![detected("eternl", "Eternl"), detected("lace", "Lace")];
        seeded.set_evm_wallets(egui_widgets::wallet_mock::evm_wallets());
        Self {
            inbox,
            sender,
            seeded,
            seeded_note: String::new(),
        }
    }
}

/// A detected wallet as the extension reports it. No icon: a headless browser
/// has none to copy, and the picker reads fine with the name alone.
fn detected(api_name: &str, name: &str) -> egui_widgets::wallet::WalletInfo {
    egui_widgets::wallet::WalletInfo {
        api_name: api_name.to_string(),
        name: name.to_string(),
        icon: None,
    }
}

pub fn show(
    ui: &mut egui::Ui,
    wallet_btn: &mut egui_widgets::WalletButton,
    connector: &mut egui_widgets::wallet::WalletConnector,
    state: &mut WalletButtonStoryState,
) {
    // The result lands on a later frame. `UiInbox::send` requests a repaint, so
    // the flow cannot sit in `Connecting` waiting for a mouse move.
    for msg in state.inbox.read(ui) {
        match msg {
            WalletMsg::Connected(Ok((result, api))) => {
                connector.apply_connect_result(result, api);
            }
            WalletMsg::Connected(Err(e)) => {
                connector.set_error(egui_widgets::wallet::ChainFamily::Cardano, e)
            }
        }
    }
    // Both halves, because they are detected by different mechanisms and a
    // count of one is exactly what made the EVM side look uninstalled: the
    // Cardano namespaces are on `window.cardano`, the EVM wallets announce
    // themselves (EIP-6963). In a browser with Rabby in it, this line is where
    // that shows up.
    ui.label(format!(
        "Detected: {} Cardano · {} EVM",
        connector.available_wallets.len(),
        connector.available_evm_wallets.len()
    ));
    if connector.available_wallets.is_empty() && connector.available_evm_wallets.is_empty() {
        ui.label(
            egui::RichText::new("No wallet extensions found. Install Eternl, Lace, Rabby, etc.")
                .color(muted(ui))
                .small(),
        );
    }
    crate::caption(
        ui,
        "Disconnected, the button is ONE control — a single Connect wallet that opens \
         the chain-then-wallet flow. It never lists wallets inline: the choice has \
         steps, and the place this sits has one line.",
    );
    ui.add_space(8.0);

    // Render the widget in a constrained width (simulating a side panel)
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(15, 15, 30))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, |ui| {
            ui.set_max_width(220.0);
            let action = wallet_btn.show(ui, connector);
            match action {
                egui_widgets::WalletAction::Connect(
                    egui_widgets::wallet::WalletChoice::Cardano(provider),
                ) => {
                    connector.set_connecting(egui_widgets::wallet::ChainFamily::Cardano);
                    log::info!("Connect requested for {provider:?}");

                    let sender = state.sender.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let result = egui_widgets::wallet::connect_wallet(provider).await;
                        let _ = sender.send(WalletMsg::Connected(result));
                    });
                }
                // No EIP-1193 bridge in this crate yet, so this arm reports
                // rather than connects. It is what the fronts will do.
                egui_widgets::WalletAction::Connect(egui_widgets::wallet::WalletChoice::Evm(
                    wallet,
                )) => log::info!("EVM connect requested for {} ({})", wallet.name, wallet.id),
                egui_widgets::WalletAction::Disconnect(family) => {
                    connector.disconnect(family);
                }
                egui_widgets::WalletAction::None => {}
            }
        });

    ui.add_space(16.0);

    crate::caption(
        ui,
        "With wallets seeded in: two chains, because a CIP-30 key and an EIP-6963 \
         announcement are different identities behind different bridges — so the flow \
         asks which chain first. The screens are the Wallet Connect story.",
    );

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(15, 15, 30))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, |ui| {
            ui.set_max_width(220.0);
            // Its own salt: this story draws two of these on one page, and the
            // flow's id is keyed to where a button sits plus its salt — a
            // `Frame` gives both the same child `Ui`, so the default salt would
            // have them share one modal.
            let mut button = egui_widgets::WalletButton::new().salt("wallet_button_seeded");
            match button.show(ui, &state.seeded) {
                egui_widgets::WalletAction::Connect(
                    egui_widgets::wallet::WalletChoice::Cardano(provider),
                ) => {
                    state.seeded_note = format!(
                        "Cardano: the host would connect {} — the CIP-30 dialog",
                        provider.display_name()
                    );
                }
                egui_widgets::WalletAction::Connect(egui_widgets::wallet::WalletChoice::Evm(
                    wallet,
                )) => {
                    state.seeded_note = format!(
                        "EVM: the host would connect {} ({})",
                        wallet.name, wallet.id
                    );
                }
                egui_widgets::WalletAction::Disconnect(family) => {
                    state.seeded_note = format!("Disconnect {}", family.display_name());
                }
                egui_widgets::WalletAction::None => {}
            }
        });

    if !state.seeded_note.is_empty() {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(&state.seeded_note)
                .color(muted(ui))
                .small(),
        );
    }

    ui.add_space(16.0);

    crate::caption(
        ui,
        "Connected, the same button becomes the identity: the `$handle` and the balance \
         when there is one, the account when there is not. Which chain the identity is \
         on is what decides that — the balance is Cardano's reading, and an EVM \
         connection has none to borrow.",
    );

    for (label, connector) in [
        (
            "Cardano, with a handle",
            wallet_fixture::cardano_with_handle("$boef"),
        ),
        ("EVM only — no balance to show", wallet_fixture::evm_only()),
    ] {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(label).color(muted(ui)).small());
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(15, 15, 30))
            .inner_margin(egui::Margin::same(12))
            .corner_radius(6.0)
            .show(ui, |ui| {
                ui.set_max_width(220.0);
                let mut button = egui_widgets::WalletButton::new().salt(label);
                let _ = button.show(ui, &connector);
            });
    }

    ui.add_space(16.0);
    let cardano = egui_widgets::wallet::ChainFamily::Cardano;
    let state_label = if connector.is_connected(cardano) {
        "Connected"
    } else if connector.is_connecting(cardano) {
        "Connecting..."
    } else {
        "Disconnected"
    };
    ui.label(format!("State: {state_label}"));
    if let Some(addr) = connector.cardano().and_then(|c| c.stake_address.as_deref()) {
        ui.label(
            egui::RichText::new(format!("Stake: {addr}"))
                .color(muted(ui))
                .small(),
        );
    }
    if let Some(ref handle) = connector.handle {
        ui.label(format!("Handle: {handle}"));
    }
}
