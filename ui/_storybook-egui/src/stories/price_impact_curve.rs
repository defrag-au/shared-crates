//! Storybook demo for the PriceImpactCurve widget.

use egui_widgets::price_impact_curve::{
    self, constant_product_impact_fn, ImpactCurvePool, PriceImpactCurveConfig,
};
use egui_widgets::split_allocation_bar::dex_color;

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

/// Build the price impact function by injecting the real AMM math from cardano-tx.
fn make_impact_fn() -> Box<price_impact_curve::PriceImpactFn> {
    constant_product_impact_fn(cardano_tx::dex::cswap::pool::constant_product_swap)
}

pub fn show(ui: &mut egui::Ui) {
    let impact_fn = make_impact_fn();

    ui.label(
        egui::RichText::new("PriceImpactCurve Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "AMM price impact curves per pool. Shows why split routing helps \u{2014} \
             by splitting, you stay in the cheap region of each pool's curve. \
             Hover for exact impact values at any ADA amount.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(500.0, ui.available_height()), |ui| {
        // Scenario 1: Aliens at 1000 ADA — clear split benefit
        // Reserves sized so 1000 ADA creates ~1-3% impact (visible curves)
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Aliens \u{2014} 1000 ADA (78/22 split)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(
                        "Splash has 3x the depth of CSWAP. The optimizer sends 78% to Splash \
                         where the curve is flatter, keeping both pools in their low-impact zones.",
                    )
                    .color(TEXT_MUTED)
                    .size(10.0),
                );
                ui.add_space(6.0);

                let pools = vec![
                    ImpactCurvePool {
                        label: "Splash".into(),
                        color: dex_color(0),
                        ada_reserves: 90_000_000_000, // 90K ADA
                        token_reserves: 800_000_000,
                        fee_bps: 78,
                        allocation: Some(780_000_000), // 780 ADA
                    },
                    ImpactCurvePool {
                        label: "CSWAP".into(),
                        color: dex_color(1),
                        ada_reserves: 30_000_000_000, // 30K ADA
                        token_reserves: 270_000_000,
                        fee_bps: 85,
                        allocation: Some(220_000_000), // 220 ADA
                    },
                ];
                let config = PriceImpactCurveConfig {
                    total_input: 1_000_000_000, // 1000 ADA
                    chart_height: 220.0,
                    ..Default::default()
                };
                price_impact_curve::show(ui, &pools, &config, &impact_fn);
            });

        ui.add_space(16.0);

        // Scenario 2: Small amount — no split benefit
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Small Swap \u{2014} 50 ADA (no split)")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(
                        "At small amounts, both curves are nearly flat. Price impact is \
                         negligible, so the optimizer routes 100% to the lower-fee pool.",
                    )
                    .color(TEXT_MUTED)
                    .size(10.0),
                );
                ui.add_space(6.0);

                let pools = vec![
                    ImpactCurvePool {
                        label: "Splash".into(),
                        color: dex_color(0),
                        ada_reserves: 90_000_000_000,
                        token_reserves: 800_000_000,
                        fee_bps: 78,
                        allocation: Some(50_000_000), // all 50 ADA
                    },
                    ImpactCurvePool {
                        label: "CSWAP".into(),
                        color: dex_color(1),
                        ada_reserves: 30_000_000_000,
                        token_reserves: 270_000_000,
                        fee_bps: 85,
                        allocation: None, // not allocated
                    },
                ];
                let config = PriceImpactCurveConfig {
                    total_input: 50_000_000, // 50 ADA
                    chart_height: 180.0,
                    ..Default::default()
                };
                price_impact_curve::show(ui, &pools, &config, &impact_fn);
            });

        ui.add_space(16.0);

        // Scenario 3: Large swap, three-way split
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Large Swap \u{2014} 5000 ADA three-way split")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(
                        "With three pools, the optimizer distributes load across all of them. \
                         The steeper CSWAP curve gets the smallest allocation.",
                    )
                    .color(TEXT_MUTED)
                    .size(10.0),
                );
                ui.add_space(6.0);

                let pools = vec![
                    ImpactCurvePool {
                        label: "Splash".into(),
                        color: dex_color(0),
                        ada_reserves: 120_000_000_000, // 120K ADA
                        token_reserves: 1_050_000_000,
                        fee_bps: 78,
                        allocation: Some(2_750_000_000), // 2750 ADA
                    },
                    ImpactCurvePool {
                        label: "Minswap".into(),
                        color: dex_color(1),
                        ada_reserves: 60_000_000_000, // 60K ADA
                        token_reserves: 525_000_000,
                        fee_bps: 30,
                        allocation: Some(1_500_000_000), // 1500 ADA
                    },
                    ImpactCurvePool {
                        label: "CSWAP".into(),
                        color: dex_color(2),
                        ada_reserves: 25_000_000_000, // 25K ADA
                        token_reserves: 220_000_000,
                        fee_bps: 85,
                        allocation: Some(750_000_000), // 750 ADA
                    },
                ];
                let config = PriceImpactCurveConfig {
                    total_input: 5_000_000_000, // 5000 ADA
                    chart_height: 220.0,
                    ..Default::default()
                };
                price_impact_curve::show(ui, &pools, &config, &impact_fn);
            });
    });
}
