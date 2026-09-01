//! Story: `ServiceBanner` — every tone, plus the two cases that decide the
//! layout: a long notice that has to wrap, and a narrow viewport.
//!
//! The wrapping case is the one worth keeping. Operator copy is written in the
//! moment ("swapping the sieve's disk, back in ~20 minutes") and nobody counts
//! characters while the service is down, so the banner has to survive a
//! sentence it was not sized for — on a phone, where the whole message is
//! about three lines wide.

use egui_widgets::{BannerTone, ServiceBanner};

use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Service Banner").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "A persistent strip saying the backend is not whole, in the operator's own \
             words. Takes layout space rather than covering content — it is a condition, \
             not an event, so a toast is the wrong instrument.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // Fixed stamps, as everywhere else in the storybook — the relative label
    // therefore ages with the file rather than reading "18 minutes ago"
    // forever. What is being reviewed is that the stamp renders and sits under
    // the message, not the number itself.
    const RAISED: i64 = 1_787_704_389;

    ui.label(
        egui::RichText::new("Under maintenance")
            .color(TEXT_MUTED)
            .small(),
    );
    ServiceBanner::new("wallet-sieve is down for planned work — back in about 20 minutes")
        .since(RAISED)
        .show(ui);
    ui.add_space(10.0);

    ui.label(
        egui::RichText::new("Back online — shown briefly after a notice lifts")
            .color(TEXT_MUTED)
            .small(),
    );
    ServiceBanner::new("wallet-sieve is back — history is loading again")
        .tone(BannerTone::Good)
        .dismissible(true)
        .show(ui);
    ui.add_space(10.0);

    ui.label(
        egui::RichText::new("Informational")
            .color(TEXT_MUTED)
            .small(),
    );
    ServiceBanner::new(
        "a full re-scan is running; some wallets will look shallow until it finishes",
    )
    .tone(BannerTone::Info)
    .show(ui);
    ui.add_space(10.0);

    ui.label(
        egui::RichText::new("A notice nobody counted the characters of")
            .color(TEXT_MUTED)
            .small(),
    );
    ServiceBanner::new(
        "wallet-sieve is offline while we move its database onto the larger volume. \
         Cached wallets will still name their counterparties, but no new history can be \
         read until this finishes. Nothing has been lost — the scan resumes where it stopped.",
    )
    .since(RAISED)
    .show(ui);
}
