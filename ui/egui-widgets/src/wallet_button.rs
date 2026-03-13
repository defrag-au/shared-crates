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
    /// Theme colors.
    pub theme: WalletButtonTheme,
}

impl WalletButton {
    pub fn new() -> Self {
        Self {
            theme: WalletButtonTheme::default(),
        }
    }

    /// Create with a custom theme.
    pub fn with_theme(theme: WalletButtonTheme) -> Self {
        Self { theme }
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

        if connector.available_wallets.is_empty() {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("No wallets detected")
                        .color(theme.text_muted)
                        .size(10.0),
                );
            });
            return action;
        }

        // Single wallet — connect directly with one button
        if connector.available_wallets.len() == 1 {
            let info = &connector.available_wallets[0];
            let btn = ui.add_sized(
                [ui.available_width(), 32.0],
                egui::Button::new(
                    RichText::new(format!("Connect {}", info.name))
                        .color(theme.accent)
                        .size(12.0),
                )
                .corner_radius(4.0),
            );
            if btn.clicked() {
                if let Some(provider) = WalletProvider::from_api_name(&info.api_name) {
                    action = WalletAction::Connect(provider);
                }
            }
            return action;
        }

        // Multiple wallets — show picker directly
        for wallet_info in &connector.available_wallets {
            let btn = ui.add_sized(
                [ui.available_width(), 30.0],
                egui::Button::new(
                    RichText::new(&wallet_info.name)
                        .color(theme.accent)
                        .size(11.0),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(egui::Stroke::new(0.5, theme.text_muted))
                .corner_radius(4.0),
            );

            // Paint icon inside the button rect (left side)
            if let Some(ref icon_url) = wallet_info.icon {
                let icon_size = 18.0;
                let icon_rect = egui::Rect::from_min_size(
                    btn.rect.left_center() - egui::vec2(-8.0, icon_size / 2.0),
                    egui::vec2(icon_size, icon_size),
                );
                ui.put(
                    icon_rect,
                    egui::Image::new(icon_url.as_str())
                        .fit_to_exact_size(egui::vec2(icon_size, icon_size))
                        .corner_radius(2.0),
                );
            }

            if btn.clicked() {
                if let Some(provider) = WalletProvider::from_api_name(&wallet_info.api_name) {
                    action = WalletAction::Connect(provider);
                }
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

        // Copy theme colors to avoid borrow conflicts
        let text_muted = self.theme.text_muted;
        let accent = self.theme.accent;

        // Top row: icon + handle/address
        ui.horizontal(|ui| {
            if let Some(ref icon_url) = connector.connected_icon {
                ui.add(
                    egui::Image::new(icon_url.as_str())
                        .fit_to_exact_size(egui::vec2(20.0, 20.0))
                        .corner_radius(3.0),
                );
            }

            if let Some(ref handle) = connector.handle {
                ui.label(RichText::new(handle).color(accent).size(12.0).strong());
            } else if let Some(ref stake) = connector.stake_address {
                let truncated = if stake.len() > 20 {
                    format!("{}...{}", &stake[..8], &stake[stake.len() - 6..])
                } else {
                    stake.clone()
                };
                ui.label(RichText::new(truncated).color(text_muted).size(11.0));
            }
        });

        // Balance row
        if let Some(ref balance) = connector.balance {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let ada = balance.ada();
                let ada_display = if ada >= 1000.0 {
                    format!("{:.0}", ada)
                } else {
                    format!("{:.2}", ada)
                };
                ui.label(
                    RichText::new(format!("{ada_display} ADA"))
                        .color(accent)
                        .size(13.0)
                        .strong(),
                );

                let tokens = balance.token_count();
                if tokens > 0 {
                    ui.label(
                        RichText::new(format!("\u{2022} {tokens} tokens"))
                            .color(text_muted)
                            .size(10.0),
                    );
                }
            });
        }

        // Disconnect at bottom
        ui.add_space(4.0);
        if ui
            .add(
                egui::Button::new(RichText::new("Disconnect").color(text_muted).size(10.0))
                    .fill(Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(0.5, text_muted))
                    .corner_radius(3.0),
            )
            .clicked()
        {
            action = WalletAction::Disconnect;
        }

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
