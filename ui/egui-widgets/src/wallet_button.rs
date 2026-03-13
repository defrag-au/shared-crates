//! Reusable wallet connection button widget for egui frontends.
//!
//! Renders a compact wallet status bar that handles:
//! - Disconnected: "Connect Wallet" button with wallet picker popup
//! - Connecting: spinner
//! - Connected: icon + name + handle + balance + disconnect
//! - Error: message + retry
//!
//! The widget does NOT spawn async tasks — it returns [`WalletAction`] values
//! that the caller dispatches through their own message channel.

use egui::{Color32, RichText};

use super::wallet::{ConnectionState, WalletConnector, WalletProvider};

/// Theme colors for the wallet button widget.
pub struct WalletButtonTheme {
    pub accent: Color32,
    pub text_primary: Color32,
    pub text_muted: Color32,
    pub error: Color32,
    pub bg: Color32,
}

impl Default for WalletButtonTheme {
    fn default() -> Self {
        Self {
            accent: Color32::from_rgb(68, 255, 68),
            text_primary: Color32::from_rgb(200, 255, 220),
            text_muted: Color32::from_rgb(96, 104, 128),
            error: Color32::from_rgb(255, 68, 68),
            bg: Color32::from_rgb(20, 30, 25),
        }
    }
}

/// Actions the caller must handle after calling [`WalletButton::show()`].
pub enum WalletAction {
    /// No action needed.
    None,
    /// User selected a wallet to connect. Caller should spawn
    /// `wallet::connect_wallet(provider)` and send the result.
    Connect(WalletProvider),
    /// User clicked disconnect. Caller should call `connector.disconnect()`.
    Disconnect,
}

/// Reusable wallet connection button widget.
pub struct WalletButton {
    /// Whether the wallet picker popup is open.
    picker_open: bool,
    /// Theme colors.
    pub theme: WalletButtonTheme,
}

impl WalletButton {
    pub fn new() -> Self {
        Self {
            picker_open: false,
            theme: WalletButtonTheme::default(),
        }
    }

    /// Create with a custom theme.
    pub fn with_theme(theme: WalletButtonTheme) -> Self {
        Self {
            picker_open: false,
            theme,
        }
    }

    /// Render the wallet button. Returns an action the caller must handle.
    pub fn show(&mut self, ui: &mut egui::Ui, connector: &WalletConnector) -> WalletAction {
        let mut action = WalletAction::None;
        let theme = &self.theme;

        egui::Frame::new()
            .fill(theme.bg)
            .corner_radius(6.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                match &connector.connection_state {
                    ConnectionState::Disconnected => {
                        action = self.draw_disconnected(ui, connector);
                    }
                    ConnectionState::Connecting => {
                        self.draw_connecting(ui);
                    }
                    ConnectionState::Connected { .. } => {
                        action = self.draw_connected(ui, connector);
                    }
                    ConnectionState::Error(err) => {
                        action = self.draw_error(ui, err, connector);
                    }
                }
            });

