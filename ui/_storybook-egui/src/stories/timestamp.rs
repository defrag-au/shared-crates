//! Story: `Timestamp` atom — consistent ISO-8601 timestamp rendering, plain or
//! as a clean badge. A pinned "now" keeps the hover's relative "x ago" stable.

use egui_widgets::Timestamp;

use crate::{ACCENT, TEXT_MUTED};

/// Pinned clock so the hover relative times are deterministic.
const NOW: i64 = 1_780_000_000;

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Timestamp").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "One atom for every timestamp in the app — ISO-8601 (UTC), fixed \
             monospace size (so it can't drift like `.small().monospace()` does), \
             with the full form + relative \"x ago\" on hover. Optional badge.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // (label, secs)
    let samples: [(&str, i64); 5] = [
        ("just now", NOW - 20),
        ("minutes", NOW - 42 * 60),
        ("hours", NOW - 5 * 3600),
        ("days", NOW - 2 * 86_400),
        ("an old one", 1_700_000_000),
    ];

    section(ui, "Plain (date + time, hover for full + relative)");
    for (note, ts) in samples {
        ui.horizontal(|ui| {
            ui.add(Timestamp::new(ts).now(NOW));
            ui.label(egui::RichText::new(note).small().color(TEXT_MUTED));
        });
    }

    ui.add_space(12.0);
    section(ui, "With seconds");
    for (_note, ts) in samples {
        ui.add(Timestamp::new(ts).now(NOW).with_seconds(true));
    }

    ui.add_space(12.0);
    section(ui, "Badge presentation");
    ui.horizontal_wrapped(|ui| {
        for (_note, ts) in samples {
            ui.add(Timestamp::new(ts).now(NOW).badge(true));
        }
    });

    ui.add_space(12.0);
    section(ui, "Coloured + larger (e.g. a header)");
    ui.add(
        Timestamp::new(NOW)
            .now(NOW)
            .with_seconds(true)
            .size(15.0)
            .color(ACCENT),
    );
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).small().strong());
    ui.add_space(3.0);
}
