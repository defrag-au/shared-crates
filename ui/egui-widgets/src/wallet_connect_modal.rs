//! `WalletConnectModal` — which wallet, as a sequence of decisions rather than a list of everything.
//!
//! ## Why a sequence
//!
//! A picker that shows every wallet on every chain asks the reader to answer a
//! question it has not asked yet: *the chain* decides *the wallets*, and a list
//! that ignores that order makes the reader do the filtering. It also has no
//! last step — a wallet that connects and then signs with a different account
//! has nowhere to say so, because a picker is over the moment it is answered.
//!
//! Three steps, and each one is skipped when its answer is already known:
//!
//! 1. **Chain** — only when the requirement admits more than one, and only
//!    chains with something to offer.
//! 2. **Wallet** — one row per detected wallet on that chain.
//! 3. **Answer** — the wallet has been asked: waiting, the account it answered
//!    with, or why it refused.
//!
//! So a link that needs a Cardano signature opens straight at Cardano wallets
//! and never mentions EVM; an EVM proof opens at the announced EVM wallets; and
//! a browser with one wallet in it has already answered step 2.
//!
//! ## What it does not own
//!
//! Connecting. It returns [`WalletAction::Connect`] and the caller spawns the
//! connect that matches the chain, like every wallet widget here. The open flag
//! and the step live in egui memory under the [`egui::Id`] the caller passes, so
//! a caller holds no state — and the id is the caller's to make unique, which is
//! what stops two surfaces opening each other's modal.
//!
//! ## One chain at a time, out of a set
//!
//! A reader can have Cardano and EVM connected at once, so every read here names
//! the chain it is about: the receipt shows *this* flow's connection, the
//! one-offer shortcut counts offers on *this* chain, and an EVM flow with a
//! Cardano wallet connected is untouched by it. The chain step is where the set
//! is visible — a chain that already has a wallet says what it is connected as.

use egui::{RichText, Ui};
use wallet_core::{WalletChoice, WalletProvider};

use crate::icons::PhosphorIcon;
use crate::id_pill::IdPill;
use crate::labelled_progress::LabelledProgress;
use crate::option_group::{GroupFlow, OptionGroup, OptionGroupItem};
use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};
use crate::wallet::{ChainConnection, ConnectionPhase, WalletConnector};
use crate::wallet_button::WalletAction;

/// The chain vocabulary this flow is expressed in.
///
/// Re-exported so a consumer can name a chain — `ChainRequirement::Only(...)` —
/// without taking a dependency on `chains` itself. It is `chains`' type, not a
/// copy of it: one vocabulary, one home.
pub use chains::ChainFamily;

/// The chains this widget can actually connect, in the order they are offered.
///
/// Midnight is a `ChainFamily` without a bridge, so it is not here — a family
/// joins this list the day there is a wallet to connect on it.
const CONNECTABLE: [ChainFamily; 2] = [ChainFamily::Cardano, ChainFamily::Evm];

/// What the action behind the modal can go ahead with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChainRequirement {
    /// Any chain this widget can connect. The chain step appears only when more
    /// than one of them has something to offer.
    #[default]
    Any,
    /// One family, and nothing else — the chain step is skipped, so the modal
    /// opens on that chain's wallets. An EVM proof says `Only(Evm)` here rather
    /// than letting the reader pick a wallet that cannot sign it.
    Only(ChainFamily),
}

impl ChainRequirement {
    /// Whether `family` can satisfy this requirement.
    pub fn admits(self, family: ChainFamily) -> bool {
        match self {
            Self::Any => CONNECTABLE.contains(&family),
            Self::Only(wanted) => wanted == family,
        }
    }

    /// Whether the connector already has what this requirement asks for.
    ///
    /// The question a surface asks before it offers to connect at all: an action
    /// that needs Cardano is not satisfied by an EVM connection, which is the
    /// whole reason this takes the connector rather than being `any_connected`.
    pub fn satisfied_by(self, connector: &WalletConnector) -> bool {
        match self {
            Self::Any => connector.any_connected(),
            Self::Only(family) => connector.is_connected(family),
        }
    }

    /// Whether the connector is mid-connect for a chain that would satisfy this.
    pub fn connecting_on(self, connector: &WalletConnector) -> bool {
        match self {
            Self::Any => connector.connections.any_connecting(),
            Self::Only(family) => connector.is_connecting(family),
        }
    }
}

