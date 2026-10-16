//! `AccountBar` — the trailing cluster of an app header: the actions a
//! connected user reaches for, and the account itself.
//!
//! ## Why it's a widget
//!
//! Every wallet-connected surface grows the same corner independently, and
//! each one gets a different part of it wrong. The routes frontend had a cart
//! toggle and a wallet panel that shared no baseline, no height and no visual
//! language — a bare button beside a bordered box, the two sitting at
//! different vertical positions because one was a `Button` and the other a
//! `Frame` with its own margins. The jpg mirror had a truncated stake address
//! hard against the viewport edge, with nothing to click and no way to copy
//! it.
//!
//! Both are the same three jobs: *who is connected*, *what they can do from
//! here*, and *how to leave*. Doing them once means they align by
//! construction rather than by each caller counting pixels.
//!
//! ## What it composes
//!
//! Nothing here paints an identity from scratch. Connected, the account is a
//! [`UserBadge`](crate::user_badge::UserBadge): the name up front, and behind
//! one click the stake address as a copyable [`IdPill`](crate::id_pill::IdPill)
//! plus whatever details the caller attaches. That matters beyond tidiness —
//! a middle-truncated address is not safe to eyeball, so it belongs behind a
//! copy button and never in the pill itself.
//!
//! Disconnected, it delegates to [`WalletButton`](crate::wallet_button::WalletButton)
//! by opening the same flow it opens — the chain-then-wallet modal, not a second
//! copy of it. The bar holds no connector state of its own.
//!
//! ## Triggers
//!
//! A trigger is a caller-named action with an optional count —
//! `[icon] Route (2)`. The count is a figure, not a dot, because *how many*
//! is the thing worth knowing without opening it. Triggers sit before the
//! account, in the order given: actions first, identity rightmost, which is
//! where a reader looks for it.
//!
//! ```no_run
//! # use egui_widgets::account_bar::{AccountBar, AccountBarAction, Trigger};
//! # use egui_widgets::icons::PhosphorIcon;
//! # use egui_widgets::wallet::WalletConnector;
//! # fn demo(ui: &mut egui::Ui, bar: &mut AccountBar, wallet: &WalletConnector) {
//! let triggers = [Trigger::new("cart", "Route", PhosphorIcon::Package)
//!     .count(2)
//!     .disabled_hint("Nothing staged yet")];
//!
//! match bar.show(ui, wallet, &triggers) {
//!     AccountBarAction::Trigger(id) if id == "cart" => { /* open the drawer */ }
//!     AccountBarAction::Disconnect(family) => { /* drop that chain's wallet */ }
//!     _ => {}
//! }
//! # }
//! ```

use egui::{Align, Color32, Layout, RichText, Sense, Ui, Vec2};

use crate::auth::Session;
use crate::icons::PhosphorIcon;
use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};
use crate::user_badge::{UserBadge, UserBadgeAction};
use crate::wallet::{
    ChainFamily, ConnectedIdentity, ConnectionPhase, WalletChoice, WalletConnector,
};
use crate::wallet_button::WalletAction;
use crate::wallet_connect_modal::{ChainRequirement, WalletConnectModal};

/// What the user did with the bar this frame.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AccountBarAction {
    None,
    /// A wallet was chosen in the connect flow, on whichever chain it is for. The
    /// caller connects it — and says what it does about a chain it cannot
    /// reach, which for most surfaces today is nothing yet.
    Connect(WalletChoice),
    /// Leave — from the account popup on the wallet path, on ONE chain.
    ///
    /// This drops the wallet, and only the one named: a reader connected on two
    /// chains keeps the other. Under [`AccountIdentity::Session`] the session
    /// survives it: a user detaching a wallet has not signed out.
    Disconnect(ChainFamily),
    /// End the session, from the identity badge's popup.
    ///
    /// Distinct from [`Self::Disconnect`] because under
    /// [`AccountIdentity::Session`] the two are different acts on different
    /// things, and a caller that conflated them would sign a user out of the
    /// app when they meant to swap wallets.
    SignOut,
    /// A trigger was clicked, carrying the id it was given.
    Trigger(String),
}

