//! Storybook demo for the ManagedWalletUtxos widget.
//!
//! Role-aware breakdown of a custodial wallet's UTxOs. The default scenario
//! mirrors a real unified mint+payments wallet that accidentally minted to
//! itself: three ADA-only UTxOs (the spendable pool) plus one asset-bearing
//! UTxO that gets flagged loudly.

use cardano_assets::utxo::{AssetQuantity, UtxoApi};
use cardano_assets::AssetId;
use egui_widgets::ManagedWalletUtxos;

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub struct ManagedWalletUtxosStoryState {
    /// Toggle the "assets are an anomaly" framing (mint wallet vs generic).
    pub assets_unexpected: bool,
    /// Include the asset-bearing UTxO (the minted-to-self anomaly).
    pub show_anomaly: bool,
    /// Render an empty wallet.
    pub empty: bool,
    /// Swap to a fragmented wallet (a scatter of small UTxOs) to show the
    /// block strip + the "consider consolidating" read.
    pub fragmented: bool,
}

impl Default for ManagedWalletUtxosStoryState {
    fn default() -> Self {
        Self {
            assets_unexpected: true,
            show_anomaly: true,
            empty: false,
            fragmented: false,
        }
    }
}

fn pure(tx: &str, idx: u32, lovelace: u64) -> UtxoApi {
    UtxoApi {
        tx_hash: tx.to_string(),
        output_index: idx,
        lovelace,
        assets: vec![],
        tags: vec![],
    }
}

/// Asset-bearing UTxO. `names_hex` are CIP-25 asset names as hex — e.g.
/// `"4b57494331"` decodes to `"KWIC1"` (the widget calls `asset_name()`).
fn with_nfts(tx: &str, idx: u32, lovelace: u64, policy: &str, names_hex: &[&str]) -> UtxoApi {
    let assets = names_hex
        .iter()
        .map(|h| AssetQuantity {
            asset_id: AssetId::new_unchecked(policy.to_string(), (*h).to_string()),
            quantity: 1,
        })
        .collect();
    UtxoApi {
        tx_hash: tx.to_string(),
        output_index: idx,
        lovelace,
        assets,
        tags: vec![],
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut ManagedWalletUtxosStoryState) {
    ui.label(
        egui::RichText::new("ManagedWalletUtxos Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Role-aware UTxO breakdown for a custodial wallet. ADA-only UTxOs are \
             the spendable mint-funding pool; asset-bearing UTxOs are flagged as an \
             anomaly (a mint+payments wallet should never hold NFTs — minted-to-self \
             or stray inventory).",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // Controls
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut state.assets_unexpected,
            "Assets unexpected (mint wallet)",
        );
        ui.add_space(12.0);
        ui.checkbox(&mut state.empty, "Empty wallet");
    });
    ui.add_enabled_ui(!state.empty, |ui| {
        ui.checkbox(
            &mut state.show_anomaly,
            "Include the minted-to-self UTxO (+2 NFTs)",
        );
        ui.checkbox(
            &mut state.fragmented,
            "Fragmented wallet (a scatter of small UTxOs)",
        );
    });
    ui.add_space(12.0);

    // Build the scenario. Mirrors the real KWIC mint wallet from testing.
    let policy = "ce110b50f2b2565e823386574da0659f0c9295aa52b76d37a9eb660e";
    let utxos: Vec<UtxoApi> = if state.empty {
        vec![]
    } else {
        let mut v = if state.fragmented {
            // A healthy bank UTxO + a long tail of small payment/change
            // fragments — trips the consolidation read.
            let mut frags = vec![pure(
                "739df5ec0000000000000000000000000000000000006df854",
                0,
                140_000_000,
            )];
            for i in 0..16u32 {
                frags.push(pure(
                    "abcd12340000000000000000000000000000000000009f0011",
                    i,
                    1_200_000 + (i as u64 % 4) * 700_000,
                ));
            }
            frags
        } else {
            vec![
                pure(
                    "739df5ec0000000000000000000000000000000000006df854",
                    2,
                    117_667_750,
                ),
                pure(
                    "d0d962d20000000000000000000000000000000000000b063a1",
                    3,
                    39_163_625,
                ),
                pure(
                    "d0d962d20000000000000000000000000000000000000b063a1",
                    7,
                    68_125_678,
                ),
            ]
        };
        if state.show_anomaly {
            // The phantom-order mint delivered 2 NFTs back to the wallet.
            v.insert(
                1,
                with_nfts(
                    "d0d962d20000000000000000000000000000000000000b063a1",
                    1,
                    1_206_800,
                    policy,
                    &["4b57494331323334", "4b57494335363738"], // "KWIC1234", "KWIC5678"
                ),
            );
        }
        v
    };

    // Widget
    ui.allocate_ui(egui::vec2(420.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ManagedWalletUtxos::new(&utxos)
                    .assets_unexpected(state.assets_unexpected)
                    .show(ui);
            });
    });

    ui.add_space(12.0);
    if ui.button("Reset").clicked() {
        *state = ManagedWalletUtxosStoryState::default();
    }
}
