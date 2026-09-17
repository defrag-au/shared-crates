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

use crate::account_bar::{AccountBar, AccountBarAction, AccountBarConfig, Trigger};
use crate::theme::{Space, SpaceExt, TextSize, ThemeExt};
use crate::wallet::WalletConnector;

/// The app's name, what it is for, and the account cluster.
pub struct AppHeader {
    title: &'static str,
    tagline: &'static str,
    account: AccountBar,
}

impl AppHeader {
    /// `tagline` may be empty; the title then stands alone.
    pub fn new(title: &'static str, tagline: &'static str) -> Self {
        Self {
            title,
            tagline,
            account: AccountBar::new(),
        }
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
        let mut action = AccountBarAction::None;
        egui::Panel::top("app_header").show(ui, |ui| {
            ui.gap(Space::Xs);
            ui.horizontal(|ui| {
                // The account bar is the tallest thing in the row and comes
                // last; claiming its height first centres the title on it.
                ui.set_min_height(self.account.height());
                ui.label(
                    RichText::new(self.title)
                        .color(ui.tokens().color.accent_cyan)
                        .strong()
                        .size(ui.text_size(TextSize::Xl2)),
                );
                if !self.tagline.is_empty() {
                    ui.label(
                        RichText::new(self.tagline)
                            .color(ui.tokens().color.text_muted)
                            .size(ui.text_size(TextSize::Base)),
                    );
                }
                between(ui);
                action = self.account.show(ui, wallet, triggers);
            });
            ui.gap(Space::Xs);
        });
        action
    }
}
