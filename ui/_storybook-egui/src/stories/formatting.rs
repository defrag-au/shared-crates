use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Formatting Utilities")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Shared formatters in egui_widgets::utils for numbers, ADA, percentages, durations, and hex truncation")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(12.0);

    // --- ADA formatting ---
    ui.label(
        egui::RichText::new("format_ada / format_lovelace")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Lovelace to ADA with comma separators. Decimals only when non-zero.")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    let ada_cases: &[(i64, &str)] = &[
        (3_000_000_000, "3,000"),
        (1_500_000, "1.5"),
        (410_000_000, "410"),
        (12_345_670_000, "12,345.67"),
        (500_000, "0.5"),
        (50_000, "0.05"),
        (1_000_000, "1"),
        (0, "0"),
    ];

    egui::Grid::new("ada_grid")
        .num_columns(3)
        .spacing([20.0, 4.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Lovelace").color(TEXT_MUTED).small());
            ui.label(egui::RichText::new("format_ada").color(TEXT_MUTED).small());
            ui.label(
                egui::RichText::new("format_lovelace")
                    .color(TEXT_MUTED)
                    .small(),
            );
            ui.end_row();

            for (lovelace, expected) in ada_cases {
                let result = egui_widgets::format_ada(*lovelace);
                let pass = result == *expected;
                let color = if pass {
                    egui_widgets::theme::SUCCESS
                } else {
                    egui_widgets::theme::ERROR
                };

                ui.label(
                    egui::RichText::new(egui_widgets::format_number(*lovelace))
                        .family(egui::FontFamily::Monospace),
                );
                ui.label(egui::RichText::new(&result).color(color));
                ui.label(egui_widgets::format_lovelace(*lovelace));
                ui.end_row();
            }
        });

    ui.add_space(16.0);

    // --- Percentage formatting ---
    ui.label(egui::RichText::new("format_percent").color(ACCENT).strong());
    ui.label(
        egui::RichText::new("Drops unnecessary trailing zeros")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    let pct_cases: &[(f64, &str)] = &[
        (4.0, "4%"),
        (4.5, "4.5%"),
        (12.75, "12.75%"),
        (0.0, "0%"),
        (100.0, "100%"),
        (5.10, "5.1%"),
    ];

    egui::Grid::new("pct_grid")
        .num_columns(2)
        .spacing([20.0, 4.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Input").color(TEXT_MUTED).small());
            ui.label(egui::RichText::new("Output").color(TEXT_MUTED).small());
            ui.end_row();

            for (input, expected) in pct_cases {
                let result = egui_widgets::format_percent(*input);
                let pass = result == *expected;
                let color = if pass {
                    egui_widgets::theme::SUCCESS
                } else {
                    egui_widgets::theme::ERROR
                };

                ui.label(
                    egui::RichText::new(format!("{input}")).family(egui::FontFamily::Monospace),
                );
                ui.label(egui::RichText::new(&result).color(color));
                ui.end_row();
            }
        });

    ui.add_space(16.0);

    // --- Number formatting ---
    ui.label(egui::RichText::new("format_number").color(ACCENT).strong());
    ui.label(
        egui::RichText::new("Integer with comma separators")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    let num_cases: &[(i64, &str)] = &[
        (0, "0"),
        (999, "999"),
        (1_000, "1,000"),
        (1_234_567, "1,234,567"),
        (-42_000, "-42,000"),
    ];

    egui::Grid::new("num_grid")
        .num_columns(2)
        .spacing([20.0, 4.0])
        .show(ui, |ui| {
            for (input, expected) in num_cases {
                let result = egui_widgets::format_number(*input);
                let pass = result == *expected;
                let color = if pass {
                    egui_widgets::theme::SUCCESS
                } else {
                    egui_widgets::theme::ERROR
                };

                ui.label(
                    egui::RichText::new(format!("{input}")).family(egui::FontFamily::Monospace),
                );
                ui.label(egui::RichText::new(&result).color(color));
                ui.end_row();
            }
        });

    ui.add_space(16.0);

    // --- Duration formatting ---
    ui.label(
        egui::RichText::new("format_duration")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Seconds to human-readable duration")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    let dur_cases: &[(u64, &str)] = &[
        (30, "30s"),
        (90, "1m"),
        (3661, "1h 1m"),
        (86400, "1d"),
        (90061, "1d 1h"),
        (1_209_600, "14d"),
    ];

    egui::Grid::new("dur_grid")
        .num_columns(2)
        .spacing([20.0, 4.0])
        .show(ui, |ui| {
            for (input, expected) in dur_cases {
                let result = egui_widgets::format_duration(*input);
                let pass = result == *expected;
                let color = if pass {
                    egui_widgets::theme::SUCCESS
                } else {
                    egui_widgets::theme::ERROR
                };

                ui.label(
                    egui::RichText::new(format!("{input}s")).family(egui::FontFamily::Monospace),
                );
                ui.label(egui::RichText::new(&result).color(color));
                ui.end_row();
            }
        });

    ui.add_space(16.0);

    // --- Hex truncation ---
    ui.label(egui::RichText::new("truncate_hex").color(ACCENT).strong());
    ui.label(
        egui::RichText::new("Shorten hex strings with ellipsis")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    let hex_input = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8";
    ui.label(
        egui::RichText::new(format!("Input: {hex_input}"))
            .family(egui::FontFamily::Monospace)
            .small(),
    );
    ui.label(format!(
        "truncate_hex(_, 8, 4) = {}",
        egui_widgets::truncate_hex(hex_input, 8, 4)
    ));
    ui.label(format!(
        "truncate_hex(_, 12, 8) = {}",
        egui_widgets::truncate_hex(hex_input, 12, 8)
    ));
    ui.label(format!(
        "Short input (unchanged): {}",
        egui_widgets::truncate_hex("abcdef", 8, 4)
    ));
}