/// What the wallet is FOR in a header whose identity is a session.
///
/// A named decision rather than a bool, because the two cases are not "with
/// and without a feature" — they are different statements about the app. An
/// app that never spends must not show a Connect affordance at all; an app
/// that does must show one even while signed in, because being signed in says
/// nothing about whether a wallet is attached.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WalletRole {
    /// No wallet in the header. The app does not spend.
    Absent,
    /// The wallet pays. Drawn beside the identity, with its own connect
    /// affordance — *who you are* and *what pays* are different facts and a
    /// reader must be able to see both at once.
    Paying,
}

/// Who the header says you are, and what established that.
///
/// The distinction is not cosmetic. [`AccountBarConfig::subtitle`] is the
/// provenance line in the account popup and is documented as read literally —
/// so an app that signs you in with Discord and merely *holds* a wallet cannot
/// use the wallet-identity path without the popup stating something untrue.
/// Holding a wallet is not signing in with one.
pub enum AccountIdentity<'a> {
    /// The connected wallet IS the identity — the app is entered by connecting
    /// one, and there is nothing else to be signed in as.
    Wallet,
    /// An authenticated session identifies the user; `role` says what the
    /// wallet, if any, is doing here.
    Session {
        session: &'a Session,
        role: WalletRole,
    },
}

/// How much room the bar has to work with.
///
/// An enum rather than a width threshold: the caller knows whether this
/// header is the whole viewport or a column in it, and a widget guessing from
/// `available_width` inside a `horizontal` reads the space left after
/// everything already placed, which is not the same question.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BarDensity {
    /// Icon and label on every trigger.
    #[default]
    Comfortable,
    /// Icons and counts only; labels move to hover text. For a header that
    /// also carries navigation, or a narrow viewport.
    Compact,
}

/// One trailing action.
pub struct Trigger<'a> {
    id: &'a str,
    label: &'a str,
    icon: PhosphorIcon,
    count: usize,
    enabled: bool,
    disabled_hint: &'a str,
}

impl<'a> Trigger<'a> {
    pub fn new(id: &'a str, label: &'a str, icon: PhosphorIcon) -> Self {
        Self {
            id,
            label,
            icon,
            count: 0,
            // Enabled unless a caller says otherwise, and a zero count does
            // NOT disable: a trigger may open something that is empty on
            // purpose, and only the caller knows which.
            enabled: true,
            disabled_hint: "",
        }
    }

    /// Shown as a badge beside the label. Zero renders no badge.
    pub fn count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Hover text while disabled — why it cannot be used yet.
    ///
    /// A disabled control with no explanation is the reader's problem to
    /// solve, and they cannot.
    pub fn disabled_hint(mut self, hint: &'a str) -> Self {
        self.disabled_hint = hint;
        self
    }
}

/// Display configuration.
pub struct AccountBarConfig {
    /// Trigger labels and the account name.
    pub label: TextSize,
    /// The count badge.
    pub count: TextSize,
    pub density: BarDensity,
    /// The provenance line in the account popup — how this session was
    /// established. Read literally, so it must be true.
    pub subtitle: &'static str,
}

impl Default for AccountBarConfig {
    fn default() -> Self {
        Self {
            label: TextSize::Md,
            count: TextSize::Sm,
            density: BarDensity::default(),
            subtitle: "Connected wallet",
        }
    }
}

/// The header's trailing cluster.
///
/// Holds nothing of the connect flow: its open flag and its step live in egui
/// memory, keyed to where the bar sits, exactly as [`WalletButton`]'s does.
#[derive(Default)]
pub struct AccountBar {
    pub config: AccountBarConfig,
    /// What the connect flow behind the Connect pill may offer — see
    /// [`Self::requirement`].
    pub requirement: ChainRequirement,
    /// How tall the pills came out last frame. Claimed up front, so the bar's
    /// centre is the pills' centre (see [`Self::show`]).
    bar_height: f32,
}

impl AccountBar {
    pub fn new() -> Self {
        Self::default()
    }

    /// How tall the bar drew last frame; `0.0` before its first.
    ///
    /// Claim it at the start of the header row — `ui.set_min_height(bar.height())`
    /// inside the `horizontal` — so what comes before the bar centres on the
    /// same line. `horizontal` centres each widget in the row as it stands when
    /// that widget is placed, and the bar, placed last, is the tallest thing in
    /// it: without this a title beside it sits high.
    pub fn height(&self) -> f32 {
        self.bar_height
    }

