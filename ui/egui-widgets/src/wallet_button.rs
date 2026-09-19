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

use super::buttons::UiButtonExt;
use super::wallet::{ConnectionState, WalletConnector, WalletProvider};
use crate::option_group::{GroupDensity, GroupFlow, OptionGroup, OptionGroupItem};
use crate::theme::{Ink, Radius, Space, SpaceExt, TextSize, ThemeExt, Token};

/// Which colour each of the wallet button's surfaces takes.
///
/// Every field defaults to a named theme token and resolves at render time.
/// The `Default` before this was five literals in the **same `#44ff44` green**
/// that `SwapModalTheme` carried — a third copy of a colour that exists in no
/// theme, which is what a per-widget palette becomes once two of them exist.
#[derive(Clone, Copy, Debug)]
pub struct WalletButtonTheme {
    pub accent: Ink,
    pub text_primary: Ink,
    pub text_muted: Ink,
    pub error: Ink,
    pub bg: Ink,
}

impl Default for WalletButtonTheme {
    fn default() -> Self {
        Self {
            accent: Ink::Token(Token::Accent),
            text_primary: Ink::Token(Token::TextPrimary),
            text_muted: Ink::Token(Token::TextMuted),
            error: Ink::Token(Token::Error),
            bg: Ink::Token(Token::BgSecondary),
        }
    }
}

/// [`WalletButtonTheme`] with every [`Ink`] resolved. Same field names, so a
/// draw site reads the same either way.
pub struct ResolvedWalletTheme {
    pub accent: Color32,
    pub text_primary: Color32,
    pub text_muted: Color32,
    pub error: Color32,
    pub bg: Color32,
}

impl WalletButtonTheme {
    pub fn resolved(&self, theme: &crate::theme::Theme) -> ResolvedWalletTheme {
        ResolvedWalletTheme {
            accent: self.accent.resolve(theme),
            text_primary: self.text_primary.resolve(theme),
            text_muted: self.text_muted.resolve(theme),
            error: self.error.resolve(theme),
            bg: self.bg.resolve(theme),
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

/// Whether the button draws its own surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PickerChrome {
    /// A filled, rounded panel with margins — right when the button stands
    /// alone in a header or sidebar and IS the surface.
    #[default]
    Framed,
    /// The contents only. For a host that is already a surface — a modal, a
    /// popup — where a second fill reads as a box inside a box.
    Bare,
}

/// Reusable wallet connection button widget.
pub struct WalletButton {
    /// Theme colors.
    pub theme: WalletButtonTheme,
    /// How much of each wallet the picker shows. [`GroupDensity::Compact`]
    /// keeps the extension's own icon and moves the name to the hover text,
    /// for a sidebar that cannot spare a row per wallet.
    pub picker_density: GroupDensity,
    /// Whether to draw the button's own panel around its contents.
    pub chrome: PickerChrome,
}

impl WalletButton {
    pub fn new() -> Self {
        Self {
            theme: WalletButtonTheme::default(),
            picker_density: GroupDensity::Full,
            chrome: PickerChrome::Framed,
        }
    }

    /// Create with a custom theme.
    pub fn with_theme(theme: WalletButtonTheme) -> Self {
        Self {
            theme,
            ..Self::new()
        }
    }

    /// Show the picker as icons only, with the wallet names as hover text.
    pub fn compact_picker(mut self, compact: bool) -> Self {
        self.picker_density = match compact {
            true => GroupDensity::Compact,
            false => GroupDensity::Full,
        };
        self
    }

    /// Draw with or without the button's own panel. See [`PickerChrome`].
    pub fn chrome(mut self, chrome: PickerChrome) -> Self {
        self.chrome = chrome;
        self
    }

    /// Render the wallet button. Returns an action the caller must handle.
    pub fn show(&mut self, ui: &mut egui::Ui, connector: &WalletConnector) -> WalletAction {
        match self.chrome {
            PickerChrome::Framed => {
                let theme = self.theme.resolved(&ui.tokens());
                egui::Frame::new()
                    .fill(theme.bg)
                    .corner_radius(ui.tokens().corner(Radius::Md))
                    .inner_margin(ui.tokens().margin(Space::Md))
                    .show(ui, |ui| self.body(ui, connector))
                    .inner
            }
            PickerChrome::Bare => self.body(ui, connector),
        }
    }

    /// The button's contents for the current connection state, unframed.
    fn body(&mut self, ui: &mut egui::Ui, connector: &WalletConnector) -> WalletAction {
        ui.set_width(ui.available_width());
        match &connector.connection_state {
            ConnectionState::Disconnected => self.draw_disconnected(ui, connector),
            ConnectionState::Connecting => {
                self.draw_connecting(ui);
                WalletAction::None
            }
            ConnectionState::Connected { .. } => self.draw_connected(ui, connector),
            ConnectionState::Error(err) => self.draw_error(ui, err, connector),
        }
    }

    fn draw_disconnected(
        &mut self,
        ui: &mut egui::Ui,
        connector: &WalletConnector,
    ) -> WalletAction {
        let mut action = WalletAction::None;
        let theme = self.theme.resolved(&ui.tokens());

        if connector.available_wallets.is_empty() {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("No wallets detected")
                        .color(theme.text_muted)
                        .size(ui.text_size(TextSize::Sm)),
                );
            });
            return action;
        }

