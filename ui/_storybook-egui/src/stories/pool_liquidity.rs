//! Storybook demo for the PoolLiquidityIndicator widget.

use egui_widgets::pool_liquidity_indicator::{self, PoolInfo, PoolLiquidityConfig};
use egui_widgets::split_allocation_bar::dex_color;

use crate::{accent, bg, muted};

pub fn show(ui: &mut egui::Ui) {
    // The DEX series ramp comes from the active theme now, so the fixtures have
    // to resolve it rather than name a constant.
    let t = egui_widgets::theme::ThemeExt::tokens(ui);
    ui.label(
        egui::RichText::new("PoolLiquidityIndicator Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Per-pool depth and health context cards. Shows relative depth bars, \
             TVL, spot price, price impact (color-coded), and allocation fraction.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    let config = PoolLiquidityConfig::default();

    ui.allocate_ui(egui::vec2(400.0, ui.available_height()), |ui| {
        // Healthy split — low impact on both
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(
                egui_widgets::theme::BG_HIGHLIGHT,
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Healthy Split \u{2014} Low Impact")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let pools = vec![
                    PoolInfo {
                        dex_label: "Splash".into(),
                        color: dex_color(0, &t),
                        ada_reserves: 2_500_000_000_000,
                        token_reserves: 22_000_000_000,
                        fee_bps: 30,
                        spot_price: 0.000114,
                        price_impact: 0.003,
                        allocation_fraction: 0.78,
                    },
                    PoolInfo {
                        dex_label: "CSWAP".into(),
                        color: dex_color(1, &t),
                        ada_reserves: 800_000_000_000,
                        token_reserves: 7_100_000_000,
                        fee_bps: 30,
                        spot_price: 0.000113,
                        price_impact: 0.005,
                        allocation_fraction: 0.22,
                    },
                ];
                pool_liquidity_indicator::show(ui, &pools, &config);
            });

        ui.add_space(12.0);

        // High impact scenario
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(
                egui_widgets::theme::BG_HIGHLIGHT,
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("High Impact Scenario")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let pools = vec![
                    PoolInfo {
                        dex_label: "Splash".into(),
                        color: dex_color(0, &t),
                        ada_reserves: 500_000_000_000,
                        token_reserves: 4_400_000_000,
                        fee_bps: 30,
                        spot_price: 0.000114,
                        price_impact: 0.018,
                        allocation_fraction: 0.65,
                    },
                    PoolInfo {
                        dex_label: "CSWAP".into(),
                        color: dex_color(1, &t),
                        ada_reserves: 150_000_000_000,
                        token_reserves: 1_300_000_000,
                        fee_bps: 50,
                        spot_price: 0.000115,
                        price_impact: 0.042,
                        allocation_fraction: 0.35,
                    },
                ];
                pool_liquidity_indicator::show(ui, &pools, &config);
            });

        ui.add_space(12.0);

        // Three pools
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(
                egui_widgets::theme::BG_HIGHLIGHT,
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Three-Pool Comparison")
                        .color(egui_widgets::theme::TEXT_SECONDARY)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(6.0);

                let pools = vec![
                    PoolInfo {
                        dex_label: "Splash".into(),
                        color: dex_color(0, &t),
                        ada_reserves: 3_000_000_000_000,
                        token_reserves: 26_000_000_000,
                        fee_bps: 30,
                        spot_price: 0.000115,
                        price_impact: 0.002,
                        allocation_fraction: 0.55,
                    },
                    PoolInfo {
                        dex_label: "Minswap".into(),
                        color: dex_color(1, &t),
                        ada_reserves: 1_200_000_000_000,
                        token_reserves: 10_500_000_000,
                        fee_bps: 30,
                        spot_price: 0.000114,
                        price_impact: 0.008,
                        allocation_fraction: 0.30,
                    },
                    PoolInfo {
                        dex_label: "CSWAP".into(),
                        color: dex_color(2, &t),
                        ada_reserves: 400_000_000_000,
                        token_reserves: 3_500_000_000,
                        fee_bps: 50,
                        spot_price: 0.000114,
                        price_impact: 0.015,
                        allocation_fraction: 0.15,
                    },
                ];
                pool_liquidity_indicator::show(ui, &pools, &config);
            });
    });
}
