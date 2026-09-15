//! Storybook demo for the PoolInspector widget.

use egui_widgets::pool_inspector::{
    self, LumpPadPoolView, PoolInspectorConfig, PoolView, RoyaltySide, SplashPoolView,
};

use crate::{accent, bg, muted, secondary};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("PoolInspector Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A pool's real state, including the value that is NOT on the curve. \
             A development surface — pool_liquidity_indicator is the trading one.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    let config = PoolInspectorConfig::default();

    ui.horizontal_top(|ui| {
        panel(
            ui,
            "LumpPad — SWOLE",
            "Live mainnet state. The bar is where the pool's LUMP actually sits: \
             the two fee buckets are claimable by anyone and are not on the curve.",
            |ui| {
                pool_inspector::show(ui, &PoolView::LumpPad(Box::new(swole())), &config);
            },
        );
        panel(
            ui,
            "Splash — LUMP/ADA",
            "Accrued treasury and royalty live INSIDE the pool UTxO. The curve \
             trades against the balance MINUS those, so the gap is the finding.",
            |ui| {
                pool_inspector::show(ui, &PoolView::SplashRoyalty(Box::new(lump_ada())), &config);
            },
        );
        panel(
            ui,
            "LumpPad — broken invariant",
            "What a mis-decoded datum looks like. `LUMP in UTxO == R + A + B` is \
             the one assertion that catches it, so the widget leads with it.",
            |ui| {
                let mut broken = swole();
                broken.quote_in_utxo += 4_242;
                broken.schedule_matches_registry = false;
                pool_inspector::show(ui, &PoolView::LumpPad(Box::new(broken)), &config);
            },
        );
    });
}

/// The SWOLE pool at `0aca3489…#0`, as of 2026-09-14.
fn swole() -> LumpPadPoolView {
    LumpPadPoolView {
        utxo_ref: "0aca3489a43d669b1aef62717933f141b38a940b9aa4923e180512c7a7b3ae6f#0".into(),
        ticker: "SWOLE".into(),
        reserve: 3_243_770,
        tokens: 756_359_192,
        platform_bucket: 167_862,
        creator_bucket: 75_732,
        virtual_reserve: 10_000_000,
        quote_in_utxo: 3_487_364,
        quote_ticker: "LUMP".into(),
        // (V + R) / T
        spot_price_num: 13_243_770,
        spot_price_den: 756_359_192,
        in_pool_bps: 7_563,
        schedule_matches_registry: true,
    }
}

/// The Splash LUMP/ADA royalty pool at `d92955b6…#1`, as of 2026-09-14.
fn lump_ada() -> SplashPoolView {
    SplashPoolView {
        utxo_ref: "d92955b626329b41222677f6d78e8feed16b6705c6885de00030b340b406efe7#1".into(),
        x: RoyaltySide {
            ticker: "ADA".into(),
            // Lovelace on chain; six decimals to read as ADA.
            decimals: 6,
            balance: 29_549_431_005,
            treasury: 148_513_382,
            royalty: 39_027_415,
        },
        y: RoyaltySide {
            ticker: "LUMP".into(),
            // LUMP is registered with 0 decimals — its raw count IS the
            // figure, and scaling it would be the same bug in reverse.
            decimals: 0,
            balance: 196_800_921,
            treasury: 1_094_651,
            royalty: 301_832,
        },
        fee_num: 99_100,
        treasury_fee: 50,
        royalty_fee: 50,
        fee_den: 100_000,
    }
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
