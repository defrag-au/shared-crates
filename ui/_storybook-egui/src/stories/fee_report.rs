//! Storybook demo for the FeeReport widget.

use egui_widgets::fee_report::{self, FeeReportConfig, FeeReportData, SideFeeData};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub struct FeeReportStoryState {
    pub you_bf_holder: bool,
    pub them_bf_holder: bool,
    pub fee_ada: u64,
}

impl Default for FeeReportStoryState {
    fn default() -> Self {
        Self {
            you_bf_holder: false,
            them_bf_holder: false,
            fee_ada: 1,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut FeeReportStoryState) {
    ui.label(
        egui::RichText::new("FeeReport Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Per-side fee breakdown for trades. Each side pays 1 ADA unless \
             they hold a Black Flag NFT, in which case their fee is waived.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // Controls
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.you_bf_holder, "You are BF holder");
        ui.add_space(16.0);
        ui.checkbox(&mut state.them_bf_holder, "Them is BF holder");
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Fee per side (ADA):")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        ui.add(egui::DragValue::new(&mut state.fee_ada).range(0..=100));
    });

    ui.add_space(12.0);

    // Build data
    let fee_lovelace = state.fee_ada * 1_000_000;
    let you_fee = if state.you_bf_holder { 0 } else { fee_lovelace };
    let them_fee = if state.them_bf_holder {
        0
    } else {
        fee_lovelace
    };

    let data = FeeReportData {
        sides: vec![
            SideFeeData {
                label: "You".into(),
                fee_lovelace: you_fee,
                waived: state.you_bf_holder,
                waiver_reason: if state.you_bf_holder {
                    Some("Black Flag holder".into())
                } else {
                    None
                },
                network_fee_share: Some(90_000),
                min_utxo_cost: Some(1_300_000),
                net_ada: Some(3_610_000),
            },
            SideFeeData {
                label: "$boef".into(),
                fee_lovelace: them_fee,
                waived: state.them_bf_holder,
                waiver_reason: if state.them_bf_holder {
                    Some("Black Flag holder".into())
                } else {
                    None
                },
                network_fee_share: Some(90_000),
                min_utxo_cost: Some(0),
                net_ada: Some(-6_090_000),
            },
        ],
        total_lovelace: you_fee + them_fee,
        network_fee: Some(180_000),
    };

    // Widget
    ui.allocate_ui(egui::vec2(320.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                fee_report::show(ui, &data, &FeeReportConfig::default());
            });
    });

    ui.add_space(12.0);
    if ui.button("Reset").clicked() {
        *state = FeeReportStoryState::default();
    }
}
