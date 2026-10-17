//! Reusable wallet connection button widget for egui frontends.
//!
//! Renders a compact wallet status bar that handles:
//! - Disconnected: one "Connect wallet" button, which opens the chain-then-wallet
//!   flow in [`WalletConnectModal`] — never a list of wallets inline, because the
//!   choice has steps and a header has one line
//! - Connecting: spinner
//! - Connected: icon + name + handle + balance + disconnect
//! - Error: message + retry
//!
//! The widget does NOT spawn async tasks — it returns [`WalletAction`] values
//! that the caller dispatches through their own message channel.

use egui::{Color32, RichText};

use super::buttons::UiButtonExt;
use super::wallet::{
    ChainFamily, ConnectedIdentity, ConnectionPhase, WalletChoice, WalletConnector,
};
use crate::theme::{Ink, Radius, Space, SpaceExt, TextSize, ThemeExt, Token};
use crate::wallet_connect_modal::{ChainRequirement, WalletConnectModal};

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
    /// User selected a wallet to connect. The caller spawns the connect that
    /// matches the chain — `wallet::connect_wallet(provider)` for
    /// [`WalletChoice::Cardano`], the EIP-1193 bridge for
    /// [`WalletChoice::Evm`] — and sends the result back.
    Connect(WalletChoice),
    /// User clicked disconnect on one chain. Caller should call
    /// `connector.disconnect(family)` — one chain's disconnect says nothing
    /// about another's, which is why this carries the chain.
    Disconnect(ChainFamily),
}

/// Which chain a single-line button is *about*.
///
/// A button that shows one identity has to pick, and the requirement is what
/// picks: a button that needs Cardano is about Cardano even when EVM is
/// connected too. With no requirement it is about whichever chain answered
/// first — `ChainFamily::ALL` order, so it does not change under the reader.
enum Facing<'a> {
    /// Nothing to show: no connection, nothing pending.
    Idle,
    /// The chain this button is about is mid-connect.
    Connecting,
    /// The identity to draw.
    Connected(ConnectedIdentity<'a>),
    /// The chain this button is about refused.
    Failed(&'a str),
}

/// Read the set as one line's worth of state, for `requirement`.
fn facing<'a>(requirement: ChainRequirement, connector: &'a WalletConnector) -> Facing<'a> {
    let about = match requirement {
        ChainRequirement::Only(family) => Some(family),
        ChainRequirement::Any => None,
    };

    // Named chain first: everything below is the answer to "what about THAT
    // chain", in the order a reader cares about it.
    if let Some(family) = about {
        if let Some(identity) = connector
            .connections
            .identities()
            .find(|i| i.family == family)
        {
            return Facing::Connected(identity);
        }
        if connector.is_connecting(family) {
            return Facing::Connecting;
        }
        if let Some(error) = connector.connections.error(family) {
            return Facing::Failed(error);
        }
        return Facing::Idle;
    }

    // No named chain: whatever is connected, then anything in flight, then the
    // first refusal.
    if let Some(identity) = connector.identities().next() {
        return Facing::Connected(identity);
    }
    if connector.connections.any_connecting() {
        return Facing::Connecting;
    }
    if let Some((_, phase)) = connector.connections.iter().find_map(|(family, phase)| {
        matches!(phase, ConnectionPhase::Error(_)).then_some((family, phase))
    }) && let ConnectionPhase::Error(error) = phase
    {
        return Facing::Failed(error);
    }
    Facing::Idle
}

/// Whether the button draws its own surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonChrome {
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
    /// Whether to draw the button's own panel around its contents.
    pub chrome: ButtonChrome,
    /// What the flow this button opens may offer. Defaults to any chain the
    /// widget can connect; an action that needs a particular one says so here
    /// and its reader never sees a wallet that could not have signed it.
    pub requirement: ChainRequirement,
    /// Distinguishes two of these that share one parent `Ui` — the flow's state
    /// is keyed to where the button sits, and two buttons in one place would
    /// otherwise share an open flag.
    pub salt: &'static str,
    /// The id the flow's state lives under, when the host needs to name it.
    /// `None` derives one from the `Ui` the button sits in — which is the right
    /// default, and unnameable from outside that frame when a host has to close
    /// the flow with the surface it lives on.
    pub flow: Option<egui::Id>,
}

impl WalletButton {
    pub fn new() -> Self {
        Self {
            theme: WalletButtonTheme::default(),
            chrome: ButtonChrome::Framed,
            requirement: ChainRequirement::Any,
            salt: "wallet_button",
            flow: None,
        }
    }

    /// Create with a custom theme.
    pub fn with_theme(theme: WalletButtonTheme) -> Self {
        Self {
            theme,
            ..Self::new()
        }
    }

    /// Draw with or without the button's own panel. See [`ButtonChrome`].
    pub fn chrome(mut self, chrome: ButtonChrome) -> Self {
        self.chrome = chrome;
        self
    }

    /// What the flow this button opens may offer. See [`ChainRequirement`].
    pub fn requirement(mut self, requirement: ChainRequirement) -> Self {
        self.requirement = requirement;
        self
    }

    /// Distinguishes two of these that share one parent `Ui`. See [`Self::salt`].
    pub fn salt(mut self, salt: &'static str) -> Self {
        self.salt = salt;
        self
    }

