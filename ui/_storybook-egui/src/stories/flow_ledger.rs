//! `FlowLedger` story — real movements from the Mekka reward wallet, the case
//! the widget was extracted from.
//!
//! The rows are the wallet's actual May–June 2026 activity in lovelace. They
//! demonstrate the three things the widget is for: a funding channel starting
//! (the withdrawal colour appearing partway down), a round trip rendered as
//! not-income, and a reconciliation that can be checked against the wallet's
//! real balance.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{FlowLedger, FlowRow, PartyBasis};

const CH_OFFRAMP: egui::Color32 = egui::Color32::from_rgb(0x5b, 0x8f, 0xd6);
const CH_WITHDRAWAL: egui::Color32 = egui::Color32::from_rgb(0xd6, 0x9b, 0x5b);
const CH_PROJECT: egui::Color32 = egui::Color32::from_rgb(0x7a, 0xa8, 0x6b);

fn ada(v: i128) -> String {
    format!("{:.2}", v as f64 / 1e6)
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Flow Ledger").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "A wallet's movements in time order — net amounts only, with a running balance and \
             a channel colour per row. The rows below are real: the Mekka reward wallet, \
             May–June 2026.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    let rows = vec![
        FlowRow::new(
            1777622184,
            11_741_250_000,
            "Byron off-ramp",
            PartyBasis::Observed,
        )
        .channel("established off-ramp", CH_OFFRAMP)
        .stakeless(true)
        .tx_id("c91970c1b46fb6e5"),
        FlowRow::new(1777622184, 2_612_740_000, "conduit", PartyBasis::Observed)
            .channel("custodial withdrawal", CH_WITHDRAWAL)
            .counterparty_key("stake1uydxeqvw9j66x4jka0gp7zqsdrt7jaue4u27t5j92w5yttc8s96yj")
            .tx_id("20e8c1aaa363d0ff"),
        FlowRow::new(
            1777708584,
            -4_025_170_000,
            "parking wallet",
            PartyBasis::Observed,
        )
        .tx_id("b21c7d7918c5436d"),
        FlowRow::new(
            1777881384,
            -12_420_170_000,
            "Pillar (provider)",
            PartyBasis::Observed,
        )
        .channel("holder payout", CH_PROJECT)
        .tx_id("4246c1ee775044b4"),
        FlowRow::new(1779073908, 14_381_620_000, "conduit", PartyBasis::Observed)
            .channel("custodial withdrawal", CH_WITHDRAWAL)
            .tx_id("d1b1c50092b4c272"),
        FlowRow::new(
            1779073908,
            4_024_830_000,
            "parking wallet",
            PartyBasis::Observed,
        )
        .recycled(true)
        .tx_id("3df252ae7e7e0c11"),
        FlowRow::new(1780315284, 13_537_040_000, "conduit", PartyBasis::Observed)
            .channel("custodial withdrawal", CH_WITHDRAWAL)
            .tx_id("13863a1933e18f62"),
        FlowRow::new(
            1780432974,
            -20_520_170_000,
            "Pillar (provider)",
            PartyBasis::Observed,
        )
        .channel("holder payout", CH_PROJECT)
        .tx_id("bfd398b8ff02efa0"),
    ];

    // The window's opening balance and the wallet's balance after the last row.
    // The footer checks opening + net == closing, which is what makes the figure
    // trustworthy rather than merely restated.
    let opening = 9_186_690_000i128;
    let resp = FlowLedger::new(&rows, &ada)
        .opening_balance(opening)
        .closing_balance(Some(18_518_660_000))
        .tolerance(1_000_000)
        .max_height(280.0)
        .show(ui);

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(
            "Note the gutter: the established off-ramp (blue) appears once, then every \
             remaining inflow is the custodial-withdrawal channel (orange). That change is the \
             finding — and it is invisible in a total.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.label(
        egui::RichText::new(
            "The 4,024.83 row is muted because it is a round trip: money this wallet sent out \
             a fortnight earlier, returned less two fees. Gross inflow still reports it; \
             genuine inflow does not.",
        )
        .color(TEXT_MUTED)
        .small(),
    );

    ui.add_space(8.0);
    if let Some(i) = resp.clicked_row {
        ui.label(
            egui::RichText::new(format!(
                "clicked row {i} — tx {}",
                rows[i].tx_id.unwrap_or("(none)")
            ))
            .color(ACCENT)
            .small(),
        );
    } else {
        ui.label(
            egui::RichText::new("Click a row — the host opens the transaction.")
                .color(TEXT_MUTED)
                .small(),
        );
    }
}
