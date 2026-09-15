//! Storybook demo for the AccountBar widget.

use egui_widgets::account_bar::{AccountBar, AccountBarConfig, BarDensity, Trigger};
use egui_widgets::icons::PhosphorIcon;
use egui_widgets::wallet::{
    ConnectionState, Network, WalletBalance, WalletConnector, WalletProvider,
};

use crate::{accent, bg, muted, secondary};

const STAKE: &str = "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9";

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("AccountBar Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The trailing cluster of an app header: what you can do from here, and \
             who you are. One row, one baseline, one visual language — the thing \
             every wallet surface used to rebuild badly on its own.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    // `show` is `&mut self` (it owns the wallet picker's state), so each panel
    // needs its own. They are cheap and hold nothing but display config.
    let mut bars = [
        AccountBar::new(),
        AccountBar::new(),
        AccountBar::new(),
        AccountBar::new(),
        AccountBar::with_config(AccountBarConfig {
            density: BarDensity::Compact,
            ..Default::default()
        }),
    ];
    let [with_handle, no_handle, empty, disconnected, compact] = &mut bars;

    // One variant per LINE, each the full width of the page.
    //
    // Side by side in narrow columns this widget cannot be judged at all: it
    // right-aligns into whatever width it is given, so a 380pt panel shows it
    // hugging an edge that is not the edge it will ever hug. Worse, at that
    // width the pills collide, which reads as a bug in the widget rather than
    // in the frame around it.
    panel(
        ui,
        "Connected, with a handle",
        "The name a human recognises, up front. The stake address is behind the \
         click as a copyable pill — a middle-truncated address is not safe to \
         eyeball, so it never appears as the label.",
        |ui| {
            with_handle.show(ui, &connected(Some("$boef")), &[cart(2)]);
        },
    );
    panel(
        ui,
        "Connected, no handle",
        "Most wallets have none. The elided stake address takes the name slot, \
         and the popup still carries the whole thing.",
        |ui| {
            no_handle.show(ui, &connected(None), &[cart(1)]);
        },
    );
    panel(
        ui,
        "Nothing staged",
        "A zero count renders no badge and does NOT disable the trigger — only \
         the caller knows whether an empty thing is worth opening. Here it is \
         disabled, with the reason on hover.",
        |ui| {
            empty.show(
                ui,
                &connected(Some("$boef")),
                &[Trigger::new("cart", "Route", PhosphorIcon::Package)
                    .enabled(false)
                    .disabled_hint("Nothing staged yet — quote a trade and add it")],
            );
        },
    );
    panel(
        ui,
        "Not connected",
        "The state every app starts in, and the one worth checking: the picker \
         sizes itself to whatever it is given, so drawn inline it spans the \
         whole header. It goes in a bounded popup behind a pill the same shape \
         as the rest of the bar.",
        |ui| {
            disconnected.show(ui, &WalletConnector::default(), &[cart(0)]);
        },
    );
    panel(
        ui,
        "Compact density, two triggers",
        "Trigger labels move to hover text; the account keeps its name, because \
         the name IS the identity. For a header that also carries navigation.",
        |ui| {
            compact.show(
                ui,
                &connected(Some("$boef")),
                &[
                    cart(12),
                    Trigger::new("history", "History", PhosphorIcon::Clock),
                ],
            );
        },
    );

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(
            "The picker behind Connect enumerates the extensions actually \
             installed in the browser, so in a storybook it reports none — \
             which is itself the state worth seeing. What this panel proves is \
             that it is BOUNDED: drawn inline it sizes itself to the page and \
             spans the whole header.",
        )
        .color(muted(ui))
        .size(11.0),
    );
}

fn cart(count: usize) -> Trigger<'static> {
    Trigger::new("cart", "Route", PhosphorIcon::Package).count(count)
}

/// A connector as it looks after a real connect, without touching a browser.
fn connected(handle: Option<&str>) -> WalletConnector {
    let mut connector = WalletConnector {
        available_wallets: Vec::new(),
        connection_state: ConnectionState::Connected {
            provider: WalletProvider::Eternl,
            address: format!("01{}", "ab".repeat(28)),
            network: Network::Mainnet,
        },
        address_hex: Some(format!("01{}", "ab".repeat(28))),
        stake_address: Some(STAKE.to_string()),
        handle: handle.map(str::to_string),
        balance: Some(WalletBalance {
            lovelace: 4_080_590_650,
            assets: Default::default(),
        }),
        api: None,
        connected_icon: None,
    };
    // Belt and braces: the struct is built field-by-field above so a new field
    // breaks this loudly rather than silently demoing a default.
    connector.handle = handle.map(str::to_string);
    connector
}

/// One variant, framed as the header it actually is: a page title on the left
/// and the bar hugging the right edge. Without something on the left there is
/// nothing for the right-alignment to be right OF, and the one behaviour worth
/// reviewing is invisible.
fn panel(ui: &mut egui::Ui, title: &str, note: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.label(
        egui::RichText::new(title)
            .color(secondary(ui))
            .size(11.0)
            .strong(),
    );
    ui.label(egui::RichText::new(note).color(muted(ui)).size(10.0));
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(bg(ui))
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui_widgets::theme::hairline(crate::highlight(ui)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Demo App")
                        .color(accent(ui))
                        .size(16.0)
                        .strong(),
                );
                body(ui);
            });
        });
    ui.add_space(14.0);
}