    pub fn with_config(config: AccountBarConfig) -> Self {
        Self {
            config,
            requirement: ChainRequirement::default(),
            bar_height: 0.0,
        }
    }

    /// What the connect flow may offer.
    ///
    /// [`ChainRequirement::Any`] by default, because a bar that has not been
    /// told what its app can sign with must not narrow the choice. An app that
    /// signs with Cardano alone sets [`ChainRequirement::Only`], and the flow
    /// then never offers a chain whose answer the app has no path to act on —
    /// which is the alternative to every caller matching a [`WalletChoice`] it
    /// cannot use.
    ///
    /// [`WalletChoice`]: crate::wallet::WalletChoice
    pub fn requirement(mut self, requirement: ChainRequirement) -> Self {
        self.requirement = requirement;
        self
    }

    /// Draw the cluster, right-aligned in whatever width it is given.
    ///
    /// Right-to-left internally so the account lands on the trailing edge and
    /// everything shares one vertical centre — the alignment the callers that
    /// hand-rolled this never got, because a `Button` and a bordered `Frame`
    /// have different intrinsic heights and no amount of `add_space` fixes
    /// that.
    /// Draw the cluster with the wallet as the identity.
    ///
    /// Shorthand for [`Self::show_as`] with [`AccountIdentity::Wallet`]; it
    /// delegates rather than duplicating, so the two cannot drift.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        connector: &WalletConnector,
        triggers: &[Trigger<'_>],
    ) -> AccountBarAction {
        self.show_as(ui, AccountIdentity::Wallet, connector, triggers)
    }

