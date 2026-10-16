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
//! taken by a prompt that opens the wallet flow in a modal, at the point the
//! reader was about to act. Connected, the action is simply there, as a proper
//! primary button — once the flow's own last step has been read and dismissed.
//!
//! ## What it composes
//!
//! [`WalletConnectModal`] is the flow: which chain, which wallet, and then what
//! the wallet answered with. The CTA's title and intro are that modal's, so the
//! words the reader sees at the point of acting are the words on the modal they
//! get. Connecting is shown with [`LabelledProgress`], so the busy mark is the
//! size of its words.
//!
//! ## What it does not own
//!
//! Connecting: it returns [`WalletCtaAction::Connect`] and the caller spawns
//! it, like every wallet widget here. And whether the action is ready — that
//! is [`ActionState`], and a blocked action says why on hover.
//!
//! The modal's open flag and its step live in egui memory, keyed to where the
//! widget sits, so the caller holds no state at all and two of these on one page
//! cannot open each other's modal.
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
//!     WalletCtaAction::Connect(_wallet) => { /* spawn the connect */ }
//!     WalletCtaAction::None => {}
//! }
//! # }
//! ```

use egui::{RichText, Ui};

use crate::icons::PhosphorIcon;
use crate::labelled_progress::LabelledProgress;
use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};
use crate::wallet::{WalletChoice, WalletConnector};
use crate::wallet_button::WalletAction;
use crate::wallet_connect_modal::{ChainRequirement, WalletConnectModal};

/// What the reader did this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletCtaAction {
    None,
    /// The connected action was pressed.
    Act,
    /// A wallet was chosen in the modal, on whichever chain it is for. The
    /// caller spawns the connect that matches.
    Connect(WalletChoice),
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
    requirement: ChainRequirement,
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
            requirement: ChainRequirement::Any,
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

    /// What the modal should say connecting is FOR, and what it will not do —
    /// a reader asked to connect a wallet reasonably wants to know whether
    /// anything will be sent.
    pub fn modal_intro(mut self, intro: &'a str) -> Self {
        self.modal_intro = Some(intro);
        self
    }

    /// What this action can actually be done with. Say it here rather than
    /// letting the reader pick a wallet that could not have signed it: an EVM
    /// proof says [`ChainRequirement::Only`] and the modal never offers Rabby
    /// for a stake-key signature.
    pub fn requirement(mut self, requirement: ChainRequirement) -> Self {
        self.requirement = requirement;
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
        let mut modal = WalletConnectModal::new(ui.id().with(("wallet_cta", self.id_salt)))
            .title(self.modal_title)
            .requirement(self.requirement);
        if let Some(intro) = self.modal_intro {
            modal = modal.intro(intro);
        }

        let mut action = WalletCtaAction::None;

        if self.requirement.satisfied_by(connector) {
            if self.action_button(ui) {
                action = WalletCtaAction::Act;
            }
        } else if self.requirement.connecting_on(connector) && !modal.is_open(ui) {
            // Connecting from somewhere else — the header, a reconnect. Say
            // so in the action's place rather than offer a second connect.
            LabelledProgress::new("Connecting wallet")
                .size(TextSize::Base)
                .show(ui);
        } else if self.connect_prompt(ui).clicked() {
            modal.open(ui);
        }

        // The flow returns nothing but a choice — no disconnect lives in it.
        if let Some(WalletAction::Connect(choice)) = modal.show(ui, connector) {
            action = WalletCtaAction::Connect(choice);
        }
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
}
