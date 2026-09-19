//! `WalletCta` — an action that needs a wallet: the button when one is connected, and a way to connect one right there when it is not.
//!
//! ## Why it's a widget
//!
//! A surface that trades puts its main action behind a wallet, and the obvious
//! way to say so is two dead ends side by side: the button disabled, and
//! "connect a wallet to trade" beside it in muted text. One is a control that
//! does nothing; the other is an instruction that is not a control. The reader
//! is left to go and find the connector themselves — usually a pill in a header
//! corner, nowhere near the thing they were about to do.
//!
//! This makes the instruction the control. Disconnected, the action's place is
//! taken by a prompt that opens the wallet picker in a modal, at the point the
//! reader was about to act. Connected, the modal closes itself and the action
//! is simply there, as a proper primary button.
//!
//! ## What it composes
//!
//! The picker is [`WalletButton`]'s — per-extension icons, the one-wallet
//! shortcut, "no wallets detected", the error state. Drawn inline that picker
//! calls `set_width(available_width())` and spans whatever it is in; inside a
//! bounded modal the same call is exactly right. Connecting is shown with
//! [`LabelledProgress`], so the busy mark is the size of its words.
//!
//! ## What it does not own
//!
//! Connecting: it returns [`WalletCtaAction::Connect`] and the caller spawns
//! it, like every wallet widget here. And whether the action is ready — that
//! is [`ActionState`], and a blocked action says why on hover.
//!
//! The modal's open flag lives in egui memory, keyed to where the widget sits,
//! so the caller holds no state at all and two of these on one page cannot
//! open each other's modal.
//!
//! ```no_run
//! # use egui_widgets::wallet_cta::{ActionState, WalletCta, WalletCtaAction};
//! # use egui_widgets::wallet::WalletConnector;
//! # fn demo(ui: &mut egui::Ui, wallet: &WalletConnector, quoted: bool) {
//! let state = if quoted {
//!     ActionState::Ready
//! } else {
//!     ActionState::Blocked { hint: "Quote a trade first" }
//! };
//! match WalletCta::new("Add to route")
//!     .state(state)
//!     .connect_label("Connect a wallet to trade")
//!     .show(ui, wallet)
//! {
//!     WalletCtaAction::Act => { /* add it */ }
//!     WalletCtaAction::Connect(_provider) => { /* spawn the connect */ }
//!     WalletCtaAction::None => {}
//! }
//! # }
//! ```

use egui::{RichText, Ui};

use crate::icons::PhosphorIcon;
use crate::labelled_progress::LabelledProgress;
use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};
use crate::wallet::{WalletConnector, WalletProvider};
use crate::wallet_button::{PickerChrome, WalletAction, WalletButton};

/// What the reader did this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletCtaAction {
    None,
    /// The connected action was pressed.
    Act,
    /// A wallet was chosen in the modal. The caller connects it.
    Connect(WalletProvider),
}

/// Whether the action can be taken yet, once a wallet is connected.
///
/// An enum, not a `bool`: a blocked action has to say WHY, and a boolean has
/// nowhere to put the reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionState<'a> {
    Ready,
    /// Not yet — `hint` is shown on hover.
    Blocked {
        hint: &'a str,
    },
}

/// How the connect prompt is drawn when no wallet is connected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ConnectPrompt {
    /// An accent link with a wallet glyph. Sits in a line of text or beside a
    /// quote without shouting over the thing being read.
    #[default]
    Link,
    /// A filled primary button, for a surface where connecting IS the main
    /// thing to do next.
    Button,
}

/// See the [module docs](self).
pub struct WalletCta<'a> {
    action_label: &'a str,
    state: ActionState<'a>,
    connect_label: &'a str,
    prompt: ConnectPrompt,
    modal_title: &'a str,
    modal_intro: Option<&'a str>,
    id_salt: &'a str,
}

impl<'a> WalletCta<'a> {
    /// An action labelled `action_label`, ready by default.
    pub fn new(action_label: &'a str) -> Self {
        Self {
            action_label,
            state: ActionState::Ready,
            connect_label: "Connect a wallet",
            prompt: ConnectPrompt::default(),
            modal_title: "Connect a wallet",
            modal_intro: None,
            id_salt: "wallet_cta",
        }
    }

    pub fn state(mut self, state: ActionState<'a>) -> Self {
        self.state = state;
        self
    }

    /// The prompt's words when no wallet is connected — say what connecting
    /// is FOR ("Connect a wallet to trade"), not just that it is possible.
    pub fn connect_label(mut self, label: &'a str) -> Self {
        self.connect_label = label;
        self
    }

    pub fn prompt(mut self, prompt: ConnectPrompt) -> Self {
        self.prompt = prompt;
        self
    }

    pub fn modal_title(mut self, title: &'a str) -> Self {
        self.modal_title = title;
        self
    }

    /// One line under the modal title. The place to say what connecting does
    /// and does not do — a reader asked to connect a wallet reasonably wants
    /// to know whether anything will be sent.
    pub fn modal_intro(mut self, intro: &'a str) -> Self {
        self.modal_intro = Some(intro);
        self
    }

    /// Distinguishes two of these that share one parent `Ui`.
    pub fn id_salt(mut self, salt: &'a str) -> Self {
        self.id_salt = salt;
        self
    }

