//! Storybook demo for the TxCart widget.

use egui_widgets::tx_cart::{
    self, TxCartConfig, TxCartItem, TxCartItemStatus, TxCartPhase, TxCartPlannedTx, TxCartPrice,
    TxCartReviewRow, TxCartState,
};

use crate::{accent, muted};

pub struct TxCartStoryState {
    pub cart: TxCartState,
    pub last_action: String,
}

impl Default for TxCartStoryState {
    fn default() -> Self {
        let items = vec![
            TxCartItem {
                id: "1".into(),
                label: "Helmies".into(),
                policy_id: "a5425bd7bc4182325188af2340415827a73f845846c165d9e14c5aed".into(),
                provider: "jpg.store".into(),
                action_label: "Cancel coll. offers".into(),
                quantity: 1,
                price: TxCartPrice::Total(5.0),
                image_url: None,
                status: TxCartItemStatus::Pending,
            },
            TxCartItem {
                id: "2".into(),
                label: "Helmies".into(),
                policy_id: "a5425bd7bc4182325188af2340415827a73f845846c165d9e14c5aed".into(),
                provider: "jpg.store".into(),
                action_label: "Cancel coll. offers".into(),
                quantity: 1,
                price: TxCartPrice::Total(5.0),
                image_url: None,
                status: TxCartItemStatus::Pending,
            },
            TxCartItem {
                id: "3".into(),
                label: "SpaceBudz".into(),
                policy_id: "d5e6bf0500378d4f0da4e8dde6becec7621cd8cbf5cbb9b87013d4cc".into(),
                provider: "jpg.store".into(),
                action_label: "Create coll. offers".into(),
                quantity: 3,
                price: TxCartPrice::Total(150.0),
                image_url: None,
                status: TxCartItemStatus::Pending,
            },
        ];
        Self {
            cart: TxCartState {
                items,
                ..TxCartState::default()
            },
            last_action: String::new(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut TxCartStoryState) {
    ui.label(
        egui::RichText::new("TxCart Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Batched transaction cart with sequential signing. Groups actions by \
             type, shows per-item status during execution.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    // Phase selector
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Phase:").color(muted(ui)).size(10.0));

        if ui
            .selectable_label(state.cart.phase == TxCartPhase::Editing, "Editing")
            .clicked()
        {
            state.cart.phase = TxCartPhase::Editing;
            for item in &mut state.cart.items {
                item.status = TxCartItemStatus::Pending;
            }
            state.cart.planned_txs.clear();
        }
        if ui
            .selectable_label(state.cart.phase == TxCartPhase::Building, "Building")
            .clicked()
        {
            state.cart.phase = TxCartPhase::Building;
            for item in &mut state.cart.items {
                item.status = TxCartItemStatus::Building;
            }
        }
        if ui
            .selectable_label(state.cart.phase == TxCartPhase::Preview, "Preview")
            .clicked()
        {
            state.cart.phase = TxCartPhase::Preview;
            state.cart.planned_txs = vec![
                // With detail, so the row opens. The `Pool` value is a real
                // 66-character UTxO reference: printed in full it set the
                // width of the whole sidebar, which is what `is_reference`
                // and its `IdPill` are for.
                TxCartPlannedTx {
                    unsigned_tx_cbor: "84a4...".into(),
                    fee: 308_842,
                    item_ids: vec!["1".into()],
                    summary: "Route ADA -> LUMP -> SWOLE (1 of 2)".into(),
                    review: vec![
                        row("You pay", "20.000000 ADA"),
                        reference(
                            "Pool",
                            "3e7ef1040f94329137c38f78299a126bd5df3e3602bcbeb1ce0918d468d7c43d#0",
                        ),
                        row("Splash LP", "0.180000 ADA"),
                        row("Splash treasury", "0.010000 ADA"),
                        row("Splash royalty", "0.010000 ADA"),
                        row(
                            "If step 2 never lands",
                            "you keep 144,635 LUMP in your own wallet",
                        ),
                    ],
                },
                TxCartPlannedTx {
                    unsigned_tx_cbor: "84a4...".into(),
                    fee: 290_719,
                    item_ids: vec!["1".into()],
                    summary: "Route ADA -> LUMP -> SWOLE (2 of 2)".into(),
                    review: vec![
                        reference(
                            "Pool",
                            "cc958510039ddf15c08ab866bb4013cc19a94e6661b30db92fd02df15760c6af#0",
                        ),
                        row("LumpPad platform (half burns)", "10,663 LUMP"),
                        row("You receive", "7,202,331 SWOLE"),
                        row("Settlement", "Exactly as quoted, or fails free"),
                    ],
                },
                // No detail: the caret is absent and the row does not click.
                // A venue that supplies no review rows must not look broken.
                TxCartPlannedTx {
                    unsigned_tx_cbor: "84a4...".into(),
                    fee: 250_000,
                    item_ids: vec!["3".into()],
                    summary: "Create 3 jpg.store offer(s)".into(),
                    review: vec![],
                },
            ];
            // First row open, so one screen shows both states: what a
            // transaction looks like expanded, and what the others look like
            // waiting to be.
            state.cart.open_tx = Some(0);
            for item in &mut state.cart.items {
                item.status = TxCartItemStatus::Pending;
            }
        }
        if ui
            .selectable_label(
                matches!(state.cart.phase, TxCartPhase::Executing { .. }),
                "Executing",
            )
            .clicked()
        {
            state.cart.phase = TxCartPhase::Executing {
                total: 2,
                completed: 1,
            };
            state.cart.items[0].status = TxCartItemStatus::Submitted {
                tx_hash: "abc123...".into(),
            };
            state.cart.items[1].status = TxCartItemStatus::Submitted {
                tx_hash: "abc123...".into(),
            };
            state.cart.items[2].status = TxCartItemStatus::Signing;
        }
        if ui
            .selectable_label(state.cart.phase == TxCartPhase::Done, "Done")
            .clicked()
        {
            state.cart.phase = TxCartPhase::Done;
            for item in &mut state.cart.items {
                item.status = TxCartItemStatus::Submitted {
                    tx_hash: "abc123def456...".into(),
                };
            }
        }
        if ui
            .selectable_label(
                matches!(state.cart.phase, TxCartPhase::Error { .. }),
                "Error",
            )
            .clicked()
        {
            state.cart.phase = TxCartPhase::Error {
                message: "Insufficient funds".into(),
            };
            state.cart.items[2].status = TxCartItemStatus::Error {
                message: "TX build failed".into(),
            };
        }
    });

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // Render the cart widget, in a SIDEBAR's width.
    //
    // Not the page's. Every real caller puts this in a panel or a drawer of
    // 320–400pt, and at full page width the one thing worth reviewing — does
    // the detail fit — cannot be seen. `CartRunner` allocates the same way.
    const SIDEBAR: f32 = 360.0;
    let config = TxCartConfig::default();
    let width = SIDEBAR.min(ui.available_width());
    let action = ui
        .allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_width(width);
                tx_cart::show(ui, &mut state.cart, &config)
            },
        )
        .inner;
    if let Some(action) = action {
        state.last_action = format!("{action:?}");
    }

    // Action log
    if !state.last_action.is_empty() {
        ui.add_space(8.0);
        ui.separator();
        ui.label(
            egui::RichText::new(format!("Last action: {}", state.last_action))
                .color(muted(ui))
                .size(9.0)
                .monospace(),
        );
    }
}

fn row(label: &str, value: &str) -> TxCartReviewRow {
    TxCartReviewRow {
        label: label.into(),
        value: value.into(),
        is_reference: false,
    }
}

/// An on-chain identifier — middle-elided with a copy button, not printed.
fn reference(label: &str, value: &str) -> TxCartReviewRow {
    TxCartReviewRow {
        is_reference: true,
        ..row(label, value)
    }
}