        // Single wallet — connect directly with one button
        if connector.available_wallets.len() == 1 {
            let info = &connector.available_wallets[0];
            let btn = ui.add_clickable_sized(
                [ui.available_width(), 32.0],
                egui::Button::new(
                    RichText::new(format!("Connect {}", info.name))
                        .color(theme.accent)
                        .size(ui.text_size(TextSize::Md)),
                )
                .corner_radius(ui.tokens().corner(Radius::Base)),
            );
            if btn.clicked()
                && let Some(provider) = WalletProvider::from_api_name(&info.api_name)
            {
                action = WalletAction::Connect(provider);
            }
            return action;
        }

        // Several wallets — ONE compound control, not a stack of buttons.
        //
        // They are alternatives to each other, so they read as one object you
        // pick within. Drawn as separate bordered buttons they read as several
        // unrelated things that happen to be adjacent, which is what a toolbar
        // looks like — and this is not a toolbar.
        let items: Vec<OptionGroupItem<'_>> = connector
            .available_wallets
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let mut item = OptionGroupItem::new(i as u64, &w.name);
                if let Some(icon) = &w.icon {
                    item = item.image(icon.as_str());
                }
                item
            })
            .collect();

        let picked = OptionGroup::new()
            .flow(GroupFlow::Stacked)
            .density(self.picker_density)
            .items(items)
            .show(ui);

        if let Some(i) = picked.clicked
            && let Some(info) = connector.available_wallets.get(i as usize)
            && let Some(provider) = WalletProvider::from_api_name(&info.api_name)
        {
            action = WalletAction::Connect(provider);
        }

        action
    }

    fn draw_connecting(&self, ui: &mut egui::Ui) {
        let theme = self.theme.resolved(&ui.tokens());
        // Not a bare `ui.spinner()`: that takes `interact_size.y`, floored at
        // the 44pt tap target under touch sizing, and dwarfed the word beside
        // it — now visible in the middle of `WalletCta`'s modal.
        crate::labelled_progress::LabelledProgress::new("Connecting")
            .size(TextSize::Base)
            .colour(theme.text_muted)
            .show(ui);
    }

    fn draw_connected(&mut self, ui: &mut egui::Ui, connector: &WalletConnector) -> WalletAction {
        let mut action = WalletAction::None;

        // Copy theme colors to avoid borrow conflicts
        let t = self.theme.resolved(&ui.tokens());
        let text_muted = t.text_muted;
        let accent = t.accent;

        // Top row: icon + handle/address
        ui.horizontal(|ui| {
            if let Some(ref icon_url) = connector.connected_icon {
                ui.add(
                    egui::Image::new(icon_url.as_str())
                        .fit_to_exact_size(egui::vec2(20.0, 20.0))
                        .corner_radius(ui.tokens().corner(Radius::Sm)),
                );
            }

            if let Some(ref handle) = connector.handle {
                ui.label(
                    RichText::new(handle)
                        .color(accent)
                        .size(ui.text_size(TextSize::Md))
                        .strong(),
                );
            } else if let Some(ref stake) = connector.stake_address {
                let truncated = if stake.len() > 20 {
                    format!("{}...{}", &stake[..8], &stake[stake.len() - 6..])
                } else {
                    stake.clone()
                };
                ui.label(
                    RichText::new(truncated)
                        .color(text_muted)
                        .size(ui.text_size(TextSize::Base)),
                );
            }
        });

        // Balance row
        if let Some(ref balance) = connector.balance {
            ui.gap(Space::Xs);
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
                        .size(ui.text_size(TextSize::Lg))
                        .strong(),
                );

                let tokens = balance.token_count();
                if tokens > 0 {
                    ui.label(
                        RichText::new(format!("\u{2022} {tokens} tokens"))
                            .color(text_muted)
                            .size(ui.text_size(TextSize::Sm)),
                    );
                }
            });
        }

        // Disconnect at bottom
        ui.gap(Space::Sm);
        if ui
            .add_clickable(
                egui::Button::new(
                    RichText::new("Disconnect")
                        .color(text_muted)
                        .size(ui.text_size(TextSize::Sm)),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(egui::Stroke::new(0.5_f32, text_muted))
                .corner_radius(ui.tokens().corner(Radius::Sm)),
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
        let theme = self.theme.resolved(&ui.tokens());

        ui.horizontal(|ui| {
            // Truncate long error messages
            let display_err = if error.len() > 40 {
                format!("{}...", &error[..37])
            } else {
                error.to_string()
            };
            ui.label(
                RichText::new(display_err)
                    .color(theme.error)
                    .size(ui.text_size(TextSize::Sm)),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_clickable(egui::Button::new(
                        RichText::new("Retry")
                            .color(theme.accent)
                            .size(ui.text_size(TextSize::Sm)),
                    ))
                    .clicked()
                {
                    // Try to reconnect with the first available wallet
                    if let Some(info) = connector.available_wallets.first()
                        && let Some(provider) = WalletProvider::from_api_name(&info.api_name)
                    {
                        action = WalletAction::Connect(provider);
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
