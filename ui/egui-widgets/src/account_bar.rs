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
//! Disconnected, it delegates to [`WalletButton`](crate::wallet_button::WalletButton),
//! which already owns the wallet picker and its per-extension icons. The bar
//! holds that button so a caller stores one thing instead of two.
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
//!     AccountBarAction::Disconnect => { /* clear the session */ }
//!     _ => {}
//! }
//! # }
//! ```

use egui::{Align, Color32, Layout, RichText, Sense, Ui, Vec2};

use crate::icons::PhosphorIcon;
use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};
use crate::user_badge::{UserBadge, UserBadgeAction};
use crate::wallet::{ConnectionState, WalletConnector, WalletProvider};
use crate::wallet_button::{WalletAction, WalletButton};

/// How wide the wallet picker gets inside its popup.
///
/// Enough for the longest extension name and its icon on one line. The picker
/// sizes itself to whatever it is given, so without a bound it takes the page.
const PICKER_WIDTH: f32 = 240.0;

/// What the user did with the bar this frame.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AccountBarAction {
    None,
    /// A wallet was chosen from the picker. The caller connects it.
    Connect(WalletProvider),
    /// Leave — from the picker's own button, or the account popup.
    Disconnect,
    /// A trigger was clicked, carrying the id it was given.
    Trigger(String),
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

/// The header's trailing cluster. Holds the wallet picker's state, so a
/// caller stores this instead of a [`WalletButton`].
#[derive(Default)]
pub struct AccountBar {
    pub config: AccountBarConfig,
    button: WalletButton,
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
            button: WalletButton::default(),
            bar_height: 0.0,
        }
    }

    /// Draw the cluster, right-aligned in whatever width it is given.
    ///
    /// Right-to-left internally so the account lands on the trailing edge and
    /// everything shares one vertical centre — the alignment the callers that
    /// hand-rolled this never got, because a `Button` and a bordered `Frame`
    /// have different intrinsic heights and no amount of `add_space` fixes
    /// that.
    pub fn show(
        &mut self,
        ui: &mut Ui,
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
        let drawn = ui.allocate_ui_with_layout(bar, Layout::right_to_left(Align::Center), |ui| {
            ui.set_item_gap_x(Space::Sm);
            action = self.cluster(ui, connector, triggers);
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
        connector: &WalletConnector,
        triggers: &[Trigger<'_>],
    ) -> AccountBarAction {
        let mut action = AccountBarAction::None;

        // The account goes in FIRST because the layout is right-to-left, which
        // puts it rightmost — where a reader looks for who they are signed in
        // as — and the triggers reversed after it, so they end up in the order
        // the caller wrote them.
        if connector.is_connected() {
            if self.account(ui, connector) == UserBadgeAction::SignOut {
                action = AccountBarAction::Disconnect;
            }
        } else {
            action = self.connect(ui, connector);
        }

        for trigger in triggers.iter().rev() {
            if self.trigger(ui, trigger) {
                action = AccountBarAction::Trigger(trigger.id.to_string());
            }
        }
        action
    }

    /// Not connected yet: a pill, with the wallet picker behind it.
    ///
    /// The picker itself is still [`WalletButton`] — it owns the per-extension
    /// icons, the connecting spinner and the error state, and a second copy
    /// would be a second set of wallet icons to keep current. What changed is
    /// where it is drawn. Inline it calls `set_width(available_width())` and
    /// spans the entire header with a stacked list of wallets, which is how
    /// the routes app ended up with a picker wider than its own page. In a
    /// bounded popup that same width call is exactly right.
    fn connect(&mut self, ui: &mut Ui, connector: &WalletConnector) -> AccountBarAction {
        let tokens = ui.tokens();
        let failed = matches!(connector.connection_state, ConnectionState::Error(_));
        let connecting = connector.is_connecting();
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
        egui::Popup::menu(&pill)
            .id(pill.id.with("account_bar_connect"))
            .gap(tokens.space(Space::Sm))
            .align(egui::RectAlign::BOTTOM_END)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                // Bounded, so the picker's own `available_width` is this and
                // not the page.
                ui.set_min_width(PICKER_WIDTH);
                ui.set_max_width(PICKER_WIDTH);
                match self.button.show(ui, connector) {
                    WalletAction::None => {}
                    WalletAction::Connect(p) => action = AccountBarAction::Connect(p),
                    WalletAction::Disconnect => action = AccountBarAction::Disconnect,
                }
            });
        action
    }

    /// The connected account, as a [`UserBadge`].
    fn account(&self, ui: &mut Ui, connector: &WalletConnector) -> UserBadgeAction {
        // A `$handle` when there is one — it is what a human recognises. The
        // stake address is the fallback, middle-elided, and it is ALSO in the
        // popup as a copyable pill, because a truncated address on its own is
        // a string you cannot do anything with.
        let stake = connector.stake_address.as_deref().unwrap_or_default();
        let name = match connector.handle.as_deref() {
            Some(handle) => handle.to_string(),
            None => elide(stake),
        };

        let mut badge = UserBadge::new(&name)
            .icon(PhosphorIcon::Wallet)
            .subtitle(self.config.subtitle)
            .avatar_url(connector.connected_icon.as_deref())
            .id_salt("account_bar");
        if !stake.is_empty() {
            badge = badge.identifier("stake", stake);
        }
        if let Some(balance) = &connector.balance {
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
