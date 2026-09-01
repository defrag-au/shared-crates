use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Dashboard Metric Cards")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Compact stat cards for KPIs with optional trends and sparklines")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(12.0);

    // --- Row of basic cards ---
    ui.label(egui::RichText::new("Basic Cards").color(ACCENT).strong());
    ui.add_space(4.0);

    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
        egui_widgets::MetricCard::new("Total Accrued", "12,345")
            .subtitle("points")
            .width(160.0)
            .show(ui);

        egui_widgets::MetricCard::new("Effective Rate", "5.2")
            .subtitle("/hour")
            .value_color(egui_widgets::theme::SUCCESS)
            .width(160.0)
            .show(ui);

        egui_widgets::MetricCard::new("Active Buffs", "3")
            .subtitle("stacked")
            .value_color(egui_widgets::theme::WARNING)
            .width(160.0)
            .show(ui);
    });

    ui.add_space(16.0);

    // --- Cards with trends ---
    ui.label(
        egui::RichText::new("With Trend Indicators")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);

    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
        egui_widgets::MetricCard::new("Holders", "1,247")
            .trend(egui_widgets::Trend::Up, "+12 today")
            .width(180.0)
            .show(ui);

        egui_widgets::MetricCard::new("Treasury", "28.5M")
            .subtitle("$ALIEN remaining")
            .trend(egui_widgets::Trend::Down, "-142K/day")
            .value_color(egui_widgets::theme::WARNING)
            .width(180.0)
            .show(ui);

        egui_widgets::MetricCard::new("Floor Price", "52 ADA")
            .trend(egui_widgets::Trend::Flat, "stable")
            .width(180.0)
            .show(ui);
    });

    ui.add_space(16.0);

    // --- Cards with inline sparklines ---
    ui.label(
        egui::RichText::new("With Sparklines")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Cards can embed a sparkline for recent trend data")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    let accrual_data = [
        0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 60.0, 72.0, 86.0, 102.0,
        120.0, 140.0, 162.0, 186.0, 212.0,
    ];
    let rate_data = [
        5.0, 5.0, 5.0, 5.0, 10.0, 10.0, 10.0, 10.0, 10.0, 15.0, 15.0, 15.0, 15.0, 15.0, 15.0, 20.0,
        20.0, 20.0, 20.0, 20.0,
    ];

    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
        egui_widgets::MetricCard::new("Accrued Points", "212")
            .trend(egui_widgets::Trend::Up, "+30/hr")
            .sparkline(&accrual_data)
            .value_color(egui_widgets::theme::ACCENT_CYAN)
            .width(220.0)
            .show(ui);

        egui_widgets::MetricCard::new("Earning Rate", "20.0/hr")
            .trend(egui_widgets::Trend::Up, "buffed")
            .sparkline(&rate_data)
            .value_color(egui_widgets::theme::SUCCESS)
            .width(220.0)
            .show(ui);
    });

    ui.add_space(16.0);

    // --- Full-width card ---
    ui.label(egui::RichText::new("Full Width").color(ACCENT).strong());
    ui.add_space(4.0);

    let runway_data = [
        30.0, 29.8, 29.5, 29.1, 28.8, 28.5, 28.2, 27.9, 27.5, 27.2, 26.9, 26.5, 26.2, 25.9, 25.5,
        25.2, 24.9, 24.5, 24.2, 23.9,
    ];

    egui_widgets::MetricCard::new("Treasury Runway", "2.1 years")
        .subtitle("at current burn rate of 142K/day")
        .trend(egui_widgets::Trend::Down, "declining")
        .sparkline(&runway_data)
        .value_color(egui_widgets::theme::WARNING)
        .show(ui);

    ui.add_space(16.0);

    // --- A real stat row: same requested width, wildly different value widths ---
    //
    // This is the case the story previously CLAIMED to cover ("fixed-width
    // cards align in horizontal rows") without ever exercising: every card
    // below asks for the same width, but the values range from "11796" to
    // "278248 .. 333004 ADA". Lifted verbatim from token-explorer's readout,
    // where the row renders visibly ragged.
    ui.label(
        egui::RichText::new("Stat row — uniform width, varied values")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("every card requests width(150); a row must not depend on the values")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    const STATS: [(&str, &str); 6] = [
        ("at", "2026-08-29"),
        ("spot", "0.02165204 ADA"),
        ("holders", "11796"),
        ("notional", "2165203 ADA"),
        ("realisable", "278248 .. 333004 ADA"),
        ("honesty", "12.9% .. 15.4%"),
    ];

    ui.label(
        egui::RichText::new("per-card width(150) — every card a different width")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
        for (label, value) in STATS {
            egui_widgets::MetricCard::new(label, value)
                .width(150.0)
                .show(ui);
        }
    });

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("MetricRow — measured, so every card shares one edge")
            .color(TEXT_MUTED)
            .small(),
    );
    STATS
        .iter()
        .fold(egui_widgets::MetricRow::new(), |row, (label, value)| {
            row.push(egui_widgets::MetricCard::new(label, value))
        })
        .show(ui);

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("mixed heights — a trend on one card must not stagger the row")
            .color(TEXT_MUTED)
            .small(),
    );
    egui_widgets::MetricRow::new()
        .push(egui_widgets::MetricCard::new("holders", "11796"))
        .push(
            egui_widgets::MetricCard::new("notional", "2165203 ADA")
                .trend(egui_widgets::Trend::Down, "-98.7%"),
        )
        .push(egui_widgets::MetricCard::new("honesty", "12.9% .. 15.4%"))
        .show(ui);

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("\u{2022} Cards show label, value, optional subtitle");
    ui.label("\u{2022} Trend arrows: green up, red down, muted flat");
    ui.label("\u{2022} Sparkline embeds inside card with matching value color");
    // Was "Fixed-width cards align in horizontal rows" — asserted, never
    // demonstrated, and false: `width()` was advisory, so six cards asking for
    // 150 rendered at six widths. The stat-row section above is the case that
    // would have caught it.
    ui.label("\u{2022} width() pins the card; an oversized value still overflows it");
    ui.label("\u{2022} MetricRow measures first, so a row shares one width, height and baseline");
    ui.label("\u{2022} MetricRow never exceeds the available width — it shrinks the shared width instead");
    ui.label("\u{2022} Full-width card stretches to available space");
}
