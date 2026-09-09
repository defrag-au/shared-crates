use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Windowed Stat Strip")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A horizontal row of stat cards — the same metric summarised across time windows",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // --- Sales activity: full data across all windows ---
    ui.label(egui::RichText::new("Sales activity").color(ACCENT).strong());
    ui.add_space(4.0);

    let active = [
        egui_widgets::StatWindow::new("24h", "3")
            .trend(egui_widgets::Trend::Up, "+50%")
            .spark(vec![0.0, 1.0, 0.0, 2.0])
            .range(18.0, 19.0, 20.0)
            .detail("vol 57"),
        egui_widgets::StatWindow::new("7d", "11")
            .trend(egui_widgets::Trend::Up, "+22%")
            .spark(vec![1.0, 0.0, 2.0, 1.0, 3.0, 1.0, 3.0])
            .range(12.0, 19.0, 31.0)
            .detail("vol 214"),
        egui_widgets::StatWindow::new("30d", "48")
            .trend(egui_widgets::Trend::Down, "-8%")
            .spark(vec![
                2.0, 1.0, 3.0, 2.0, 4.0, 1.0, 0.0, 2.0, 3.0, 1.0, 2.0, 5.0, 3.0, 2.0, 1.0,
            ])
            .range(9.0, 18.0, 42.0)
            .detail("vol 930"),
    ];
    // The returned response surfaces the hovered sparkline bucket so the caller
    // can render its own tooltip (here just the indices; real callers show the
    // fills behind the bucket).
    egui_widgets::StatStrip::new(&active)
        .empty_note("no fills")
        .show(ui)
        .spark_tooltip(|ui, window, bucket| {
            ui.label(
                egui::RichText::new(format!("window {window} \u{00b7} bucket {bucket}")).small(),
            );
        });

    ui.add_space(16.0);

    // --- Sparse: quiet near windows fall back to the empty note ---
    ui.label(
        egui::RichText::new("Sparse activity")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Empty windows keep their zeroed headline and show the empty note")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    let sparse = [
        egui_widgets::StatWindow::new("24h", "0"),
        egui_widgets::StatWindow::new("7d", "0"),
        egui_widgets::StatWindow::new("30d", "4")
            .trend(egui_widgets::Trend::Flat, "0%")
            .spark(vec![0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 1.0])
            .range(10.0, 19.0, 19.0)
            .detail("vol 66"),
    ];
    egui_widgets::StatStrip::new(&sparse)
        .empty_note("no fills")
        .show(ui);

    ui.add_space(16.0);

    // --- Accent color + custom width ---
    ui.label(
        egui::RichText::new("Accent + narrow cards")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);

    let holders = [
        egui_widgets::StatWindow::new("Holders", "1,247").detail("+12 today"),
        egui_widgets::StatWindow::new("Listed", "88").detail("7.1% of supply"),
        egui_widgets::StatWindow::new("Floor", "52 ADA").detail("stable"),
    ];
    egui_widgets::StatStrip::new(&holders)
        .value_color(egui_widgets::theme::ACCENT_CYAN)
        .min_card_width(150.0)
        .show(ui);

    ui.add_space(16.0);

    // --- THE CASE THE OTHER EXAMPLES ALL MISS ---
    //
    // Every strip above uses a short, uniform trend ("+50%", "-8%"), and under
    // those the old widget looked fine while being badly broken: `card_width`
    // was a floor, each card is a `Frame`, and a frame grows to its content —
    // so the card width tracked the length of the trend string. Four windows
    // with four different magnitudes came out four different widths, wrapped
    // ragged, and the story's own test case claimed "cards align in a
    // fixed-width horizontal row" while demonstrating only values that happened
    // to be the same size.
    //
    // `MetricRow` has this exact warning in its docs. It is repeated here as a
    // story, because a comment in another module does not fail when it regresses
    // and a visibly ragged strip does.
    ui.label(
        egui::RichText::new("Unequal trend widths (regression case)")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Trend strings differ by 10+ characters. Every card must still be the same width, \
             the same height, and share one caption baseline — and the strip must fill the \
             container rather than trail off.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    let ragged = [
        egui_widgets::StatWindow::new("24h", "+0 \u{20b3}")
            .trend(egui_widgets::Trend::Flat, "+0 \u{20b3} / -0 \u{20b3}")
            .detail("0 tx"),
        egui_widgets::StatWindow::new("7d", "-502 \u{20b3}")
            .trend(egui_widgets::Trend::Down, "+373 \u{20b3} / -876 \u{20b3}")
            .spark(vec![3.0, 2.0, 1.0, 2.0, 3.0, 2.0, 3.0])
            .detail("2 tx"),
        egui_widgets::StatWindow::new("30d", "-30676 \u{20b3}")
            .trend(
                egui_widgets::Trend::Down,
                "+804523 \u{20b3} / -835199 \u{20b3}",
            )
            .spark(vec![2.0, 3.0, 1.0, 4.0, 2.0, 5.0, 1.0, 3.0, 2.0, 4.0])
            .detail("25 tx"),
        egui_widgets::StatWindow::new("all", "+416482 \u{20b3}")
            .trend(
                egui_widgets::Trend::Up,
                "+8473849 \u{20b3} / -8057366 \u{20b3}",
            )
            .spark(vec![1.0, 8.0, 2.0, 9.0, 1.0, 1.0, 2.0, 1.0, 1.0, 2.0])
            .detail("500 tx"),
    ];
    egui_widgets::StatStrip::new(&ragged)
        .empty_note("no movement")
        .show(ui);

    ui.add_space(16.0);

    // --- Narrow container: the strip has to WRAP into even rows ---
    //
    // Shown inside a pinned-width `Ui` rather than only at a narrow viewport,
    // so the wrap is visible on a desktop storybook run. The old widget packed
    // greedily and produced ragged groups (three cards and a lonely fourth);
    // the columns are chosen up front now, so every row is full but the last.
    ui.label(
        egui::RichText::new("Narrow container (wraps into even rows)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.scope(|ui| {
        ui.set_max_width(420.0);
        egui_widgets::StatStrip::new(&ragged)
            .empty_note("no movement")
            .show(ui);
    });

    ui.add_space(16.0);

    // --- All-empty state ---
    ui.label(egui::RichText::new("All empty").color(ACCENT).strong());
    ui.add_space(4.0);

    let empty = [
        egui_widgets::StatWindow::new("24h", "0"),
        egui_widgets::StatWindow::new("7d", "0"),
        egui_widgets::StatWindow::new("30d", "0"),
    ];
    egui_widgets::StatStrip::new(&empty)
        .empty_note("no fills")
        .show(ui);

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("\u{2022} Every card is the SAME width, whatever its trend string");
    ui.label("\u{2022} Every card is the same height, with or without a sparkline");
    ui.label("\u{2022} Captions share one baseline along the bottom of the row");
    ui.label("\u{2022} The strip fills its container, and wraps into even rows");
    ui.label("\u{2022} Headline stays visible even for empty windows");
    ui.label("\u{2022} Trend arrow + delta sit right of the headline");
    ui.label("\u{2022} Sparkline shows per-window activity shape");
    ui.label("\u{2022} Range bars share one axis across windows (median tick)");
    ui.label("\u{2022} Empty windows (no marks) show the strip's empty note");
    ui.label("\u{2022} Accent color and card width are configurable");
}