/// Which screen the flow is on. Derived every frame, never stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Which chain, when the requirement admits more than one.
    Chain,
    /// Which wallet, on the chain already settled.
    Wallet,
    /// The wallet has been asked — waiting, answered, or refused.
    Answer,
}

/// What the flow has settled so far. Lives in egui memory under the modal's id.
#[derive(Clone, Default)]
struct FlowState {
    open: bool,
    /// The chain being offered: the requirement's, or the chain step's answer.
    family: Option<ChainFamily>,
    /// The wallet this flow asked, set as the choice is handed to the caller —
    /// so the answer step can name it, and so the one-offer shortcut fires once.
    asked: Option<WalletChoice>,
    /// The reader asked to choose a wallet again — the receipt named the
    /// account that answered, and they want a different one. Forces the wallet
    /// step even though the chain already has a connection.
    reconnecting: bool,
}

/// One wallet the modal can offer, and what the caller gets if it is picked.
struct Offer {
    choice: WalletChoice,
    name: String,
    icon: Option<String>,
    /// Hover text. The raw handle for an EVM wallet: what a bug report needs,
    /// and not what anyone picks a wallet by.
    hover: Option<String>,
}

/// The wallets `family` has to offer, as the connector currently knows them.
fn offers_on(connector: &WalletConnector, family: ChainFamily) -> Vec<Offer> {
    match family {
        ChainFamily::Cardano => connector
            .available_wallets
            .iter()
            .filter_map(|info| {
                let provider = WalletProvider::from_api_name(&info.api_name)?;
                Some(Offer {
                    choice: WalletChoice::Cardano(provider),
                    name: info.name.clone(),
                    icon: info.icon.clone(),
                    hover: None,
                })
            })
            .collect(),
        ChainFamily::Evm => connector
            .available_evm_wallets
            .iter()
            .map(|wallet| Offer {
                choice: WalletChoice::Evm(wallet.clone()),
                name: wallet.name.clone(),
                icon: wallet.icon.clone(),
                hover: Some(wallet.id.as_str().to_string()),
            })
            .collect(),
        // No bridge to it yet, so there is nothing to offer.
        ChainFamily::Midnight => Vec::new(),
    }
}

/// What to call the wallet a choice names.
fn choice_name(choice: &WalletChoice) -> &str {
    match choice {
        WalletChoice::Cardano(provider) => provider.display_name(),
        WalletChoice::Evm(wallet) => &wallet.name,
    }
}

/// See the [module docs](self).
pub struct WalletConnectModal<'a> {
    id: egui::Id,
    title: &'a str,
    intro: Option<&'a str>,
    requirement: ChainRequirement,
}

