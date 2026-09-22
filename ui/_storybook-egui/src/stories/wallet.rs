use crate::muted;
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
}

impl Default for WalletButtonStoryState {
    fn default() -> Self {
        let (sender, inbox) = UiInbox::channel();
        Self { inbox, sender }
    }
}

pub fn show(
    ui: &mut egui::Ui,
    wallet_btn: &mut egui_widgets::WalletButton,
    connector: &mut egui_widgets::wallet::WalletConnector,
    state: &mut WalletButtonStoryState,
) {
    // The result lands on a later frame. `UiInbox::send` requests a repaint, so
    // the picker cannot sit in `Connecting` waiting for a mouse move.
    for msg in state.inbox.read(ui) {
        match msg {
            WalletMsg::Connected(Ok((result, api))) => {
                connector.apply_connect_result(result);
                connector.api = Some(api);
            }
            WalletMsg::Connected(Err(e)) => connector.set_error(e),
        }
    }
    ui.label(format!(
        "Detected wallets: {}",
        connector.available_wallets.len()
    ));
    if connector.available_wallets.is_empty() {
        ui.label(
            egui::RichText::new("No wallet extensions found. Install Eternl, Lace, etc.")
                .color(muted(ui))
                .small(),
        );
    }
    crate::caption(
        ui,
        "The picker is ONE control — a single border with hairline separators, \
         not a stack of buttons. The wallets are alternatives to each other, so \
         they read as one object you pick within rather than several unrelated \
         things that happen to be adjacent.",
    );
    ui.add_space(6.0);

    // Compact is the sidebar case: the extension's own icon is the most
    // recognisable thing about a wallet, and the name becomes the hover text.
    let mut compact =
        wallet_btn.picker_density == egui_widgets::option_group::GroupDensity::Compact;
    if ui
        .checkbox(&mut compact, "compact picker (icons only)")
        .changed()
    {
        wallet_btn.picker_density = match compact {
            true => egui_widgets::option_group::GroupDensity::Compact,
            false => egui_widgets::option_group::GroupDensity::Full,
        };
    }
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
                egui_widgets::WalletAction::Connect(provider) => {
                    connector.set_connecting();
                    log::info!("Connect requested for {provider:?}");

                    let sender = state.sender.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let result = egui_widgets::wallet::connect_wallet(provider).await;
                        let _ = sender.send(WalletMsg::Connected(result));
                    });
                }
                egui_widgets::WalletAction::Disconnect => {
                    connector.disconnect();
                }
                egui_widgets::WalletAction::None => {}
            }
        });

    ui.add_space(16.0);

    // State info
    let state_label = if connector.is_connected() {
        "Connected"
    } else if connector.is_connecting() {
        "Connecting..."
    } else {
        "Disconnected"
    };
    ui.label(format!("State: {state_label}"));
    if let Some(ref addr) = connector.stake_address {
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
