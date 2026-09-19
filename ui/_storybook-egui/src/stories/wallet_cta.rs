//! Storybook demo for the WalletCta widget.

use egui_widgets::wallet::{
    ConnectionState, Network, WalletBalance, WalletConnector, WalletProvider,
};
use egui_widgets::wallet_cta::{ActionState, ConnectPrompt, WalletCta};

use crate::{accent, bg, muted, secondary};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("WalletCta Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "An action that needs a wallet. Connected, it is the page's primary \
             button; not connected, its place is taken by a prompt that opens the \
             wallet picker in a modal — the instruction IS the control, instead of \
             a disabled button beside some muted text.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    let disconnected = WalletConnector::default();

    panel(
        ui,
        "Not connected — link (default)",
        "Click it: the picker opens in a modal, right where the reader was about \
         to act. In a storybook there are no wallet extensions, so the picker \
         honestly reports none.",
        |ui| {
            WalletCta::new("Add to route")
                .connect_label("Connect a wallet to trade")
                .modal_intro(
                    "Routes are built here and signed in your own wallet. Nothing \
                     moves until you approve it there.",
                )
                .id_salt("link")
                .show(ui, &disconnected);
        },
    );
    panel(
        ui,
        "Not connected — button",
        "For a surface where connecting is the main thing to do next.",
        |ui| {
            WalletCta::new("Add to route")
                .connect_label("Connect a wallet to trade")
                .prompt(ConnectPrompt::Button)
                .id_salt("button")
                .show(ui, &disconnected);
        },
    );
    panel(
        ui,
        "Connecting",
        "Started from somewhere else — the header, say. The action's place says \
         so rather than offering a second connect.",
        |ui| {
            WalletCta::new("Add to route")
                .id_salt("connecting")
                .show(ui, &connecting());
        },
    );
    panel(
        ui,
        "Connected, ready",
        "The primary button, in the same treatment as the cart's Prepare.",
        |ui| {
            WalletCta::new("Add to route")
                .id_salt("ready")
                .show(ui, &connected());
        },
    );
    panel(
        ui,
        "Connected, blocked",
        "Disabled with a reason on hover — a blocked action says why.",
        |ui| {
            WalletCta::new("Add to route")
                .state(ActionState::Blocked {
                    hint: "Nothing to add — this amount has no route",
                })
                .id_salt("blocked")
                .show(ui, &connected());
        },
    );
}

fn connecting() -> WalletConnector {
    WalletConnector {
        connection_state: ConnectionState::Connecting,
        ..WalletConnector::default()
    }
}

/// A connector as it looks after a real connect, without touching a browser.
fn connected() -> WalletConnector {
    WalletConnector {
        connection_state: ConnectionState::Connected {
            provider: WalletProvider::Eternl,
            address: format!("01{}", "ab".repeat(28)),
            network: Network::Mainnet,
        },
        address_hex: Some(format!("01{}", "ab".repeat(28))),
        stake_address: Some(
            "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9".to_string(),
        ),
        balance: Some(WalletBalance {
            lovelace: 4_080_590_650,
            assets: Default::default(),
        }),
        ..WalletConnector::default()
    }
}

/// One variant per line: a title, a note, and the widget in a frame.
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
        .show(ui, body);
    ui.add_space(14.0);
}