impl<'a> WalletConnectModal<'a> {
    /// A flow whose open flag and step live in egui memory under `id`.
    ///
    /// The id is the caller's to keep stable — and to keep *unique*, which is
    /// what stops a header button and a page's call to action from opening each
    /// other's modal.
    pub fn new(id: egui::Id) -> Self {
        Self {
            id,
            title: "Connect a wallet",
            intro: None,
            requirement: ChainRequirement::Any,
        }
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    /// One line under the title. The place to say what connecting does and does
    /// not do — a reader asked to connect a wallet reasonably wants to know
    /// whether anything will be sent.
    pub fn intro(mut self, intro: &'a str) -> Self {
        self.intro = Some(intro);
        self
    }

    pub fn requirement(mut self, requirement: ChainRequirement) -> Self {
        self.requirement = requirement;
        self
    }

    /// The id this flow's state lives under, for a caller that has not built one.
    pub fn id_for(ui: &Ui, salt: &str) -> egui::Id {
        ui.id().with(("wallet_connect", salt))
    }

    /// Open the flow. A no-op while it is already open, so a host may call it
    /// every frame — which is how a story or a preview can hold one open.
    pub fn open(&self, ui: &Ui) {
        ui.ctx().data_mut(|d| {
            let mut state = d.get_temp::<FlowState>(self.id).unwrap_or_default();
            if !state.open {
                state = FlowState {
                    open: true,
                    ..FlowState::default()
                };
            }
            d.insert_temp(self.id, state);
        });
    }

    pub fn close(&self, ui: &Ui) {
        ui.ctx().data_mut(|d| {
            let mut state = d.get_temp::<FlowState>(self.id).unwrap_or_default();
            state.open = false;
            d.insert_temp(self.id, state);
        });
    }

    pub fn is_open(&self, ui: &Ui) -> bool {
        ui.ctx()
            .data(|d| d.get_temp::<FlowState>(self.id))
            .is_some_and(|s| s.open)
    }

    /// Draw the flow, if it is open. `Some` when the reader picked a wallet.
    pub fn show(&self, ui: &mut Ui, connector: &WalletConnector) -> Option<WalletAction> {
        crate::icons::ensure_fonts(ui);
        let mut state = ui
            .ctx()
            .data(|d| d.get_temp::<FlowState>(self.id))
            .unwrap_or_default();
        // Never from a measuring pass: it sees every click as outside the modal
        // and would close it the frame it opened.
        if !state.open || ui.is_sizing_pass() {
            return None;
        }

        // A chain is offered when there is a wallet to pick OR one already
        // connected: a connection is a chain the reader can still act on — see
        // what it answered, and reconnect it — with nothing left to choose.
        let offered: Vec<(ChainFamily, Vec<Offer>)> = CONNECTABLE
            .iter()
            .copied()
            .filter(|family| self.requirement.admits(*family))
            .map(|family| (family, offers_on(connector, family)))
            .filter(|(family, offers)| !offers.is_empty() || connector.is_connected(*family))
            .collect();

        // A requirement that names a chain has already answered step 1 — and
        // the chain step is not offered at all, so nothing can contradict it.
        if let ChainRequirement::Only(family) = self.requirement {
            state.family = Some(family);
        }
        // A chain choice with one answer is not a choice.
        if state.family.is_none() && offered.len() == 1 {
            state.family = Some(offered[0].0);
        }
        let on_chain: &[Offer] = state
            .family
            .and_then(|family| offered.iter().find(|(f, _)| *f == family))
            .map(|(_, offers)| offers.as_slice())
            .unwrap_or(&[]);

        // What the flow's chain has to say, when it has been asked. Per-chain:
        // a Cardano connection is not an answer to an EVM flow, which is the
        // bug the set model exists to make unrepresentable.
        let answer = state
            .family
            .and_then(|family| connector.connections.phase(family));
        let settled = answer.is_some();
        let connected = matches!(answer, Some(ConnectionPhase::Connected(_)));
        let refused = matches!(answer, Some(ConnectionPhase::Error(_)));

        let mut action = None;
        // The one-offer shortcut: nothing to choose, so ask it. Skipped when the
        // connector is already mid-conversation on this chain, which is also
        // what stops an error from being retried behind the reader's back.
        if state.asked.is_none() && !settled && !state.reconnecting && on_chain.len() == 1 {
            state.asked = Some(on_chain[0].choice.clone());
            action = Some(WalletAction::Connect(on_chain[0].choice.clone()));
        }

        let step = if state.reconnecting {
            // The reader asked to choose a wallet again — the receipt said which
            // account answered and they want a different one, which a wallet
            // owns the choice of and a reconnect re-asks.
            Step::Wallet
        } else if state.asked.is_some() || settled || on_chain.len() == 1 {
            Step::Answer
        } else if state.family.is_none() {
            Step::Chain
        } else {
            Step::Wallet
        };

        let mut dismissed = false;
        let mut step_back = false;
        let mut rechoose = false;
        let mut retry = false;

        let response = egui::Modal::new(self.id.with("modal")).show(ui.ctx(), |ui| {
            // Clamped to the viewport, as `AboutModal` does: a flat width wider
            // than a phone would make the modal the thing that overflows.
            let room = (ui.ctx().content_rect().width() - 32.0).max(240.0);
            ui.set_min_width(300.0_f32.min(room));
            ui.set_max_width(380.0_f32.min(room));

            ui.label(
                RichText::new(self.title)
                    .size(ui.text_size(TextSize::Xl))
                    .strong(),
            );
            if let Some(intro) = self.intro {
                ui.label(muted(ui, intro));
            }
            if let Some(family) = state.family {
                // What the steps so far have settled. Also the only place the
                // chosen chain is named once the chain step is behind us.
                ui.label(muted(
                    ui,
                    &match (&state.asked, family) {
                        (Some(choice), _) => {
                            format!("{} \u{203a} {}", family.display_name(), choice_name(choice))
                        }
                        (None, family) => family.display_name().to_string(),
                    },
                ));
            }
            ui.gap(Space::Lg);

            match step {
                Step::Chain => {
                    if offered.is_empty() {
                        ui.label(muted(ui, "No wallets detected."));
                        ui.gap(Space::Sm);
                        ui.label(muted(
                            ui,
                            "Extensions are detected when the page loads — if you have just \
                             installed one, reload and try again.",
                        ));
                    } else {
                        // A connected chain says what it is connected AS: a
                        // reader switching chains wants to know which one
                        // already has a wallet before they pick it.
                        let notes: Vec<Option<String>> = offered
                            .iter()
                            .map(|(family, _)| {
                                connector
                                    .connections
                                    .connection(*family)
                                    .map(|c| format!("connected as {}", c.identity().wallet))
                            })
                            .collect();
                        let items: Vec<OptionGroupItem<'_>> = offered
                            .iter()
                            .enumerate()
                            .map(|(i, (family, _))| {
                                let mut item =
                                    OptionGroupItem::new(i as u64, family.display_name());
                                if let Some(note) = &notes[i] {
                                    item = item.note(note);
                                }
                                item
                            })
                            .collect();
                        let picked = OptionGroup::new()
                            .flow(GroupFlow::Stacked)
                            .items(items)
                            .show(ui)
                            .clicked;
                        if let Some(family) = picked.and_then(|i| offered.get(i as usize)) {
                            state.family = Some(family.0);
                        }
                    }
                }
                Step::Wallet => {
                    if on_chain.is_empty() {
                        let family = state.family.unwrap_or(ChainFamily::Cardano);
                        ui.label(muted(
                            ui,
                            &format!("No {} wallets detected.", family.display_name()),
                        ));
                        ui.gap(Space::Sm);
                        ui.label(muted(
                            ui,
                            "Extensions are detected when the page loads — if you have just \
                             installed one, reload and try again.",
                        ));
                    } else {
                        let items: Vec<OptionGroupItem<'_>> = on_chain
                            .iter()
                            .enumerate()
                            .map(|(i, offer)| {
                                let mut item = OptionGroupItem::new(i as u64, &offer.name);
                                if let Some(icon) = &offer.icon {
                                    item = item.image(icon);
                                }
                                if let Some(hover) = &offer.hover {
                                    item = item.hover(hover);
                                }
                                item
                            })
                            .collect();
                        let picked = OptionGroup::new()
                            .flow(GroupFlow::Stacked)
                            .items(items)
                            .show(ui)
                            .clicked;
                        if let Some(offer) = picked.and_then(|i| on_chain.get(i as usize)) {
                            state.asked = Some(offer.choice.clone());
                            state.reconnecting = false;
                            action = Some(WalletAction::Connect(offer.choice.clone()));
                        }
                    }
                }
                Step::Answer => match answer {
                    Some(ConnectionPhase::Connected(connection)) => receipt(ui, connection),
                    Some(ConnectionPhase::Error(error)) => {
                        ui.label(
                            RichText::new(error.as_str())
                                .color(ui.tokens().color.error)
                                .size(ui.text_size(TextSize::Sm)),
                        );
                    }
                    _ => waiting(ui, &state),
                },
            }

            ui.gap(Space::Lg);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match (step, connected) {
                    // The flow's own end: the receipt has been read. `Reconnect`
                    // is the answer to "that is the wrong account" — the wallet
                    // owns which account it gives, so the only thing to do about
                    // it is ask again, which is also how the reader switches
                    // accounts inside their wallet.
                    (Step::Answer, true) => {
                        dismissed |= primary_button(ui, "Done");
                        if ui.button("Reconnect").clicked() {
                            rechoose = true;
                        }
                    }
                    // Refusing is an answer too — and the same wallet is the one
                    // to ask again, not whichever happens to be first.
                    (Step::Answer, _) if refused => {
                        retry = primary_button(ui, "Try again");
                        if ui.button("Cancel").clicked() {
                            dismissed = true;
                        }
                    }
                    _ => {
                        if ui.button("Cancel").clicked() {
                            dismissed = true;
                        }
                    }
                }
                // Only where there is somewhere to go back TO: a chain choice
                // that was never offered, or a wallet list with one row, are not
                // screens the reader can return to.
                let can_go_back = match step {
                    Step::Wallet => offered.len() > 1,
                    Step::Answer => !connected && on_chain.len() > 1,
                    Step::Chain => false,
                };
                if can_go_back && ui.button("Back").clicked() {
                    step_back = true;
                }
            });
        });

