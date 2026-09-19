//! `AppHeader` — the bar across the top of a wallet-connected app: its name,
//! one line on what it is for, anything the app wants beside them, and the
//! account.
//!
//! ## Why it's a widget
//!
//! Every frontend drew this row by hand, and no two agreed. Surveyed across the
//! egui apps: titles at 14, 16, 20 and 22pt, some cyan and some not; one bar a
//! top panel, one scrolling away with the page, one drawn straight into the
//! root `Ui`; and the wallet as `AccountBar` in two apps, a hand-rolled
//! `$handle | N ADA` menu chip in another, a 220px inline picker in a third.
//! [`AccountBar`] had already fixed the trailing corner; the rest of the row
//! was still everyone's own.
//!
//! One widget means a new app gets the header by constructing it, and an
//! existing one gets consistent by deleting its own.
//!
//! ## Shape
//!
//! A top panel, so it stays put while the page scrolls under it, and so a side
//! panel drawn after it sits BELOW it rather than beside it. Inside: the title,
//! the tagline, whatever the caller draws in `between` (tabs, a chain pulse, a
//! count), then the [`AccountBar`] right-aligned.
//!
//! ```no_run
//! # use egui_widgets::app_header::AppHeader;
//! # use egui_widgets::account_bar::AccountBarAction;
//! # use egui_widgets::wallet::WalletConnector;
//! # fn demo(ui: &mut egui::Ui, header: &mut AppHeader, wallet: &WalletConnector) {
//! match header.show(ui, wallet, &[], |_ui| {}) {
//!     AccountBarAction::Connect(provider) => { /* connect it */ }
//!     AccountBarAction::Disconnect => { /* clear the session */ }
//!     _ => {}
//! }
//! # }
//! ```

use egui::{RichText, Ui};

use crate::account_bar::{
    AccountBar, AccountBarAction, AccountBarConfig, AccountIdentity, Trigger,
};
use crate::theme::{Space, SpaceExt, TextSize, ThemeExt};
use crate::wallet::WalletConnector;

/// The app's name, what it is for, and the account cluster.
pub struct AppHeader {
    title: String,
    tagline: String,
    account: AccountBar,
}

impl AppHeader {
    /// `tagline` may be empty; the title then stands alone.
    pub fn new(title: impl Into<String>, tagline: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            tagline: tagline.into(),
            account: AccountBar::new(),
        }
    }

    /// Retitle the header for what is currently on screen.
    ///
    /// Owned rather than `&'static str` because a real app's header is not one
    /// fixed string: a route that shows a named thing — a collection, a wallet
    /// — titles the bar with that name, which is only known at runtime. An app
    /// with a fixed name simply never calls this.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// Set the tagline, or clear it with `""`.
    ///
    /// Callers narrow this per breakpoint: the tagline is the first thing worth
    /// dropping when the bar is short of room, and the header renders no
    /// tagline at all when it is empty.
    pub fn set_tagline(&mut self, tagline: impl Into<String>) {
        self.tagline = tagline.into();
    }

    /// Configure the account cluster (density, text sizes, popup subtitle).
    pub fn account(mut self, config: AccountBarConfig) -> Self {
        self.account = AccountBar::with_config(config);
        self
    }

    /// Draw the header as a top panel. Call it before any other panel, so the
    /// others lay out beneath it.
    ///
    /// `between` draws after the title and before the account — tabs, a chain
    /// pulse, a status count. It is laid out left to right on the title's line.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        wallet: &WalletConnector,
        triggers: &[Trigger<'_>],
        between: impl FnOnce(&mut Ui),
    ) -> AccountBarAction {
        self.show_as(ui, AccountIdentity::Wallet, wallet, triggers, between)
    }

    /// [`Self::show`], saying explicitly what identifies the user.
    ///
    /// An app that signs in with something other than a wallet — a Discord
    /// session, an auth worker — passes [`AccountIdentity::Session`] so the
    /// account popup describes what actually happened.
    pub fn show_as(
        &mut self,
        ui: &mut Ui,
        identity: AccountIdentity<'_>,
        wallet: &WalletConnector,
        triggers: &[Trigger<'_>],
        between: impl FnOnce(&mut Ui),
    ) -> AccountBarAction {
        let mut action = AccountBarAction::None;
        egui::Panel::top("app_header").show(ui, |ui| {
            ui.gap(Space::Xs);
            ui.horizontal(|ui| {
                // The account bar is the tallest thing in the row and comes
                // last; claiming its height first centres the title on it.
                ui.set_min_height(self.account.height());
                ui.label(
                    RichText::new(&self.title)
                        .color(ui.tokens().color.accent_cyan)
                        .strong()
                        .size(ui.text_size(TextSize::Xl2)),
                );
                if !self.tagline.is_empty() {
                    ui.label(
                        RichText::new(&self.tagline)
                            .color(ui.tokens().color.text_muted)
                            .size(ui.text_size(TextSize::Base)),
                    );
                }
                between(ui);
                action = self.account.show_as(ui, identity, wallet, triggers);
            });
            ui.gap(Space::Xs);
        });
        action
    }
}
