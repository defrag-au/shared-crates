//! `ChannelBands` story — the Mekka reward wallet's income by channel, month by
//! month, with the holder payout overlaid.
//!
//! This is the actual exhibit the widget was extracted from. The established
//! off-ramp funds every distribution for eight months and then reads zero,
//! while a channel four weeks old carries the largest payout in the wallet's
//! history. In a monthly total none of that is visible.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{assign_colors, ChannelBands, ChannelSeries};

const PERIODS: [&str; 10] = ["09", "10", "11", "12", "01", "02", "03", "04", "05", "06"];

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Channel Bands").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Where the money came from, period by period. Stacked composition over a discrete \
             time axis, with an optional same-unit reference line.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // Colours assigned once over the full channel set, by name — so hiding a
    // channel could never repaint the others.
    let colors = assign_colors(&["off-ramp", "conduit", "project wallets", "recycled"]);

    let series = vec![
        ChannelSeries::new(
            "off-ramp",
            colors["off-ramp"],
            vec![
                807.0, 2170.0, 6342.0, 9722.0, 11696.0, 12882.0, 11859.0, 14578.0, 11742.0, 0.0,
            ],
        ),
        ChannelSeries::new(
            "conduit",
            colors["conduit"],
            vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 16994.0, 13537.0],
        ),
        ChannelSeries::new(
            "project wallets",
            colors["project wallets"],
            vec![
                3056.0, 7195.0, 1.0, 0.0, 500.0, 0.0, 2961.0, 410.0, 1956.0, 0.0,
            ],
        ),
        ChannelSeries::new(
            "recycled",
            colors["recycled"],
            vec![
                1671.0, 390.0, 0.0, 0.0, 1410.0, 215.0, 0.0, 0.0, 10034.0, 0.0,
            ],
        ),
    ];

    let payouts = [
        5051.0, 4020.0, 6342.0, 7400.0, 9400.0, 10000.0, 12000.0, 12180.0, 12420.0, 20520.0,
    ];

    let fmt = |v: f64| format!("{v:.0} ADA");
    ChannelBands::new(&PERIODS, &series)
        .overlay("paid to holders", &payouts)
        .height(220.0)
        .format_value(&fmt)
        .show(ui);

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(
            "Months are 2025-09 → 2026-06. The line is what reached holders — same unit, same \
             axis as the bars, so it is a reference line and not a second scale.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.label(
        egui::RichText::new(
            "Read the last two bars: the blue off-ramp funds every month for eight months, then \
             reads zero — and the tallest payout in the wallet's history sits above a bar made \
             entirely of a channel that was four weeks old. Hover any month for the breakdown.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
}
