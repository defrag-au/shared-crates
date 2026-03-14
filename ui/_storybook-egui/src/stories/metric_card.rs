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

    ui.horizontal(|ui| {
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

    ui.horizontal(|ui| {
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

    ui.horizontal(|ui| {
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
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("\u{2022} Cards show label, value, optional subtitle");
    ui.label("\u{2022} Trend arrows: green up, red down, muted flat");
    ui.label("\u{2022} Sparkline embeds inside card with matching value color");
    ui.label("\u{2022} Fixed-width cards align in horizontal rows");
    ui.label("\u{2022} Full-width card stretches to available space");
}