    /// Draw the cluster, saying explicitly what identifies the user.
    pub fn show_as(
        &mut self,
        ui: &mut Ui,
        identity: AccountIdentity<'_>,
        connector: &WalletConnector,
        triggers: &[Trigger<'_>],
    ) -> AccountBarAction {
        crate::icons::ensure_fonts(ui);
        let mut action = AccountBarAction::None;

        // Right-to-left, one pass, with every pill built by `ui.horizontal`.
        //
        // Three other shapes were tried and each fails on something:
        //
        // - `with_layout(left_to_right)` inside the pills states the direction
        //   but sizes to `available_rect`, so the first pill spans the bar and
        //   clips its own text.
        // - One `horizontal` wrapper holding the cluster fills the width the
        //   same way, putting the cluster back on the left.
        // - Measuring with a `sizing_pass` probe and right-aligning behind a
        //   spacer lays out correctly but draws every widget twice a frame,
        //   which breaks the account popup — it never opens.
        //
        // `ui.horizontal` is the only thing that shrinks to its content, and
        // it takes its direction from the parent. So the parent stays
        // right-to-left and each pill lays its own contents out to match.
        //
        // The bar is as tall as the pills came out last frame, not
        // `interact_size.y`. A pill is a `Frame`, and a `Frame` draws from the
        // TOP of the space it is given whatever the layout's alignment says, so
        // a pill taller than that space hung below its centre — and below
        // everything a caller centred beside the bar. Given exactly its own
        // height, top and centre are the same place.
        let height = self.bar_height.max(ui.spacing().interact_size.y);
        let bar = Vec2::new(ui.available_width(), height);
        // Keyed to the ui the caller handed us, not the one the pills are laid
        // out in: `allocate_ui_with_layout` mints its child id from the
        // allocation counter, so a widget appearing above the bar would move
        // it — and a modal keyed to a moving id closes itself.
        let connect_id = WalletConnectModal::id_for(ui, "account_bar");
        let drawn = ui.allocate_ui_with_layout(bar, Layout::right_to_left(Align::Center), |ui| {
            ui.set_item_gap_x(Space::Sm);
            action = self.cluster(ui, identity, connector, triggers, connect_id);
        });
        let measured = drawn.response.rect.height();
        if (measured - self.bar_height).abs() > 0.5 {
            self.bar_height = measured;
            ui.ctx().request_repaint();
        }

        action
    }

    /// The pills: actions, then the account.
    fn cluster(
        &mut self,
        ui: &mut Ui,
        identity: AccountIdentity<'_>,
        connector: &WalletConnector,
        triggers: &[Trigger<'_>],
        connect_id: egui::Id,
    ) -> AccountBarAction {
        let mut action = AccountBarAction::None;

        // The account goes in FIRST because the layout is right-to-left, which
        // puts it rightmost — where a reader looks for who they are signed in
        // as — and the triggers reversed after it, so they end up in the order
        // the caller wrote them.
        match identity {
            AccountIdentity::Wallet => {
                if connector.any_connected() {
                    if let Some(family) = self.accounts(ui, connector) {
                        action = AccountBarAction::Disconnect(family);
                    }
                } else {
                    action = self.connect(ui, connector, connect_id);
                }
            }
            AccountIdentity::Session { session, role } => {
                // Identity first, so it stays rightmost whether or not a
                // wallet is attached — the account does not move under the
                // reader when they connect one.
                if self.session_account(ui, session) == UserBadgeAction::SignOut {
                    action = AccountBarAction::SignOut;
                }
                // Then the wallet, to its left: a second pill, because it
                // answers a different question. `Disconnect` here drops the
                // WALLET and leaves the session alone, which is why sign-out
                // has its own action.
                if role == WalletRole::Paying {
                    if connector.any_connected() {
                        if let Some(family) = self.accounts(ui, connector) {
                            action = AccountBarAction::Disconnect(family);
                        }
                    } else {
                        let connect = self.connect(ui, connector, connect_id);
                        if connect != AccountBarAction::None {
                            action = connect;
                        }
                    }
                }
            }
        }

        for trigger in triggers.iter().rev() {
            if self.trigger(ui, trigger) {
                action = AccountBarAction::Trigger(trigger.id.to_string());
            }
        }
        action
    }

    /// The identity badge for a session-authenticated app.
    ///
    /// The subtitle comes from the session rather than
    /// [`AccountBarConfig::subtitle`], whose default says "Connected wallet" —
    /// true on the wallet path and a lie on this one. The popup's provenance
    /// line is read literally, so it is derived from what actually proved the
    /// identity.
    fn session_account(&self, ui: &mut Ui, session: &Session) -> UserBadgeAction {
        let name = session.label();
        let avatar = session.avatar_url();

        let mut badge = UserBadge::new(&name)
            .icon(PhosphorIcon::User)
            .subtitle("Signed in")
            .avatar_url(avatar.as_deref())
            .id_salt("account_bar_session");

        // The stake address appears only when the session actually proved one
        // (strategy C). A Discord session has no stake to show, and an empty
        // identifier row would imply otherwise.
        if let Some(stake) = session.claims.stake.as_deref().filter(|s| !s.is_empty()) {
            badge = badge.identifier("stake", stake);
        }
        badge.show(ui)
    }

    /// Not connected yet: a pill that opens the wallet flow.
    ///
    /// The flow is [`WalletConnectModal`] — the same one [`WalletButton`] opens,
    /// because a second connect surface is a second set of chain and wallet
    /// rules to keep in step. It draws over the whole app, so the pill stays a
    /// pill: nothing is stacked inside the bar's own height.
    fn connect(
        &mut self,
        ui: &mut Ui,
        connector: &WalletConnector,
        modal_id: egui::Id,
    ) -> AccountBarAction {
        let tokens = ui.tokens();
        let failed = connector
            .connections
            .iter()
            .any(|(_, phase)| matches!(phase, ConnectionPhase::Error(_)));
        let connecting = connector.connections.any_connecting();
        let ink = if failed {
            tokens.color.error
        } else {
            tokens.color.accent
        };
        let label = if connecting { "Connecting" } else { "Connect" };

        let pill = Pill {
            icon: PhosphorIcon::Wallet,
            label: (self.config.density != BarDensity::Compact).then_some(label),
            count: 0,
            ink,
            fill: tokens.color.bg_secondary,
            clickable: true,
        }
        .show(
            ui,
            ui.text_size(self.config.label),
            ui.text_size(self.config.count),
        );
        if pill.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let pill = if self.config.density == BarDensity::Compact {
            pill.on_hover_text(label)
        } else {
            pill
        };

        let mut action = AccountBarAction::None;
        if ui.is_sizing_pass() {
            return action;
        }
        // Built fresh every frame: the open flag and the step it has reached
        // live in egui memory under `modal_id`.
        let modal = WalletConnectModal::new(modal_id).requirement(self.requirement);
        if pill.clicked() {
            modal.open(ui);
        }
        match modal.show(ui, connector) {
            Some(WalletAction::Connect(choice)) => action = AccountBarAction::Connect(choice),
            // The flow returns choices, not disconnects — the badge's popup is
            // where a chain is dropped.
            Some(WalletAction::Disconnect(family)) => action = AccountBarAction::Disconnect(family),
            Some(WalletAction::None) | None => {}
        }
        action
    }

    /// One badge per connected chain — the cluster's account half.
    ///
    /// A reader can have Cardano and EVM connected at once, and they are
    /// different identities answering different questions, so each gets its own
    /// pill rather than the newest one hiding the other. `Some(family)` when the
    /// reader asked to drop that one.
    fn accounts(&self, ui: &mut Ui, connector: &WalletConnector) -> Option<ChainFamily> {
        let mut sign_out = None;
        for identity in connector.identities() {
            // The cluster lays out right-to-left, so the FIRST identity drawn
            // lands rightmost — Cardano, in `ChainFamily::ALL` order, where a
            // reader looks for the account they sign with day to day.
            if self.account(ui, connector, identity) == UserBadgeAction::SignOut {
                sign_out = Some(identity.family);
            }
        }
        sign_out
    }

    /// One connected account, as a [`UserBadge`].
    fn account(
        &self,
        ui: &mut Ui,
        connector: &WalletConnector,
        identity: ConnectedIdentity<'_>,
    ) -> UserBadgeAction {
        let cardano = identity.family == ChainFamily::Cardano;
        // A `$handle` when there is one — it is what a human recognises. The
        // account is the fallback, middle-elided, and it is ALSO in the popup as
        // a copyable pill, because a truncated address on its own is a string
        // you cannot do anything with.
        let account = identity.account;
        let name = match (cardano, connector.handle.as_deref()) {
            (true, Some(handle)) => handle.to_string(),
            _ => elide(account),
        };

        // The subtitle names the chain when there is more than one connection to
        // tell apart; with a single connection the caller's subtitle stands, as
        // it always did.
        let subtitle = if connector.identities().count() > 1 {
            identity.family.display_name()
        } else {
            self.config.subtitle
        };

        let mut badge = UserBadge::new(&name)
            .icon(PhosphorIcon::Wallet)
            .subtitle(subtitle)
            .avatar_url(identity.icon)
            .id_salt(match identity.family {
                ChainFamily::Cardano => "account_bar",
                ChainFamily::Evm => "account_bar_evm",
                ChainFamily::Midnight => "account_bar_midnight",
            });
        if !account.is_empty() {
            badge = badge.identifier("stake", account);
        }
        // The balance is the Cardano connection's reading; an EVM pill has none
        // to show and must not borrow Cardano's.
        if cardano && let Some(balance) = &connector.balance {
            badge = badge.detail("Balance", format!("{:.2} ADA", balance.ada()));
            let tokens = balance.token_count();
            if tokens > 0 {
                badge = badge.detail("Tokens", tokens.to_string());
            }
        }
        badge.show(ui)
    }

    /// One trigger. Returns true when clicked.
    fn trigger(&self, ui: &mut Ui, trigger: &Trigger<'_>) -> bool {
        let compact = self.config.density == BarDensity::Compact;
        let tokens = ui.tokens();
        let label_size = ui.text_size(self.config.label);
        let count_size = ui.text_size(self.config.count);

        // Live means "there is something in there". It is the only state
        // worth colouring: a cart with two items in it is a different thing
        // from an empty one, and the count alone is easy to miss in a header.
        let live = trigger.count > 0;
        let (fill, ink) = if !trigger.enabled {
            (tokens.color.bg_secondary, ui.visuals().weak_text_color())
        } else if live {
            (tokens.color.bg_secondary, tokens.color.accent)
        } else {
            (ui.visuals().faint_bg_color, tokens.color.text_secondary)
        };

        let response = Pill {
            icon: trigger.icon,
            label: (!compact).then_some(trigger.label),
            count: trigger.count,
            ink,
            fill,
            clickable: trigger.enabled,
        }
        .show(ui, label_size, count_size);

        if !trigger.enabled {
            if !trigger.disabled_hint.is_empty() {
                return response.on_hover_text(trigger.disabled_hint).clicked();
            }
            return false;
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        // In compact density the label is the only thing that says what this
        // opens, so it has to go somewhere.
        if compact {
            return response.on_hover_text(trigger.label).clicked();
        }
        response.clicked()
    }
}

/// The one shape every control in this bar takes.
///
/// A struct rather than seven positional arguments, and shared rather than
/// written twice, because "the connect button looks like the cart button
/// looks like the account" is the entire point of the widget — two copies of
/// this geometry would drift the first time either was touched.
struct Pill<'a> {
    icon: PhosphorIcon,
    /// `None` in compact density, where the label moves to hover text.
    label: Option<&'a str>,
    /// Rendered as a badge beside the label. Zero renders none.
    count: usize,
    ink: Color32,
    fill: Color32,
    clickable: bool,
}

impl Pill<'_> {
    fn show(self, ui: &mut Ui, label_size: f32, count_size: f32) -> egui::Response {
        let tokens = ui.tokens();
        // Same geometry as `UserBadge`'s pill, so the bar reads as one control
        // group rather than as widgets that happen to be adjacent.
        egui::Frame::group(ui.style())
            .fill(self.fill)
            .inner_margin(tokens.margin_xy(Space::Md, Space::Sm))
            .corner_radius(tokens.corner(Radius::Full))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_item_gap_x(Space::Base);
                    // `ui.horizontal` is the only container that shrinks to
                    // its content, which a pill in a header must do — but it
                    // takes its DIRECTION from the parent, and this bar's
                    // parent is right-to-left. Adding the pieces pre-reversed
                    // cancels that out, so the pill reads the same either way.
                    let icon = |ui: &mut Ui| {
                        ui.label(self.icon.rich_text(16.0, self.ink));
                    };
                    let label = |ui: &mut Ui| {
                        if let Some(text) = self.label {
                            ui.label(RichText::new(text).size(label_size).color(self.ink));
                        }
                    };
                    let badge = |ui: &mut Ui| {
                        if self.count > 0 {
                            count_badge(ui, self.count, count_size);
                        }
                    };
                    if ui.layout().prefer_right_to_left() {
                        badge(ui);
                        label(ui);
                        icon(ui);
                    } else {
                        icon(ui);
                        label(ui);
                        badge(ui);
                    }
                });
            })
            .response
            .interact(if self.clickable {
                Sense::click()
            } else {
                Sense::hover()
            })
    }
}