        if step_back {
            match step {
                Step::Chain => {}
                Step::Wallet => state.family = None,
                // Back from an answer is "choose another wallet", the same as
                // the receipt's Reconnect.
                Step::Answer => {
                    state.reconnecting = true;
                    state.asked = None;
                }
            }
        }
        if rechoose {
            state.reconnecting = true;
            state.asked = None;
        }
        if retry {
            let again = state
                .asked
                .clone()
                .or_else(|| on_chain.first().map(|offer| offer.choice.clone()));
            if let Some(choice) = again {
                state.asked = Some(choice.clone());
                action = Some(WalletAction::Connect(choice));
            }
        }
        // Escape and a click on the scrim are dismissals too.
        if response.should_close() || dismissed {
            state.open = false;
        }
        ui.ctx().data_mut(|d| d.insert_temp(self.id, state));
        action
    }
}

/// The wallet has been asked and nothing has come back yet.
fn waiting(ui: &mut Ui, state: &FlowState) {
    let label = match state.asked.as_ref() {
        Some(choice) => format!("Waiting for {}", choice_name(choice)),
        None => "Waiting for your wallet".to_string(),
    };
    LabelledProgress::new(&label)
        .size(TextSize::Base)
        .colour(ui.tokens().color.text_muted)
        .show(ui);
    ui.gap(Space::Sm);
    ui.label(muted(ui, "Approve it in your wallet."));
    if matches!(state.asked, Some(WalletChoice::Evm(_))) {
        ui.label(muted(
            ui,
            "An EVM wallet may ask twice — once to connect, once to sign.",
        ));
    }
}

