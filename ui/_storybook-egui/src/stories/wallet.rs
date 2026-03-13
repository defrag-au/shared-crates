use crate::TEXT_MUTED;

pub fn show(
    ui: &mut egui::Ui,
    wallet_btn: &mut egui_widgets::WalletButton,
    connector: &mut egui_widgets::wallet::WalletConnector,
) {
    ui.label(format!(
        "Detected wallets: {}",
        connector.available_wallets.len()
    ));
    if connector.available_wallets.is_empty() {
        ui.label(
            egui::RichText::new("No wallet extensions found. Install Eternl, Lace, etc.")
                .color(TEXT_MUTED)
                .small(),
        );
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
                .color(TEXT_MUTED)
                .small(),
        );
    }
    if let Some(ref handle) = connector.handle {
        ui.label(format!("Handle: {handle}"));
    }
}