/// The count, as a filled pill on the accent.
fn count_badge(ui: &mut Ui, count: usize, size: f32) {
    let tokens = ui.tokens();
    let text = count.to_string();
    let galley = ui.painter().layout_no_wrap(
        text.clone(),
        egui::FontId::proportional(size),
        tokens.color.bg_primary,
    );
    // Round, not oval, until the number needs the width — a single digit in a
    // stretched pill reads as a different control from a double digit one.
    let diameter = galley.size().y + ui.tokens().space(Space::Xs) * 2.0;
    let width = diameter.max(galley.size().x + ui.tokens().space(Space::Sm) * 2.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, diameter), Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same((diameter / 2.0) as u8),
        tokens.color.accent,
    );
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        tokens.color.bg_primary,
    );
}

/// A stake address as a name: enough of both ends to recognise, never enough
/// to verify. The popup carries the whole thing, copyable.
///
/// Elision itself is [`crate::id_pill::truncate_middle`] rather than an
/// eighth private copy of it in this crate.
fn elide(stake: &str) -> String {
    if stake.is_empty() {
        // Connected, but the wallet has not reported a stake address yet.
        // Naming the state beats an empty pill that looks broken.
        return "Connected".to_string();
    }
    crate::id_pill::truncate_middle(stake, 8, 6)
}