    pub fn show(self, ui: &mut Ui, connector: &WalletConnector) -> WalletCtaAction {
        crate::icons::ensure_fonts(ui);
        // Keyed to where the widget sits AND a salt, so two on one page cannot
        // share — and so close — one modal. (A shared popup id is exactly how
        // the account bar's menu once became impossible to open.)
        let id = ui.id().with(("wallet_cta", self.id_salt));
        let mut open = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let mut action = WalletCtaAction::None;

        if connector.is_connected() {
            // The job is done; nothing to keep open.
            open = false;
            if self.action_button(ui) {
                action = WalletCtaAction::Act;
            }
        } else if connector.is_connecting() && !open {
            // Connecting from somewhere else — the header, a reconnect. Say
            // so in the action's place rather than offer a second connect.
            LabelledProgress::new("Connecting wallet")
                .size(TextSize::Base)
                .show(ui);
        } else if self.connect_prompt(ui).clicked() {
            open = true;
        }

        // Never from a sizing pass: a measuring pass sees every click as
        // outside the modal and would close it the frame it opened.
        if open
            && !ui.is_sizing_pass()
            && let Some(provider) = self.modal(ui, connector, id, &mut open)
        {
            action = WalletCtaAction::Connect(provider);
        }

        ui.ctx().data_mut(|d| d.insert_temp(id, open));
        action
    }

    /// The connected action, styled as the surface's primary button.
    fn action_button(&self, ui: &mut Ui) -> bool {
        let tokens = ui.tokens();
        let ready = self.state == ActionState::Ready;
        // The same primary treatment as the cart's own Prepare — this IS the
        // page's main action, and a default grey button said otherwise.
        let (fill, ink) = if ready {
            (tokens.color.accent_green, tokens.color.bg_primary)
        } else {
            (tokens.color.bg_secondary, ui.visuals().weak_text_color())
        };
        let response = ui.add_enabled(
            ready,
            egui::Button::new(
                RichText::new(self.action_label)
                    .color(ink)
                    .size(ui.text_size(TextSize::Md))
                    .strong(),
            )
            .fill(fill)
            .corner_radius(tokens.corner(Radius::Md))
            .min_size(egui::vec2(0.0, 32.0)),
        );
        let clicked = response.clicked();
        if response.hovered() && ready {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if let ActionState::Blocked { hint } = self.state {
            response.on_disabled_hover_text(hint);
        }
        clicked
    }

    /// The prompt shown in the action's place while no wallet is connected.
    fn connect_prompt(&self, ui: &mut Ui) -> egui::Response {
        let tokens = ui.tokens();
        let size = ui.text_size(TextSize::Md);
        match self.prompt {
            ConnectPrompt::Link => {
                ui.horizontal(|ui| {
                    ui.set_item_gap_x(Space::Sm);
                    // The glyph through `rich_text`, which carries the icon
                    // font; the link beside it in the body face.
                    ui.label(PhosphorIcon::Wallet.rich_text(size, tokens.color.accent));
                    ui.add(egui::Link::new(
                        RichText::new(self.connect_label)
                            .color(tokens.color.accent)
                            .size(size),
                    ))
                })
                .inner
            }
            ConnectPrompt::Button => {
                let response = ui.add(
                    egui::Button::new(
                        RichText::new(self.connect_label)
                            .color(tokens.color.bg_primary)
                            .size(size)
                            .strong(),
                    )
                    .fill(tokens.color.accent)
                    .corner_radius(tokens.corner(Radius::Md))
                    .min_size(egui::vec2(0.0, 32.0)),
                );
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                response
            }
        }
    }

    /// The picker, in a modal. `Some` when a wallet was chosen.
    fn modal(
        &self,
        ui: &Ui,
        connector: &WalletConnector,
        id: egui::Id,
        open: &mut bool,
    ) -> Option<WalletProvider> {
        let mut picked = None;
        let mut dismissed = false;
        let response = egui::Modal::new(id.with("modal")).show(ui.ctx(), |ui| {
            // Clamped to the viewport, as `AboutModal` does: a flat width wider
            // than a phone would make the modal the thing that overflows.
            let room = (ui.ctx().content_rect().width() - 32.0).max(240.0);
            ui.set_min_width(300.0_f32.min(room));
            ui.set_max_width(380.0_f32.min(room));

            ui.label(
                RichText::new(self.modal_title)
                    .size(ui.text_size(TextSize::Xl))
                    .strong(),
            );
            if let Some(intro) = self.modal_intro {
                ui.label(
                    RichText::new(intro)
                        .color(ui.tokens().color.text_muted)
                        .size(ui.text_size(TextSize::Sm)),
                );
            }
            ui.gap(Space::Lg);

            // Built fresh: the picker holds only its theme and density, and
            // the connection state it draws is the caller's `connector`.
            // `Bare`: the modal is already the surface. Framed, the picker drew
            // its own filled panel inside it — a box inside a box.
            match WalletButton::new()
                .chrome(PickerChrome::Bare)
                .show(ui, connector)
            {
                WalletAction::Connect(provider) => picked = Some(provider),
                WalletAction::None | WalletAction::Disconnect => {}
            }

            ui.gap(Space::Lg);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    dismissed = true;
                }
            });
        });
        // Escape and a click on the scrim are dismissals too.
        if response.should_close() || dismissed {
            *open = false;
        }
        picked
    }
}