/// The wallet answered: which one, and the account it is signed in as.
///
/// The account is the point of this screen. A wallet holds several, the reader
/// may have the wrong one selected, and the difference is the one thing that
/// makes a downstream "this wallet does not sign with that key" answerable
/// instead of a mystery.
///
/// Chain-agnostic: it draws a [`ConnectedIdentity`], which is what every family
/// projects to, so an EVM receipt names the `0x…` account the same way a
/// Cardano one names a stake key.
fn receipt(ui: &mut Ui, connection: &ChainConnection) {
    let identity = connection.identity();

    ui.horizontal(|ui| {
        ui.set_item_gap_x(Space::Sm);
        if let Some(icon) = identity.icon {
            let px = ui.text_size(TextSize::Xl);
            ui.add(
                egui::Image::new(icon)
                    .fit_to_exact_size(egui::vec2(px, px))
                    .corner_radius(ui.tokens().corner(Radius::Xs)),
            );
        }
        ui.label(
            RichText::new(identity.wallet)
                .size(ui.text_size(TextSize::Md))
                .strong(),
        );
        ui.label(
            PhosphorIcon::CheckCircle
                .rich_text(ui.text_size(TextSize::Md), ui.tokens().color.accent_green),
        );
    });

    ui.gap(Space::Sm);
    IdPill::new("signed in as", identity.account)
        .copyable(true)
        .show(ui);
}

fn primary_button(ui: &mut Ui, label: &str) -> bool {
    let tokens = ui.tokens();
    let response = ui.add(
        egui::Button::new(
            RichText::new(label)
                .color(tokens.color.bg_primary)
                .size(ui.text_size(TextSize::Md))
                .strong(),
        )
        .fill(tokens.color.accent)
        .corner_radius(tokens.corner(Radius::Md))
        .min_size(egui::vec2(0.0, 32.0)),
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

fn muted(ui: &Ui, text: &str) -> RichText {
    RichText::new(text)
        .color(ui.tokens().color.text_muted)
        .size(ui.text_size(TextSize::Sm))
}