        action
    }

    fn draw_disconnected(
        &mut self,
        ui: &mut egui::Ui,
        connector: &WalletConnector,
    ) -> WalletAction {
        let mut action = WalletAction::None;
        let theme = &self.theme;

        if !self.picker_open {
            // Single "Connect Wallet" button
            let btn = ui.button(
                RichText::new("Connect Wallet")
                    .color(theme.accent)
                    .size(12.0),
            );
            if btn.clicked() {
                if connector.available_wallets.len() == 1 {
                    // Only one wallet — connect directly
                    if let Some(provider) =
                        WalletProvider::from_api_name(&connector.available_wallets[0].api_name)
                    {
                        action = WalletAction::Connect(provider);
                    }
                } else {
                    self.picker_open = true;
                }
            }
            if connector.available_wallets.is_empty() {
                ui.label(
                    RichText::new("No wallets detected")
                        .color(theme.text_muted)
                        .size(9.0),
                );
            }
        } else {
            // Wallet picker — show available wallets
            ui.label(
                RichText::new("Select wallet:")
                    .color(theme.text_muted)
                    .size(10.0),
            );
            ui.add_space(4.0);

            for wallet_info in &connector.available_wallets {
                ui.horizontal(|ui| {
                    // Wallet icon (from CIP-30 base64 data URL)
                    if let Some(ref icon_url) = wallet_info.icon {
                        ui.add(
                            egui::Image::new(icon_url.as_str())
                                .fit_to_exact_size(egui::vec2(16.0, 16.0))
                                .corner_radius(2.0),
                        );
                    }

                    let btn = ui.button(
                        RichText::new(&wallet_info.name)
                            .color(theme.accent)
                            .size(11.0),
                    );
                    if btn.clicked() {
                        if let Some(provider) = WalletProvider::from_api_name(&wallet_info.api_name)
                        {
                            action = WalletAction::Connect(provider);
                            self.picker_open = false;
                        }
                    }
                });
            }

            ui.add_space(4.0);
            if ui
                .button(RichText::new("Cancel").color(theme.text_muted).size(10.0))
                .clicked()
            {
                self.picker_open = false;
            }
        }

        action
    }

    fn draw_connecting(&self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(
                RichText::new("Connecting...")
                    .color(theme.text_muted)
                    .size(11.0),
            );
        });
    }

    fn draw_connected(&mut self, ui: &mut egui::Ui, connector: &WalletConnector) -> WalletAction {
        let mut action = WalletAction::None;
        let theme = &self.theme;

        ui.horizontal(|ui| {
            // Wallet icon
            if let Some(ref icon_url) = connector.connected_icon {
                ui.add(
                    egui::Image::new(icon_url.as_str())
                        .fit_to_exact_size(egui::vec2(16.0, 16.0))
                        .corner_radius(2.0),
                );
            }

            // Provider name
            if let Some(name) = connector.provider_name() {
                ui.label(RichText::new(name).color(theme.text_primary).size(11.0));
            }

            // Handle or truncated stake
            let display = connector
                .handle
                .as_deref()
                .or(connector.stake_address.as_deref().map(|s| {
                    // Truncate stake address inline — return full if short
                    if s.len() > 20 {
                        &s[..12]
                    } else {
                        s
                    }
                }))
                .unwrap_or("");
            if !display.is_empty() {
                ui.label(RichText::new(display).color(theme.accent).size(10.0));
            }

            // Balance (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Disconnect button
                if ui
                    .button(RichText::new("x").color(theme.text_muted).size(10.0))
                    .on_hover_text("Disconnect")
                    .clicked()
                {
                    action = WalletAction::Disconnect;
                }

                // ADA balance
                if let Some(ref balance) = connector.balance {
                    let ada = balance.lovelace / 1_000_000;
                    ui.label(
                        RichText::new(format!("{ada} ADA"))
                            .color(theme.accent)
                            .size(10.0),
                    );
                }
            });
        });

        action
    }

    fn draw_error(
        &mut self,
        ui: &mut egui::Ui,
        error: &str,
        connector: &WalletConnector,
    ) -> WalletAction {
        let mut action = WalletAction::None;
        let theme = &self.theme;

        ui.horizontal(|ui| {
            // Truncate long error messages
            let display_err = if error.len() > 40 {
                format!("{}...", &error[..37])
            } else {
                error.to_string()
            };
            ui.label(RichText::new(display_err).color(theme.error).size(10.0));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("Retry").color(theme.accent).size(10.0))
                    .clicked()
                {
                    // Try to reconnect with the first available wallet
                    if let Some(info) = connector.available_wallets.first() {
                        if let Some(provider) = WalletProvider::from_api_name(&info.api_name) {
                            action = WalletAction::Connect(provider);
                        }
                    }
                }
            });
        });

        action
    }
}

impl Default for WalletButton {
    fn default() -> Self {
        Self::new()
    }
}
