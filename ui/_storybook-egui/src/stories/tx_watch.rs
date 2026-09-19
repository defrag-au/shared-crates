//! Storybook demo for the TxWatch widget.

use egui_widgets::tx_watch::{self, TxPhase, TxStage, TxWatchConfig, WatchedTx};

use crate::{accent, bg, muted, secondary};

const TX1: &str = "b07f443a47e12e3951b8e4f9e63e66877f48eea8ac603bc1283139068a8a9b53";
const TX2: &str = "bac802ec4a3670ab9383ab52afb275d2455ccc33b31eed7e91bbd0418101f5fd";

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("TxWatch Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Several transactions on their way to chain. Watches; never drives — \
             tx_flight is the one with the buttons. The active stage BREATHES, so a \
             40-second wait for a block does not read as a hung screen.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    let config = TxWatchConfig::default();

    ui.horizontal_top(|ui| {
        panel(
            ui,
            "Chained route — mid-flight",
            "Transaction 1 landed and the user already holds the LUMP. 2 is \
             waiting for a block; its `confirm` mark is the one pulsing.",
            |ui| tx_watch::show(ui, &mid_flight(), &config),
        );
        panel(
            ui,
            "Signing the bundle",
            "One wallet dialog for both, via CIP-103. Neither has been submitted \
             yet, so both sit on `sign`.",
            |ui| tx_watch::show(ui, &signing(), &config),
        );
        panel(
            ui,
            "Complete",
            "Both confirmed. Nothing pulses and the widget stops asking for \
             repaints.",
            |ui| tx_watch::show(ui, &complete(), &config),
        );
    });

    ui.add_space(12.0);

    ui.horizontal_top(|ui| {
        panel(
            ui,
            "Stalled — the second leg failed",
            "The pool moved between transactions. Transaction 1 still landed, and \
             the note says what the user is holding because of it — which is the \
             whole safety story of a chained plan.",
            |ui| tx_watch::show(ui, &stalled(), &config),
        );
        panel(
            ui,
            "Single transaction",
            "A claim or a single-leg trade. No 'n of m' counter when there is only \
             one.",
            |ui| tx_watch::show(ui, &single(), &config),
        );
    });
}

fn leg_one(phase: TxPhase) -> WatchedTx {
    WatchedTx {
        label: "Swap ADA for LUMP".into(),
        phase,
        landed_note: Some("You now hold 65,862 LUMP".into()),
    }
}

fn leg_two(phase: TxPhase) -> WatchedTx {
    WatchedTx {
        label: "Buy SWOLE with LUMP".into(),
        phase,
        landed_note: Some("You now hold 3,120,727 SWOLE".into()),
    }
}

fn mid_flight() -> Vec<WatchedTx> {
    vec![
        leg_one(TxPhase::Confirmed {
            tx_hash: TX1.into(),
        }),
        leg_two(TxPhase::Confirming {
            tx_hash: TX2.into(),
        }),
    ]
}

fn signing() -> Vec<WatchedTx> {
    vec![leg_one(TxPhase::Signing), leg_two(TxPhase::Signing)]
}

fn complete() -> Vec<WatchedTx> {
    vec![
        leg_one(TxPhase::Confirmed {
            tx_hash: TX1.into(),
        }),
        leg_two(TxPhase::Confirmed {
            tx_hash: TX2.into(),
        }),
    ]
}

fn stalled() -> Vec<WatchedTx> {
    vec![
        leg_one(TxPhase::Confirmed {
            tx_hash: TX1.into(),
        }),
        leg_two(TxPhase::Failed {
            stage: TxStage::Confirm,
            error: "The SWOLE pool moved before this landed — nothing was spent. \
                    Re-quote to try again."
                .into(),
        }),
    ]
}

fn single() -> Vec<WatchedTx> {
    vec![WatchedTx::new(
        "Claim LumpPad pool fees",
        TxPhase::Submitting,
    )]
}

fn panel(ui: &mut egui::Ui, title: &str, caption: &str, body: impl FnOnce(&mut egui::Ui)) {
    // Explicit top-down: `allocate_ui` inherits the parent's layout, and these
    // sit inside a `horizontal_top`.
    ui.allocate_ui_with_layout(
        egui::vec2(330.0, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::Frame::new()
                .fill(bg(ui))
                .corner_radius(6.0)
                .inner_margin(12.0)
                .stroke(egui_widgets::theme::hairline(crate::highlight(ui)))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .color(secondary(ui))
                            .size(11.0)
                            .strong(),
                    );
                    ui.label(egui::RichText::new(caption).color(muted(ui)).size(10.0));
                    ui.add_space(8.0);
                    body(ui);
                });
        },
    );
}
