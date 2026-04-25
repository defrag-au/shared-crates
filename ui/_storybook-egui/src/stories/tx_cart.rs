//! Storybook demo for the TxCart widget.

use egui_widgets::tx_cart::{
    self, TxCartConfig, TxCartItem, TxCartItemStatus, TxCartPhase, TxCartPlannedTx, TxCartState,
};

use crate::{ACCENT, TEXT_MUTED};

pub struct TxCartStoryState {
    pub cart: TxCartState,
    pub last_action: String,
}

impl Default for TxCartStoryState {
    fn default() -> Self {
        let mut cart = TxCartState::default();
        cart.items = vec![
            TxCartItem {
                id: "1".into(),
                label: "Helmies".into(),
                policy_id: "a5425bd7bc4182325188af2340415827a73f845846c165d9e14c5aed".into(),
                provider: "jpg.store".into(),
                action_label: "Cancel coll. offers".into(),
                quantity: 1,
                ada_per_item: 5.0,
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
                ada_per_item: 5.0,
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
                ada_per_item: 50.0,
                image_url: None,
                status: TxCartItemStatus::Pending,
            },
        ];
        Self {
            cart,
            last_action: String::new(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut TxCartStoryState) {
    ui.label(
        egui::RichText::new("TxCart Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Batched transaction cart with sequential signing. Groups actions by \
             type, shows per-item status during execution.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // Phase selector
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Phase:").color(TEXT_MUTED).size(10.0));

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
                TxCartPlannedTx {
                    unsigned_tx_cbor: "84a4...".into(),
                    fee: 387_348,
                    item_ids: vec!["1".into(), "2".into()],
                    summary: "Cancel 2 jpg.store offer(s)".into(),
                },
                TxCartPlannedTx {
                    unsigned_tx_cbor: "84a4...".into(),
                    fee: 250_000,
                    item_ids: vec!["3".into()],
                    summary: "Create 3 jpg.store offer(s)".into(),
                },
            ];
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

    // Render the cart widget
    let config = TxCartConfig::default();
    if let Some(action) = tx_cart::show(ui, &mut state.cart, &config) {
        state.last_action = format!("{action:?}");
    }

    // Action log
    if !state.last_action.is_empty() {
        ui.add_space(8.0);
        ui.separator();
        ui.label(
            egui::RichText::new(format!("Last action: {}", state.last_action))
                .color(TEXT_MUTED)
                .size(9.0)
                .monospace(),
        );
    }
}
