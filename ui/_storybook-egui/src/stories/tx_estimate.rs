//! Storybook demo for the TxEstimate widget.

use egui_widgets::tx_estimate::{self, TxEstimateConfig, TxEstimateData};
use egui_widgets::UtxoCost;

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub struct TxEstimateStoryState {
    pub ada_sending: u64,
    pub ada_receiving: u64,
    pub nft_count: u32,
    pub inbound_nft_count: u32,
    pub bf_holder: bool,
    pub platform_fee_ada: u64,
}

impl Default for TxEstimateStoryState {
    fn default() -> Self {
        Self {
            ada_sending: 0,
            ada_receiving: 5,
            nft_count: 1,
            inbound_nft_count: 0,
            bf_holder: false,
            platform_fee_ada: 1,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut TxEstimateStoryState) {
    ui.label(
        egui::RichText::new("TxEstimate Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Per-wallet transaction estimate shown during negotiation. Displays platform fee, \
             network fee, min UTxO, and net ADA impact for the local user only.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // Preset scenarios
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Presets:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        if ui.selectable_label(false, "Sell NFT for 5 ADA").clicked() {
            state.ada_sending = 0;
            state.ada_receiving = 5;
            state.nft_count = 1;
            state.inbound_nft_count = 0;
            state.bf_holder = false;
        }
        if ui.selectable_label(false, "Buy NFT for 10 ADA").clicked() {
            state.ada_sending = 10;
            state.ada_receiving = 0;
            state.nft_count = 0;
            state.inbound_nft_count = 1;
            state.bf_holder = false;
        }
        if ui.selectable_label(false, "NFT-for-NFT swap").clicked() {
            state.ada_sending = 0;
            state.ada_receiving = 0;
            state.nft_count = 1;
            state.inbound_nft_count = 1;
            state.bf_holder = false;
        }
        if ui.selectable_label(false, "BF holder sells").clicked() {
            state.ada_sending = 0;
            state.ada_receiving = 5;
            state.nft_count = 1;
            state.inbound_nft_count = 0;
            state.bf_holder = true;
        }
    });

    ui.add_space(8.0);

    // Controls
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("ADA sending:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.ada_sending).range(0..=100));
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("ADA receiving:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.ada_receiving).range(0..=100));
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("NFTs offered:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.nft_count).range(0..=10));
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("NFTs receiving:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.inbound_nft_count).range(0..=10));
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("Platform fee (ADA):")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.platform_fee_ada).range(0..=10));
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.bf_holder, "Black Flag holder");
    });

    ui.add_space(12.0);

    // Compute estimated costs
    let platform_fee_lovelace = state.platform_fee_ada * 1_000_000;
    let network_fee: u64 = 180_000; // ~0.18 ADA typical
                                    // Rough min UTxO: ~1.3 ADA per NFT output (realistic for 1-3 assets under one policy)
    let min_utxo = if state.nft_count > 0 {
        1_300_000 + (state.nft_count.saturating_sub(1) as u64) * 100_000
    } else {
        0
    };

    // Inbound min UTxO: ADA arriving locked alongside peer's assets
    let inbound_min_utxo = if state.inbound_nft_count > 0 {
        1_300_000 + (state.inbound_nft_count.saturating_sub(1) as u64) * 100_000
    } else {
        0
    };

    let effective_platform = if state.bf_holder {
        0
    } else {
        platform_fee_lovelace
    };

    let mut utxo_costs = Vec::new();
    if min_utxo > 0 {
        utxo_costs.push(UtxoCost {
            lovelace: min_utxo,
            inbound: false,
        });
    }
    if inbound_min_utxo > 0 {
        utxo_costs.push(UtxoCost {
            lovelace: inbound_min_utxo,
            inbound: true,
        });
    }

    let net_ada = (state.ada_receiving * 1_000_000) as i64
        - (state.ada_sending * 1_000_000) as i64
        - min_utxo as i64
        - network_fee as i64
        - effective_platform as i64
        + inbound_min_utxo as i64;

    let data = TxEstimateData {
        platform_fee: platform_fee_lovelace,
        network_fee,
        utxo_costs,
        net_ada,
        waived: state.bf_holder,
        waiver_reason: if state.bf_holder {
            Some("Black Flag holder".into())
        } else {
            None
        },
    };

    let config = TxEstimateConfig::default();

    // Widget
    ui.allocate_ui(egui::vec2(280.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                tx_estimate::show(ui, &data, &config);
            });
    });

    ui.add_space(12.0);

    // Debug info
    ui.collapsing("Debug values", |ui| {
        ui.label(
            egui::RichText::new(format!(
                "platform_fee: {effective_platform} lovelace\nnetwork_fee: {network_fee} lovelace\nmin_utxo: {min_utxo} lovelace\nnet_ada: {net_ada} lovelace"
            ))
            .color(TEXT_MUTED)
            .size(10.0)
            .family(egui::FontFamily::Monospace),
        );
    });

    ui.add_space(8.0);
    if ui.button("Reset").clicked() {
        *state = TxEstimateStoryState::default();
    }
}
