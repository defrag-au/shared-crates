//! Storybook demo for the TradeFlow widget.

use egui_widgets::trade_flow::{self, TradeFlowConfig, TradeFlowData};
use egui_widgets::FlowAsset;

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub struct TradeFlowStoryState {
    pub give_nfts: u32,
    pub give_ada: u64,
    pub get_nfts: u32,
    pub get_ada: u64,
    pub peer_passthrough_ada: u64,
    pub peer_label: String,
}

impl Default for TradeFlowStoryState {
    fn default() -> Self {
        // Default = the buyer's view of a 1600-ADA NFT purchase from a seller whose
        // NFTs sit in fat UTxOs (the case that confuses a hardware wallet).
        Self {
            give_nfts: 0,
            give_ada: 1600,
            get_nfts: 2,
            get_ada: 0,
            peer_passthrough_ada: 2780,
            peer_label: "$djo".into(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut TradeFlowStoryState) {
    ui.label(
        egui::RichText::new("TradeFlow Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The local user's view of a swap in give / get / net terms. Flags the UTxO \
             rebalancing (a counterparty's own ADA, spent with the assets they send and \
             returned as change) so the inflated \"send\" a hardware wallet shows reads as an \
             explained mechanic, not a surprise.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // Preset scenarios
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Presets:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        if ui
            .selectable_label(false, "Buyer: pay 1600 for 2 NFTs")
            .clicked()
        {
            state.give_nfts = 0;
            state.give_ada = 1600;
            state.get_nfts = 2;
            state.get_ada = 0;
            state.peer_passthrough_ada = 2780;
            state.peer_label = "$djo".into();
        }
        if ui
            .selectable_label(false, "Seller: sell 2 NFTs for 1600")
            .clicked()
        {
            state.give_nfts = 2;
            state.give_ada = 0;
            state.get_nfts = 0;
            state.get_ada = 1600;
            state.peer_passthrough_ada = 3;
            state.peer_label = "$boef".into();
        }
        if ui.selectable_label(false, "NFT-for-NFT swap").clicked() {
            state.give_nfts = 1;
            state.give_ada = 0;
            state.get_nfts = 1;
            state.get_ada = 0;
            state.peer_passthrough_ada = 3;
            state.peer_label = "$partner".into();
        }
        if ui
            .selectable_label(false, "Buy NFT + 5 ADA sweetener")
            .clicked()
        {
            state.give_nfts = 0;
            state.give_ada = 5;
            state.get_nfts = 1;
            state.get_ada = 0;
            state.peer_passthrough_ada = 20;
            state.peer_label = "$seller".into();
        }
    });

    ui.add_space(8.0);

    // Controls
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("NFTs you give:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.give_nfts).range(0..=8));
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("ADA you give:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.give_ada).range(0..=100_000));
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("NFTs you get:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.get_nfts).range(0..=8));
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("ADA you get:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.get_ada).range(0..=100_000));
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Partner's UTxO rebalancing (ADA):")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.peer_passthrough_ada).range(0..=100_000));
    });

    ui.add_space(12.0);

    // Build the view-model from the controls.
    let you_give: Vec<FlowAsset> = (0..state.give_nfts)
        .map(|i| FlowAsset::new(format!("SpaceBud #{}", 7800 + i)))
        .collect();
    let you_get: Vec<FlowAsset> = (0..state.get_nfts)
        .map(|i| FlowAsset::new(format!("SpaceBud #{}", 2900 + i)))
        .collect();

    // Min-UTxO you lock for the assets you receive (~1.29 ADA for the first,
    // a little more per extra asset under one output).
    let min_utxo_locked = if state.get_nfts > 0 {
        1_290_000 + (state.get_nfts.saturating_sub(1) as u64) * 100_000
    } else {
        0
    };
    let network_fee: u64 = 90_000; // your half of a ~0.18 ADA fee

    let net_ada = (state.get_ada * 1_000_000) as i64
        - (state.give_ada * 1_000_000) as i64
        - min_utxo_locked as i64
        - network_fee as i64;

    let data = TradeFlowData {
        you_give,
        you_give_ada: state.give_ada * 1_000_000,
        you_get,
        you_get_ada: state.get_ada * 1_000_000,
        net_ada,
        network_fee,
        min_utxo_locked,
        peer_passthrough_ada: state.peer_passthrough_ada * 1_000_000,
        peer_label: state.peer_label.clone(),
    };

    let config = TradeFlowConfig::default();

    // Widget
    ui.allocate_ui(egui::vec2(360.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(14.0)
            .stroke(egui::Stroke::new(
                1.0_f32,
                egui_widgets::theme::BG_HIGHLIGHT,
            ))
            .show(ui, |ui| {
                trade_flow::show(ui, &data, &config);
            });
    });

    ui.add_space(12.0);

    ui.collapsing("Debug values", |ui| {
        ui.label(
            egui::RichText::new(format!(
                "net_ada: {net_ada} lovelace\nmin_utxo_locked: {min_utxo_locked} lovelace\n\
                 network_fee: {network_fee} lovelace\npeer_passthrough: {} lovelace",
                state.peer_passthrough_ada * 1_000_000
            ))
            .color(TEXT_MUTED)
            .size(10.0)
            .family(egui::FontFamily::Monospace),
        );
    });

    ui.add_space(8.0);
    if ui.button("Reset").clicked() {
        *state = TradeFlowStoryState::default();
    }
}