    /// Name the flow's state yourself instead of deriving it from the `Ui`.
    ///
    /// For a host that closes the surface this button sits on: a flow left open
    /// there survives the close and reappears, with whatever it had already
    /// settled, the next time the surface is drawn. An id derived from the
    /// button's parent cannot be named from outside the frame that drew it, so
    /// the host names one here and closes it with `WalletConnectModal::close`.
    pub fn flow(mut self, id: egui::Id) -> Self {
        self.flow = Some(id);
        self
    }

    /// The flow this button opens.
    ///
    /// Built fresh every frame: the open flag and the step it has reached live
    /// in egui memory under the id, so the caller holds no state at all.
    fn modal(&self, ui: &egui::Ui) -> WalletConnectModal<'static> {
        let id = self
            .flow
            .unwrap_or_else(|| WalletConnectModal::id_for(ui, self.salt));
        WalletConnectModal::new(id).requirement(self.requirement)
    }

    /// Render the wallet button. Returns an action the caller must handle.
    pub fn show(&mut self, ui: &mut egui::Ui, connector: &WalletConnector) -> WalletAction {
        let modal = self.modal(ui);
        let mut action = match self.chrome {
            ButtonChrome::Framed => {
                let theme = self.theme.resolved(&ui.tokens());
                egui::Frame::new()
                    .fill(theme.bg)
                    .corner_radius(ui.tokens().corner(Radius::Md))
                    .inner_margin(ui.tokens().margin(Space::Md))
                    .show(ui, |ui| self.body(ui, connector, &modal))
                    .inner
            }
            ButtonChrome::Bare => self.body(ui, connector, &modal),
        };

        // Over the surface it was opened from, wherever that surface is.
        if let Some(chosen) = modal.show(ui, connector) {
            action = chosen;
        }
        action
    }

    /// The button's contents for the current connection state, unframed.
    fn body(
        &mut self,
        ui: &mut egui::Ui,
        connector: &WalletConnector,
        modal: &WalletConnectModal<'_>,
    ) -> WalletAction {
        ui.set_width(ui.available_width());
        match facing(self.requirement, connector) {
            Facing::Idle => self.draw_disconnected(ui, modal),
            Facing::Connecting => {
                self.draw_connecting(ui);
                WalletAction::None
            }
            Facing::Connected(identity) => self.draw_connected(ui, connector, identity),
            Facing::Failed(error) => self.draw_error(ui, error, modal),
        }
    }

    fn draw_disconnected(
        &mut self,
        ui: &mut egui::Ui,
        modal: &WalletConnectModal<'_>,
    ) -> WalletAction {
        let theme = self.theme.resolved(&ui.tokens());
        // One control, and the decision is not made here: the flow asks which
        // chain, then which wallet, then shows what the wallet answered. Drawn
        // inline this was every wallet on every chain at once, in a header.
        // "Nothing detected" is not a state of the button either — the flow says
        // so where it can also say what to do about it.
        let response = ui.add_clickable_sized(
            [ui.available_width(), 32.0],
            egui::Button::new(
                RichText::new("Connect wallet")
                    .color(theme.accent)
                    .size(ui.text_size(TextSize::Md)),
            )
            .corner_radius(ui.tokens().corner(Radius::Base)),
        );
        if response.clicked() {
            modal.open(ui);
        }
        WalletAction::None
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

    /// One identity, drawn as the button's contents.
    ///
    /// The balance row is Cardano's: it is a reading about that connection, and
    /// only that chain has one (see [`WalletConnector::balance`]). The identity
    /// says which chain is on screen, so the row cannot appear under an EVM
    /// address.
    fn draw_connected(
        &mut self,
        ui: &mut egui::Ui,
        connector: &WalletConnector,
        identity: ConnectedIdentity<'_>,
    ) -> WalletAction {
        let mut action = WalletAction::None;
        let cardano = identity.family == ChainFamily::Cardano;

        // Copy theme colors to avoid borrow conflicts
        let t = self.theme.resolved(&ui.tokens());
        let text_muted = t.text_muted;
        let accent = t.accent;

        // Top row: icon + handle/address
        ui.horizontal(|ui| {
            if let Some(icon_url) = identity.icon {
                ui.add(
                    egui::Image::new(icon_url)
                        .fit_to_exact_size(egui::vec2(20.0, 20.0))
                        .corner_radius(ui.tokens().corner(Radius::Sm)),
                );
            }

            // The `$handle` when there is one — Cardano's, and only when it has
            // been read: it is a name a human recognises, where the address is
            // a string they cannot check.
            let handle = if cardano {
                connector.handle.as_deref()
            } else {
                None
            };
            match handle {
                Some(handle) => {
                    ui.label(
                        RichText::new(handle)
                            .color(accent)
                            .size(ui.text_size(TextSize::Md))
                            .strong(),
                    );
                }
                None => {
                    let account = identity.account;
                    let truncated = if account.len() > 20 {
                        format!("{}...{}", &account[..8], &account[account.len() - 6..])
                    } else {
                        account.to_string()
                    };
                    ui.label(
                        RichText::new(truncated)
                            .color(text_muted)
                            .size(ui.text_size(TextSize::Base)),
                    );
                }
            }
        });

        // Balance row
        if cardano && let Some(ref balance) = connector.balance {
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
            action = WalletAction::Disconnect(identity.family);
        }

        action
    }

    fn draw_error(
        &mut self,
        ui: &mut egui::Ui,
        error: &str,
        modal: &WalletConnectModal<'_>,
    ) -> WalletAction {
        let action = WalletAction::None;
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
                    // Back through the flow, which knows which wallet was
                    // asked — rather than guessing at the first detected one.
                    modal.open(ui);
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
